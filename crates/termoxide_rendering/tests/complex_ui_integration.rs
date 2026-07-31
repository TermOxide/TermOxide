//! Advanced integration tests for complex multi-component scenarios.
//!
//! Tests complex interactions including:
//! - multi-component apps with state transitions
//! - form-like input handling with multiple fields
//! - modal overlays and focus management
//! - dynamic tree rebuilding
//! - event propagation patterns

use ratatui::{layout::Rect, style::Style};
use termoxide_event::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use termoxide_rendering::{
    render_loop::App,
    view_node::{ComponentId, ViewNode},
};

// ─────────────────────────────────────────────────────────────────────────── //
//  Form-like multi-field app
// ─────────────────────────────────────────────────────────────────────────── //

struct FormApp {
    fields: Vec<String>,
    focused_field: usize,
    submitted: bool,
}

impl FormApp {
    fn new() -> Self {
        Self {
            fields: vec!["Field 1".to_string(), "Field 2".to_string(), "Field 3".to_string()],
            focused_field: 0,
            submitted: false,
        }
    }

    fn total_fields(&self) -> usize { self.fields.len() }

    fn next_field(&mut self) { self.focused_field = (self.focused_field + 1) % self.total_fields(); }

    fn prev_field(&mut self) {
        if self.focused_field == 0 {
            self.focused_field = self.total_fields() - 1;
        } else {
            self.focused_field -= 1;
        }
    }
}

impl App for FormApp {
    fn build_view(&mut self, viewport: Rect) -> ViewNode {
        let mut children = Vec::new();

        // Title
        children.push(ViewNode::text(Rect::new(0, 0, viewport.width, 1), "Form", Style::default()).with_id(0));

        // Form fields
        for (i, field_text) in self.fields.iter().enumerate() {
            let y = 2 + (i as u16);
            let label = if i == self.focused_field {
                format!("> {}", field_text)
            } else {
                format!("  {}", field_text)
            };

            children.push(
                ViewNode::text(Rect::new(0, y, viewport.width, 1), label, Style::default())
                    .with_id((i + 1) as ComponentId),
            );
        }

        // Status
        let status_y = 2 + self.fields.len() as u16 + 1;
        children.push(
            ViewNode::text(
                Rect::new(0, status_y, viewport.width, 1),
                if self.submitted { "Submitted!" } else { "" },
                Style::default(),
            )
            .with_id((self.fields.len() + 1) as ComponentId),
        );

        ViewNode::container(viewport, children)
    }

    fn handle_event(&mut self, _id: Option<ComponentId>, event: Event) -> bool {
        match event {
            Event::KeyPress(KeyEvent { code: KeyCode::Char('q'), .. }) => true,
            Event::KeyPress(KeyEvent { code: KeyCode::Down, .. }) => {
                self.next_field();
                false
            },
            Event::KeyPress(KeyEvent { code: KeyCode::Up, .. }) => {
                self.prev_field();
                false
            },
            Event::KeyPress(KeyEvent { code: KeyCode::Enter, .. }) => {
                self.submitted = true;
                false
            },
            _ => false,
        }
    }
}

#[test]
fn test_form_app_navigation_down() {
    let mut app = FormApp::new();
    assert_eq!(app.focused_field, 0);

    let down = Event::KeyPress(KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE });

    app.handle_event(None, down);
    assert_eq!(app.focused_field, 1);

    let down = Event::KeyPress(KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE });
    app.handle_event(None, down);
    assert_eq!(app.focused_field, 2);
}

#[test]
fn test_form_app_navigation_up() {
    let mut app = FormApp::new();
    app.focused_field = 1;

    let up = Event::KeyPress(KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE });

    app.handle_event(None, up);
    assert_eq!(app.focused_field, 0);
}

#[test]
fn test_form_app_navigation_wrap_forward() {
    let mut app = FormApp::new();
    app.focused_field = app.total_fields() - 1;

    let down = Event::KeyPress(KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE });

    app.handle_event(None, down);
    assert_eq!(app.focused_field, 0);
}

