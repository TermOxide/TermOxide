mod channel;
mod error;
mod frame;
mod message;
mod transport;

#[expect(unused_imports, reason = "not named outside `ipc` yet")]
pub(crate) use channel::ChildChannel;
pub(crate) use channel::HostChannel;
pub(crate) use error::{IpcError, Result};
pub(crate) use message::{Request, Response};
pub(crate) use transport::{Listener, connect};

#[cfg(test)]
mod tests;
