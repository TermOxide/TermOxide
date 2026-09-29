//! Why the main loop stopped abnormally.
//!
//! Everything is carried and printed by [`LoopError`] itself: `color_eyre`
//! sections only attach anything once the app installed `color_eyre`.

use std::{error::Error, fmt, path::Path};

use termoxide_rendering::renderer::RenderError;

/// The main loop stopped because it failed, its teardown failed, or both.
///
/// Returned inside the [`color_eyre::Report`] of
/// [`run_with_app`](crate::run_with_app); get it back with
/// `report.downcast_ref::<LoopError>()`.
#[derive(Debug)]
#[non_exhaustive]
pub struct LoopError {
    failure: Option<LoopFailure>,
    teardown: Option<termoxide_event::Error>,
}

impl LoopError {
    /// `None` when both succeeded.
    pub(crate) fn from_parts(failure: Option<LoopFailure>, teardown: Option<termoxide_event::Error>) -> Option<Self> {
        (failure.is_some() || teardown.is_some()).then_some(Self { failure, teardown })
    }

    /// What ended the loop; `None` when only the teardown failed.
    pub fn failure(&self) -> Option<&LoopFailure> { self.failure.as_ref() }

    /// The error the input reader stopped on.
    pub fn teardown(&self) -> Option<&termoxide_event::Error> { self.teardown.as_ref() }
}

impl fmt::Display for LoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(failure) = &self.failure {
            write!(f, "{failure}")?;
        }
        if let Some(teardown) = &self.teardown {
            let lead = if self.failure.is_some() { "\nalso, the" } else { "the" };
            write!(f, "{lead} input reader failed during teardown: {teardown}")?;
        }
        Ok(())
    }
}

impl Error for LoopError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.failure {
            Some(LoopFailure::Render(error)) => Some(error),
            Some(LoopFailure::Panic(_)) | None => self.teardown.as_ref().map(|error| error as &(dyn Error + 'static)),
        }
    }
}

/// What ended the main loop.
#[derive(Debug)]
#[non_exhaustive]
pub enum LoopFailure {
    /// A frame failed to render.
    Render(RenderError),
    /// One of the application's [`App`](crate::App) methods panicked.
    Panic(AppPanic),
}

impl fmt::Display for LoopFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render(error) => write!(f, "render failed: {error}"),
            Self::Panic(panic) => write!(f, "{panic}"),
        }
    }
}

impl From<AppPanic> for LoopFailure {
    fn from(panic: AppPanic) -> Self { Self::Panic(panic) }
}

/// A panic caught in one of the application's [`App`](crate::App) methods.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AppPanic {
    method: AppMethod,
    message: String,
    location: Option<PanicLocation>,
    trace: Trace,
}

impl AppPanic {
    pub(crate) fn new(method: AppMethod, message: String, location: Option<PanicLocation>, trace: Trace) -> Self {
        Self { method, message, location, trace }
    }

    pub fn method(&self) -> AppMethod { self.method }

    /// `Box<dyn Any>` when the payload was not a string.
    pub fn message(&self) -> &str { &self.message }

    /// `None` when the app replaced the panic hook after the loop installed it.
    pub fn location(&self) -> Option<&PanicLocation> { self.location.as_ref() }

    pub fn trace(&self) -> &Trace { &self.trace }
}

impl fmt::Display for AppPanic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "app panicked in `{}`: {}", self.method, self.message)?;
        match &self.location {
            Some(location) => write!(f, "\n  at {location}")?,
            None => write!(f, "\n  at <unknown location: panic hook replaced>")?,
        }
        match &self.trace {
            Trace::Frames(frames) => {
                write!(f, "\nstack (most recent call first):")?;
                let width = frames.iter().map(|frame| frame.symbol.chars().count()).max().unwrap_or(0);
                for frame in frames {
                    match frame.source() {
                        Some(source) => write!(f, "\n  {:<width$}  {source}", frame.symbol)?,
                        None => write!(f, "\n  {}", frame.symbol)?,
                    }
                }
                write!(f, "\n  termoxide: App::{}", self.method)
            },
            Trace::Disabled => write!(f, "\n  no backtrace available (set RUST_BACKTRACE=1 to enable)"),
            Trace::Unavailable => write!(f, "\n  no backtrace available (no debug symbols)"),
        }
    }
}

/// The [`App`](crate::App) method a panic was caught in; displays as its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppMethod {
    TrackView,
    OnTick,
    HandleEvent,
    BuildView,
}

impl fmt::Display for AppMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TrackView => "track_view",
            Self::OnTick => "on_tick",
            Self::HandleEvent => "handle_event",
            Self::BuildView => "build_view",
        })
    }
}

/// Where a panic was raised.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PanicLocation {
    file: String,
    line: u32,
    column: u32,
}

impl PanicLocation {
    pub(crate) fn new(file: impl Into<String>, line: u32, column: u32) -> Self {
        Self { file: file.into(), line, column }
    }

    pub fn file(&self) -> &str { &self.file }

    pub fn line(&self) -> u32 { self.line }

    pub fn column(&self) -> u32 { self.column }
}

impl fmt::Display for PanicLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// The app's calls that led to a panic.
///
/// Captured when `RUST_LIB_BACKTRACE`, or else `RUST_BACKTRACE`, is set to
/// anything but `0`; with neither set, in builds with debug assertions only.
/// That is how `termoxide` was built, which matches the app unless it overrides
/// the profile per package.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Trace {
    /// Most recent call first, from the panic to the call TermOxide made into
    /// the app, without standard library and runtime frames.
    Frames(Vec<TraceFrame>),
    Disabled,
    /// No frame of the app was found, typically for lack of debug symbols
    /// (the workspace `release` profile strips them).
    Unavailable,
}

/// One call in a [`Trace`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TraceFrame {
    symbol: String,
    file: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
}

impl TraceFrame {
    pub(crate) fn new(symbol: impl Into<String>, file: Option<String>, line: Option<u32>, column: Option<u32>) -> Self {
        Self { symbol: symbol.into(), file, line, column }
    }

    /// Demangled, without the hash.
    pub fn symbol(&self) -> &str { &self.symbol }

    pub fn file(&self) -> Option<&str> { self.file.as_deref() }

    pub fn line(&self) -> Option<u32> { self.line }

    pub fn column(&self) -> Option<u32> { self.column }

    /// `file:line:column`, as far as known, relative to the current directory
    /// when under it.
    fn source(&self) -> Option<String> {
        let file = self.file.as_deref()?;
        let cwd = std::env::current_dir().ok();
        let file = cwd
            .as_deref()
            .and_then(|cwd| Path::new(file).strip_prefix(cwd).ok())
            .map_or_else(|| file.to_owned(), |relative| relative.display().to_string());
        Some(match (self.line, self.column) {
            (Some(line), Some(column)) => format!("{file}:{line}:{column}"),
            (Some(line), None) => format!("{file}:{line}"),
            (None, _) => file,
        })
    }
}

#[cfg(test)]
mod tests;
