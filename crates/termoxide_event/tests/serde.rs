#![allow(clippy::unwrap_used)]

use termoxide_event::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

fn round_trip(event: &Event) -> Event { postcard::from_bytes(&postcard::to_allocvec(event).unwrap()).unwrap() }

#[test]
fn events_round_trip() {
    let events = [
        Event::ChannelReady,
        Event::KeyPress(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL | KeyModifiers::SHIFT)),
        Event::KeyPress(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
        Event::Mouse(MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            3,
            7,
            KeyModifiers::ALT,
        )),
        Event::Mouse(MouseEvent::new(MouseEventKind::ScrollUp, 0, 0, KeyModifiers::NONE)),
    ];

    for event in events {
        assert_eq!(round_trip(&event), event);
    }
}
