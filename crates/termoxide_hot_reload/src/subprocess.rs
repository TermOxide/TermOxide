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

    /// Sends `request` and waits for the answer.
    pub(crate) fn request(&mut self, request: &Request) -> ipc::Result<Response> {
        self.channel.send(request)?;
        self.channel.recv(RESPONSE_TIMEOUT)
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

/// Runs the subprocess side: connects to the host's `pipe` and runs `app` for
/// it, answering its requests until the host goes away.
///
/// The app runs natively here until it runs as a wasm guest, hence `guest`.
///
/// `app` is the app as built; the host's [`Request::Start`] says whether to
/// run it or resume from the state it sends instead.
#[cfg(feature = "guest")]
pub(crate) async fn serve<A>(pipe: &str, app: A) -> ipc::Result<()>
where
    A: termoxide::App + Clone + serde::Serialize + serde::de::DeserializeOwned + 'static,
{
    use crate::session::Session;

    let mut channel = ipc::connect(pipe, CONNECT_TIMEOUT)?;
    log::info!("connected to the host");
    let mut fresh = Some(app);
    let mut session: Option<Session<A>> = None;
    loop {
        // `recv_timeout` treats a deadline that overflows as no deadline, so this
        // waits for the next request however long it takes.
        let request = match channel.recv(Duration::MAX) {
            Ok(request) => request,
            Err(IpcError::Closed) => return Ok(()),
            Err(e) => return Err(e),
        };
        let response = if let Some(session) = session.as_mut() {
            session.handle(request).await?
        } else {
            match request {
                Request::Ping => Response::Pong,
                Request::Start { state } => {
                    let app = fresh.take().ok_or(IpcError::Unexpected)?;
                    let (started, response) = Session::start(app, state.as_deref())?;
                    session = Some(started);
                    response
                },
                _ => return Err(IpcError::Unexpected),
            }
        };
        channel.send(&response)?;
    }
}
