use std::{
    io,
    sync::atomic::{AtomicU32, Ordering},
    thread,
    time::{Duration, Instant},
};

use interprocess::local_socket::{
    GenericNamespaced,
    ListenerNonblockingMode,
    ListenerOptions,
    Stream,
    ToNsName,
    traits::{Listener as _, Stream as _},
};

use super::{
    channel::{Channel, ChildChannel, HostChannel},
    error::{IpcError, Result},
};

/// How long to wait between two attempts to accept or to connect.
const RETRY_DELAY: Duration = Duration::from_millis(5);

/// The host's listening end of the pipe, before the subprocess has connected.
pub(crate) struct Listener {
    listener: interprocess::local_socket::Listener,
    name: String,
}

impl Listener {
    /// Creates the pipe under a name unique to this host process.
    ///
    /// Must happen before the subprocess is spawned, so the name can be handed
    /// to it.
    pub(crate) fn bind() -> Result<Self> {
        static NEXT_ID: AtomicU32 = AtomicU32::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let name = format!("termoxide-hot-reload-{}-{id}", std::process::id());

        let listener = ListenerOptions::new()
            .name(name.as_str().to_ns_name::<GenericNamespaced>()?)
            .nonblocking(ListenerNonblockingMode::Accept)
            .create_sync()?;
        Ok(Self { listener, name })
    }

    /// The pipe's name, to pass to the subprocess at spawn.
    pub(crate) fn name(&self) -> &str { &self.name }

    /// Waits up to `timeout` for the subprocess to connect, and returns the
    /// host's end of the connection.
    ///
    /// Accepts exactly one client. Fails with `Timeout` if none connects in
    /// time.
    pub(crate) fn accept(self, timeout: Duration) -> Result<HostChannel> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.listener.accept() {
                Ok(stream) => return Ok(Channel::new(stream)),
                Err(e) if e.kind() != io::ErrorKind::WouldBlock => return Err(e.into()),
                Err(_) if Instant::now() >= deadline => return Err(IpcError::Timeout),
                Err(_) => thread::sleep(RETRY_DELAY),
            }
        }
    }
}

/// Connects the subprocess to the host's pipe called `name`, waiting up to
/// `timeout`, and returns the subprocess's end of the connection.
pub(crate) fn connect(name: &str, timeout: Duration) -> Result<ChildChannel> {
    let deadline = Instant::now() + timeout;
    loop {
        match Stream::connect(name.to_ns_name::<GenericNamespaced>()?) {
            Ok(stream) => return Ok(Channel::new(stream)),
            Err(e) if !is_not_ready(&e) => return Err(e.into()),
            Err(_) if Instant::now() >= deadline => return Err(IpcError::Timeout),
            Err(_) => thread::sleep(RETRY_DELAY),
        }
    }
}

/// Whether connecting failed only because the host is not accepting yet.
fn is_not_ready(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused | io::ErrorKind::ResourceBusy
    )
}
