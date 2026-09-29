//! Catches panics in [`App`](crate::App) methods.
//!
//! A panic's location and stack only exist inside the panic hook, so a
//! process-wide hook records them while the thread is inside [`Guard::call`],
//! and forwards every other panic to the hook it replaced.

use std::{
    any::Any,
    cell::Cell,
    panic::{self, AssertUnwindSafe, PanicHookInfo},
    sync::Once,
};

use backtrace::Backtrace;

use crate::error::{AppMethod, AppPanic, PanicLocation, Trace, TraceFrame};

struct Captured {
    message: String,
    location: Option<PanicLocation>,
    backtrace: Backtrace,
}

thread_local! {
    /// How many guarded calls the current thread is inside of.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    static CAPTURED: Cell<Option<Captured>> = const { Cell::new(None) };
}

/// Install the capturing hook once per process, wrapping the current one.
///
/// Never uninstalled: swapping hooks per loop would race between loops running
/// at the same time.
pub(crate) fn install_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if capturing() {
                record(info);
            } else {
                previous(info);
            }
        }));
    });
}

fn capturing() -> bool { DEPTH.try_with(|depth| depth.get() > 0).unwrap_or(false) }

/// Runs inside the hook: prints nothing (the alternate screen would swallow it)
/// and leaves resolving the backtrace for later.
fn record(info: &PanicHookInfo<'_>) {
    let captured = Captured {
        message: payload_message(info.payload()),
        location: info.location().map(|at| PanicLocation::new(at.file(), at.line(), at.column())),
        backtrace: Backtrace::new_unresolved(),
    };
    let _ = CAPTURED.try_with(|slot| slot.set(Some(captured)));
}

fn payload_message(payload: &(dyn Any + Send)) -> String {
    match (payload.downcast_ref::<&str>(), payload.downcast_ref::<String>()) {
        (Some(message), _) => (*message).to_owned(),
        (None, Some(message)) => message.clone(),
        (None, None) => "Box<dyn Any>".to_owned(),
    }
}

/// Runs `App` methods, turning their panics into [`AppPanic`]s.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Guard {
    /// Whether a caught panic keeps its stack.
    trace: bool,
}

impl Guard {
    pub(crate) fn new(trace: bool) -> Self { Self { trace } }

    /// `AssertUnwindSafe` holds because the loop stops at the first panic.
    pub(crate) fn call<T>(self, method: AppMethod, call: impl FnOnce() -> T) -> Result<T, AppPanic> {
        // Drop what a panic the app caught itself may have left behind.
        CAPTURED.with(Cell::take);
        DEPTH.with(|depth| depth.set(depth.get() + 1));
        let result = panic::catch_unwind(AssertUnwindSafe(call));
        DEPTH.with(|depth| depth.set(depth.get() - 1));

        result.map_err(|payload| {
            match CAPTURED.with(Cell::take) {
                Some(captured) => {
                    let trace = if self.trace { trim(resolve(captured.backtrace)) } else { Trace::Disabled };
                    AppPanic::new(method, captured.message, captured.location, trace)
                },
                // The app replaced the hook after the loop installed it.
                None => AppPanic::new(method, payload_message(payload.as_ref()), None, Trace::Unavailable),
            }
        })
    }
}

/// `RUST_LIB_BACKTRACE` if set, else `RUST_BACKTRACE`, where anything but `0`
/// enables the trace; with neither set, debug builds trace and release builds
/// don't.
pub(crate) fn trace_enabled(lib: Option<&str>, rust: Option<&str>, debug: bool) -> bool {
    lib.or(rust).map_or(debug, |value| value != "0")
}

pub(crate) fn trace_from_env() -> bool {
    let var = |name| std::env::var_os(name).map(|value| value.to_string_lossy().into_owned());
    trace_enabled(
        var("RUST_LIB_BACKTRACE").as_deref(),
        var("RUST_BACKTRACE").as_deref(),
        cfg!(debug_assertions),
    )
}

/// One symbol of a backtrace; `symbol` is `None` when it could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawFrame {
    symbol: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
}

