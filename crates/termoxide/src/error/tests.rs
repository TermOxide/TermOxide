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

fn panic_with(location: Option<PanicLocation>, trace: Trace) -> LoopError {
    let panic = AppPanic::new(AppMethod::HandleEvent, "counter overflowed".to_owned(), location, trace);
    LoopError::from_parts(Some(LoopFailure::Panic(panic)), None).expect("error")
}

fn main_rs() -> Option<PanicLocation> { Some(PanicLocation::new("src/main.rs", 42, 9)) }

#[test]
fn display_of_a_panic_with_a_trace() {
    let frames = vec![
        TraceFrame::new("app::State::bump", Some("/work/app/src/main.rs".to_owned()), Some(42), Some(9)),
        TraceFrame::new(
            "<app::State as termoxide::App>::handle_event",
            Some("/work/app/src/main.rs".to_owned()),
            Some(61),
            Some(17),
        ),
        TraceFrame::new("app::unknown_source", None, None, None),
        TraceFrame::new("app::no_column", Some("/work/app/src/lib.rs".to_owned()), Some(3), None),
    ];

    let error = panic_with(main_rs(), Trace::Frames(frames));

    assert_eq!(
        error.to_string(),
        [
            "app panicked in `handle_event`: counter overflowed",
            "  at src/main.rs:42:9",
            "stack (most recent call first):",
            "  app::State::bump                              /work/app/src/main.rs:42:9",
            "  <app::State as termoxide::App>::handle_event  /work/app/src/main.rs:61:17",
            "  app::unknown_source",
            "  app::no_column                                /work/app/src/lib.rs:3",
            "  termoxide: App::handle_event",
        ]
        .join("\n")
    );
}

#[test]
fn display_of_a_trace_shows_files_under_the_current_directory_relative_to_it() {
    let cwd = std::env::current_dir().expect("current directory");
    let file = cwd.join("src").join("main.rs").display().to_string();
    let frames = vec![TraceFrame::new("app::main", Some(file), Some(1), Some(2))];

    let error = panic_with(main_rs(), Trace::Frames(frames));

    let relative = std::path::Path::new("src").join("main.rs").display().to_string();
    assert!(error.to_string().contains(&format!("  app::main  {relative}:1:2\n")), "{error}");
}

#[test]
fn display_of_a_panic_with_the_trace_disabled() {
    let error = panic_with(main_rs(), Trace::Disabled);

    assert_eq!(
        error.to_string(),
        "app panicked in `handle_event`: counter overflowed\n  at src/main.rs:42:9\n  no backtrace available (set \
         RUST_BACKTRACE=1 to enable)"
    );
}

#[test]
fn display_of_a_panic_without_symbols() {
    let error = panic_with(main_rs(), Trace::Unavailable);

    assert_eq!(
        error.to_string(),
        "app panicked in `handle_event`: counter overflowed\n  at src/main.rs:42:9\n  no backtrace available (no \
         debug symbols)"
    );
}

#[test]
fn display_of_a_panic_whose_location_was_lost() {
    let error = panic_with(None, Trace::Unavailable);

    assert!(
        error.to_string().contains("\n  at <unknown location: panic hook replaced>\n"),
        "{error}"
    );
}

#[test]
fn display_of_a_panic_with_a_teardown_failure() {
    let panic = AppPanic::new(AppMethod::OnTick, "boom".to_owned(), main_rs(), Trace::Disabled);

    let error = LoopError::from_parts(Some(LoopFailure::Panic(panic)), Some(reader_error())).expect("error");

    assert!(
        error.to_string().ends_with(
            "(set RUST_BACKTRACE=1 to enable)\nalso, the input reader failed during teardown: terminal operation \
             failed: tty gone"
        ),
        "{error}"
    );
    assert!(error.source().is_some_and(|source| source.is::<termoxide_event::Error>()));
}

#[test]
fn app_method_displays_its_name() {
    let names = [
        AppMethod::TrackView,
        AppMethod::OnTick,
        AppMethod::HandleEvent,
        AppMethod::BuildView,
    ]
    .map(|method| method.to_string());

    assert_eq!(names, ["track_view", "on_tick", "handle_event", "build_view"]);
}
