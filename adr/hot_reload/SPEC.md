# Hot reload — implementation spec

**Project:** signal-driven, JSX-like TUI framework in Rust
**Status:** design complete, partially drafted — transport and ABI live-tested across four candidates
(including the naive full-process-restart baseline); the subprocess/IPC layer, fuel/epoch config,
trap-to-recovery wiring, the rebuild step's background thread, and its kill timeout are not yet built.
Reload latency is `cargo build`-dominated and not sub-second — do not promise a specific number anywhere
derived from this document.
**Companion:** `adr/hot_reload/index.html` — same decisions with reasoning and rejected alternatives. This
document states requirements only. When something here looks arbitrary, the ADR says why.

Read §1 and §2 before writing any code. Everything else can be read on demand.

---

## 1. Invariants

These hold everywhere. A change that violates one is a design change, not a bug fix.

1. **The app author's surface is a state type and three functions.** No host code, no FFI, no manual
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
/// The entire app-author surface. `wasm_app!(App)` generates every
/// #[no_mangle] export from this.
pub trait WasmApp {
    type State: Default + Serialize + DeserializeOwned;
    fn on_tick(state: &mut Self::State);
    fn handle_event(state: &mut Self::State, event: Event) -> bool;   // returns quit
    fn build_view(state: &Self::State, width: u16, height: u16) -> Vec<Line>;
}

pub struct Line { pub color: Color, pub text: String }

/// Framework-fixed, not backend-specific. Needs enriching (RGB, bold/
/// italic/underline/dim/strikethrough, background) before v1 — see §9.
pub enum Color { Default, Cyan, Yellow, Green, Magenta }

pub enum Event {
    ChannelReady,
    KeyPress { code: KeyCode, modifiers: Modifiers },
}
```

Host-side, the whole config surface is where the swappable crate lives — no per-app function pointers,
no per-app type parameter:

```rust
pub struct WasmConfig { pub logic_dir: PathBuf }

pub fn run(config: WasmConfig) -> Result<()>;
pub fn run_persistent(config: WasmConfig, snapshot_path: PathBuf) -> Result<()>;
```

### 2.1 Export names and signature checking

```rust
pub const EXPORT_STATE_PTR: &str = "state_ptr";
pub const EXPORT_OUTPUT_PTR: &str = "output_ptr";
pub const EXPORT_ON_TICK: &str = "on_tick";
pub const EXPORT_HANDLE_EVENT: &str = "handle_event";
pub const EXPORT_BUILD_VIEW: &str = "build_view";
pub const EXPORT_MEMORY: &str = "memory";
```

Resolve every export via `Instance::get_typed_func::<Params, Results>(&mut store, NAME)`. This validates
the requested signature against the module's actual declared type at load time and returns `Err` on a
mismatch — do not resolve exports any other way. This checks **function types only**. It says nothing
about the meaning of the bytes passed through linear memory: a stale guest can keep identical function
signatures while changing tag tables, line encoding, buffer sizes or state layout, and both sides will
decode successfully into wrong values. That is what §2.3 is for; this check is half of invariant 3, not
all of it.

### 2.2 State encoding

State crosses the boundary via `postcard::to_slice`/`from_bytes`, generic over any
`S: Serialize + DeserializeOwned`, inside the guest-side macro expansion only. The outer host (§3) holds the resulting
bytes as an opaque `Vec<u8>` and passes them back in unexamined — host code must never contain a type
parameter for app state. Every call that changes state returns the new bytes to the outer host, which
keeps the last few generations (between two and five, number open) so a respawned subprocess (§4.3)
resumes from them and a state that keeps crashing the child can be stepped back.

```rust
pub const STATE_CAPACITY: usize = 4096;
pub const OUTPUT_CAPACITY: usize = 2048;
```

### 2.3 Wire-schema and state compatibility

Two separate fingerprints, checked at two separate points. Neither is covered by §2.1.

```rust
/// Exported by the guest. Covers every byte-level convention the framework defines:
/// tag tables (Color, KeyCode, modifier bits, event kinds), line encoding, buffer
/// capacities, state-encoding convention. Version number or build-time fingerprint — open.
pub const EXPORT_WIRE_SCHEMA: &str = "wire_schema";
```

- **Wire schema** — host reads `wire_schema` before resolving any other export and compares it to its own
  expected value. Mismatch → reject the generation exactly like a failed rebuild (§4.3). `postcard` is
  positional and not self-describing (structs have no field names, enum variants are encoded by
  discriminant), so a reordered or inserted variant decodes *successfully* as the wrong one — it will not
  fail on its own.
- **State schema** — the state blob carries a fingerprint produced by the guest. On mismatch the new
  generation must **not** decode the blob: reset state to `Default` and write a notice to the log (§5).
  The generated decode must not use `unwrap_or_default()`; it has to tell "fingerprint mismatch" apart from
  "decode failed" and report both. Migration between state shapes is out of v1.
- Persistence (§7) has the same problem across a host *restart*: the snapshot file must carry the state
  fingerprint and be checked on startup, or a snapshot from an older build is decoded wrongly.

Not built on any draft. The drafts' generated decode currently does `postcard::from_bytes(bytes)
.unwrap_or_default()`, which loses state silently on a decode error and does nothing about a decode that
succeeds wrongly.

---

## 3. Process & IPC

```text
outer host (owns the terminal, invariant 2)
  │  spawns, framework-authored
  ▼