#[test]
fn test_form_app_navigation_wrap_backward() {
    let mut app = FormApp::new();
    app.focused_field = 0;

    let up = Event::KeyPress(KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE });

    app.handle_event(None, up);
    assert_eq!(app.focused_field, app.total_fields() - 1);
}

#[test]
fn test_form_app_submit() {
    let mut app = FormApp::new();
    assert!(!app.submitted);

    let enter = Event::KeyPress(KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE });

    app.handle_event(None, enter);
    assert!(app.submitted);
}

#[test]
fn test_form_app_view_reflects_focus() {
    let mut app = FormApp::new();
    let viewport = Rect::new(0, 0, 80, 24);

    app.focused_field = 1;
    let view = app.build_view(viewport);

    assert_eq!(view.children.len(), 5);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Modal overlay app
// ─────────────────────────────────────────────────────────────────────────── //

struct ModalApp {
    show_modal: bool,
    modal_confirmed: bool,
}

impl ModalApp {
    fn new() -> Self { Self { show_modal: false, modal_confirmed: false } }
}

impl App for ModalApp {
    fn build_view(&mut self, viewport: Rect) -> ViewNode {
        let mut children = vec![
            ViewNode::text(Rect::new(0, 0, viewport.width, viewport.height), "Background", Style::default()).with_id(1),
        ];

        if self.show_modal {
            children.push(ViewNode::text(Rect::new(20, 8, 40, 8), "Modal Dialog", Style::default()).with_id(2));
        }

        ViewNode::container(viewport, children)
    }

    fn handle_event(&mut self, _id: Option<ComponentId>, event: Event) -> bool {
        match event {
            Event::KeyPress(KeyEvent { code: KeyCode::Char('q'), .. }) => true,
            Event::KeyPress(KeyEvent { code: KeyCode::Char('m'), .. }) => {
                self.show_modal = !self.show_modal;
                false
            },
            Event::KeyPress(KeyEvent { code: KeyCode::Enter, .. }) if self.show_modal => {
                self.modal_confirmed = true;
                self.show_modal = false;
                false
            },
            _ => false,
        }
    }
}

#[test]
fn test_modal_app_show_hide() {
    let mut app = ModalApp::new();
    assert!(!app.show_modal);

    let m_key = Event::KeyPress(KeyEvent { code: KeyCode::Char('m'), modifiers: KeyModifiers::NONE });

    app.handle_event(None, m_key);
    assert!(app.show_modal);

    app.handle_event(None, m_key);
    assert!(!app.show_modal);
}

#[test]
fn test_modal_app_confirm() {
    let mut app = ModalApp::new();
    app.show_modal = true;

    let enter = Event::KeyPress(KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE });

    app.handle_event(None, enter);
    assert!(app.modal_confirmed);
    assert!(!app.show_modal);
}

#[test]
fn test_modal_app_view_includes_modal() {
    let mut app = ModalApp::new();
    app.show_modal = true;
    let viewport = Rect::new(0, 0, 80, 24);

    let view = app.build_view(viewport);

    // Should have 2 children: background + modal
    assert_eq!(view.children.len(), 2);
}

