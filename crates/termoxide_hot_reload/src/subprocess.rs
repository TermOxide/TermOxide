use std::{
    env,
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{Receiver, TryRecvError},
    time::Duration,
};

use color_eyre::{Result, eyre::eyre};
use log::Level;
use wasmtime::component::Component;

use crate::{
    component::{GuestInstance, Runtime},
    ipc::{self, HostChannel, IpcError, Listener, Outcome, Request, Response},
    logging::{LOG_ENV, relay_output},
    schema,
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
pub(crate) fn serve(pipe: &str, guest: &Path) -> Result<()> {
    let mut channel = ipc::connect(pipe, CONNECT_TIMEOUT)?;
    log::info!("connected to the host");
    let mut generations = Generations {
        runtime: Runtime::new()?,
        current: None,
        component: None,
        previous: None,
        next: None,
        state: Vec::new(),
    };
    loop {
        // `recv_timeout` treats a deadline that overflows as no deadline, so this
        // waits for the next request however long it takes.
        let request = match channel.recv(Duration::MAX) {
            Ok(request) => request,
            Err(IpcError::Closed) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let response = generations.answer(request, guest)?;
        channel.send(&response)?;
    }
}

/// The subprocess's generations of the app: the running one, and the next one
/// while it compiles.
struct Generations {
    runtime: Runtime,
    current: Option<GuestInstance>,
    /// What `current` is an instance of, to restart it after a crash.
    component: Option<Component>,
    /// The generation before the last reload, to go back to if the new one
    /// crashes.
    previous: Option<Component>,
    next: Option<Receiver<Result<Component>>>,
    /// The app's latest state, read from the answers passing through: what a
    /// new generation starts from.
    state: Vec<u8>,
}

impl Generations {
    /// Answers `request`. The first generation is the component at `first`,
    /// loaded on [`Request::Start`]: compiling takes a while, and a ping must not
    /// wait for that. A new one replaces it at the first tick after it compiled.
    fn answer(&mut self, request: Request, first: &Path) -> Result<Response> {
        if let Request::Load { guest } = request {
            log::info!("compiling {}", guest.display());
            self.next = Some(self.runtime.compile_in_background(guest));
            return Ok(Response::Loading);
        }
        if matches!(request, Request::Tick)
            && let Some(component) = self.compiled()
            && let Some(reloaded) = self.swap(&component)
        {
            return Ok(reloaded);
        }

        let response = match (&mut self.current, request) {
            (None, Request::Ping) => Response::Pong,
            (None, request @ Request::Start { .. }) => {
                log::info!("loading {}", first.display());
                let component = self.runtime.compile(first)?;
                let response = self.current.insert(self.generation(&component)?).handle(&request)?;
                self.component = Some(component);
                response
            },
            (None, _) => return Err(eyre!("the host sent a request before starting the app")),
            (Some(current), request) => {
                match current.handle(&request) {
                    Ok(response) => response,
                    Err(e) => return self.recover(&e),
                }
            },
        };
        match &response {
            Response::Started { state, .. } | Response::Done(Outcome { state: Some(state), .. }) => {
                self.state.clone_from(state);
            },
            _ => {},
        }
        Ok(response)
    }

    /// The next generation's component, once compiled. A failed compile is
    /// logged and forgotten: the running generation stays.
    fn compiled(&mut self) -> Option<Component> {
        let compiled = match self.next.as_ref()?.try_recv() {
            Ok(compiled) => compiled,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(eyre!("compiling the new generation stopped")),
        };
        self.next = None;
        compiled.map_err(|e| log::error!("{e:#}; keeping the running generation")).ok()
    }

    /// Starts `component` from the current state and makes it the running
    /// generation, returning the answer to the tick it takes the place of. If
    /// it fails to start, logs why and keeps the running one.
    fn swap(&mut self, component: &Component) -> Option<Response> {
        match self.start(component) {
            Ok((generation, state)) => {
                log::info!("reloaded the app");
                self.current = Some(generation);
                self.previous = self.component.replace(component.clone());
                self.state.clone_from(&state);
                Some(Response::Reloaded { state })
            },
            Err(e) => {
                log::error!("the new generation failed to start, keeping the running one: {e:#}");
                None
            },
        }
    }

    /// Replaces the generation that crashed on a request, the request dropped:
    /// with the one before the last reload if there is one, else with a new
    /// instance of the same, from the latest state either way. The host counts
    /// these, and stops the app when they come too often.
    fn recover(&mut self, error: &color_eyre::Report) -> Result<Response> {
        log::error!("the app crashed, so its request was dropped: {error:#}");
        let component = match self.previous.take() {
            Some(previous) => {
                log::warn!("going back to the generation before the last reload");
                previous
            },
            None => self.component.clone().ok_or_else(|| eyre!("no generation to restart"))?,
        };
        let (generation, state) = self.start(&component)?;
        self.current = Some(generation);
        self.component = Some(component);
        self.state.clone_from(&state);
        Ok(Response::Recovered { state })
    }

    /// A new instance of `component`, once it is known to speak this
    /// subprocess's protocol.
    fn generation(&self, component: &Component) -> Result<GuestInstance> {
        let mut generation = self.runtime.instantiate(component)?;
        let (theirs, ours) = (generation.wire_schema()?, schema::wire().map_err(|e| eyre!(e))?);
        if theirs != ours {
            return Err(eyre!("the guest was built for another version of the protocol"));
        }
        Ok(generation)
    }

    /// A new generation of `component`, started from the current state. The
    /// guest starts it afresh instead when another state type saved it.
    fn start(&self, component: &Component) -> Result<(GuestInstance, Vec<u8>)> {
        let mut generation = self.generation(component)?;
        match generation.handle(&Request::Start { state: Some(self.state.clone()) })? {
            Response::Started { state, reset, .. } => {
                if reset {
                    log::warn!("the app's state type changed, so it starts afresh");
                }
                Ok((generation, state))
            },
            _ => Err(eyre!("the new generation didn't start")),
        }
    }
}
