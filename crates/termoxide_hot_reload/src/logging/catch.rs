use std::{
    io::{BufRead, BufReader, Read},
    panic,
    thread::{self, JoinHandle},
};

use log::Level;

use super::logger::CHILD_TARGET;

/// Logs every line of the child's `output` at `level`, as the child's, until
/// it ends: its stdout at `info`, its stderr at `debug`.
///
/// Runs on a thread of its own and returns its handle.
pub(crate) fn relay_output(output: impl Read + Send + 'static, level: Level) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(output);
        let mut line = Vec::new();
        // Read as bytes, so output that isn't UTF-8 is still relayed.
        while let Ok(1..) = reader.read_until(b'\n', &mut line) {
            let text = String::from_utf8_lossy(&line);
            log::log!(target: CHILD_TARGET, level, "{}", text.trim_end_matches(['\r', '\n']));
            line.clear();
        }
    })
}

/// Installs a panic hook that logs the panic and its location at `error`.
///
/// With `forward`, the hook in place before runs afterwards, so an app's own
/// hook (`color_eyre`'s, say) still prints. The subprocess doesn't forward:
/// its stderr is relayed into the log already, which would log it twice.
pub(crate) fn log_panics(forward: bool) {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        log::error!("{info}");
        if forward {
            previous(info);
        }
    }));
}
