//! Traverses the [`ViewNode`] tree and paints into a ratatui [`Buffer`].
//!
//! [`Renderer`] is the bridge between the logical UI tree and raw terminal
//! cells.  It performs two distinct jobs on every frame:
//!
//! 1. **Paint** — walk the [`ViewNode`] tree top-down, dispatching each node's
//!    [`ViewContent`] to the appropriate draw routine, accumulating the results
//!    in an in-memory [`Buffer`].
//!
//! 2. **Diff & flush** — compare the freshly painted buffer against the
//!    previous frame's buffer, encode only the *changed* cells as ANSI escape
//!    sequences, and hand them to ratatui's [`Terminal::draw`] machinery.
//!
//! ## Position in the pipeline
//!
//! ```text
//! ViewNode tree (from components)
//!       │
//!       ▼
//!   Renderer::render_frame()
//!   ├── draw_node(root, buf)    ← recursive tree walk
//!   │   ├── draw_node(child_1, buf)
//!   │   └── draw_node(child_2, buf)
//!   │       └── …
//!   └── terminal.draw(|frame| frame.render_buffer(buf))
//!             │
//!             ▼
//!         stdout  (escape sequences for changed cells only)
//! ```
//!
//! ## Dirty-node optimisation
//!
//! If [`ViewNode::is_subtree_dirty`] returns `false` for a sub-tree, the
//! renderer skips that sub-tree entirely.  This means only the regions of the
//! screen that actually changed incur any CPU work.  The diffing built into
//! ratatui handles the rest: even if a node is re-drawn, only cells whose byte
//! content changed produce output bytes.
//!
//! ## Example
//!
//! ```rust,no_run
//! use ratatui::{backend::CrosstermBackend, Terminal};
//! use termoxide_rendering::renderer::Renderer;
//! use termoxide_rendering::view_node::{ViewNode, ViewContent};
//! use ratatui::layout::Rect;
//! use ratatui::style::{Style, Color};
//! use std::io::stdout;
//!
//! let backend  = CrosstermBackend::new(stdout());
//! let terminal = Terminal::new(backend).unwrap();
//! let mut renderer = Renderer::new(terminal).unwrap();
//!
//! let mut root = ViewNode::text(
//!     Rect::new(0, 0, 80, 24),
//!     "Hello, TermOxide!",
//!     Style::default().fg(Color::Cyan),
//! );
//!
//! renderer.render_frame(&mut root).unwrap();
//! ```

use crossterm::ExecutableCommand;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{Terminal, backend::Backend, buffer::Buffer, layout::Rect, style::Style};

use crate::view_node::{ViewContent, ViewNode};

// ─────────────────────────────────────────────────────────────────────────── //
//  Error type
// ─────────────────────────────────────────────────────────────────────────── //

/// Errors that can arise during a render pass.
#[derive(Debug)]
pub enum RenderError {
    /// The underlying ratatui / crossterm I/O operation failed.
    Io(std::io::Error),

    /// A user-provided raw draw closure panicked.
    DrawPanic,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "render I/O error: {e}"),
            Self::DrawPanic => write!(f, "render draw callback panicked"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::DrawPanic => None,
        }
    }
}

impl From<std::io::Error> for RenderError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Renderer
// ─────────────────────────────────────────────────────────────────────────── //

/// Walks the [`ViewNode`] tree and produces terminal output via ratatui.
///
/// Each [`Renderer`] owns a ratatui [`Terminal`], which in turn manages the
/// double-buffering and escape-sequence diffing.  The renderer adds a third
/// layer on top: the [`ViewNode`] dirty-flag check, which skips sub-trees that
/// have not changed since the last frame.
///
/// ## Generic parameter
///
/// `B` is any ratatui [`Backend`] (typically [`CrosstermBackend<Stdout>`]).
/// The type parameter is exposed so that tests can substitute a
/// [`TestBackend`][ratatui::backend::TestBackend].
///
/// ## Thread safety
///
/// `Renderer` is **not** `Sync`.  It must be owned and driven from a single
/// thread (the render thread inside [`RenderLoop`][crate::render_loop::RenderLoop]).
pub struct Renderer<B: Backend> {
    /// Ratatui terminal — owns the backend and the two-frame diff buffer.
    terminal: Terminal<B>,
}

