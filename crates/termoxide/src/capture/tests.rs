use super::*;

#[test]
fn trace_follows_the_build_when_no_variable_is_set() {
    assert!(trace_enabled(None, None, true));
    assert!(!trace_enabled(None, None, false));
}

#[test]
fn rust_backtrace_overrides_the_build_both_ways() {
    for debug in [true, false] {
        assert!(trace_enabled(None, Some("1"), debug));
        assert!(trace_enabled(None, Some("full"), debug));
        assert!(!trace_enabled(None, Some("0"), debug));
    }
}

#[test]
fn rust_lib_backtrace_takes_precedence_over_rust_backtrace() {
    for debug in [true, false] {
        assert!(trace_enabled(Some("1"), Some("0"), debug));
        assert!(!trace_enabled(Some("0"), Some("1"), debug));
        assert!(!trace_enabled(Some("0"), None, debug));
        assert!(trace_enabled(Some("full"), None, debug));
    }
}

#[test]
fn hook_captures_only_inside_a_guarded_call() {
    let guard = Guard::new(false);

    assert!(!capturing());
    assert!(guard.call(AppMethod::OnTick, capturing).expect("no panic"));
    assert!(
        guard
            .call(AppMethod::OnTick, || guard.call(AppMethod::BuildView, capturing))
            .expect("no panic")
            .expect("no panic")
    );
    assert!(!capturing());
}

#[test]
fn guard_passes_the_result_through() {
    assert_eq!(Guard::new(false).call(AppMethod::HandleEvent, || 42).expect("no panic"), 42);
}

#[test]
fn payload_message_reads_string_payloads() {
    assert_eq!(payload_message(&"literal"), "literal");
    assert_eq!(payload_message(&String::from("formatted")), "formatted");
    assert_eq!(payload_message(&42), "Box<dyn Any>");
}

fn frame(symbol: &str, line: u32) -> RawFrame {
    RawFrame {
        symbol: Some(symbol.to_owned()),
        file: Some("/src/main.rs".to_owned()),
        line: Some(line),
        column: Some(5),
    }
}

fn unnamed() -> RawFrame { RawFrame { symbol: None, file: None, line: None, column: None } }

/// From the backtrace capture up to the panic.
fn machinery() -> Vec<RawFrame> {
    vec![
        frame("backtrace::capture::Backtrace::new_unresolved", 1),
        frame("termoxide::capture::record", 2),
        frame("termoxide::capture::install_hook::{{closure}}", 3),
        frame("<alloc::boxed::Box<F,A> as core::ops::function::Fn<Args>>::call", 4),
        frame("std::panicking::rust_panic_with_hook", 5),
        unnamed(),
        frame("__rustc::rust_begin_unwind", 6),
        frame("core::panicking::panic_fmt", 7),
    ]
}

/// From the loop calling the app out to the thread entry.
fn outer() -> Vec<RawFrame> {
    vec![
        frame("termoxide::pump_events::{{closure}}", 100),
        frame("termoxide::capture::Guard::call", 101),
        frame("termoxide::drive::{{closure}}", 102),
        frame("tokio::runtime::task::harness::poll", 103),
        frame("main", 104),
        frame("__libc_start_main", 105),
    ]
}

fn symbols(trace: &Trace) -> Vec<&str> {
    match trace {
        Trace::Frames(frames) => frames.iter().map(TraceFrame::symbol).collect(),
        other => panic!("expected frames, got {other:?}"),
    }
}

#[test]
fn trim_keeps_the_user_frames_of_a_panic_in_user_code() {
    let user = [
        frame("app::State::bump", 42),
        frame("core::ops::function::FnOnce::call_once", 9),
        frame("<app::State as termoxide::App>::handle_event", 61),
    ];

    let trace = trim(machinery().into_iter().chain(user).chain(outer()));

    assert_eq!(symbols(&trace), [
        "app::State::bump",
        "<app::State as termoxide::App>::handle_event"
    ]);
    let Trace::Frames(frames) = &trace else { unreachable!() };
    assert_eq!(
        (frames[0].file(), frames[0].line(), frames[0].column()),
        (Some("/src/main.rs"), Some(42), Some(5))
    );
}

#[test]
fn trim_keeps_only_the_call_into_framework_code_that_panicked() {
    let inner = [
        frame("core::option::expect_failed", 10),
        frame("reactive_graph::signal::RwSignal<T>::try_get", 11),
        frame("termoxide_reactive::signal::Signal<T>::with", 12),
        frame("termoxide_reactive::signal::Signal<T>::get", 13),
        frame("app::view", 20),
        frame("<app::State as termoxide::App>::track_view", 30),
    ];

    let trace = trim(machinery().into_iter().chain(inner).chain(outer()));

    assert_eq!(symbols(&trace), [
        "termoxide_reactive::signal::Signal<T>::get",
        "app::view",
        "<app::State as termoxide::App>::track_view",
    ]);
}

#[test]
fn trim_is_unavailable_without_a_user_frame() {
    assert_eq!(trim(machinery().into_iter().chain(outer())), Trace::Unavailable);
    assert_eq!(trim([unnamed(), unnamed()]), Trace::Unavailable);
    assert_eq!(trim([]), Trace::Unavailable);
}

#[test]
fn trim_skips_unnamed_frames_between_user_frames() {
    let user = [frame("app::inner", 1), unnamed(), frame("app::outer", 2)];

    let trace = trim(machinery().into_iter().chain(user).chain(outer()));

    assert_eq!(symbols(&trace), ["app::inner", "app::outer"]);
}

#[test]
fn classify_names_the_origin_of_a_symbol() {
    assert_eq!(classify(Some("<user::T as termoxide::App>::handle_event")), Origin::User);
    assert_eq!(classify(Some("<&mut user::T as core::fmt::Debug>::fmt")), Origin::User);
    assert_eq!(classify(Some("<user::T>::method")), Origin::User);
    assert_eq!(classify(Some("user::f::{{closure}}")), Origin::User);
    assert_eq!(classify(Some("serde::de::Deserialize::deserialize")), Origin::User);
    assert_eq!(classify(Some("mainframe::run")), Origin::User);
    assert_eq!(classify(Some("termoxide::drive::{{closure}}")), Origin::Framework);
    assert_eq!(
        classify(Some("<termoxide_reactive::Signal<T> as core::clone::Clone>::clone")),
        Origin::Framework
    );
    assert_eq!(classify(Some("termoxide::capture::Guard::call")), Origin::Capture);
    assert_eq!(
        classify(Some("<core::panic::AssertUnwindSafe<F> as FnOnce<()>>::call_once")),
        Origin::System
    );
    assert_eq!(
        classify(Some("tokio::runtime::park::CachedParkThread::block_on")),
        Origin::System
    );
    assert_eq!(classify(Some("__rustc::rust_begin_unwind")), Origin::System);
    assert_eq!(classify(Some("main")), Origin::System);
    assert_eq!(classify(None), Origin::System);
}
