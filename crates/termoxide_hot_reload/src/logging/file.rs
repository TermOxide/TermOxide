use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use super::{
    location::{file_name, log_dir},
    logger::now,
};

/// The shared log file of one host run, written one whole line at a time.
pub(crate) struct LogFile {
    /// The file lock keeps the two processes apart; the mutex keeps this
    /// process's threads apart, since on Unix they all share one lock.
    file: Mutex<File>,
    path: PathBuf,
}

impl LogFile {
    /// Creates this run's log file in the per-user log directory, named after
    /// the current time.
    pub(crate) fn create() -> io::Result<Self> { Self::create_in(&log_dir()?) }

    /// Creates this run's log file in `dir`, so tests don't touch the user's
    /// directories.
    pub(super) fn create_in(dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(file_name(now()));
        // Appending, like the subprocess: a plain write would land at this
        // handle's own position, over the lines the subprocess added since.
        // Readable too: Windows only locks a handle with read or write access,
        // and an append-only handle has neither.
        let file = File::options().create_new(true).read(true).append(true).open(&path)?;
        let path = path.canonicalize().unwrap_or(path);
        Ok(Self { file: Mutex::new(file), path })
    }

    /// Opens the file the host created, to append to it from the subprocess.
    pub(crate) fn open(path: PathBuf) -> io::Result<Self> {
        let path = path.canonicalize().unwrap_or(path);
        let file = File::options().read(true).append(true).open(&path)?;
        Ok(Self { file: Mutex::new(file), path })
    }

    /// Where the file is, to pass to the subprocess at spawn.
    pub(crate) fn path(&self) -> &Path { &self.path }

    /// Appends `line` and a newline while holding an exclusive lock on the
    /// file, so a line from the other process can't land in the middle of it.
    pub(crate) fn write_line(&self, line: &str) -> io::Result<()> {
        let mut file = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        let line = format!("{line}\n");

        file.lock()?;
        let written = file.write_all(line.as_bytes());
        file.unlock()?;
        written
    }
}