/// Every symbol of `backtrace`, innermost first, inlined calls included.
fn resolve(mut backtrace: Backtrace) -> Vec<RawFrame> {
    backtrace.resolve();
    let unnamed = RawFrame { symbol: None, file: None, line: None, column: None };
    backtrace
        .frames()
        .iter()
        .flat_map(|frame| {
            match frame.symbols() {
                [] => vec![unnamed.clone()],
                symbols => {
                    symbols
                        .iter()
                        .map(|symbol| {
                            RawFrame {
                                symbol: symbol.name().map(|name| format!("{name:#}")),
                                file: symbol.filename().map(|file| file.display().to_string()),
                                line: symbol.lineno(),
                                column: symbol.colno(),
                            }
                        })
                        .collect()
                },
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// The app and its own dependencies, which a symbol cannot tell apart.
    User,
    Framework,
    /// This module: the hook and the guard.
    Capture,
    /// The standard library, the runtime, TermOxide's dependencies, unnamed frames.
    System,
}

const SYSTEM_CRATES: &[&str] = &[
    "std",
    "core",
    "alloc",
    "panic_unwind",
    "backtrace",
    "tokio",
    "any_spawner",
    "reactive_graph",
    "ratatui",
    "crossterm",
    "color_eyre",
    "eyre",
];

/// Runtime and thread entry symbols, after `color-eyre`'s list.
const RUNTIME_PREFIXES: &[&str] = &[
    "__rust",
    "___rust",
    "_rust_begin_unwind",
    "rust_begin_unwind",
    "rust_panic",
    "__pthread",
    "__scrt_common_main_seh",
    "BaseThreadInitThunk",
    "RtlUserThreadStart",
    "_start",
    "__libc_start",
    "start_thread",
    "__clone",
    "clone3",
];

fn classify(symbol: Option<&str>) -> Origin {
    let Some(symbol) = symbol else {
        return Origin::System;
    };
    if symbol.starts_with(concat!(module_path!(), "::")) {
        return Origin::Capture;
    }
    if symbol == "main" || symbol == "_main" || RUNTIME_PREFIXES.iter().any(|prefix| symbol.starts_with(prefix)) {
        return Origin::System;
    }

    // `<A as B>::m` and `<A>::m` belong to `A`'s crate.
    let mut subject = symbol;
    if let Some(inner) = symbol.strip_prefix('<') {
        subject = ["&", "mut ", "*const ", "*mut ", "dyn ", "[", "("]
            .iter()
            .fold(inner, |subject, prefix| subject.trim_start_matches(prefix));
    }
    let krate = subject
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or_default();
    if krate.starts_with("termoxide") {
        Origin::Framework
    } else if krate.is_empty() || SYSTEM_CRATES.contains(&krate) {
        Origin::System
    } else {
        Origin::User
    }
}

/// Keep the app's frames, innermost first.
///
/// Drops the leading panic machinery; when the panic comes from framework code
/// the app called, keeps only that call; then keeps the app's frames up to the
/// first framework frame, which is where the loop called into the app.
fn trim(frames: impl IntoIterator<Item = RawFrame>) -> Trace {
    let frames: Vec<(Origin, RawFrame)> = frames
        .into_iter()
        .map(|frame| (classify(frame.symbol.as_deref()), frame))
        .collect();
    let Some(first_user) = frames.iter().position(|(origin, _)| *origin == Origin::User) else {
        return Trace::Unavailable;
    };

    let machinery = frames[..first_user]
        .iter()
        .take_while(|(origin, _)| matches!(origin, Origin::System | Origin::Capture))
        .count();
    let called = frames[machinery..first_user]
        .iter()
        .rev()
        .find(|(origin, _)| *origin == Origin::Framework);
    let user = frames[first_user..]
        .iter()
        .take_while(|(origin, _)| matches!(origin, Origin::User | Origin::System))
        .filter(|(origin, _)| *origin == Origin::User);

    Trace::Frames(
        called
            .into_iter()
            .chain(user)
            .filter_map(|(_, frame)| {
                Some(TraceFrame::new(
                    frame.symbol.clone()?,
                    frame.file.clone(),
                    frame.line,
                    frame.column,
                ))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests;