subprocess (hosts wasmtime; no TTY, renders nothing)
  │  dedicated named pipe, reserved exclusively for this
  ▼
IPC channel: tick/event/build_view requests and responses
```

- The channel is a named pipe. The host creates the server end **before** spawning the child, under a
  name unique to that host process, and passes the name at spawn (argv or environment). The child
  connects. Accept exactly one client; where the platform allows, refuse remote clients. Never route
  logging through it — see §5.
- Windows uses a named pipe. The Unix equivalent (Unix domain socket or FIFO) is not decided. A Unix
  socket file outlives a crash and must be cleaned up.
- Child death is observed as the pipe closing. The pipe is still a byte stream, so messages need framing.
- The subprocess's `stdin` is set to null at spawn, so it can't read the host's keystrokes. Its `stdout`
  and `stderr` are piped to the host, which logs each line through the framework logger (§5), tagged as
  the child's: `stdout` at `info`, `stderr` at `debug`. Never left inherited, which would draw over the
  TUI. Never pointed at the log file directly, which would bypass the §5 lock.
- The harness installs a panic hook that logs through the framework logger.
- Not yet built: every draft as of this writing runs `wasmtime` in the outer host process directly. This
  is the one section of this spec without a working implementation behind it.

---

## 4. Hang & crash protection

Three independent failure modes. None of the three mechanisms below substitutes for another.

### 4.1 Inside a guest call

```rust
// at Engine construction
config.consume_fuel(true);          // or: config.epoch_interruption(true)

