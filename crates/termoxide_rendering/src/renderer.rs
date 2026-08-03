//! Traverses the [`ViewNode`] tree and paints into a ratatui [`Buffer`].
//!
//! [`Renderer`] is the bridge between the logical UI tree and raw terminal
//! cells.  It performs two distinct jobs on every frame:
//!
//! 1. **Paint** — walk the [`ViewNode`] tree top-down, dispatching each node's [`ViewContent`] to the appropriate draw
//!    routine, accumulating the results in an in-memory [`Buffer`].
//!
//! 2. **Diff & flush** — compare the freshly painted buffer against the previous frame's buffer, encode only the
//!    *changed* cells as ANSI escape sequences, and hand them to ratatui's [`Terminal::draw`] machinery.
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
//! ## Traversal model
//!
//! The renderer walks the full tree on each frame and relies on ratatui's
//! diffing to avoid writing unchanged cells to the terminal.
//!
//! ## Example
//!
//! ```rust,no_run
//! use std::io::stdout;
//!
//! use ratatui::{
//!     Terminal,
//!     backend::CrosstermBackend,
//!     layout::Rect,
//!     style::{Color, Style},
//! };
//! use termoxide_rendering::{
//!     renderer::Renderer,
//!     view_node::{ViewContent, ViewNode},
//! };
//!
//! let backend = CrosstermBackend::new(stdout());
//! let terminal = Terminal::new(backend).unwrap();
//! let mut renderer = Renderer::new(terminal).unwrap();
//!
//! let mut root = ViewNode::text(Rect::new(0, 0, 80, 24), "Hello, TermOxide!", Style::default().fg(Color::Cyan));
//!
//! renderer.render_frame(&mut root).unwrap();
//! ```

use crossterm::{
    ExecutableCommand,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::Backend, buffer::Buffer, layout::Rect, style::Style};

use crate::view_node::{ViewContent, ViewNode};

// ───────────────────────────────────────────────────────────────────────────
// //  Error type
// ───────────────────────────────────────────────────────────────────────────
// //

/// Errors that can arise during a render pass.
#[derive(Debug)]
pub enum RenderError {
    /// The underlying ratatui / crossterm I/O operation failed.
    Io(std::io::Error),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "render I/O error: {e}"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for RenderError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}

// ───────────────────────────────────────────────────────────────────────────
// //  Renderer
// ───────────────────────────────────────────────────────────────────────────
// //

/// Walks the [`ViewNode`] tree and produces terminal output via ratatui.
///
/// Each [`Renderer`] owns a ratatui [`Terminal`], which in turn manages the
/// double-buffering and escape-sequence diffing.  The renderer feeds the
/// terminal a freshly painted buffer on every call.
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
/// thread by the application's main loop.
pub struct Renderer<B: Backend> {
    /// Ratatui terminal — owns the backend and the two-frame diff buffer.
    terminal: Terminal<B>,
    /// Tracks whether terminal UI mode was enabled so it can be restored once.
    terminal_mode_active: bool,
}

impl<B: Backend> Renderer<B> {
    /// Create a new `Renderer` wrapping the given ratatui terminal.
    ///
    /// The terminal's cursor is hidden immediately so that it does not
    /// flicker during rendering.
    ///
    /// ## Raw mode
    ///
    /// The renderer does **not** manage raw mode: that belongs to
    /// [`EventStream`][termoxide_event::EventStream], which needs it to read
    /// input and restores it on drop. Create the stream before the renderer and
    /// keep it alive for at least as long, otherwise this alternate screen will
    /// be driven by a cooked terminal.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Io`] if entering the alternate screen or hiding
    /// the cursor fails.
    pub fn new(mut terminal: Terminal<B>) -> Result<Self, RenderError> {
        if let Err(e) = (|| -> std::io::Result<()> {
            std::io::stdout().execute(EnterAlternateScreen)?;
            terminal.hide_cursor()?;
            Ok(())
        })() {
            let _ = std::io::stdout().execute(LeaveAlternateScreen);
            return Err(RenderError::Io(e));
        }
        Ok(Self { terminal, terminal_mode_active: true })
    }

    /// Create a `Renderer` for testing, which does not enter the alternate
    /// screen.
    #[cfg(test)]
    pub(crate) fn new_for_test(terminal: Terminal<B>) -> Self { Self { terminal, terminal_mode_active: false } }

    /// Return the current terminal viewport size as a [`Rect`].
    ///
    /// This is the authoritative size the layout engine should use when
    /// computing node positions.
    pub fn viewport(&self) -> Rect { self.terminal.size().unwrap_or_default() }

    // ── Main render entry point ──────────────────────────────────────────────
    // //

    /// Render one complete frame from `root` to the terminal.
    ///
    /// The method:
    /// 1. Calls [`draw_node`][Self::draw_node] recursively on `root`, filling a temporary [`Buffer`] from the top of
    ///    the tree down.
    /// 2. Hands the filled buffer to ratatui's `Terminal::draw`, which diffs it against the previous frame and writes
    ///    only the changed cells.
    /// 3. Returns without additional bookkeeping; ratatui handles the diff against the previous frame.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Io`] on any I/O failure inside ratatui.
    pub fn render_frame(&mut self, root: &mut ViewNode) -> Result<(), RenderError> {
        // Draw directly into the frame buffer that ratatui provides.
        // ratatui handles the diff against the previous frame and writes
        // only changed cells to stdout.
        self.terminal.draw(|frame| {
            let buf = frame.buffer_mut();
            Self::draw_node(root, buf);
        })?;

        Ok(())
    }

