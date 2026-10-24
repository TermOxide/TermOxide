use std::{fmt, sync::OnceLock};

use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use time::{OffsetDateTime, UtcOffset, macros::format_description};

use super::file::LogFile;

/// Target of the records the host logs on the child's behalf, so they are
/// tagged as the child's rather than the host's.
pub(crate) const CHILD_TARGET: &str = "termoxide::child";

/// The most detailed level written: the child's relayed stderr is `debug`.
const LEVEL: LevelFilter = LevelFilter::Debug;

/// Which side a line comes from, written at the start of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tag {
    Host,
    Child,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "for the wasm guest's lines, once there is a guest")
    )]
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
struct Logger {
    file: LogFile,
    /// The tag of this process's own records.
    tag: Tag,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool { metadata.level() <= LEVEL }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let tag = if record.target() == CHILD_TARGET { Tag::Child } else { self.tag };
        // Nowhere left to report a failure to log.
        let _ = self.file.write_line(&format_line(record, tag));
    }

    /// Every line is written as it comes, so there is nothing to flush.
    fn flush(&self) {}
}

/// Makes the logger this process's `log` backend, tagging its own records
/// with `tag`.
pub(crate) fn install(file: LogFile, tag: Tag) -> Result<(), SetLoggerError> {
    log::set_boxed_logger(Box::new(Logger { file, tag }))?;
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
