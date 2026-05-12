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
//! let mut root = ViewNode::text(
//!     Rect::new(0, 0, 80, 24),
//!     "Hello, TermOxide!",
//!     Style::default().fg(Color::Cyan),
//! );
//!
//! renderer.render_frame(&mut root).unwrap();
//! ```

use crossterm::{
    ExecutableCommand,
    event::{DisableMouseCapture, EnableMouseCapture},
    terminal::{
        EnterAlternateScreen,
        LeaveAlternateScreen,
        disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{
    Terminal,
    backend::Backend,
    buffer::Buffer,
    layout::Rect,
    style::Style,
};

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
/// thread (the render thread inside
/// [`RenderLoop`][crate::render_loop::RenderLoop]).
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
    /// # Errors
    ///
    /// Returns [`RenderError::Io`] if hiding the cursor fails.
    pub fn new(mut terminal: Terminal<B>) -> Result<Self, RenderError> {
        enable_raw_mode()?;
        if let Err(e) = (|| -> std::io::Result<()> {
            std::io::stdout().execute(EnterAlternateScreen)?;
            std::io::stdout().execute(EnableMouseCapture)?;
            terminal.hide_cursor()?;
            Ok(())
        })() {
            let _ = std::io::stdout().execute(DisableMouseCapture);
            let _ = std::io::stdout().execute(LeaveAlternateScreen);
            let _ = disable_raw_mode();
            return Err(RenderError::Io(e));
        }
        Ok(Self {
            terminal,
            terminal_mode_active: true,
        })
    }

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
    /// 1. Calls [`draw_node`][Self::draw_node] recursively on `root`, filling
    ///    a temporary [`Buffer`] from the top of the tree down.
    /// 2. Hands the filled buffer to ratatui's `Terminal::draw`, which diffs
    ///    it against the previous frame and writes only the changed cells.
    /// 3. Returns without additional bookkeeping; ratatui handles the diff
    ///    against the previous frame.
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
            }
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
    /// the user's terminal in raw mode without a visible cursor.
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
        // Disable mouse capture and restore the alternate screen and raw mode.
        std::io::stdout().execute(DisableMouseCapture)?;
        std::io::stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;
        self.terminal_mode_active = false;
        Ok(())
    }

    /// Borrow the underlying [`Terminal`].
    pub fn terminal(&self) -> &Terminal<B> { &self.terminal }

    /// Mutably borrow the underlying [`Terminal`].
    pub fn terminal_mut(&mut self) -> &mut Terminal<B> { &mut self.terminal }
}

impl<B: Backend> Drop for Renderer<B> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
