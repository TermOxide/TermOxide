use serde::{Deserialize, Serialize};

/// A message the host sends to the subprocess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Request {
    /// Asks the subprocess to prove the channel works end to end.
    Ping,
}

/// A message the subprocess sends back to the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Response {
    /// The answer to [`Request::Ping`].
    Pong,
}