impl<B: Backend> Renderer<B> {
    /// Create a new `Renderer` wrapping the given ratatui terminal.
    ///
    /// The terminal's cursor is hidden immediately so that it does not
    /// flicker during rendering.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Io`] if hiding the cursor fails.
    pub fn new(mut terminal: Terminal<B>) -> Result<Self, RenderError> {
        enable_raw_mode()?;
        std::io::stdout().execute(EnterAlternateScreen)?;
        // Enable mouse capture so the application receives mouse events.
        std::io::stdout().execute(EnableMouseCapture)?;
        terminal.hide_cursor()?;
        Ok(Self { terminal })
    }

    /// Return the current terminal viewport size as a [`Rect`].
    ///
    /// This is the authoritative size the layout engine should use when
    /// computing node positions.
    pub fn viewport(&self) -> Rect {
        self.terminal.size().unwrap_or_default()
    }

    // ── Main render entry point ────────────────────────────────────────────── //

    /// Render one complete frame from `root` to the terminal.
    ///
    /// The method:
    /// 1. Calls [`draw_node`][Self::draw_node] recursively on `root`, filling
    ///    a temporary [`Buffer`] from the top of the tree down.
    /// 2. Hands the filled buffer to ratatui's `Terminal::draw`, which diffs
    ///    it against the previous frame and writes only the changed cells.
    /// 3. Calls [`ViewNode::mark_clean`] on `root` so that unchanged nodes are
    ///    skipped on the next call.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Io`] on any I/O failure inside ratatui, or
    /// [`RenderError::DrawPanic`] if a raw draw callback panics.
    pub fn render_frame(&mut self, root: &mut ViewNode) -> Result<(), RenderError> {
        let mut draw_error = None;

        // Draw directly into the frame buffer that ratatui provides.
        // ratatui handles the diff against the previous frame and writes
        // only changed cells to stdout.
        self.terminal.draw(|frame| {
            if draw_error.is_some() {
                return;
            }

            let buf = frame.buffer_mut();
            if let Err(err) = Self::draw_node(root, buf) {
                draw_error = Some(err);
            }
        })?;

        if let Some(err) = draw_error {
            return Err(err);
        }

        // Clear dirty flags now that the frame is committed.
        root.mark_clean();
        Ok(())
    }

    // ── Recursive tree walk ────────────────────────────────────────────────── //