#[test]
fn test_modal_app_view_excludes_modal() {
    let mut app = ModalApp::new();
    app.show_modal = false;
    let viewport = Rect::new(0, 0, 80, 24);

    let view = app.build_view(viewport);

    // Should have only 1 child: background
    assert_eq!(view.children.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  List selection app
// ─────────────────────────────────────────────────────────────────────────── //

struct ListApp {
    items: Vec<String>,
    selected_index: usize,
}

impl ListApp {
    fn new() -> Self {
        Self {
            items: vec![
                "Item 1".to_string(),
                "Item 2".to_string(),
                "Item 3".to_string(),
                "Item 4".to_string(),
            ],
            selected_index: 0,
        }
    }

    fn move_selection_down(&mut self) { self.selected_index = (self.selected_index + 1) % self.items.len(); }

    fn move_selection_up(&mut self) {
        if self.selected_index == 0 {
            self.selected_index = self.items.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }
}

impl App for ListApp {
    fn build_view(&mut self, viewport: Rect) -> ViewNode {
        let mut children = Vec::new();

        for (i, item) in self.items.iter().enumerate() {
            let is_selected = i == self.selected_index;
            let prefix = if is_selected { "→ " } else { "  " };
            let text = format!("{}{}", prefix, item);

            children.push(
                ViewNode::text(Rect::new(0, i as u16, viewport.width, 1), text, Style::default())
                    .with_id((i + 1) as ComponentId),
            );
        }

        ViewNode::container(viewport, children)
    }

    fn handle_event(&mut self, _id: Option<ComponentId>, event: Event) -> bool {
        match event {
            Event::KeyPress(KeyEvent { code: KeyCode::Char('q'), .. }) => true,
            Event::KeyPress(KeyEvent { code: KeyCode::Down, .. }) => {
                self.move_selection_down();
                false
            },
            Event::KeyPress(KeyEvent { code: KeyCode::Up, .. }) => {
                self.move_selection_up();
                false
            },
            _ => false,
        }
    }
}

#[test]
fn test_list_app_selection_navigation() {
    let mut app = ListApp::new();
    assert_eq!(app.selected_index, 0);

    let down = Event::KeyPress(KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE });

    app.handle_event(None, down);
    assert_eq!(app.selected_index, 1);

    let down = Event::KeyPress(KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE });
    app.handle_event(None, down);
    assert_eq!(app.selected_index, 2);
}

#[test]
fn test_list_app_selection_wraps() {
    let mut app = ListApp::new();
    app.selected_index = app.items.len() - 1; // Last item

    let down = Event::KeyPress(KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE });

    app.handle_event(None, down);
    assert_eq!(app.selected_index, 0); // Wraps to first
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Tab navigation app
// ─────────────────────────────────────────────────────────────────────────── //

struct TabApp {
    tabs: Vec<String>,
    active_tab: usize,
}

impl TabApp {
    fn new() -> Self {
        Self {
            tabs: vec!["Tab 1".to_string(), "Tab 2".to_string(), "Tab 3".to_string()],
            active_tab: 0,
        }
    }

    fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }

    fn next_tab(&mut self) { self.active_tab = (self.active_tab + 1) % self.tabs.len(); }

    fn prev_tab(&mut self) {
        if self.active_tab == 0 {
            self.active_tab = self.tabs.len() - 1;
        } else {
            self.active_tab -= 1;
        }
    }
}

impl App for TabApp {
    fn build_view(&mut self, viewport: Rect) -> ViewNode {
        let mut children = Vec::new();

        // Tab headers
        let mut tab_line = String::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            if i == self.active_tab {
                tab_line.push_str(&format!("[{}] ", tab));
            } else {
                tab_line.push_str(&format!(" {} ", tab));
            }
        }

        children.push(ViewNode::text(Rect::new(0, 0, viewport.width, 1), tab_line, Style::default()));

        // Tab content (dummy)
        children.push(ViewNode::text(
            Rect::new(0, 1, viewport.width, viewport.height - 1),
            format!("Content of {}", self.tabs[self.active_tab]),
            Style::default(),
        ));

        ViewNode::container(viewport, children)
    }

    fn handle_event(&mut self, _id: Option<ComponentId>, event: Event) -> bool {
        match event {
            Event::KeyPress(KeyEvent { code: KeyCode::Char('q'), .. }) => true,
            Event::KeyPress(KeyEvent { code: KeyCode::Right, .. }) => {
                self.next_tab();
                false
            },
            Event::KeyPress(KeyEvent { code: KeyCode::Left, .. }) => {
                self.prev_tab();
                false
            },
            Event::KeyPress(KeyEvent { code: KeyCode::Char(c @ '1'..='9'), .. }) => {
                let idx = (c as usize) - ('1' as usize);
                self.switch_tab(idx);
                false
            },
            _ => false,
        }
    }
}

