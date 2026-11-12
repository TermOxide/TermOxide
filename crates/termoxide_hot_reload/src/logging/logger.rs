use std::{fmt, sync::OnceLock};

use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use time::{OffsetDateTime, UtcOffset, macros::format_description};

use super::file::LogFile;

/// Target of the records the host logs on the child's behalf, so they are
/// tagged as the child's rather than the host's.
pub(crate) const CHILD_TARGET: &str = "termoxide::child";
/// Target of the app's records and output, so they are tagged as the app's.
pub(crate) const APP_TARGET: &str = "termoxide::app";

/// The most detailed level written for the framework's own records: the
/// child's relayed stderr is `debug`.
const LEVEL: LevelFilter = LevelFilter::Debug;
/// The most detailed level written for any other crate's records: wasmtime's
/// compiler alone logs a hundred thousand `debug` lines per component.
const DEPENDENCY_LEVEL: LevelFilter = LevelFilter::Warn;

/// Which side a line comes from, written at the start of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tag {
    Host,
    Child,
    App,
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Tag::Host => "[framework:host]",
            Tag::Child => "[framework:child]",
            Tag::App => "[app]",
        })
    }
}

/// The `log` backend: writes each record to the log file as one tagged line.
pub(super) struct Logger {
    pub(super) file: LogFile,
    /// The tag of this process's own records.
    pub(super) tag: Tag,
    /// In a release run, the app's lib, whose records are the app's.
    pub(super) app: Option<&'static str>,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        is_written(metadata) || (self.is_app(metadata.target()) && metadata.level() <= LEVEL)
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let tag = match record.target() {
            CHILD_TARGET => Tag::Child,
            APP_TARGET => Tag::App,
            target if self.is_app(target) => Tag::App,
            _ => self.tag,
        };
        // Nowhere left to report a failure to log.
        let _ = self.file.write_line(&format_line(record, tag));
    }

    /// Every line is written as it comes, so there is nothing to flush.
    fn flush(&self) {}
}

impl Logger {
    /// Whether `target`, a module path, is in the app's lib.
    fn is_app(&self, target: &str) -> bool {
        self.app.is_some_and(|lib| {
            target
                .strip_prefix(lib)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
        })
    }
}

/// Whether a record is written: the framework's own up to `debug`, other
/// crates' only from `warn` up.
pub(super) fn is_written(metadata: &Metadata<'_>) -> bool {
    let max = if metadata.target().starts_with("termoxide") {
        LEVEL
    } else {
        DEPENDENCY_LEVEL
    };
    metadata.level() <= max
}

/// Makes the logger this process's `log` backend, tagging its own records
/// with `tag`.
pub(crate) fn install(file: LogFile, tag: Tag) -> Result<(), SetLoggerError> {
    install_logger(Logger { file, tag, app: None })
}

/// Makes the logger the `log` backend of a release run, whose app is linked
/// in: the records of `lib`, the app's lib, are tagged as the app's.
pub(crate) fn install_native(file: LogFile, lib: &'static str) -> Result<(), SetLoggerError> {
    install_logger(Logger { file, tag: Tag::Host, app: Some(lib) })
}

fn install_logger(logger: Logger) -> Result<(), SetLoggerError> {
    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(LEVEL);
    Ok(())
}

/// One log line: time, level, tag, then the message.
pub(super) fn format_line(record: &Record<'_>, tag: Tag) -> String {
    let time = now()
        .format(format_description!("[hour]:[minute]:[second].[subsecond digits:3]"))
        .unwrap_or_default();
    format!("{time} {:<5} {tag} {}", record.level(), record.args())
}

/// The current local time.
///
/// The offset is read once and kept: on Unix it can only be read while the
/// process has a single thread, so the first call must happen before others
/// start. Falls back to UTC when it can't be read.
pub(super) fn now() -> OffsetDateTime {
    static OFFSET: OnceLock<UtcOffset> = OnceLock::new();
    let offset = *OFFSET.get_or_init(|| UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC));
    OffsetDateTime::now_utc().to_offset(offset)
}
