use std::{
    env,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use color_eyre::{Result, eyre::eyre};
use serde::Deserialize;

/// The app's package, as `run_app!` captures it in the app's own crate.
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct Package {
    pub manifest_dir: &'static str,
    /// The app's name, where it isn't the executable's: `termoxide run` hosts
    /// any app.
    pub app: Option<&'static str>,
}

impl Package {
    pub(crate) fn manifest(self) -> PathBuf { Path::new(self.manifest_dir).join("Cargo.toml") }

    /// What names the directory of the app's logs and state.
    pub(crate) fn app_name(self) -> std::io::Result<String> {
        self.app.map_or_else(crate::logging::exe_name, |app| Ok(app.to_owned()))
    }
}

/// The target the guest is built for.
pub(crate) const GUEST_TARGET: &str = "wasm32-wasip2";

/// How often a rebuild is checked for having finished or run out of time.
const POLL: Duration = Duration::from_millis(100);

/// Builds the app as a wasm component and returns the path to it.
///
/// Cargo's progress and errors go to the terminal: this runs before the host
/// takes the screen over.
pub(crate) fn guest(package: Package) -> Result<PathBuf> {
    let output = cargo(package).stderr(Stdio::inherit()).output()?;
    component(package, output.status, &output.stdout)
}

/// Builds the app again while it runs: cargo's progress and errors go to the
/// log, and a build still running after `timeout` is killed.
pub(crate) fn rebuild(package: Package, timeout: Duration, running: &Running) -> Result<PathBuf> {
    let mut command = cargo(package);
    // A group of its own, so stopping the build reaches the compilers cargo
    // started as well.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    running.0.store(child.id(), Ordering::Relaxed);
    let stdout = child.stdout.take().map(|mut out| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = out.read_to_end(&mut bytes);
            bytes
        })
    });
    if let Some(stderr) = child.stderr.take() {
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                log::info!("cargo: {line}");
            }
        });
    }

    let status = wait_with_timeout(&mut child, timeout);
    running.0.store(0, Ordering::Relaxed);
    let stdout = stdout.and_then(|reader| reader.join().ok()).unwrap_or_default();
    component(package, status?, &stdout)
}

/// The rebuild in flight, if any: its cargo's process id, or 0.
#[derive(Clone, Default)]
pub(crate) struct Running(Arc<AtomicU32>);

impl Running {
    /// Kills the rebuild in flight, which then fails like any build.
    pub(crate) fn stop(&self) {
        let pid = self.0.swap(0, Ordering::Relaxed);
        if pid != 0 {
            kill_group(pid);
        }
    }
}

/// Kills the process group `pid` leads, if it leads one.
#[cfg(unix)]
fn kill_group(pid: u32) {
    use rustix::process::{Pid, Signal, kill_process_group};

    if let Some(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw) {
        let _ = kill_process_group(pid, Signal::KILL);
    }
}

/// Windows has no process groups to kill: cargo's own death has to do.
#[cfg(not(unix))]
fn kill_group(_pid: u32) {}

/// Waits for `child` to exit, and kills it if it is still running after
/// `timeout`.
pub(crate) fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            kill_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
            return Err(eyre!("building the guest took longer than {timeout:?}, so it was stopped"));
        }
        thread::sleep(POLL);
    }
}

/// `cargo build` for the guest, with JSON on stdout to find the component in.
fn cargo(package: Package) -> Command {
    let mut command = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command
        .args([
            "build",
            "--lib",
            "--target",
            GUEST_TARGET,
            "--no-default-features",
            "--features",
            "guest",
        ])
        .args(["--message-format", "json-render-diagnostics", "--manifest-path"])
        .arg(package.manifest())
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    command
}

/// The component a build that ended with `status` produced, found in its JSON
/// `stdout`.
fn component(package: Package, status: ExitStatus, stdout: &[u8]) -> Result<PathBuf> {
    if !status.success() {
        return Err(eyre!("building the guest for {GUEST_TARGET} failed ({status})"));
    }
    String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Message>(line).ok())
        .filter(|message| message.reason == "compiler-artifact" && message.target.kind.iter().any(|k| k == "cdylib"))
        .flat_map(|message| message.filenames)
        .find(|file| file.extension().is_some_and(|ext| ext == "wasm"))
        .ok_or_else(|| {
            eyre!(
                "{} built no wasm component: is its lib a `cdylib`?",
                package.manifest().display()
            )
        })
}

/// The parts of a line of cargo's JSON output this needs.
#[derive(Deserialize)]
struct Message {
    reason: String,
    #[serde(default)]
    target: Target,
    #[serde(default)]
    filenames: Vec<PathBuf>,
}

#[derive(Default, Deserialize)]
struct Target {
    kind: Vec<String>,
}

#[cfg(test)]
mod tests;
