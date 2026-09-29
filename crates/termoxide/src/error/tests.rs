use std::io;

use super::*;

fn render_failure() -> LoopFailure { LoopFailure::Render(RenderError::Io(io::Error::other("disk on fire"))) }

fn reader_error() -> termoxide_event::Error { termoxide_event::Error::Terminal(io::Error::other("tty gone")) }

#[test]
fn loop_error_travels_inside_a_report() {
    fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<LoopError>();
}

#[test]
fn from_parts_is_none_when_nothing_failed() {
    assert!(LoopError::from_parts(None, None).is_none());
}

#[test]
fn display_of_a_render_failure() {
    let error = LoopError::from_parts(Some(render_failure()), None).expect("error");

    assert_eq!(error.to_string(), "render failed: render I/O error: disk on fire");
}

#[test]
fn display_of_a_teardown_failure_alone() {
    let error = LoopError::from_parts(None, Some(reader_error())).expect("error");

    assert_eq!(
        error.to_string(),
        "the input reader failed during teardown: terminal operation failed: tty gone"
    );
}

#[test]
fn display_of_a_render_failure_with_a_teardown_failure() {
    let error = LoopError::from_parts(Some(render_failure()), Some(reader_error())).expect("error");

    assert_eq!(
        error.to_string(),
        "render failed: render I/O error: disk on fire\nalso, the input reader failed during teardown: terminal \
         operation failed: tty gone"
    );
}

#[test]
fn source_is_the_render_error_first_then_the_teardown_error() {
    let both = LoopError::from_parts(Some(render_failure()), Some(reader_error())).expect("error");
    let teardown_only = LoopError::from_parts(None, Some(reader_error())).expect("error");

    assert!(both.source().is_some_and(|source| source.is::<RenderError>()));
    assert!(
        teardown_only
            .source()
            .is_some_and(|source| source.is::<termoxide_event::Error>())
    );
}
