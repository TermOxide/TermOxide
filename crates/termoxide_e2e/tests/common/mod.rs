//! Runs the `probe` binary in a pseudo-terminal and reads its screen through a
//! terminal emulator. Checks poll until they hold, failing after [`DEADLINE`].

use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use nix::sys::termios::LocalFlags;
use portable_pty::{Child, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};

const DEADLINE: Duration = Duration::from_secs(2);
/// Wide enough that no line of a panic report wraps.
pub const ROWS: u16 = 24;
pub const COLS: u16 = 200;

/// First line of every probe frame.
const TITLE: &str = "termoxide e2e probe";

fn pty_size(rows: u16, cols: u16) -> PtySize { PtySize { rows, cols, pixel_width: 0, pixel_height: 0 } }

pub struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    parser: Arc<Mutex<vt100::Parser>>,
}

impl Session {
    pub fn start() -> Self { Self::start_with(&[]) }

    /// The backtrace variables are removed unless `env` sets them, since some
    /// environments export them globally.
    pub fn start_with(env: &[(&str, &str)]) -> Self {
        let pair = native_pty_system().openpty(pty_size(ROWS, COLS)).expect("open a pty");

        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_probe"));
        command.cwd(env!("CARGO_MANIFEST_DIR"));
        command.env("TERM", "xterm-256color");
        command.env_remove("RUST_BACKTRACE");
        command.env_remove("RUST_LIB_BACKTRACE");
        for (key, value) in env {
            command.env(key, value);
        }

        let child = pair.slave.spawn_command(command).expect("spawn the probe");
        // Otherwise the reader never sees end of file.
        drop(pair.slave);

        // Read continuously, or the probe blocks once the pty buffer is full.
        let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
        let mut reader = pair.master.try_clone_reader().expect("pty reader");
        thread::spawn({
            let parser = Arc::clone(&parser);
            move || {
                let mut buf = [0; 4096];
                while let Ok(read @ 1..) = reader.read(&mut buf) {
                    parser.lock().unwrap().process(&buf[..read]);
                }
            }
        });
        let writer = pair.master.take_writer().expect("pty writer");

        Self { master: pair.master, writer, child, parser }
    }

    pub fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write to the pty");
        self.writer.flush().expect("flush the pty");
    }

    pub fn contents(&self) -> String { self.parser.lock().unwrap().screen().contents() }

    pub fn wait_until(&self, what: &str, mut holds: impl FnMut(&vt100::Screen) -> bool) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            {
                let parser = self.parser.lock().unwrap();
                if holds(parser.screen()) {
                    return;
                }
                if Instant::now() >= deadline {
                    panic!("timed out waiting for {what}; screen:\n{}", parser.screen().contents());
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn wait_for_text(&self, text: &str) {
        self.wait_until(&format!("{text:?}"), |screen| screen.contents().contains(text));
    }

    pub fn wait_for_startup(&self) {
        self.wait_until("the first frame", |screen| {
            screen.alternate_screen() && screen.contents().contains(TITLE)
        });
    }

    pub fn wait_for_restored_screen(&self) {
        self.wait_until("the primary screen with a visible cursor", |screen| {
            !screen.alternate_screen() && !screen.hide_cursor()
        });
    }

    pub fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = self.child.try_wait().expect("poll the probe") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "the probe did not exit; screen:\n{}",
                self.contents()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Raw mode: no line editing, no echo.
    fn raw_mode(&self) -> bool {
        let termios = self.master.get_termios().expect("read the pty's termios");
        !termios.local_flags.intersects(LocalFlags::ICANON | LocalFlags::ECHO)
    }

    pub fn wait_for_raw_mode(&self, raw: bool) {
        let deadline = Instant::now() + DEADLINE;
        while self.raw_mode() != raw {
            assert!(Instant::now() < deadline, "raw mode never became {raw}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Resizes the emulator first, so it is ready for the repaint.
    pub fn resize(&self, rows: u16, cols: u16) {
        self.parser.lock().unwrap().screen_mut().set_size(rows, cols);
        self.master.resize(pty_size(rows, cols)).expect("resize the pty");
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
    }
}