    /// Recursively draw `node` into `buf`.
    ///
    /// Sub-trees where [`ViewNode::is_subtree_dirty`] returns `false` are
    /// skipped — this is the primary optimisation that keeps frame work O(dirty
    /// nodes) rather than O(all nodes).
    ///
    /// The traversal is depth-first, parent before children, so that child
    /// content always appears on top of parent background fills.
    pub fn draw_node(node: &ViewNode, buf: &mut Buffer) -> Result<(), RenderError> {
        // Fast path: nothing changed in this sub-tree.
        if !node.is_subtree_dirty() {
            return Ok(());
        }

        // Draw this node's own content.
        match &node.content {
            ViewContent::Container => {
                // Pure layout node — no visual output. Children are handled
                // below.
            }

            ViewContent::Text { text, style } => {
                Self::draw_text(buf, node.area, text, *style);
            }

            ViewContent::Raw(f) => {
                let draw_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    f(buf, node.area);
                }));
                if draw_result.is_err() {
                    return Err(RenderError::DrawPanic);
                }
            }
        }

        // Recurse into children (document order).
        for child in &node.children {
            Self::draw_node(child, buf)?;
        }

        Ok(())
    }

    // ── Primitive draw helpers ─────────────────────────────────────────────── //

    /// Write `text` into `buf` at `area.x, area.y`, applying `style`.
    ///
    /// The text is truncated to `area.width` terminal columns.  No wrapping is
    /// performed — multi-line text must be expressed as sibling `Text` nodes.
    fn draw_text(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let buf_area = *buf.area();
        let buf_right = buf_area.x.saturating_add(buf_area.width);
        let buf_bottom = buf_area.y.saturating_add(buf_area.height);

        // Text nodes are single-line, so skip draws whose baseline row is off-screen.
        if area.y < buf_area.y || area.y >= buf_bottom {
            return;
        }

        let node_left = area.x;
        let node_right = area.x.saturating_add(area.width);
        let visible_left = node_left.max(buf_area.x);
        let visible_right = node_right.min(buf_right);

        if visible_left >= visible_right {
            return;
        }

        // If the node starts off-screen to the left, skip that many chars.
        let skip_chars = visible_left.saturating_sub(node_left) as usize;
        let max_chars = visible_right.saturating_sub(visible_left) as usize;

        let clipped: String = text.chars().skip(skip_chars).take(max_chars).collect();
        if clipped.is_empty() {
            return;
        }

        buf.set_string(visible_left, area.y, &clipped, style);
    }

    // ── Terminal lifecycle ─────────────────────────────────────────────────── //

    /// Restore the terminal to a clean state.
    ///
    /// Must be called before the process exits (or panics) to avoid leaving
    /// the user's terminal in raw mode without a visible cursor.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Io`] if the restore operation fails.  The caller
    /// should log the error but not panic — the process is about to exit anyway.
    pub fn restore(&mut self) -> Result<(), RenderError> {
        self.terminal.show_cursor()?;
        // Disable mouse capture and restore the alternate screen and raw mode.
        std::io::stdout().execute(DisableMouseCapture)?;
        std::io::stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;
        Ok(())
    }

    /// Borrow the underlying [`Terminal`].
    pub fn terminal(&self) -> &Terminal<B> {
        &self.terminal
    }

    /// Mutably borrow the underlying [`Terminal`].
    pub fn terminal_mut(&mut self) -> &mut Terminal<B> {
        &mut self.terminal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn draw_text_truncates_to_node_width() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 5, 1));
        let node = ViewNode::text(Rect::new(0, 0, 3, 1), "abcdef", Style::default());

        Renderer::<TestBackend>::draw_node(&node, &mut buf).unwrap();

        assert_eq!(buf.get(0, 0).symbol(), "a");
        assert_eq!(buf.get(1, 0).symbol(), "b");
        assert_eq!(buf.get(2, 0).symbol(), "c");
        assert_eq!(buf.get(3, 0).symbol(), " ");
    }

    #[test]
    fn clean_subtree_is_skipped() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 5, 1));
        let mut node = ViewNode::text(Rect::new(0, 0, 5, 1), "hello", Style::default());
        node.dirty = false;

        Renderer::<TestBackend>::draw_node(&node, &mut buf).unwrap();

        assert_eq!(buf.get(0, 0).symbol(), " ");
    }

    #[test]
    fn zero_area_text_node_is_noop() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
        let node = ViewNode::text(Rect::new(0, 0, 0, 1), "x", Style::default());

        Renderer::<TestBackend>::draw_node(&node, &mut buf).unwrap();

        assert_eq!(buf.get(0, 0).symbol(), " ");
    }

    #[test]
    fn raw_node_can_write_into_buffer() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 5, 1));
        let node = ViewNode::raw(Rect::new(0, 0, 5, 1), |buf, area| {
            buf.set_string(area.x, area.y, "ok", Style::default());
        });

        Renderer::<TestBackend>::draw_node(&node, &mut buf).unwrap();

        assert_eq!(buf.get(0, 0).symbol(), "o");
        assert_eq!(buf.get(1, 0).symbol(), "k");
    }

    #[test]
    fn out_of_bounds_text_area_is_ignored() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        let node = ViewNode::text(Rect::new(0, 50, 3, 1), "boom", Style::default());

        Renderer::<TestBackend>::draw_node(&node, &mut buf).unwrap();
        assert_eq!(buf.get(0, 0).symbol(), " ");
    }

    #[test]
    fn raw_draw_closure_panic_becomes_error() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        let node = ViewNode::raw(Rect::new(0, 0, 1, 1), |_, _| {
            panic!("renderer raw closure panic");
        });

        let result = Renderer::<TestBackend>::draw_node(&node, &mut buf);
        assert!(matches!(result, Err(RenderError::DrawPanic)));
    }

    #[test]
    fn text_draw_clips_left_side_against_buffer_origin() {
        let mut buf = Buffer::empty(Rect::new(2, 0, 3, 1));
        let node = ViewNode::text(Rect::new(0, 0, 5, 1), "abcde", Style::default());

        Renderer::<TestBackend>::draw_node(&node, &mut buf).unwrap();

        assert_eq!(buf.get(2, 0).symbol(), "c");
        assert_eq!(buf.get(3, 0).symbol(), "d");
        assert_eq!(buf.get(4, 0).symbol(), "e");
    }
}
