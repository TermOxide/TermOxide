//! Measures what a TermOxide redraw costs. See `README.md` for how to run it
//! and what each number means.
//!
//! Two measurements, because the framework's frame limit hides part of the answer:
//!
//! - `live` runs the real loop through [`run_with_app`] and reports the speed, CPU and memory an application gets.
//! - `cost` calls the renderer directly, with no frame limit, and reports how long one frame takes.

mod probe;
mod scene;
mod stats;

use std::{
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::{Write as _, stdout},
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, Instant},
};

use any_spawner::Executor;
use color_eyre::{
    Result,
    eyre::{WrapErr, bail, eyre},
};
use probe::{Sample, Usage};
use ratatui::{Terminal, backend::CrosstermBackend};
use scene::{BenchApp, FrameClock, Scenario, is_quit, view};
use stats::{FrameStats, summarize};
use termoxide::run_with_app;
use termoxide_event::{EventStream, EventStreamConfig};
use termoxide_rendering::renderer::Renderer;

/// How often saturated scenarios write the signal: faster than the frame limit,
/// so a redraw is always due. Tokio timers have millisecond granularity.
const WRITE_PERIOD: Duration = Duration::from_millis(1);
/// Frames per second to reserve room for: above the framework's frame limit.
const LIVE_FPS_CEILING: usize = 100;

const CSV_HEADER: &str = "mode,scenario,cols,rows,run,seconds,frames,frame_ms_mean,frame_ms_p99,frame_ms_max,fps_avg,\
                          fps_low_1pct,cpu_percent,terminal_cpu_percent,rss_start_kib,rss_end_kib,rss_peak_kib";

const USAGE: &str = "usage: redraw_bench <live|cost> <idle|typical|small|full|many> --cols N --rows N [--seconds N] \
                     [--warmup N] [--run N] [--out DIR]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Live,
    Cost,
}

impl Mode {
    const fn name(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Cost => "cost",
        }
    }
}

#[derive(Debug)]
struct Options {
    mode: Mode,
    scenario: Scenario,
    cols: u16,
    rows: u16,
    seconds: u64,
    warmup: u64,
    run: u32,
    out: PathBuf,
}

fn parse_value<T: FromStr>(flag: &str, value: &str) -> Result<T> {
    value.parse().map_err(|_| eyre!("invalid value `{value}` for {flag}\n{USAGE}"))
}

fn parse_options(mut args: impl Iterator<Item = String>) -> Result<Options> {
    let mode = match args.next().as_deref() {
        Some("live") => Mode::Live,
        Some("cost") => Mode::Cost,
        _ => bail!(USAGE),
    };
    let scenario: Scenario = args.next().ok_or_else(|| eyre!(USAGE))?.parse()?;
    if mode == Mode::Cost && !scenario.is_saturated() {
        bail!("`cost` draws in a closed loop, so it only applies to small, full and many");
    }

    let (mut cols, mut rows) = (None, None);
    let mut options = Options {
        mode,
        scenario,
        cols: 0,
        rows: 0,
        seconds: 120,
        warmup: 5,
        run: 1,
        out: PathBuf::from("results"),
    };

    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| eyre!("missing value for {flag}\n{USAGE}"))?;
        match flag.as_str() {
            "--cols" => cols = Some(parse_value(&flag, &value)?),
            "--rows" => rows = Some(parse_value(&flag, &value)?),
            "--seconds" => options.seconds = parse_value(&flag, &value)?,
            "--warmup" => options.warmup = parse_value(&flag, &value)?,
            "--run" => options.run = parse_value(&flag, &value)?,
            "--out" => options.out = PathBuf::from(value),
            _ => bail!("unknown option {flag}\n{USAGE}"),
        }
    }

    options.cols = cols.ok_or_else(|| eyre!("--cols is required\n{USAGE}"))?;
    options.rows = rows.ok_or_else(|| eyre!("--rows is required\n{USAGE}"))?;
    Ok(options)
}

/// What a run produced.
struct Outcome {
    stats: Option<FrameStats>,
    usage: Option<Usage>,
    /// One CSV line per frame, kept so that any other statistic can be computed later.
    raw: String,
}

