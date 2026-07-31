use std::{
    error::Error,
    io::{self, Write},
    thread,
    time::Duration,
};

use termoxide_event::{
    EventStream,
    event::{Event, KeyCode, KeyEvent},
};

fn main() -> Result<(), Box<dyn Error>> {
    let stream = EventStream::new();

    // Poll loop: drain whatever input arrived, react to it, then sleep a
    // frame budget so we don't busy-wait. The `'run` label lets us break out
    // of the outer loop from inside the inner `for`.
    'run: loop {
        for event in stream.poll_events() {
            println!("Received event: {event:?}\r");
            io::stdout().flush()?;
            if matches!(event, Event::KeyPress(KeyEvent { code: KeyCode::Esc, .. })) {
                println!("Exiting...\r");
                io::stdout().flush()?;
                break 'run;
            }
        }
        thread::sleep(Duration::from_millis(16));
    }

    // Stop the reader thread and restore the terminal, inspecting the outcome.
    match stream.teardown() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(Box::new(error)),
        Err(_panic) => Err("reader thread panicked".into()),
    }
}
