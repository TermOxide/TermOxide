//! Turns a list of frame times into the numbers the report shows.

use std::time::Duration;

/// Summary of one run's frame times.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameStats {
    pub frames: usize,
    pub mean_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub fps_avg: f64,
    /// Speed during the slowest 1% of frames: the mean of those frame times,
    /// converted to frames per second.
    pub fps_low_1pct: f64,
}

/// Summarize `frame_times`, or return `None` when there are none (an idle app
/// draws nothing, so this is an expected outcome).
pub fn summarize(frame_times: &[Duration]) -> Option<FrameStats> {
    if frame_times.is_empty() {
        return None;
    }

    let mut sorted_ms: Vec<f64> = frame_times.iter().map(|time| time.as_secs_f64() * 1000.0).collect();
    sorted_ms.sort_by(f64::total_cmp);

    let frames = sorted_ms.len();
    let mean_ms = mean(&sorted_ms);
    // Rounded up, so that a short run still counts its slowest frame.
    let slowest = frames.div_ceil(100);
    let low_mean_ms = mean(&sorted_ms[frames - slowest..]);

    Some(FrameStats {
        frames,
        mean_ms,
        p99_ms: sorted_ms[(frames * 99).div_ceil(100) - 1],
        max_ms: sorted_ms[frames - 1],
        fps_avg: 1000.0 / mean_ms,
        fps_low_1pct: 1000.0 / low_mean_ms,
    })
}

fn mean(values: &[f64]) -> f64 { values.iter().sum::<f64>() / values.len() as f64 }

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration { Duration::from_millis(value) }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} is not {expected}");
    }

    #[test]
    fn no_frames_give_no_summary() {
        assert_eq!(summarize(&[]), None);
    }

    #[test]
    fn even_frames_have_a_low_equal_to_the_average() {
        let stats = summarize(&[ms(10); 100]).expect("frames");

        assert_close(stats.fps_avg, 100.0);
        assert_close(stats.fps_low_1pct, 100.0);
    }

    #[test]
    fn one_slow_frame_in_a_hundred_sets_the_low() {
        let mut times = vec![ms(10); 99];
        times.push(ms(100));

        let stats = summarize(&times).expect("frames");

        assert_eq!(stats.frames, 100);
        assert_close(stats.fps_low_1pct, 10.0);
        assert_close(stats.max_ms, 100.0);
        assert_close(stats.mean_ms, 10.9);
    }

    #[test]
    fn the_low_rounds_the_slowest_frames_up() {
        // 150 frames: 1% is 1.5 frames, so the two slowest count.
        let mut times = vec![ms(10); 148];
        times.extend([ms(20), ms(30)]);

        let stats = summarize(&times).expect("frames");

        assert_close(stats.fps_low_1pct, 40.0);
    }

    #[test]
    fn a_single_frame_is_its_own_percentile() {
        let stats = summarize(&[ms(25)]).expect("frames");

        assert_close(stats.p99_ms, 25.0);
        assert_close(stats.fps_low_1pct, 40.0);
    }
}