/// Run the real framework loop and time the frames it asks for.
async fn run_live(options: &Options) -> Result<Outcome> {
    let capacity = usize::try_from(options.seconds)? * LIVE_FPS_CEILING;
    let clock = FrameClock::with_capacity(capacity);
    let app = BenchApp::new(options.scenario, clock.clone());
    let terminal_pid = probe::find_terminal();

    let writer = options.scenario.is_saturated().then(|| {
        let app = app.clone();
        tokio::task::spawn_local(async move {
            loop {
                tokio::time::sleep(WRITE_PERIOD).await;
                app.advance();
            }
        })
    });

    let measure = async {
        tokio::time::sleep(Duration::from_secs(options.warmup)).await;
        let start = Sample::take(terminal_pid)?;
        clock.start();
        tokio::time::sleep(Duration::from_secs(options.seconds)).await;
        clock.stop();
        let end = Sample::take(terminal_pid)?;
        probe::usage_between(&start, &end)
    };

    // `run_with_app` only returns when asked to quit, so reaching the end of the
    // measurement drops it instead, which restores the terminal.
    let usage = tokio::select! {
        result = run_with_app(app) => {
            result?;
            bail!("the run was stopped from the keyboard");
        }
        usage = measure => usage?,
    };

    if let Some(writer) = writer {
        writer.abort();
    }

    let gaps: Vec<Duration> = clock.starts().windows(2).map(|pair| pair[1].duration_since(pair[0])).collect();
    let mut raw = String::from("frame_ms\n");
    for gap in &gaps {
        writeln!(raw, "{:.6}", gap.as_secs_f64() * 1000.0)?;
    }

    Ok(Outcome { stats: summarize(&gaps), usage: Some(usage), raw })
}

/// Draw in a closed loop, without the frame limit, and time each frame.
///
/// CPU and memory are left out: the loop keeps one core busy on purpose, and the
/// recording grows with the number of frames.
fn run_cost(options: &Options) -> Result<Outcome> {
    let mut timings: Vec<(Duration, Duration)> = Vec::new();
    {
        // Same order as the framework: raw mode first, then the alternate screen.
        let events = EventStream::with_config(EventStreamConfig::default().mouse_capture(false));
        let mut renderer = Renderer::new(Terminal::new(CrosstermBackend::new(stdout()))?)?;
        let viewport = renderer.viewport();

        let began = Instant::now();
        let warmup = Duration::from_secs(options.warmup);
        let total = warmup + Duration::from_secs(options.seconds);
        let mut frame = 0_u64;

        while began.elapsed() < total {
            if events.poll_events().into_iter().any(is_quit) {
                bail!("the run was stopped from the keyboard");
            }

            let frame_start = Instant::now();
            let mut root = view(options.scenario, frame, viewport);
            let built = Instant::now();
            renderer.render_frame(&mut root)?;
            let rendered = Instant::now();

            if began.elapsed() >= warmup {
                timings.push((built.duration_since(frame_start), rendered.duration_since(built)));
            }
            frame += 1;
        }
    }

    let totals: Vec<Duration> = timings.iter().map(|(build, render)| *build + *render).collect();
    let mut raw = String::from("build_ms,render_ms\n");
    for (build, render) in &timings {
        writeln!(raw, "{:.6},{:.6}", build.as_secs_f64() * 1000.0, render.as_secs_f64() * 1000.0)?;
    }

    Ok(Outcome { stats: summarize(&totals), usage: None, raw })
}

/// A cell of the results table, empty when there is nothing to report.
fn cell<T: std::fmt::Display>(value: Option<T>) -> String { value.map_or_else(String::new, |value| value.to_string()) }

