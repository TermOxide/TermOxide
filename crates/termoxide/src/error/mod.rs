//! Why the main loop stopped abnormally.
//!
//! Everything is carried and printed by [`LoopError`] itself: `color_eyre`
//! sections only attach anything once the app installed `color_eyre`.

use std::{error::Error, fmt};

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
            None => self.teardown.as_ref().map(|error| error as &(dyn Error + 'static)),
        }
    }
}

/// What ended the main loop.
#[derive(Debug)]
#[non_exhaustive]
pub enum LoopFailure {
    /// A frame failed to render.
    Render(RenderError),
}

impl fmt::Display for LoopFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render(error) => write!(f, "render failed: {error}"),
        }
    }
}

#[cfg(test)]
mod tests;
