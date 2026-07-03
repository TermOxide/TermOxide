use std::{io, sync::mpsc, time::Duration};

use crossterm::{
    event::{poll, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};

pub fn print_events(shutdown: &mpsc::Receiver<()>) -> io::Result<()> {
    loop {
        match shutdown.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {},
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

    let result = print_events(&shutdown);

    disable_raw_mode()?;

    result
}
