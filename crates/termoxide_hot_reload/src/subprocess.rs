use std::{
    env,
    process::{Child, Command, Stdio},
    time::Duration,
};

use crate::ipc::{self, HostChannel, IpcError, Listener, Request, Response};

/// Set on the subprocess to the name of the pipe it must connect to.
pub(crate) const PIPE_ENV: &str = "TERMOXIDE_HOT_RELOAD_PIPE";

/// How long the subprocess has to connect once spawned.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the subprocess has to answer a request.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(1);

/// The host's handle on the subprocess, killed when dropped.
pub(crate) struct Subprocess {
    child: Child,
    channel: HostChannel,
}

impl Subprocess {
    /// Spawns this same executable as the subprocess and waits for it to
    /// connect.
    ///
    /// Its stdin, stdout and stderr are null, so it can neither draw over the
    /// host's terminal nor read its keys.
    pub(crate) fn spawn() -> ipc::Result<Self> {
        let listener = Listener::bind()?;
        let mut child = Command::new(env::current_exe()?)
            .env(PIPE_ENV, listener.name())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        match listener.accept(CONNECT_TIMEOUT) {
            Ok(channel) => Ok(Self { child, channel }),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(e)
            },
        }
    }

    /// Sends a ping and waits for the pong.
    pub(crate) fn ping(&mut self) -> ipc::Result<()> {
        self.channel.send(&Request::Ping)?;
        match self.channel.recv(RESPONSE_TIMEOUT)? {
            Response::Pong => Ok(()),
        }
    }
}

impl Drop for Subprocess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Runs the subprocess side: connects to the host's `pipe` and answers its
/// requests until the host goes away.
pub(crate) fn serve(pipe: &str) -> ipc::Result<()> {
    let mut channel = ipc::connect(pipe, CONNECT_TIMEOUT)?;
    loop {
        // `recv_timeout` treats a deadline that overflows as no deadline, so this
        // waits for the next request however long it takes.
        match channel.recv(Duration::MAX) {
            Ok(Request::Ping) => channel.send(&Response::Pong)?,
            Err(IpcError::Closed) => return Ok(()),
            Err(e) => return Err(e),
        }
    }
}
