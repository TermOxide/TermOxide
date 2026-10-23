# termoxide_e2e

End-to-end tests for the TermOxide main loop. They run a real application, the
`probe` binary, in a pseudo-terminal (PTY) and read its screen back through a
terminal emulator (`vt100`). Test-only: not published, and excluded from
coverage reports.

## What is covered

`tests/pty.rs` checks what only a real terminal shows:

- startup on the alternate screen, with the cursor hidden;
- keys reaching the app, and a resize reaching the next frame;
- raw mode on while running, and the terminal restored after `q` and Ctrl-C;
- a panic in `handle_event` or `track_view` reported on the restored terminal, with its location and stack, and with
  `RUST_BACKTRACE=0`.

The tests only run on unix: they read raw mode from the PTY's termios.

`tests/hot_reload.rs` checks hot reload on the fixture app in `fixtures/reload_app`, which counts presses of `a`:

- an edit to the running app shows on screen, and the count it had survives;
- a broken edit leaves the app running, and fixing it reloads it;
- a new field in the app's state makes it start afresh.

Each test copies the fixture under a name of its own into cargo's test directory (`target/tmp`), builds it there,
and edits the copy, never the fixture. All copies share one target directory, so only the first build compiles the
dependencies. Their logs go to the copy's `state` directory, not the user's.

## Running

```sh
cargo test -p termoxide_e2e
```

The PTY tests take well under a second; the hot-reload tests take about 15 s, and a
couple of minutes the first time, while their dependencies build. Every check polls the emulated screen until it
holds and fails after a deadline, 2 s by default, or 5 minutes for what waits on a build. `TERMOXIDE_E2E_DEADLINE_SECS`
overrides it, for runs that slow the probe down (see below).

## Coverage in CI

Coverage runs `cargo tarpaulin --follow-exec`, so that tarpaulin also traces the
probe processes. Their code paths only run in a real terminal and would
otherwise never be counted: `run_with_app`, the `EventStream` wiring, and
`termoxide_event`'s terminal input reader. The crate's own files (the harness
and the probe) are excluded from the report, in `.github/codecov.yml` and with
`--exclude-files "crates/termoxide_e2e/*"`.

Measured locally (WSL, tarpaulin 0.37.5) on `termoxide`,
`termoxide_rendering` and `termoxide_e2e`:

|                         | without `--follow-exec` | with it            |
|-------------------------|-------------------------|--------------------|
| PTY tests               | 0.8 s                   | 586 s              |
| whole tarpaulin run     | 698 s                   | 1119 s             |
| lines covered           | 396 / 584 (67.8 %)      | 456 / 584 (78.1 %) |

### Limits

- **Slow.** Tarpaulin stops a traced process at every instrumented line, so the probe runs orders of magnitude slower
  (about a minute per PTY test instead of a few milliseconds). This cost is paid on every coverage run that includes
  this crate: a PR changing it, and every push to `main` or even a dependencies from our crates.
- **Deadline.** The default 2 s is far too short for a traced probe, so CI sets `TERMOXIDE_E2E_DEADLINE_SECS=120`.
- **Linux only.** `--follow-exec` relies on ptrace; the PTY tests are unix-only anyway.
- **Tarpaulin is not pinned.** CI installs the latest release; the numbers above are from 0.37.5.

### Possible failures

- `timed out waiting for the first frame; screen:` (empty screen), `raw mode never became …` or `the probe did not
  exit`: the traced probe was slower than the deadline. Raise `TERMOXIDE_E2E_DEADLINE_SECS`; the passing checks don't
  get slower, since they return as soon as they hold.
- `Failed to run tests: Attempting to handle tarpaulin being signaled`: despite its wording, a traced process was
  killed by a signal tarpaulin doesn't handle (it accepts `SIGKILL`, `SIGTRAP`, `SIGCHLD` and `SIGTERM`). The harness
  avoids this by quitting a probe still running with `q` when a test ends; it only falls back to `portable-pty`'s
  `kill`, which sends `SIGHUP`, when the probe doesn't exit in time. If it shows up, a probe was killed that way:
  look for the timeout above in the same run.

### The hot-reload tests stay out of coverage

`tests/hot_reload.rs` is compiled out of tarpaulin's runs (`#![cfg(not(tarpaulin))]`, a cfg tarpaulin sets), and CI
runs it with plain `cargo test` right after tarpaulin, when `termoxide_hot_reload` or this crate changed. Traced with
`--follow-exec`, it doesn't work. Measured locally (WSL, tarpaulin 0.36.0, `TERMOXIDE_E2E_DEADLINE_SECS=120`):

| Run                          | Result                                                                                   |
|------------------------------|------------------------------------------------------------------------------------------|
| plain `cargo test`           | 3 passed, ~15 s (~120 s with a cold target directory)                                    |
| tarpaulin, default `--timeout` | 3 passed in 671 s, then `Timed out waiting for test response`: the cold build gave tarpaulin nothing to trace for over 60 s |
| tarpaulin, `--timeout 900`   | 1 of 3 failed after 933 s, then `A segfault occurred while executing tests`              |

The cargo builds themselves aren't traced. The segfault most likely comes from `wasmtime`, which relies on its own
signal handlers to run compiled code; under ptrace, tarpaulin takes the signal for a crash.

### Matching CI coverage locally

On Linux, from the repository root, with the dev shell (`nix develop`) or any stable toolchain:

```sh
cargo install cargo-tarpaulin   # or: nix shell nixpkgs#cargo-tarpaulin
TERMOXIDE_E2E_DEADLINE_SECS=120 cargo tarpaulin -p termoxide -p termoxide_rendering -p termoxide_e2e \
    --follow-exec --ignore-tests --exclude-files "*/tests/*" --exclude-files "crates/termoxide_e2e/*" \
    --out Html --output-dir ./coverage
```

Pass the crates your change touches with `-p`, as `.github/workflows/pr.yaml` does, or `--workspace --exclude
cargo-extract` to match the baseline run of `.github/workflows/push-coverage.yml`. Leave out `--follow-exec` for a
quick run: the tests still run, but nothing the probe executes is counted.
