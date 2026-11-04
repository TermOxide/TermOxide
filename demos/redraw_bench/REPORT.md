# Redraw cost baseline

First measurement of what a TermOxide redraw costs, taken with the tool in this folder. It gives reference numbers to
compare against once the hot reload work lands. See [README.md](README.md) for how each number is produced.

## Conditions

- **Date**: 2026-11-04.
- **Framework code**: `dev/trunk` at `0cc8506d`. The framework was not modified; only `demos/redraw_bench/` was added.
- **Machine**: AMD Ryzen 5 PRO 5675U, 14 GiB of RAM, on mains power, CPU governor and GNOME power profile set to
  `performance`. Heavy applications were closed and the machine was left untouched during the run.
- **Terminal**: kitty 0.41.1 with its default settings (`--config NONE`), one window per measurement.
- **Protocol**: 3 runs per measurement, 120 s in `live` mode and 30 s in `cost` mode, the first 5 s ignored. Values below
  are the mean of the 3 runs. The frame rate differed by at most 0.2 FPS from one run to another.

The numbers compare with each other on this machine only.

## Results

### `live` mode: the real framework loop

The loop limits itself to about 60 FPS, which is a frame every 16.7 ms.

80x24 terminal:

| Scenario  | FPS   | 1% low | App CPU | kitty CPU |
|-----------|-------|--------|---------|-----------|
| `idle`    | 0     |        | 1.0 %   | 0.0 %     |
| `typical` | 10.0  | 9.9    | 1.0 %   | 1.0 %     |
| `small`   | 58.4  | 54.2   | 2.3 %   | 4.1 %     |
| `full`    | 57.8  | 53.7   | 3.3 %   | 7.2 %     |
| `many`    | 57.8  | 53.8   | 3.3 %   | 4.3 %     |

200x50 terminal:

| Scenario  | FPS   | 1% low | App CPU | kitty CPU |
|-----------|-------|--------|---------|-----------|
| `idle`    | 0     |        | 1.0 %   | 0.0 %     |
| `typical` | 10.0  | 9.9    | 1.4 %   | 1.1 %     |
| `small`   | 57.3  | 53.1   | 4.2 %   | 4.5 %     |
| `full`    | 54.6  | 50.9   | 7.5 %   | 16.1 %    |
| `many`    | 57.1  | 52.8   | 4.9 %   | 4.5 %     |

CPU is expressed in percent of one core. The longest frame of any run was 20.5 ms.

### `cost` mode: the renderer without the frame limit

Time to build and draw one frame, including the write to the terminal.

| Scenario | 80x24 per frame | 80x24 FPS | 200x50 per frame | 200x50 FPS |
|----------|-----------------|-----------|------------------|------------|
| `small`  | 0.039 ms        | about 25 800 | 0.203 ms      | about 4 900 |
| `full`   | 0.128 ms        | about 7 800  | 0.667 ms      | about 1 500 |
| `many`   | 0.109 ms        | about 9 200  | 0.272 ms      | about 3 700 |

The longest frame of any `cost` run was 4.7 ms. The 1% low of these runs is in `results.csv` and varies between runs.

### Memory

The memory held in RAM peaked at 3.0 to 3.3 MiB in 80x24 and 4.0 to 4.8 MiB in 200x50. Between the start and the end of
a 120 s run, it grew by 16 KiB at most.

## Not measured

- **Keyboard and mouse input**: the scenarios write a signal directly. A frame rate under a flow of input events is not
  covered.
- **Other terminals and other machines**: only kitty on one laptop.
- **Hot reload**: it is not part of `dev/trunk` yet. These numbers are the reference to compare it against, with the
  tool run again on the same machine.
