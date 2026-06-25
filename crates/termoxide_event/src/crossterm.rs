use std::{io, sync::mpsc, time::Duration};
use crossterm::{
    event::{poll, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};

fn print_events(shutdown: &mpsc::Receiver<()>) -> io::Result<()> {
    loop {
        if shutdown.try_recv().is_ok() {
            break;
        }

        if poll(Duration::from_millis(100))? {
            let event = read()?;
            println!("Event::{event:?}\r");
        } else {
            println!(".\r");
        }
    }

    Ok(())
}

pub fn read_events(shutdown: mpsc::Receiver<()>) -> io::Result<()> {
    enable_raw_mode()?;

    if let Err(e) = print_events(&shutdown) {
        println!("Error: {e:?}\r");
    }

    disable_raw_mode()
}

#[cfg(test)]
mod tests {
    use super::print_events;
    use std::{sync::mpsc, thread, time::Duration};

    #[test]
    fn print_events_stops_when_shutdown_already_signaled() {
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        shutdown_tx.send(()).unwrap();

        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = print_events(&shutdown_rx);
            let _ = done_tx.send(result);
        });

        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("print_events did not return: the shutdown did not break the loop");
        result.expect("print_events returned an error");
    }
}
