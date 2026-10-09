use std::{
    env,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use log::Level;

use crate::{
    ipc::{self, HostChannel, IpcError, Listener, Request, Response},
    logging::{LOG_ENV, relay_output},
};

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
    /// Spawns this same executable as the subprocess, logging to the file at
    /// `log`, and waits for it to connect.
    ///
    /// Its stdin is null, so it can't read the host's keys; its stdout and
    /// stderr are relayed into the log rather than drawn over the terminal.
    pub(crate) fn spawn(log: &Path) -> ipc::Result<Self> {
        let listener = Listener::bind()?;
        let mut child = Command::new(env::current_exe()?)
            .env(PIPE_ENV, listener.name())
            .env(LOG_ENV, log)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(stdout) = child.stdout.take() {
            relay_output(stdout, Level::Info);
        }
        if let Some(stderr) = child.stderr.take() {
            relay_output(stderr, Level::Debug);
        }

        match listener.accept(CONNECT_TIMEOUT) {
            Ok(channel) => {
                log::info!("subprocess {} connected", child.id());
                Ok(Self { child, channel })
            },
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
    log::info!("connected to the host");
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
