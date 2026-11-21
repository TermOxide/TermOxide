use std::{
    env,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use color_eyre::eyre::eyre;
use log::Level;

use crate::{
    component::GuestInstance,
    ipc::{self, HostChannel, IpcError, Listener, Request, Response},
    logging::{LOG_ENV, relay_output},
};

/// Set on the subprocess to the name of the pipe it must connect to.
pub(crate) const PIPE_ENV: &str = "TERMOXIDE_HOT_RELOAD_PIPE";
/// Set on the subprocess to the path of the app's wasm component.
pub(crate) const GUEST_ENV: &str = "TERMOXIDE_HOT_RELOAD_GUEST";

/// How long the subprocess has to connect once spawned.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the subprocess has to answer a request.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(1);
/// How long the subprocess has to start the app: it compiles the component
/// first.
const START_TIMEOUT: Duration = Duration::from_secs(60);

/// The host's handle on the subprocess, killed when dropped.
pub(crate) struct Subprocess {
    child: Child,
    channel: HostChannel,
}

impl Subprocess {
    /// Spawns this same executable as the subprocess, to run the component at
    /// `guest` and log to the file at `log`, and waits for it to connect.
    ///
    /// Its stdin is null, so it can't read the host's keys; its stdout and
    /// stderr are relayed into the log rather than drawn over the terminal.
    pub(crate) fn spawn(log: &Path, guest: &Path) -> ipc::Result<Self> {
        let listener = Listener::bind()?;
        let mut child = Command::new(env::current_exe()?)
            .env(PIPE_ENV, listener.name())
            .env(GUEST_ENV, guest)
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

    /// Sends `request` and waits for the answer.
    pub(crate) fn request(&mut self, request: &Request) -> ipc::Result<Response> {
        let timeout = if matches!(request, Request::Start { .. }) {
            START_TIMEOUT
        } else {
            RESPONSE_TIMEOUT
        };
        self.channel.send(request)?;
        self.channel.recv(timeout)
    }

    /// Sends a ping and waits for the pong.
    pub(crate) fn ping(&mut self) -> ipc::Result<()> {
        match self.request(&Request::Ping)? {
            Response::Pong => Ok(()),
            _ => Err(IpcError::Unexpected),
        }
    }
}

impl Drop for Subprocess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Runs the subprocess side: connects to the host's `pipe` and hands its
/// requests to the app's component at `guest`, until the host goes away.
///
/// The component is loaded on [`Request::Start`]: compiling it takes a while,
/// and a ping must not wait for that.
pub(crate) fn serve(pipe: &str, guest: &Path) -> color_eyre::Result<()> {
    let mut channel = ipc::connect(pipe, CONNECT_TIMEOUT)?;
    log::info!("connected to the host");
    let mut instance = None;
    loop {
        // `recv_timeout` treats a deadline that overflows as no deadline, so this
        // waits for the next request however long it takes.
        let request = match channel.recv(Duration::MAX) {
            Ok(request) => request,
            Err(IpcError::Closed) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let response = match (&mut instance, request) {
            (None, Request::Ping) => Response::Pong,
            (None, request @ Request::Start { .. }) => {
                log::info!("loading {}", guest.display());
                instance.insert(GuestInstance::load(guest)?).handle(&request)?
            },
            (None, _) => return Err(eyre!("the host sent a request before starting the app")),
            (Some(instance), request) => instance.handle(&request)?,
        };
        channel.send(&response)?;
    }
}