    // ── Recursive tree walk ──────────────────────────────────────────────────
    // //

    /// Recursively draw `node` into `buf`.
    ///
    /// The traversal is depth-first, parent before children, so that child
    /// content always appears on top of parent background fills.
    pub fn draw_node(node: &ViewNode, buf: &mut Buffer) {
        // Draw this node's own content.
        match &node.content {
            ViewContent::Container => {
                // Pure layout node — no visual output. Children are handled
                // below.
            },

            ViewContent::Text { text, style } => {
                Self::draw_text(buf, node.area, text, *style);
            },

            ViewContent::Raw(f) => {
                f(buf, node.area);
            },
        }

        // Recurse into children (document order).
        for child in &node.children {
            Self::draw_node(child, buf);
        }
    }

    // ── Primitive draw helpers ───────────────────────────────────────────────
    // //

    /// Write `text` into `buf` at `area.x, area.y`, applying `style`.
    ///
    /// The text is truncated to `area.width` terminal columns.  No wrapping is
    /// performed — multi-line text must be expressed as sibling `Text` nodes.
    fn draw_text(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let max_chars = area.width as usize;
        let truncated: String = text.chars().take(max_chars).collect();

        buf.set_string(area.x, area.y, &truncated, style);
    }

    // ── Terminal lifecycle ───────────────────────────────────────────────────
    // //

    /// Restore the terminal to a clean state.
    ///
    /// Must be called before the process exits (or panics) to avoid leaving
    /// the user staring at the alternate screen without a visible cursor.
    /// Raw mode is not touched here — see [`Self::new`].
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Io`] if the restore operation fails.  The caller
    /// should log the error but not panic — the process is about to exit
    /// anyway.
    pub fn restore(&mut self) -> Result<(), RenderError> {
        if !self.terminal_mode_active {
            return Ok(());
        }

        self.terminal.show_cursor()?;
        std::io::stdout().execute(LeaveAlternateScreen)?;
        self.terminal_mode_active = false;
        Ok(())
    }

    /// Borrow the underlying [`Terminal`].
    pub fn terminal(&self) -> &Terminal<B> { &self.terminal }

    /// Mutably borrow the underlying [`Terminal`].
    pub fn terminal_mut(&mut self) -> &mut Terminal<B> { &mut self.terminal }
}

impl<B: Backend> Drop for Renderer<B> {
    fn drop(&mut self) { let _ = self.restore(); }
}

#[cfg(test)]
mod tests {
    use std::io::IsTerminal;

    use ratatui::{
        backend::{CrosstermBackend, TestBackend},
        layout::Rect,
        style::Style,
    };

    use super::*;

    #[test]
    fn draw_node_truncates_text_to_area() {
        let area = Rect::new(0, 0, 3, 1);
        let node = ViewNode::text(area, "abcdef", Style::default());
        let mut buf = Buffer::empty(area);

        Renderer::<TestBackend>::draw_node(&node, &mut buf);

        assert_eq!(buf.get(0, 0).symbol(), "a");
        assert_eq!(buf.get(1, 0).symbol(), "b");
        assert_eq!(buf.get(2, 0).symbol(), "c");
    }

    #[test]
    fn draw_node_renders_children_after_parent() {
        let area = Rect::new(0, 0, 1, 1);
        let parent = ViewNode::raw(area, |buf, rect| {
            buf.get_mut(rect.x, rect.y).set_symbol("A");
        })
        .with_children(vec![ViewNode::text(area, "B", Style::default())]);

        let mut buf = Buffer::empty(area);
        Renderer::<TestBackend>::draw_node(&parent, &mut buf);

        assert_eq!(buf.get(0, 0).symbol(), "B");
    }

    #[test]
    fn render_frame_writes_to_backend_buffer() {
        let backend = TestBackend::new(4, 1);
        let terminal = Terminal::new(backend).expect("terminal");
        let mut renderer = Renderer::new_for_test(terminal);
        let area = Rect::new(0, 0, 4, 1);
        let mut root = ViewNode::text(area, "hey!", Style::default());

        renderer.render_frame(&mut root).expect("render");

        let buffer = renderer.terminal().backend().buffer();
        assert_eq!(buffer.get(0, 0).symbol(), "h");
        assert_eq!(buffer.get(1, 0).symbol(), "e");
        assert_eq!(buffer.get(2, 0).symbol(), "y");
        assert_eq!(buffer.get(3, 0).symbol(), "!");
    }

    #[test]
    fn restore_is_noop_when_inactive() {
        let backend = TestBackend::new(2, 1);
        let terminal = Terminal::new(backend).expect("terminal");
        let mut renderer = Renderer::new_for_test(terminal);

        assert!(!renderer.terminal_mode_active);
        assert!(renderer.restore().is_ok());
        assert!(!renderer.terminal_mode_active);
    }

    #[test]
    fn new_and_restore_work_on_terminal() {
        if !std::io::stdout().is_terminal() {
            return;
        }

        let backend = CrosstermBackend::new(std::io::stdout());
        let terminal = Terminal::new(backend).expect("terminal");
        let mut renderer = Renderer::new(terminal).expect("renderer");

        assert!(renderer.terminal_mode_active);
        renderer.restore().expect("restore");
        assert!(!renderer.terminal_mode_active);
    }
}
