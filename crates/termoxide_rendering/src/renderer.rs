//! Traverses the [`ViewNode`] tree and paints into a ratatui [`Buffer`].
//!
//! It paints the tree into ratatui's frame buffer.

use crossterm::{
    ExecutableCommand,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::Backend, buffer::Buffer, layout::Rect, style::Style};

use crate::view_node::{ViewContent, ViewNode};

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

/// Renders a [`ViewNode`] tree through a ratatui terminal.
pub struct Renderer<B: Backend> {
    /// Ratatui terminal — owns the backend and the two-frame diff buffer.
    terminal: Terminal<B>,
    /// Tracks whether terminal UI mode was enabled so it can be restored once.
    terminal_mode_active: bool,
}

impl<B: Backend> Renderer<B> {
    /// Create a renderer and enter the alternate screen.
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

    /// Create a renderer for tests without changing the terminal.
    #[cfg(any(test, feature = "test-util"))]
    #[doc(hidden)]
    pub fn new_for_test(terminal: Terminal<B>) -> Self { Self { terminal, terminal_mode_active: false } }

    /// Return the current terminal viewport.
    pub fn viewport(&self) -> Rect { self.terminal.size().unwrap_or_default() }

    /// Render one frame.
    pub fn render_frame(&mut self, root: &mut ViewNode) -> Result<(), RenderError> {
        self.terminal.draw(|frame| {
            let buf = frame.buffer_mut();
            Self::draw_node(root, buf);
        })?;

        Ok(())
    }

    /// Draw a node and its children into a buffer.
    pub fn draw_node(node: &ViewNode, buf: &mut Buffer) {
        match &node.content {
            ViewContent::Container { style } => {
               buf.set_style(node.area, *style);
            },

            ViewContent::Fragment => {},

            ViewContent::Text { text, style } => {
                Self::draw_text(buf, node.area, text, *style);
            },

            ViewContent::Raw(f) => {
                f(buf, node.area);
            },
        }

        for child in &node.children {
            Self::draw_node(child, buf);
        }
    }

    /// Write text into a buffer.
    fn draw_text(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let max_chars = area.width as usize;
        let truncated: String = text.chars().take(max_chars).collect();

        buf.set_string(area.x, area.y, &truncated, style);
    }

    /// Restore the terminal.
    pub fn restore(&mut self) -> Result<(), RenderError> {
        if !self.terminal_mode_active {
            return Ok(());
        }

        self.terminal.show_cursor()?;
        std::io::stdout().execute(LeaveAlternateScreen)?;
        self.terminal_mode_active = false;
        Ok(())
    }

    /// Borrow the underlying terminal.
    pub fn terminal(&self) -> &Terminal<B> { &self.terminal }

    /// Mutably borrow the underlying terminal.
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
        style::{Color, Style},
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
    fn draw_node_paints_the_container_style_across_its_area() {
        let area = Rect::new(0, 0, 2, 2);
        let node = ViewNode::styled_container(area, Vec::new(), Style::default().bg(Color::Blue));
        let mut buf = Buffer::empty(area);

        Renderer::<TestBackend>::draw_node(&node, &mut buf);

        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(buf.get(x, y).bg, Color::Blue);
        }
    }

    #[test]
    fn draw_node_leaves_a_default_styled_container_invisible() {
        let area = Rect::new(0, 0, 1, 1);
        let node = ViewNode::container(area, Vec::new());
        let mut buf = Buffer::empty(area);

        Renderer::<TestBackend>::draw_node(&node, &mut buf);

        assert_eq!(*buf.get(0, 0), ratatui::buffer::Cell::default());
    }

    #[test]
    fn draw_node_draws_children_over_the_container_style() {
        let area = Rect::new(0, 0, 1, 1);
        let node = ViewNode::styled_container(
            area,
            vec![ViewNode::text(area, "X", Style::default().fg(Color::Red))],
            Style::default().bg(Color::Blue),
        );
        let mut buf = Buffer::empty(area);

        Renderer::<TestBackend>::draw_node(&node, &mut buf);

        assert_eq!(buf.get(0, 0).symbol(), "X");
        assert_eq!(buf.get(0, 0).fg, Color::Red);
        assert_eq!(buf.get(0, 0).bg, Color::Blue);
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