fn csv_row(options: &Options, outcome: &Outcome) -> String {
    let stats = outcome.stats.as_ref();
    let usage = outcome.usage.as_ref();
    let fixed = |value: Option<f64>| cell(value.map(|value| format!("{value:.3}")));

    [
        options.mode.name().to_string(),
        options.scenario.name().to_string(),
        options.cols.to_string(),
        options.rows.to_string(),
        options.run.to_string(),
        options.seconds.to_string(),
        stats.map_or(0, |stats| stats.frames).to_string(),
        fixed(stats.map(|stats| stats.mean_ms)),
        fixed(stats.map(|stats| stats.p99_ms)),
        fixed(stats.map(|stats| stats.max_ms)),
        fixed(stats.map(|stats| stats.fps_avg)),
        fixed(stats.map(|stats| stats.fps_low_1pct)),
        fixed(usage.map(|usage| usage.cpu_percent)),
        fixed(usage.and_then(|usage| usage.terminal_cpu_percent)),
        cell(usage.map(|usage| usage.rss_start_kib)),
        cell(usage.map(|usage| usage.rss_end_kib)),
        cell(usage.map(|usage| usage.rss_peak_kib)),
    ]
    .join(",")
}

/// Append the summary to `results.csv` and keep the frame times next to it.
fn write_results(options: &Options, outcome: &Outcome) -> Result<()> {
    let raw_dir = options.out.join("raw");
    fs::create_dir_all(&raw_dir).wrap_err("cannot create the results directory")?;

    let raw_name = format!(
        "{}-{}-{}x{}-run{}.csv",
        options.mode.name(),
        options.scenario.name(),
        options.cols,
        options.rows,
        options.run
    );
    fs::write(raw_dir.join(raw_name), &outcome.raw).wrap_err("cannot write the frame times")?;

    let summary = options.out.join("results.csv");
    let is_new = !summary.exists();
    let mut file = OpenOptions::new().create(true).append(true).open(summary)?;
    if is_new {
        writeln!(file, "{CSV_HEADER}")?;
    }
    writeln!(file, "{}", csv_row(options, outcome))?;
    Ok(())
}

/// Keep a failure where `run.sh` can see it: the terminal window running this
/// process closes as soon as it exits.
fn log_failure(out: &Path, options_line: &str, error: &color_eyre::Report) {
    let _ = fs::create_dir_all(out);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(out.join("errors.log")) {
        let _ = writeln!(file, "{options_line}: {error}");
    }
}

async fn run(options: &Options) -> Result<()> {
    let size = Terminal::new(CrosstermBackend::new(stdout())).and_then(|terminal| terminal.size());
    let size = size.wrap_err("not running in a terminal")?;
    let (cols, rows) = (size.width, size.height);
    if (cols, rows) != (options.cols, options.rows) {
        bail!("expected a {}x{} terminal, found {cols}x{rows}", options.cols, options.rows);
    }

    let outcome = match options.mode {
        Mode::Live => run_live(options).await?,
        Mode::Cost => run_cost(options)?,
    };
    write_results(options, &outcome)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;
    Executor::init_tokio()?;

    let options = parse_options(std::env::args().skip(1))?;
    let local = tokio::task::LocalSet::new();
    let result = local.run_until(run(&options)).await;

    if let Err(error) = &result {
        let name = format!("{} {} run {}", options.mode.name(), options.scenario.name(), options.run);
        log_failure(&options.out, &name, error);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Options> { parse_options(line.split_whitespace().map(String::from)) }

    #[test]
    fn options_are_read_with_their_defaults() {
        let options = parse("live small --cols 80 --rows 24").expect("valid");

        assert_eq!(options.mode, Mode::Live);
        assert_eq!(options.scenario, Scenario::Small);
        assert_eq!((options.cols, options.rows), (80, 24));
        assert_eq!((options.seconds, options.warmup, options.run), (120, 5, 1));
    }

    #[test]
    fn the_terminal_size_is_required() {
        assert!(parse("live small --cols 80").is_err());
    }

    #[test]
    fn cost_refuses_scenarios_with_no_continuous_redraw() {
        assert!(parse("cost idle --cols 80 --rows 24").is_err());
        assert!(parse("cost typical --cols 80 --rows 24").is_err());
    }

    #[test]
    fn a_csv_row_has_one_cell_per_header_column() {
        let options = parse("live idle --cols 80 --rows 24").expect("valid");
        let outcome = Outcome { stats: None, usage: None, raw: String::new() };

        assert_eq!(csv_row(&options, &outcome).split(',').count(), CSV_HEADER.split(',').count());
    }
}
