# Hot reload — implementation spec

**Project:** signal-driven, JSX-like TUI framework in Rust
**Status:** design complete, the dev loop built — transport and ABI live-tested across four candidates
(including the naive full-process-restart baseline). Built: the subprocess running the app's component,
the protocol, the shared log, rebuilding on save, the schema fingerprints, epoch interruption and
recovery, and persistence with its trace reader. Not yet built: linking natively for release (§8), the
WASI filesystem and network capabilities (§6), and validating a module before loading it.
Reload latency is `cargo build`-dominated and not sub-second — do not promise a specific number anywhere
derived from this document.
**Companion:** `adr/hot_reload/index.html` — same decisions with reasoning and rejected alternatives. This
document states requirements only. When something here looks arbitrary, the ADR says why.

Read §1 and §2 before writing any code. Everything else can be read on demand.

---

## 1. Invariants

These hold everywhere. A change that violates one is a design change, not a bug fix.

1. **The app author's surface is the framework's own `App` trait.** No host code, no FFI, no manual
   export list on the app side — a single macro invocation generates the rest.
2. **The guest never touches the terminal.** Rendering, raw mode, and the alternate screen are owned
   exclusively by the outer host process, on every transport.
3. **An ABI mismatch is a caught error, never undefined behavior and never silent misdecoding.** Two
   separate checks, neither sufficient alone: the function-signature check (§2.1) and a wire-schema
   version check (§2.3). A stale build loading against a newer host must fail cleanly and recoverably.
4. **State crosses the boundary as opaque, generically-encoded bytes.** Host never decodes it; only the
   guest ever holds a real, typed value.
5. **A trap or a failed rebuild discards the current generation and keeps the previous one running.**
   Never propagate to process exit.
6. **No frame-loop call (`on_tick`/`handle_event`/`build_view`) may legitimately take more than a few
   milliseconds.** Any overrun is a bug by contract, not a case to diagnose further.
7. **The terminal, the IPC channel, and both log destinations are four separate streams.** Nothing is
   ever written to more than one of them.
8. **Capabilities (filesystem, network, stdio) are granted permissively, not withheld by default.** The
   sandbox's safety properties don't depend on capability restriction.

---

## 2. Core types

```rust
/// The entire app-author surface: termoxide's own trait, the one a native app
/// implements. `wasm_app!(expr)` builds the app with `expr` and exports it.
pub trait App {
    fn track_view(&self);
    fn on_tick(&self);
    fn handle_event(&self, event: Event) -> bool;   // returns quit
    fn build_view(&self, viewport: Rect) -> ViewNode;
    fn mouse_capture(&self) -> bool { true }
}
```