// per call
store.set_fuel(BUDGET)?;            // or: store.set_epoch_deadline(N)
```

Cranelift inserts the check at every loop backedge and function entry — a bare `loop {}` traps on budget
exhaustion with no cooperation required from the guest's own code. Not configured in `Engine::default()`;
must be set explicitly. Budget size and what "exceeded" does (discard the call vs. the whole subprocess)
are open.

### 4.2 Around the subprocess round-trip

A flat timeout on "did the subprocess respond to this request," independent of §4.1, also covering the child connecting to the IPC pipe (§3), which has no built-in timeout — catches a hang in
the harness code or in `wasmtime` instantiation itself, neither of which a guest-call-scoped fuel/epoch
budget can see. No heartbeat mechanism: a heartbeat only proves the responding thread is alive, not that
a specific in-flight call will return (identical failure shape to fuel exhaustion, without the hard
bound) — do not build one.

### 4.3 Recovery

Every call site (`on_tick.call(...)`, `handle_event.call(...)`, `build_view.call(...)`) must match on the
`Result` explicitly and, on `Err`, discard the current generation and continue with the previous one —
**not** propagate with `?`. This is currently unimplemented everywhere; every existing draft's `drive()`
loop uses `?` and exits the host process on a trap.

A lost subprocess (the pipe closed, a §4.2 timeout, a malformed message, or the child exiting) is a
different case: the whole child is gone, previous generation included. Kill it if still alive, reap it,
log why, and respawn it in the background on a fresh pipe, starting from the outer host's state (§2.2).
Never reconnect to it. Drop the in-flight call; never retry it. The host loop keeps rendering its last
frame and handling input meanwhile. How many respawns within what window before the host stops and
waits for the next rebuild is open; Erlang/OTP supervisor restart intensity is the reference to study.

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

Triggering a rebuild on file save must run `cargo build` on its own background thread, never inline in the
host loop — the same pattern `termoxide_event`'s input reading and the hot-lib-reloader draft's rebuild
watcher already use. §4.1 and §4.2 don't cover this case (no cooperative checkpoint exists inside `cargo`
to instrument, and there's no subprocess abstraction wrapping a one-shot build invocation), so this is its
own, simpler mechanism: isolate the wait, not bound it.

**This is not, by itself, a complete fix.** Moving the wait off the main thread keeps the host loop
(rendering, input, ticking) responsive even if the build never returns — but the `cargo build` child
process itself is still running, unbounded, on that background thread. Still required, not yet implemented
on any draft:

- **A kill timeout** on the background thread — minutes, not milliseconds, since a real build can
  legitimately take a long time. On expiry, kill the child process outright and report the failure through
  the shared log (§5), using the same "discard and keep the previous generation" handling as an ordinary
  failed build (§4.3).
- **A single-in-flight guard** — a file save arriving while a rebuild is already running must not spawn a
  second, overlapping `cargo build` competing for the same target directory and lock.

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
| Shared log file (same file) | `[app]` lines | Guest → host-provided `log(ptr, len)` import → same file |

- One shared file per host run, tagged per source — not per-source files, not a spawned terminal window.
  Named after the host's start time (no colons) and pid, in `termoxide/<app>/logs/` under `dirs::state_dir()`,
  falling back to `dirs::data_local_dir()`: `%LOCALAPPDATA%\termoxide\<app>\logs\` on Windows,
  `$XDG_STATE_HOME/termoxide/<app>/logs/` on Linux (default under `~/.local/state/`),
  `~/Library/Application Support/termoxide/<app>/logs/` on macOS; `<app>` is the executable's file stem.
  The host passes the path to the child at spawn.
- Framework code logs through the `log` facade (`log::info!` etc.); the backend formats each tagged line and
  commits it under the lock below.
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
- App-side logging never uses `WasiCtxBuilder::inherit_stdio()` — that wires the guest directly to the
  subprocess's own stdio, so its output would reach the log only as the child's relayed `stdout` (§3),
  tagged as the framework's rather than the app's. Use a custom `stdout`/`stderr` writer instead: a
  small buffering sink that accumulates arbitrary partial writes and commits each completed line under
  the same lock as every other writer, regardless of the app's own write pattern. Install a
  `log::Log` backend forwarding to this sink inside the `wasm_app!` macro expansion, so the app author
  uses ordinary `log::info!`/`warn!` — never raw `println!`/`eprintln!`, which were confirmed live to
  compile but silently no-op at runtime on `wasm32-unknown-unknown` with no WASI wired.

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
surface. Not yet wired into any draft — today's guests have zero filesystem/network/env access because
nothing is wired, not because anything is deliberately withheld.

---

## 7. Persistence

```rust
pub fn run_persistent(config: WasmConfig, snapshot_path: PathBuf) -> Result<()> {
    let initial = std::fs::read(&snapshot_path).unwrap_or_default();
    drive(config, initial, |bytes| { let _ = std::fs::write(&snapshot_path, bytes); })
}
```

- Opt-in, layered on the base transport. A reload alone never loses state — persistence exists only for
  surviving a full restart of the host process itself.
- No new encoding step beyond §2.2: the same opaque bytes already crossing the reload boundary are what
  gets written to disk — but the snapshot must carry the state-schema fingerprint (§2.3) and be checked
  on startup.
- `snapshot_path` is fixed, not pid-keyed — a freshly started host (new pid) must be able to find what
  the previous one wrote.

---

## 8. Release builds

A release binary compiles `logic` natively and links it directly into the host — none of §§3–6 ship: no
`wasmtime`, no subprocess, no IPC, no fuel/epoch, no `postcard` marshaling. The distinguishing switch is
target architecture, not build profile — a release-profile `wasm32` build still needs the dev-loop export
path (optimized `logic`, still run through hot-reload during iteration), so `cfg(debug_assertions)` is the
wrong gate.

```rust
// wasm_app!(App) expands to one of these, chosen by the compiler, not the macro:
#[cfg(target_arch = "wasm32")]
mod __termoxide_wasm_exports { /* state_ptr/output_ptr/on_tick/handle_event/build_view, postcard marshaling — §2 */ }

