use std::{
    env,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
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
    /// The app's executable is an optimized build, so the guest is one too:
    /// what `run_app!` sees of the profile, `debug_assertions` being off.
    pub release: bool,
}

impl Package {
    pub(crate) fn manifest(self) -> PathBuf { Path::new(self.manifest_dir).join("Cargo.toml") }
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
pub(crate) fn rebuild(package: Package, timeout: Duration) -> Result<PathBuf> {
    let mut child = cargo(package).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
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

    let status = wait_with_timeout(&mut child, timeout)?;
    let stdout = stdout.and_then(|reader| reader.join().ok()).unwrap_or_default();
    component(package, status, &stdout)
}

/// Waits for `child` to exit, and kills it if it is still running after
/// `timeout`.
pub(crate) fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
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
    if package.release {
        command.arg("--release");
    }
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