- The app value is the state: its fields are `Signal`s, and the app type derives `Serialize` and
  `Deserialize`. A signal crosses as its value and comes back as a new signal holding it
  (`termoxide_reactive`'s `serde` feature); subscriptions don't cross.
- `Event` is `termoxide_event`'s; `ViewNode` is `termoxide_rendering`'s, with no `Raw` content. Both cross
  through their crates' `serde` feature.
- In a `ViewNode`, colours cross as `termoxide_layout`'s `Color`. The style and area are ratatui's `Style`
  (with `Modifier`) and `Rect`, stand-ins until the resolved style (with `FontStyle` for modifiers) and a
  framework rect replace them; each swap is a wire-schema change (§2.3). Underline colours don't cross.
- The guest runs the reactive runtime itself, on a local executor. It tells the host a repaint is due with
  a dirty flag on its replies (§3), set by `track_view`'s render effect once the guest has yielded to let it
  run.
- A signal's type says whether its changes are recorded in the state trace (§7): `Signal<T>` is
  `Signal<T, Recorded>`, and `Signal::new(v).unrecorded()` gives a `Signal<T, Unrecorded>`, for state that
  changes too often to be worth an entry each time. The mode is the type the app stores the signal as, for
  its whole lifetime: state is restored by rebuilding each signal from its value, so nothing kept on the
  signal itself would survive.

Host-side, the whole config surface is the app's crate itself — no per-app function pointers, no per-app
type parameter. Its `cdylib` declares the app; its executable only calls `run_app!()`, or
`run_app!(persistent)` (§7), which captures the crate's manifest directory to build the guest from (§2.4):

```rust
// lib.rs, a `cdylib` + `rlib`
termoxide_hot_reload::wasm_app!(Counter::default());

// main.rs, the host, inside a tokio `LocalSet`
termoxide_hot_reload::run_app!().await
```

### 2.1 Export names and signature checking

The guest is a `wasm32-wasip2` component with two exports and one import:

```wit
world guest {
  enum level { error, warn, info, debug, trace }
  import log: func(level: level, message: string);
  export handle: func(request: list<u8>) -> result<list<u8>, string>;
  export wire-schema: func() -> u64;
}
```

`handle` takes a `postcard`-encoded request of the protocol (§3) and returns the encoded response, so the
subprocess forwards the pipe's messages unchanged. `wire-schema` is the protocol's fingerprint (§2.3). `log`
carries the app's log records (§5). The drafts' core-module exports (`state_ptr`, `output_ptr`, `memory`,
one per call) have no counterpart.

Resolve every export via a typed function (`Instance::get_typed_func::<Params, Results>(&mut store, NAME)`, or
`wasmtime::component`'s equivalent, checked against the WIT types). This validates
the requested signature against the module's actual declared type at load time and returns `Err` on a
mismatch — do not resolve exports any other way. This checks **function types only**. It says nothing
about the meaning of the bytes passed through linear memory: a stale guest can keep identical function
signatures while changing tag tables, line encoding, buffer sizes or state layout, and both sides will
decode successfully into wrong values. That is what §2.3 is for; this check is half of invariant 3, not
all of it. With a single `handle`, it covers that one signature: every argument is bytes anyway.

### 2.2 State encoding

State crosses the boundary via `postcard`, as the encoded app value (`A: App + Serialize + DeserializeOwned`),
inside the guest-side macro expansion only. The outer host (§3) holds the resulting
bytes as an opaque `Vec<u8>` and passes them back in unexamined — host code must never contain a type
parameter for app state. Every call that changes state returns the new bytes to the outer host, and only
those: the guest compares the encoded app with the last bytes it sent, so any signal counts, not only those
the view reads. The host
keeps the latest state, so a respawned subprocess (§4.3) resumes from it. Whether to keep a few earlier
generations too, to step back from a state that keeps crashing the child, is open until there is data on how
often that happens; a persistent run sets such a state's trace aside instead (§7).

### 2.3 Wire-schema and state compatibility

Two separate fingerprints, checked at two separate points. Neither is covered by §2.1.

A fingerprint is traced from the types, never bumped by hand: `serde-reflection` traces a type's shape as
`serde` sees it in a binary format, and a 64-bit FNV-1a hash of that trace is the fingerprint (not `std`'s
hasher, whose algorithm may change between Rust releases). The tracer's registry is taken unchecked: nested
enums only get their variants' names and order resolved, which is all a fingerprint needs.

- **Wire schema** — the fingerprint of the protocol's `Request` and `Response` (§3), which contain every
  framework type that crosses: the event types, `ViewNode`, `Color`, the style. The guest exports it as
  `wire-schema`; the subprocess checks it against its own on every generation, before the first request.
  Mismatch → reject the generation exactly like a failed rebuild (§4.3). `postcard` is positional and not
  self-describing (structs have no field names, enum variants are encoded by discriminant), so a reordered
  or inserted variant decodes *successfully* as the wrong one — it will not fail on its own.
- **State schema** — the state bytes are the app type's fingerprint (8 bytes, little-endian), then the
  `postcard`-encoded app. A generation given state with another fingerprint, or too short to hold one, does
  **not** decode it: it starts from the app as built and says so (`Started { reset: true }`), and the
  subprocess logs a warning. State with the right fingerprint that still fails to decode is an error, not a
  reset: the generation is rejected. Migration between state shapes is out of v1.
- Persistence (§7) has the same problem across a host *restart*: every entry of the state trace holds the
  state bytes, fingerprint included, so a trace from an older build is caught the same way.

The drafts' generated decode did `postcard::from_bytes(bytes).unwrap_or_default()`, which loses state
silently on a decode error and does nothing about a decode that succeeds wrongly.

### 2.4 Guest build

The guest compiles against `termoxide`, `termoxide_event` and `termoxide_rendering` with
`default-features = false`: their `terminal` feature (on by default) holds `crossterm`, the renderer, the
input reader and the run loop, none of which build for WASI. What remains is the `App` trait, `ViewNode` and
the event types. CI's `wasm` leg builds every changed crate that way for `wasm32-wasip2`, except the e2e
tests.

`termoxide_hot_reload` has two explicit features: `host` for the executable (terminal side, pipe, log file,
`wasmtime`, building the guest) and `guest` for the component (running the app, its executor, its logger);
ungated code is the protocol both share. The app's lib is a `cdylib` + `rlib` with a `guest` feature that
enables the crate's. Before taking the terminal over, the host runs `cargo build --lib --target
wasm32-wasip2 --no-default-features --features guest` on the app's manifest and takes the `.wasm` from
cargo's JSON output; rebuilding on save reuses this.

---

## 3. Process & IPC

```text
outer host (owns the terminal, invariant 2)
  │  spawns, framework-authored
  ▼
subprocess (hosts wasmtime; no TTY, renders nothing)
  │  dedicated named pipe, reserved exclusively for this
  ▼
IPC channel: one request enum, one reply enum
```

- The channel is a named pipe. The host creates the server end **before** spawning the child, under a
  name unique to that host process, and passes the name at spawn (argv or environment). The child
  connects. Accept exactly one client; where the platform allows, refuse remote clients. Never route
  logging through it — see §5.
- It is an `interprocess` local socket with a namespaced name: a named pipe on Windows, an abstract
  socket on Linux, a socket file under `/tmp` on macOS. The name is the host's pid and a counter, so a
  socket file a crash leaves behind never collides with a later run. A connection accepted from the
  non-blocking listener is switched to blocking: macOS hands the listener's mode down to it, and the
  first write would fail with `EAGAIN`.
- Child death is observed as the pipe closing. The pipe is still a byte stream, so messages need framing.
- One request enum and one reply enum:

  | Request | Reply |
  |---|---|
  | `Ping` | `Pong` |
  | `Start { state }`, the host's state if it has one | `Started { mouse_capture, state, reset }` |
  | `Event`, `Tick` | `Done(Outcome { quit, dirty, state, recorded })`; at the tick a new generation takes over, `Reloaded { state }` |
  | `BuildView(Rect)` | `View(ViewNode)` |
  | `Load { guest }`, a rebuilt component (§4.4) | `Loading`; never reaches the component |
  | `Describe { state }`, for reading a trace (§7) | `Described { fingerprint, app }`: the app as JSON |
  | any request the app crashed on (§4.3) | `Recovered { state }` |

  An outcome carries the state bytes only when they changed (§2.2), and `recorded` when a recorded signal
  did (§7).
- The subprocess's `stdin` is set to null at spawn, so it can't read the host's keystrokes. Its `stdout`
  and `stderr` are piped to the host, which logs each line through the framework logger (§5), tagged as
  the child's: `stdout` at `info`, `stderr` at `debug`. Never left inherited, which would draw over the
  TUI. Never pointed at the log file directly, which would bypass the §5 lock.
- The harness installs a panic hook that logs through the framework logger.
- The subprocess loads the component when `Start` arrives — compiling it takes seconds, so `Start` has its
  own timeout (60 s) — and hands every request to `handle`. Compiled components go through `wasmtime`'s
  compilation cache, so an unchanged component compiles once. It serves on a thread of its own, outside the
  app's async runtime: `wasmtime` runs the component's stdio on a runtime of its own, and one can't start
  inside another. The guest lets the render effect run by polling its local executor, where a native run
  yields to tokio.

---

## 4. Hang & crash protection

Three independent failure modes. None of the three mechanisms below substitutes for another.

### 4.1 Inside a guest call

```rust
// at Engine construction, and a thread that calls engine.increment_epoch() every 10 ms
config.epoch_interruption(true);

// per call, and before instantiating, which runs the component's start-up code
store.set_epoch_deadline(50);       // 500 ms
```

Epoch interruption, not fuel: the budget is wall-clock time, what the frame-call contract (invariant 6)
is about, where fuel counts instructions and costs a check on every one. Cranelift inserts the check at
every loop backedge and function entry — a bare `loop {}` traps on budget exhaustion with no cooperation
required from the guest's own code. Not configured in `Engine::default()`; must be set explicitly. The
budget is far above any legitimate frame call, even in a debug build, and under the host's one-second
reply timeout (§4.2), so a runaway call is caught in the subprocess, which recovers (§4.3), rather than
losing the whole subprocess. The store's initial deadline is 0, so instantiating without setting one traps
at once.

### 4.2 Around the subprocess round-trip

A flat timeout on "did the subprocess respond to this request," independent of §4.1, also covering the
child connecting to the IPC pipe (§3), which has no built-in timeout: 5 s to connect, 1 s per request, 60 s
for `Start`, which compiles the component — catches a hang in
the harness code or in `wasmtime` instantiation itself, neither of which a guest-call-scoped fuel/epoch
budget can see. No heartbeat mechanism: a heartbeat only proves the responding thread is alive, not that
a specific in-flight call will return (identical failure shape to fuel exhaustion, without the hard
bound) — do not build one.

### 4.3 Recovery

A guest call that traps (a panic, the §4.1 budget) never propagates. The subprocess drops the request,
starts a new generation from the latest state and answers `Recovered { state }`: from the component before
the last reload if the trap follows one, since the new code is the likely cause, else from a new instance of
the same component. The drafts' `drive()` loops used `?` and exited the host process on a trap.

A lost subprocess (the pipe closed, a §4.2 timeout, a malformed message, or the child exiting) is a
different case: the whole child is gone, previous generation included. Kill it if still alive, reap it,
log why, and respawn it in the background on a fresh pipe, starting from the outer host's state (§2.2).
Never reconnect to it. Drop the in-flight call; never retry it.

Both count as restarts, limited Erlang/OTP-style: at most 3 within any 10 s. Beyond that the host stops the
app until the next successful rebuild, which starts it again and clears the count, as does a successful
reload. A persistent run also sets its state trace aside (§7). While the app isn't running, the host keeps
its last frame on screen with a notice line at the bottom, and Ctrl+C quits, since the app's own quit key
can't be reached.

A failed rebuild (`build_and_load` returning `Ok(None)`/`Err`) already leaves the previous generation's
binding untouched — the same pattern §4.3's fix needs to extend to in-flight traps, not a new mechanism.

### 4.4 The rebuild step itself

```rust
// the fix: never call this inline in the render/event loop
std::thread::spawn(move || {
    let output = Command::new("cargo").args(["build", ...]).output();
    // ... send result back to the host loop via a channel, same shape
    // as the existing notify-based file-watcher channel
});
```

A `notify` watcher on the app's `src/` and its manifest triggers a rebuild. Only edits count (a file
created, removed, renamed or written): building reads every source, so counting reads or metadata changes
would rebuild forever. Changes are batched until the sources stay quiet for 200 ms. The rebuild must run
`cargo build` on its own background thread, never inline in the
host loop — the same pattern `termoxide_event`'s input reading and the hot-lib-reloader draft's rebuild
watcher already use. §4.1 and §4.2 don't cover this case (no cooperative checkpoint exists inside `cargo`
to instrument, and there's no subprocess abstraction wrapping a one-shot build invocation), so this is its
own, simpler mechanism: isolate the wait, not bound it.

**This is not, by itself, a complete fix.** Moving the wait off the main thread keeps the host loop
(rendering, input, ticking) responsive even if the build never returns — but the `cargo build` child
process itself would still be running, unbounded, on that background thread. Two more pieces bound it:

- **A kill timeout** of 10 minutes — minutes, not milliseconds, since a real build can legitimately take a
  long time. On expiry, the child process is killed outright and the failure reported through the shared
  log (§5), handled like an ordinary failed build: the running generation stays.
- **A single-in-flight guard** — one rebuild thread. A file save arriving while a rebuild is running makes
  one more build after it, never a second, overlapping `cargo build` competing for the same target
  directory and lock.

Cargo's output goes to the log, the terminal being the host's. A built component reaches the subprocess
as `Load`; it compiles there beside the running generation and takes over at the next tick, from the
latest state.

Do not state a reload-latency number anywhere derived from this spec. Compile time dominates, is not
sub-second even in the best case, and is currently unbounded in the worst case — background-threading the
wait fixes UI responsiveness, not that underlying fact.

---

## 5. Logging

Four destinations. Confusing any two of them is the bug class this section exists to prevent.

| Destination | Content | Mechanism |
|---|---|---|
| Outer host's real stdout | Rendered TUI | Never shared with anything else |
| Subprocess named pipe | IPC protocol | Reserved exclusively — §3 |
| Subprocess `stdin` | Nothing | Null — §3 |
| Subprocess `stdout`/`stderr` | Stray prints, runtime crash output | Piped to host, logged line by line (`info`/`debug`) — §3 |
| Shared log file | `[framework:host]`/`[framework:child]` lines | Both processes `OpenOptions::append(true)` directly |
| Shared log file (same file) | `[app]` lines | Guest's `log` records → `log` import; guest's stdout/stderr → line sink (`info`/`debug`) |

- One shared file per host run, tagged per source — not per-source files, not a spawned terminal window.
  Named after the host's start time (no colons) and pid, in `termoxide/<app>/logs/` under `dirs::state_dir()`,
  falling back to `dirs::data_local_dir()`: `%LOCALAPPDATA%\termoxide\<app>\logs\` on Windows,
  `$XDG_STATE_HOME/termoxide/<app>/logs/` on Linux (default under `~/.local/state/`),
  `~/Library/Application Support/termoxide/<app>/logs/` on macOS; `<app>` is the executable's file stem.
  The host passes the path to the child at spawn.
- Framework code logs through the `log` facade (`log::info!` etc.); the backend formats each tagged line and
  commits it under the lock below. It writes the framework's records from `debug` and other crates' only
  from `warn`: `wasmtime`'s compiler alone logs about a hundred thousand `debug` lines per component.
- Every writer formats the full line first, then writes it **while holding an exclusive cross-process
  lock on the log file for the entire write** — held for that one line only, never across user code.
  A single `write_all` call is **not** sufficient on its own: `Write::write_all` loops on `write` after a
  short write, so one call can become several syscalls, and `O_APPEND`/`FILE_APPEND_DATA` only makes each
  *syscall* atomic, not the logical line. Routing every record through one writer that owns the file is an
  equivalent alternative, but here it would mean carrying log records over the IPC channel reserved in §3.
  Stress-tested: two uncoordinated processes writing 20,000 lines each, one `write_all` per line,
  concurrently, to the same file — 40,000 lines, zero malformed, on this project's Windows/NTFS setup.
  That is evidence the common path behaves, not proof of the guarantee; a short write is exactly the case
  such a run is unlikely to hit. The OS releases the lock if the holding process exits or is killed.
- With `File::lock`, each process also serialises its own threads' writes (a mutex): on Unix the lock
  belongs to the open file, shared by all its threads. Both processes open the file readable as well as
  appending: Windows refuses to lock a handle opened with `append(true)` alone.
- Both processes log their panics through the logger. The host's hook then hands over to the hook
  installed before it (an app's own, such as `color_eyre`'s); the child's doesn't, since its stderr is
  relayed into the log already.
- App-side output never uses `WasiCtxBuilder::inherit_stdio()` — that wires the guest directly to the
  subprocess's own stdio, so its output would reach the log only as the child's relayed `stdout` (§3),
  tagged as the framework's rather than the app's. The guest's stdout and stderr go to a line sink
  instead: one buffer shared by every stream `wasmtime` opens, committing each completed line under the
  same lock as every other writer, whatever the app's write pattern; stdout at `info`, stderr at `debug`.
- The guest installs a `log::Log` backend before building the app, forwarding each record through the
  `log` import, so the app author uses ordinary `log::info!`/`warn!`. It forwards every crate's records
  from `debug` up: inside the guest it can't tell the app's crates from its dependencies. Records and
  output are both tagged `[app]`.

---

## 6. Capabilities (WASI)

Configure permissively by default — this is a hang/crash/ABI-safety design, not a sandboxing one
(see the ADR's Domain 1 for the threat model this rests on).

```rust
WasiCtxBuilder::new()
    .preopened_dir("/", DirPerms::all(), FilePerms::all())   // adjust root per platform
    .inherit_network()                                        // preview2 WASI surface; base preview1 has no sockets
    .stdout(custom_sink) .stderr(custom_sink)                 // §5 — never .inherit_stdio()
```

Base WASI preview1 has no socket support at all; network access requires the preview2/component-model
surface, which is why the guest targets `wasm32-wasip2`. Not yet wired into any draft — today's guests have zero filesystem/network/env access because
nothing is wired, not because anything is deliberately withheld.

---

## 7. Persistence

```rust
// main.rs
termoxide_hot_reload::run_app!(persistent).await
```

- Opt-in, layered on the base transport. A reload alone never loses state — persistence exists only for
  surviving a full restart of the host process itself. `TERMOXIDE_HOT_RELOAD_PERSIST=0` or `1` overrides
  `run_app!` for one run; `TERMOXIDE_HOT_RELOAD_FRESH=1` starts that run from a clean state.
- No new encoding step beyond §2.2: the same opaque bytes already crossing the reload boundary are what
  gets written to disk, fingerprint included (§2.3).
- A **state trace**, not a single snapshot: `state.bin` in `termoxide/<app>/`, next to the logs (§5) —
  fixed, not pid-keyed, so a freshly started host finds what the previous one wrote. It starts with the line
  `termoxide state trace 1`, then a header naming the app's manifest directory, then one `postcard` entry
  `{ at, state }` (milliseconds since the Unix epoch, the state bytes) per change of a recorded signal (§2).
  Each run starts a new trace with the state the app started from, written beside the old one and renamed
  over it, then appends; the next run starts from the last entry. An entry cut short by a host killed while
  writing it is skipped.
- A trace that isn't one, or whose last state fails to decode, is renamed `state.bin.unreadable` and the
  app starts afresh. When the app stops for crashing too often (§4.3), its trace is renamed
  `state.bin.crashed`, so the next run doesn't start from the state that crashed it. A trace saved by
  another state type resets the app (§2.3), and the run's own trace replaces it.
- `termoxide_dump <trace> [--manifest <Cargo.toml>]`, a binary of `termoxide_hot_reload` (`host`
  feature), prints a trace: only the app's own component can decode its state, so it builds the guest from
  the manifest directory the header names, then asks it to `Describe` each entry. It prints each entry's
  time, its state type's fingerprint — highlighted where it changed from the previous entry — and the app as
  JSON.

---

## 8. Release builds

A release binary compiles `logic` natively and links it directly into the host — none of §§3–6 ship: no
`wasmtime`, no subprocess, no IPC, no fuel/epoch, no `postcard` marshaling. The distinguishing switch is
target architecture, not build profile — a release-profile `wasm32` build still needs the dev-loop export
path (optimized `logic`, still run through hot-reload during iteration), so `cfg(debug_assertions)` is the
wrong gate.

```rust
// wasm_app!(expr) expands to this, kept or dropped by the compiler, not the macro:
#[cfg(target_arch = "wasm32")]
const _: () = { /* the `handle` export, over the app built with `expr` — §2.1 */ };
// and to nothing natively.
```

Native linking needs no named-symbol export or runtime resolution at all: once `logic` is just another
crate in the same compilation graph, the host's native entry point is termoxide's own `run_with_app(app)`,
the loop a native app already runs. There is nothing for the macro's
native branch to generate beyond what the app's own trait impl already provides.

`logic`'s `[lib] crate-type` must list both `cdylib` (the artifact the dev path loads) and `rlib` (so the
native host can depend on it as an ordinary crate) — not a new pattern; the hot-lib-reloader draft already
uses the equivalent dual declaration (`["rlib", "dylib"]`) for the same reason, one level native.

The app author's source is identical in both cases: one `impl App`, one `wasm_app!` call. The `cfg`
branching is entirely internal to the macro's expansion. The dev side is built; linking natively for release
is not.

---

## 9. v1 scope

**In:** termoxide's `App` trait, `wasm_app!` macro-generated exports, the `postcard`-encoded app value,
signature-checked and wire-schema-checked reload (§2.1, §2.3), opt-in persistence (§7).

**Out of v1, structurally preserved:**

| Capability | What must be true now |
|---|---|
| Partial/diff-based redraw | State and view data cross as generic, opaque bytes — never a fixed per-field layout |
| Backend-agnosticism (ratatui / crossterm / other) | Guest never depends on a terminal backend; ratatui's `Style`/`Rect` in `ViewNode` are stand-ins, to be replaced (§2) |
| Async / multi-threaded host loop | Host↔subprocess communication is pipe-based IPC, already the shape async runtimes expect |
| Component Model / WIT migration | Encoding is generic (`postcard` over any `Serialize` type), not hand-rolled per app |

---

## 10. Unverified — check before relying on

1. Why the hot-reload end-to-end tests fail under `cargo tarpaulin --follow-exec`: the run times out on the
   first build, then, given more time, segfaults — most likely `wasmtime`'s signal handlers under ptrace.
   CI runs them with plain `cargo test` after the coverage run instead.
2. Windows' exact atomic-append guarantee for `OpenOptions::append(true)` under all handle-sharing modes
   — stress-tested for this project's own read/write pattern (§5), which is evidence, not proof; Windows'
   documented guarantee is narrower (a handle opened with only `FILE_APPEND_DATA`) than what Rust's
   cross-platform `.append(true)` is confirmed to request in every case. The §5 lock makes the line-level
   guarantee independent of this. The lock itself is now tested on Windows and Linux: three processes
   append concurrently in the logging tests, and with each line deliberately written in two halves the
   lock kept every line whole, while without it lines interleaved. This also surfaced the read-access
   requirement in §5.
3. Whether `wasmtime`'s cooperative async yielding composes cleanly with the epoch budget in §4.1. It needs
   `Config::async_support(true)` + `.call_async()`, and `Store::epoch_deadline_async_yield_and_update(...)`
   in place of the trap — yielding and the budget would then have to be counted separately, since an
   exhausted deadline no longer traps. Reasoned from documented behavior, not yet built or tested here.

---

## 11. Glossary

**Generation** — one loaded version of the app's `logic` module; a reload produces a new one.
**Guest** — the app's `logic` compiled to a `wasm32-wasip2` component, running inside `wasmtime`.
**Host** — the outer, framework-owned process that owns the terminal.
**Subprocess** — the framework-controlled child process that hosts `wasmtime`, between host and guest.
**Trap** — a wasm execution fault (OOB memory access, fuel/epoch exhaustion) caught as a Rust `Result::Err`.
**Fuel** — a per-call instruction budget `wasmtime` decrements and checks at compiled backedges.
**Epoch** — a wall-clock-driven alternative to fuel; host increments a counter on a timer, guest checks it.
**State trace** — the file a persistent run appends the app's state to, one entry per recorded change (§7).
