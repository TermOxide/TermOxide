mod app;
mod catch;
mod file;
mod location;
mod logger;

pub(crate) use app::AppOutput;
pub(crate) use catch::{log_panics, relay_output};
pub(crate) use file::LogFile;
pub(crate) use location::{LOG_ENV, app_dir};
pub(crate) use logger::{APP_TARGET, Tag, install};

#[cfg(test)]
mod tests;