#[cfg(not(target_arch = "wasm32"))]
mod __termoxide_native_bridge { /* near-empty — see below */ }
```

Native linking needs no named-symbol export or runtime resolution at all: once `logic` is just another
crate in the same compilation graph, the host's native entry point is an ordinary generic function —
`run_native::<A: WasmApp>()` calling `A::on_tick(&mut state)` directly. There is nothing for the macro's
native branch to generate beyond what the app's own trait impl already provides.

`logic`'s `[lib] crate-type` must list both `cdylib` (the artifact the dev path loads) and `rlib` (so the
native host can depend on it as an ordinary crate) — not a new pattern; the hot-lib-reloader draft already
uses the equivalent dual declaration (`["rlib", "dylib"]`) for the same reason, one level native.

The app author's source is identical in both cases: one `impl WasmApp for App`, one `wasm_app!(App)` call.
The `cfg` branching is entirely internal to the macro's expansion. Not yet built on any draft.

---

## 9. v1 scope

**In:** the `WasmApp` trait, `wasm_app!` macro-generated exports, `postcard`-encoded generic state,
signature-checked and wire-schema-checked reload (§2.1, §2.3), opt-in persistence (§7).

**Out of v1, structurally preserved:**

| Capability | What must be true now |
|---|---|
| Partial/diff-based redraw | State and view data cross as generic, opaque bytes — never a fixed per-field layout |
| Backend-agnosticism (ratatui / crossterm / other) | Guest never depends on a rendering backend's types; only host translates `Color`/`Line` |
| Async / multi-threaded host loop | Host↔subprocess communication is pipe-based IPC, already the shape async runtimes expect |
| Component Model / WIT migration | Encoding is generic (`postcard` over any `Serialize` type), not hand-rolled per app |

---

## 10. Unverified — check before relying on

1. Richer `Color`/style vocabulary (RGB, bold/italic/underline/dim/strikethrough, background) — shape
   depends on the backend-agnosticism work in §9 landing first; don't lock in a representation before that.
2. Windows' exact atomic-append guarantee for `OpenOptions::append(true)` under all handle-sharing modes
   — stress-tested for this project's own read/write pattern (§5), which is evidence, not proof; Windows'
   documented guarantee is narrower (a handle opened with only `FILE_APPEND_DATA`) than what Rust's
   cross-platform `.append(true)` is confirmed to request in every case. The §5 lock makes the line-level
   guarantee independent of this. The lock itself is now tested on Windows and Linux: three processes
   append concurrently in the logging tests, and with each line deliberately written in two halves the
   lock kept every line whole, while without it lines interleaved. This also surfaced the read-access
   requirement in §5.
3. Whether `wasmtime`'s cooperative async yielding composes cleanly with the fuel budget in §4.1. It needs
   `Config::async_support(true)` + `.call_async()`, `Config::consume_fuel(true)` with fuel supplied, and
   `Store::fuel_async_yield_interval(...)` — the yield interval and the total budget are separate settings,
   and exhausting the total still traps. Reasoned from documented behavior, not yet built or tested here.

---

## 11. Glossary

**Generation** — one loaded version of the app's `logic` module; a reload produces a new one.
**Guest** — the app's compiled `logic` module, running inside `wasmtime`.
**Host** — the outer, framework-owned process that owns the terminal.
**Subprocess** — the framework-controlled child process that hosts `wasmtime`, between host and guest.
**Trap** — a wasm execution fault (OOB memory access, fuel/epoch exhaustion) caught as a Rust `Result::Err`.
**Fuel** — a per-call instruction budget `wasmtime` decrements and checks at compiled backedges.
**Epoch** — a wall-clock-driven alternative to fuel; host increments a counter on a timer, guest checks it.
