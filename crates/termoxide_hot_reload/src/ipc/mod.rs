//! The protocol both sides share, and the pipe the host side carries it on.

#[cfg(feature = "host")]
mod channel;
mod error;
#[cfg(feature = "host")]
mod frame;
mod message;
#[cfg(feature = "host")]
mod transport;

#[cfg(feature = "host")]
#[expect(unused_imports, reason = "not named outside `ipc` yet")]
pub(crate) use channel::ChildChannel;
#[cfg(feature = "host")]
pub(crate) use channel::HostChannel;
pub(crate) use error::{IpcError, Result};
pub(crate) use message::{Outcome, Request, Response};
#[cfg(feature = "host")]
pub(crate) use transport::{Listener, connect};

#[cfg(all(test, feature = "host"))]
mod tests;
