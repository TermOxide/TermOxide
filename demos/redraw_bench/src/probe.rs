//! Reads CPU and memory use from `/proc`, so the measurements need no extra tool.
//!
//! Linux only: the numbers this tool reports are compared on one reference
//! machine rather than across operating systems.

use std::{fs, time::Instant};

use color_eyre::{
    Result,
    eyre::{OptionExt, WrapErr},
};

/// Kernel clock ticks per second in `/proc/<pid>/stat`. Part of the Linux ABI,
/// and 100 whatever frequency the kernel itself runs at.
const USER_HZ: f64 = 100.0;

/// Process name looked for among the ancestors to find the terminal.
const TERMINAL_PROCESS: &str = "kitty";

/// One reading of a process's `/proc/<pid>/stat`.
struct ProcStat {
    name: String,
    parent: u32,
    /// CPU time consumed so far by all the threads, in clock ticks.
    cpu_ticks: u64,
}

fn read_stat(pid: &str) -> Result<ProcStat> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).wrap_err("cannot read /proc stat")?;
    parse_stat(&stat)
}

/// Parse `pid (name) state parent ...`. The name can itself contain spaces and
/// parentheses, so the fields are counted from the last `)`.
fn parse_stat(stat: &str) -> Result<ProcStat> {
    let (start, fields) = stat.rsplit_once(')').ok_or_eyre("malformed /proc stat")?;
    let name = start.split_once('(').ok_or_eyre("malformed /proc stat")?.1.to_string();
    let fields: Vec<&str> = fields.split_whitespace().collect();

    // Offsets: the documented field numbers (ppid 4, utime 14, stime 15) minus 3,
    // since `pid`, `name` and the 1-based numbering are already gone.
    let number = |index: usize| -> Result<u64> {
        let field = fields.get(index).ok_or_eyre("truncated /proc stat")?;
        field.parse().wrap_err("malformed /proc stat field")
    };

    Ok(ProcStat {
        name,
        parent: u32::try_from(number(1)?).wrap_err("parent pid out of range")?,
        cpu_ticks: number(11)? + number(12)?,
    })
}

/// Read one `Vm...` line of `/proc/self/status`, in KiB.
fn memory_kib(field: &str) -> Result<u64> {
    let status = fs::read_to_string("/proc/self/status").wrap_err("cannot read /proc status")?;
    parse_memory_kib(&status, field)
}

fn parse_memory_kib(status: &str, field: &str) -> Result<u64> {
    let line = status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .ok_or_eyre("missing /proc status field")?;
    let value = line.split_whitespace().next().ok_or_eyre("empty /proc status field")?;
    value.parse().wrap_err("malformed /proc status field")
}

/// Find the terminal this process runs in, by walking up its parents.
pub fn find_terminal() -> Option<u32> {
    let mut stat = read_stat("self").ok()?;
    while stat.parent > 1 {
        let parent = stat.parent;
        stat = read_stat(&parent.to_string()).ok()?;
        if stat.name == TERMINAL_PROCESS {
            return Some(parent);
        }
    }
    None
}

/// CPU and memory use at one instant.
pub struct Sample {
    at: Instant,
    own_ticks: u64,
    terminal_ticks: Option<u64>,
    rss_kib: u64,
}

impl Sample {
    /// Read the current state of this process and of `terminal`, if any.
    pub fn take(terminal: Option<u32>) -> Result<Self> {
        Ok(Self {
            at: Instant::now(),
            own_ticks: read_stat("self")?.cpu_ticks,
            terminal_ticks: terminal
                .and_then(|pid| read_stat(&pid.to_string()).ok())
                .map(|stat| stat.cpu_ticks),
            rss_kib: memory_kib("VmRSS")?,
        })
    }
}

/// CPU and memory use over a measured period.
#[derive(Debug, Clone, Copy)]
pub struct Usage {
    /// 100 means one CPU core fully busy.
    pub cpu_percent: f64,
    pub terminal_cpu_percent: Option<f64>,
    pub rss_start_kib: u64,
    pub rss_end_kib: u64,
    /// Highest memory use since the process started.
    pub rss_peak_kib: u64,
}

/// Compare two samples. Must run before the process starts tearing down, since
/// it also reads the current memory peak.
pub fn usage_between(start: &Sample, end: &Sample) -> Result<Usage> {
    let seconds = end.at.duration_since(start.at).as_secs_f64();
    let percent = |ticks: u64| ticks as f64 / USER_HZ / seconds * 100.0;

    Ok(Usage {
        cpu_percent: percent(end.own_ticks - start.own_ticks),
        terminal_cpu_percent: start
            .terminal_ticks
            .zip(end.terminal_ticks)
            .map(|(before, after)| percent(after - before)),
        rss_start_kib: start.rss_kib,
        rss_end_kib: end.rss_kib,
        rss_peak_kib: memory_kib("VmHWM")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_fields_are_counted_from_the_last_parenthesis() {
        let stat = "42 (a (weird) name) S 7 0 0 0 -1 0 0 0 0 0 150 50 0 0 20 0 1 0 0 0 0";

        let parsed = parse_stat(stat).expect("valid stat");

        assert_eq!(parsed.name, "a (weird) name");
        assert_eq!(parsed.parent, 7);
        assert_eq!(parsed.cpu_ticks, 200);
    }

    #[test]
    fn a_truncated_stat_is_an_error() {
        assert!(parse_stat("42 (name) S 7").is_err());
    }

    #[test]
    fn memory_fields_are_read_in_kib() {
        let status = "Name:\tbench\nVmHWM:\t    2048 kB\nVmRSS:\t    1024 kB\n";

        assert_eq!(parse_memory_kib(status, "VmRSS").expect("present"), 1024);
        assert_eq!(parse_memory_kib(status, "VmHWM").expect("present"), 2048);
        assert!(parse_memory_kib(status, "VmPeak").is_err());
    }

    #[test]
    fn this_process_can_be_sampled() {
        let start = Sample::take(None).expect("sample");
        let end = Sample::take(None).expect("sample");

        assert!(start.rss_kib > 0);
        assert!(end.own_ticks >= start.own_ticks);
    }
}
