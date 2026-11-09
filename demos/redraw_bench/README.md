# redraw_bench

Measures what a TermOxide redraw costs today: frames per second, 1% low, CPU and memory. The framework itself is not
modified, so the numbers describe the code as it ships. Linux only, because CPU and memory are read from `/proc`.

## Running it

Plug the laptop in, close heavy applications, then:

```sh
./run.sh
```

Every measurement opens its own [kitty](https://sw.kovidgoyal.net/kitty/) window, using kitty's default settings. The
full set takes about 1 hour 15 minutes. Pressing `q` in a window abandons that measurement and records it in
`errors.log`, so the machine should stay untouched during the run.

Results land in `results/<date>-<commit>/`, which git ignores:

- `results.csv`: one line per measurement.
- `raw/`: the time of every frame, to compute any other statistic later.
- `machine.txt`: the commit, the hardware, and whether the machine was on battery.

For a single measurement, build the tool, then run it from this folder in a terminal that already has the requested
size:

```sh
cargo build --release
target/release/redraw_bench live small --cols 80 --rows 24 --seconds 120 --warmup 5 --run 1 --out results/manual
```

`BENCH_RUNS`, `BENCH_LIVE_SECONDS`, `BENCH_COST_SECONDS` and `BENCH_WARMUP` shorten `run.sh` when checking the tool
itself. Numbers from a shortened run are not meant to be compared.

## What is measured

Two modes, because the framework limits itself to about 60 frames per second, and a speed limit hides how much margin
is left under it:

- **`live`**: runs the real framework loop. Reports the speed, CPU and memory an application gets.
- **`cost`**: calls the renderer directly, with no frame limit. Reports how long one frame takes, so `fps_avg` is the
  speed the renderer could reach on its own. Sending the frame to the terminal is part of that time, so a slow terminal
  counts.

The situations, each one in an 80x24 and a 200x50 terminal:

| Scenario  | What changes on screen                             | How often                  | Modes        |
|-----------|----------------------------------------------------|----------------------------|--------------|
| `idle`    | Nothing                                            | Never                      | `live`       |
| `typical` | One line out of three, like the demo apps          | 10 times per second        | `live`       |
| `small`   | One line out of three                              | As often as possible       | `live`, `cost` |
| `full`    | Every cell                                         | As often as possible       | `live`, `cost` |
| `many`    | Two cells, in a tree of 1000 nodes                 | As often as possible       | `live`, `cost` |

In `live` mode, "as often as possible" means writing a signal every millisecond. The CPU reported for those scenarios
includes the cost of these writes.

## Reading the columns

- **`frame_ms_mean`, `frame_ms_p99`, `frame_ms_max`**: time per frame in milliseconds. In `live` mode, the time between
  the start of two frames. In `cost` mode, the time to build and draw one frame.
- **`fps_avg`**: 1000 divided by `frame_ms_mean`.
- **`fps_low_1pct`**: speed during the slowest 1% of frames, that is the mean time of those frames converted to frames
  per second. It shows the stutter an average hides. An `idle` run draws no frame, so these columns stay empty.
- **`cpu_percent`**: CPU used by the application, all threads included. 100 means one core fully busy, not the whole
  processor. Only reported in `live` mode, since `cost` keeps a core busy on purpose.
- **`terminal_cpu_percent`**: same unit, for the kitty process displaying the application.
- **`rss_start_kib`, `rss_end_kib`, `rss_peak_kib`**: memory held in RAM at the start and end of the measurement, and
  its peak. An end value that keeps growing with the duration would point to a leak.

The first seconds of each run (`--warmup`) are not counted.

## Limits

- The numbers only compare with each other on one machine: `machine.txt` says which one.
- Windows of the requested size depend on the screen. A measurement refuses to start in a terminal of another size.
- Changes driven by keyboard or mouse input are not measured: the scenarios write a signal directly.
