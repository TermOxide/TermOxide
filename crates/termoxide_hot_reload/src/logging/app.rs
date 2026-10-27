use std::{
    io,
    pin::Pin,
    sync::{Arc, Mutex, PoisonError},
    task::{Context, Poll},
};

use log::Level;
use tokio::io::AsyncWrite;
use wasmtime_wasi::cli::{IsTerminal, StdoutStream};

use super::logger::APP_TARGET;

/// The app's stdout or stderr: logs each line the app writes at `level`, as the
/// app's, instead of letting it reach a terminal it doesn't own.
///
/// The app may write a line in several pieces, through several streams; they
/// all share one buffer, so a line is logged once whole. What is left without a
/// newline is logged when the output is dropped.
pub(crate) struct AppOutput(Arc<Lines>);

struct Lines {
    level: Level,
    pending: Mutex<Vec<u8>>,
}

impl AppOutput {
    pub(crate) fn new(level: Level) -> Self { Self(Arc::new(Lines { level, pending: Mutex::new(Vec::new()) })) }
}

impl Lines {
    fn write(&self, bytes: &[u8]) {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        for line in take_lines(&mut pending, bytes) {
            log::log!(target: APP_TARGET, self.level, "{line}");
        }
    }
}

impl Drop for Lines {
    fn drop(&mut self) {
        let pending = self.pending.get_mut().unwrap_or_else(PoisonError::into_inner);
        if !pending.is_empty() {
            log::log!(target: APP_TARGET, self.level, "{}", String::from_utf8_lossy(pending));
        }
    }
}

/// Appends `bytes` to `pending` and takes out every line they complete, without
/// its line ending. Bytes that aren't UTF-8 are still logged, replaced.
pub(super) fn take_lines(pending: &mut Vec<u8>, bytes: &[u8]) -> Vec<String> {
    pending.extend_from_slice(bytes);
    let mut lines = Vec::new();
    while let Some(end) = pending.iter().position(|&byte| byte == b'\n') {
        let line: Vec<u8> = pending.drain(..=end).collect();
        lines.push(String::from_utf8_lossy(&line).trim_end_matches(['\r', '\n']).to_owned());
    }
    lines
}

impl IsTerminal for AppOutput {
    fn is_terminal(&self) -> bool { false }
}

impl StdoutStream for AppOutput {
    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> { Box::new(Writer(Arc::clone(&self.0))) }
}

/// One stream onto an [`AppOutput`]. Logging is quick, so every write
/// completes at once.
struct Writer(Arc<Lines>);

impl AsyncWrite for Writer {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, bytes: &[u8]) -> Poll<io::Result<usize>> {
        self.0.write(bytes);
        Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> { Poll::Ready(Ok(())) }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> { Poll::Ready(Ok(())) }
}