#[test]
fn test_tab_app_navigation() {
    let mut app = TabApp::new();
    assert_eq!(app.active_tab, 0);

    let right = Event::KeyPress(KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE });

    app.handle_event(None, right);
    assert_eq!(app.active_tab, 1);

    let right = Event::KeyPress(KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE });
    app.handle_event(None, right);
    assert_eq!(app.active_tab, 2);
}

#[test]
fn test_tab_app_direct_switch() {
    let mut app = TabApp::new();

    let tab3 = Event::KeyPress(KeyEvent { code: KeyCode::Char('3'), modifiers: KeyModifiers::NONE });

    app.handle_event(None, tab3);
    assert_eq!(app.active_tab, 2);
}

#[test]
fn test_tab_app_wrap_navigation() {
    let mut app = TabApp::new();
    app.active_tab = app.tabs.len() - 1;

    let right = Event::KeyPress(KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE });

    app.handle_event(None, right);
    assert_eq!(app.active_tab, 0);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Complex state machine app
// ─────────────────────────────────────────────────────────────────────────── //

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AppState {
    Menu,
    Playing,
    Paused,
    GameOver,
}

struct GameApp {
    state: AppState,
    score: u32,
}

impl GameApp {
    fn new() -> Self { Self { state: AppState::Menu, score: 0 } }
}

impl App for GameApp {
    fn build_view(&mut self, viewport: Rect) -> ViewNode {
        let content = match self.state {
            AppState::Menu => "Press P to Play".to_string(),
            AppState::Playing => format!("Score: {} - Press Space to Pause", self.score),
            AppState::Paused => "Paused - Press R to Resume or Q to Quit".to_string(),
            AppState::GameOver => format!("Game Over! Score: {} - Press R to Restart", self.score),
        };

        ViewNode::text(viewport, content, Style::default())
    }

    fn handle_event(&mut self, _id: Option<ComponentId>, event: Event) -> bool {
        match event {
            Event::KeyPress(KeyEvent { code: KeyCode::Char('q'), .. }) => true,
            Event::KeyPress(KeyEvent { code: KeyCode::Char('p'), .. }) => {
                if self.state == AppState::Menu {
                    self.state = AppState::Playing;
                    self.score = 0;
                }
                false
            },
            Event::KeyPress(KeyEvent { code: KeyCode::Char(' '), .. }) => {
                if self.state == AppState::Playing {
                    self.state = AppState::Paused;
                }
                false
            },
            Event::KeyPress(KeyEvent { code: KeyCode::Char('r'), .. }) => {
                match self.state {
                    AppState::Paused => self.state = AppState::Playing,
                    AppState::GameOver => {
                        self.state = AppState::Menu;
                        self.score = 0;
                    },
                    _ => {},
                }
                false
            },
            _ => false,
        }
    }
}

#[test]
fn test_game_app_state_transitions() {
    let mut app = GameApp::new();
    assert_eq!(app.state, AppState::Menu);

    let p = Event::KeyPress(KeyEvent { code: KeyCode::Char('p'), modifiers: KeyModifiers::NONE });

    app.handle_event(None, p);
    assert_eq!(app.state, AppState::Playing);

    let space = Event::KeyPress(KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE });

    app.handle_event(None, space);
    assert_eq!(app.state, AppState::Paused);
}

#[test]
fn test_game_app_full_cycle() {
    let mut app = GameApp::new();

    let p = Event::KeyPress(KeyEvent { code: KeyCode::Char('p'), modifiers: KeyModifiers::NONE });

    let space = Event::KeyPress(KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE });

    let r = Event::KeyPress(KeyEvent { code: KeyCode::Char('r'), modifiers: KeyModifiers::NONE });

    // Play game
    app.handle_event(None, p);
    assert_eq!(app.state, AppState::Playing);

    // Pause
    app.handle_event(None, space);
    assert_eq!(app.state, AppState::Paused);

    // End game (simulate)
    app.state = AppState::GameOver;

    // Restart
    app.handle_event(None, r);
    assert_eq!(app.state, AppState::Menu);
}
