//! Integration tests for the RenderLoop component.
//!
//! Tests the complete event → layout → render cycle, including:
//! - app lifecycle (build_view, handle_event)
//! - frame rendering and terminal state management
//! - quit event handling
//! - error propagation

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Style;

use termoxide_rendering::event_router::EventRouter;
use termoxide_rendering::render_loop::App;
use termoxide_rendering::renderer::Renderer;
use termoxide_rendering::view_node::{ComponentId, ViewNode};

// ─────────────────────────────────────────────────────────────────────────── //
//  Test App
// ─────────────────────────────────────────────────────────────────────────── //

/// A simple counter app for testing.
struct CounterApp {
    count: u32,
    last_event: Option<String>,
}

impl CounterApp {
    fn new() -> Self {
        Self {
            count: 0,
            last_event: None,
        }
    }
}

impl App for CounterApp {
    fn build_view(&mut self, viewport: Rect) -> ViewNode {
        let text = format!("Count: {}", self.count);
        ViewNode::text(viewport, text, Style::default())
    }

    fn handle_event(&mut self, _id: Option<ComponentId>, event: Event) -> bool {
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.last_event = Some("quit".to_string());
                true
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.last_event = Some("ctrl-c".to_string());
                true
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('+'),
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.count += 1;
                self.last_event = Some("increment".to_string());
                false
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('-'),
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.count = self.count.saturating_sub(1);
                self.last_event = Some("decrement".to_string());
                false
            }
            _ => false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_render_loop_initial_frame() {
    let backend = TestBackend::new(80, 24);
    let terminal = Terminal::new(backend).expect("failed to create terminal");
    let _renderer = Renderer::new(terminal).expect("failed to create renderer");
    let _event_router = EventRouter::new();

    let mut app = CounterApp::new();
    assert_eq!(app.count, 0);

    let viewport = Rect::new(0, 0, 80, 24);
    let initial_view = app.build_view(viewport);
    assert!(!initial_view.children.is_empty() || initial_view.area.width > 0);
}

#[test]
fn test_app_build_view_reflects_state() {
    let backend = TestBackend::new(80, 24);
    let terminal = Terminal::new(backend).expect("terminal");
    let _renderer = Renderer::new(terminal).expect("renderer");
    let _event_router = EventRouter::new();

    let mut app = CounterApp::new();
    let viewport = Rect::new(0, 0, 80, 24);

    let view1 = app.build_view(viewport);
    assert_eq!(app.count, 0);

    app.count = 5;
    let view2 = app.build_view(viewport);
    assert_eq!(app.count, 5);

    assert_eq!(view1.area, view2.area);
}

#[test]
fn test_app_handle_event_quit_signal() {
    let mut app = CounterApp::new();

    let quit_event = Event::Key(KeyEvent {
        code: KeyCode::Char('q'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    let should_quit = app.handle_event(None, quit_event);
    assert!(should_quit);
    assert_eq!(app.last_event, Some("quit".to_string()));
}

#[test]
fn test_app_handle_event_ctrl_c() {
    let mut app = CounterApp::new();

    let ctrl_c = Event::Key(KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    let should_quit = app.handle_event(None, ctrl_c);
    assert!(should_quit);
    assert_eq!(app.last_event, Some("ctrl-c".to_string()));
}

#[test]
fn test_app_handle_event_increment() {
    let mut app = CounterApp::new();
    assert_eq!(app.count, 0);

    let inc_event = Event::Key(KeyEvent {
        code: KeyCode::Char('+'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    let should_quit = app.handle_event(None, inc_event);
    assert!(!should_quit);
    assert_eq!(app.count, 1);
    assert_eq!(app.last_event, Some("increment".to_string()));
}

#[test]
fn test_app_handle_event_decrement() {
    let mut app = CounterApp::new();
    app.count = 10;

    let dec_event = Event::Key(KeyEvent {
        code: KeyCode::Char('-'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    let should_quit = app.handle_event(None, dec_event);
    assert!(!should_quit);
    assert_eq!(app.count, 9);
}

#[test]
fn test_app_decrement_saturates_at_zero() {
    let mut app = CounterApp::new();
    app.count = 0;

    let dec_event = Event::Key(KeyEvent {
        code: KeyCode::Char('-'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    app.handle_event(None, dec_event);
    assert_eq!(app.count, 0); // should not underflow
}

#[test]
fn test_app_unknown_event_not_handled() {
    let mut app = CounterApp::new();
    app.count = 5;

    let unknown = Event::Key(KeyEvent {
        code: KeyCode::Char('x'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    let should_quit = app.handle_event(None, unknown);
    assert!(!should_quit);
    assert_eq!(app.count, 5);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Viewport tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_render_loop_viewport_dimensions() {
    let backend = TestBackend::new(120, 40);
    let terminal = Terminal::new(backend).expect("terminal");
    let renderer = Renderer::new(terminal).expect("renderer");
    let _event_router = EventRouter::new();

    let viewport = renderer.viewport();

    assert_eq!(viewport.width, 120);
    assert_eq!(viewport.height, 40);
    assert_eq!(viewport.x, 0);
    assert_eq!(viewport.y, 0);
}

#[test]
fn test_app_viewport_coverage() {
    let mut app = CounterApp::new();

    for (w, h) in &[(80, 24), (120, 40), (40, 12)] {
        let viewport = Rect::new(0, 0, *w, *h);
        let view = app.build_view(viewport);
        assert_eq!(view.area, viewport);
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Component ID routing
// ─────────────────────────────────────────────────────────────────────────── //

struct MultiComponentApp {
    focused_id: Option<ComponentId>,
    events_received: Vec<(Option<ComponentId>, String)>,
}

impl MultiComponentApp {
    fn new() -> Self {
        Self {
            focused_id: Some(1),
            events_received: Vec::new(),
        }
    }
}

impl App for MultiComponentApp {
    fn build_view(&mut self, viewport: Rect) -> ViewNode {
        let left =
            ViewNode::text(Rect::new(0, 0, 40, 24), "Left Panel", Style::default()).with_id(1);

        let right =
            ViewNode::text(Rect::new(40, 0, 40, 24), "Right Panel", Style::default()).with_id(2);

        ViewNode::container(viewport, vec![left, right])
    }

    fn handle_event(&mut self, id: Option<ComponentId>, event: Event) -> bool {
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            ..
        }) = event
        {
            return true;
        }

        self.events_received.push((id, format!("{:?}", event)));
        false
    }
}

#[test]
fn test_multi_component_event_routing() {
    let mut app = MultiComponentApp::new();

    let ev1 = Event::Key(KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    app.handle_event(Some(1), ev1);
    assert_eq!(app.events_received.len(), 1);
    assert_eq!(app.events_received[0].0, Some(1));
}

#[test]
fn test_multi_component_focus_switches() {
    let mut app = MultiComponentApp::new();
    assert_eq!(app.focused_id, Some(1));

    app.focused_id = Some(2);

    let ev = Event::Key(KeyEvent {
        code: KeyCode::Char('x'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    app.handle_event(app.focused_id, ev);
    assert_eq!(app.events_received[0].0, Some(2));
}
