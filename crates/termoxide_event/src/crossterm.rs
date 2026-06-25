use std::{io, time::Duration};
use crossterm::{
    event::{poll, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};

fn print_events() -> io::Result<()> {
    loop {
        // Wait up to 1s for another event
        if poll(Duration::from_millis(1_000))? {
            // It's guaranteed that read() won't block if `poll` returns `Ok(true)`
            let event = read()?;

            println!("Event::{event:?}\r");

        } else {
            // Timeout expired, no event for 1s
            println!(".\r");
        }
    }
}

pub fn read_events() -> io::Result<()> {
    enable_raw_mode()?;

    if let Err(e) = print_events() {
        println!("Error: {e:?}\r");
    }

    disable_raw_mode()
}
