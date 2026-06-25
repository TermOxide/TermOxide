pub mod crossterm;
use std::{sync::mpsc, thread};
use crossterm::read_events;

pub fn setup_events() -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        if let Err(error) = tx.send(()) {
            dbg!(error);
            return;
        }
        let _ = read_events();
    });

    return rx;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::setup_events;

    #[test]
    fn check_receive() {
        let rx = setup_events();
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(true);
    }
}
