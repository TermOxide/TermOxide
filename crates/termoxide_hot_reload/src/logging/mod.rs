mod catch;
mod file;
mod location;
mod logger;

pub(crate) use catch::{log_panics, relay_output};
pub(crate) use file::LogFile;
pub(crate) use location::LOG_ENV;
pub(crate) use logger::{Tag, install};

#[cfg(test)]
mod tests;
