use std::{io, sync::mpsc, time::Duration};

use crossterm::{
    event::{poll, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use crate::event::{Event, KeyCode};

#[derive(Debug)]
pub enum SendEventError {
    ChannelError(mpsc::SendError<Event>),
    TerminalError(io::Error),
}

impl From<io::Error> for SendEventError {
    fn from(error: io::Error) -> Self { SendEventError::TerminalError(error) }
}

fn translate(event: crossterm::event::Event) -> Option<Event> {
    match event {
        crossterm::event::Event::Key(key) => {
            if key.kind == crossterm::event::KeyEventKind::Press {
                if let Some(key_code) = to_keycode(key.code) {
                    Some(Event::KeyPress(key_code))
                } else {
                    None
                }
            } else {
                None
            }
        },
        _ => None,
    }
}

fn to_keycode(code: crossterm::event::KeyCode) -> Option<KeyCode> {
    use crossterm::event::KeyCode as Ct;
    match code {
        Ct::Backspace => Some(KeyCode::Backspace),
        Ct::Enter => Some(KeyCode::Enter),
        Ct::Left => Some(KeyCode::Left),
        Ct::Right => Some(KeyCode::Right),
        Ct::Up => Some(KeyCode::Up),
        Ct::Down => Some(KeyCode::Down),
        Ct::Home => Some(KeyCode::Home),
        Ct::End => Some(KeyCode::End),
        Ct::PageUp => Some(KeyCode::PageUp),
        Ct::PageDown => Some(KeyCode::PageDown),
        Ct::Tab => Some(KeyCode::Tab),
        Ct::BackTab => Some(KeyCode::BackTab),
        Ct::Delete => Some(KeyCode::Delete),
        Ct::Insert => Some(KeyCode::Insert),
        Ct::F(n) => Some(KeyCode::F(n)),
        Ct::Char(c) => Some(KeyCode::Char(c)),
        Ct::Null => Some(KeyCode::Null),
        Ct::Esc => Some(KeyCode::Esc),
        _ => None,
    }
}

pub fn send_events(
    events_tx: &mpsc::Sender<Event>,
    shutdown: &mpsc::Receiver<()>,
) -> Result<(), SendEventError> {
    loop {
        match shutdown.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {},
        }

        if poll(Duration::from_millis(100))
            .map_err(SendEventError::TerminalError)?
        {
            let event = read().map_err(SendEventError::TerminalError)?;
            if let Some(translated_event) = translate(event) {
                events_tx
                    .send(translated_event)
                    .map_err(SendEventError::ChannelError)?;
            }
        }
    }

    Ok(())
}

pub fn read_events(
    events_tx: mpsc::Sender<Event>,
    shutdown_rx: mpsc::Receiver<()>,
) -> Result<(), SendEventError> {
    enable_raw_mode().map_err(SendEventError::TerminalError)?;

    let result = send_events(&events_tx, &shutdown_rx);

    if let Err(disable_error) = disable_raw_mode()
        && result.is_ok()
    {
        return Err(disable_error.into());
    }

    result
}
