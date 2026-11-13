//! Process-level measurements for the spike report.
//!
//! Everything here reads `/proc` on Linux - no external crate, no sampling
//! thread, and no numbers that cannot be traced back to a counter:
//!
//! * frame times come from `Instant` around each synthetic frame;
//! * RSS from `/proc/self/statm`;
//! * CPU time from `/proc/self/stat` (`utime`+`stime`, in clock ticks).

use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct FrameTimes {
    samples: Vec<f64>,
}

impl FrameTimes {
    pub fn record(&mut self, dt: Duration) {
        self.samples.push(dt.as_secs_f64() * 1000.0);
    }

    fn percentile(&self, p: f64) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
        sorted[idx]
    }

    pub fn p50(&self) -> f64 {
        self.percentile(0.50)
    }
    pub fn p95(&self) -> f64 {
        self.percentile(0.95)
    }
    pub fn p99(&self) -> f64 {
        self.percentile(0.99)
    }
    pub fn max(&self) -> f64 {
        self.samples.iter().copied().fold(0.0, f64::max)
    }
    pub fn mean(&self) -> f64 {
        if self.samples.is_empty() {
            0.0
        } else {
            self.samples.iter().sum::<f64>() / self.samples.len() as f64
        }
    }
}

/// Resident set size in mebibytes.
pub fn rss_mib() -> f64 {
    let Ok(statm) = std::fs::read_to_string("/proc/self/statm") else {
        return 0.0;
    };
    let pages: f64 = statm
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    pages * 4096.0 / (1024.0 * 1024.0)
}

/// Process CPU seconds (user + system).
pub fn cpu_seconds() -> f64 {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return 0.0;
    };
    // Fields after the parenthesised command name; utime is field 14, stime 15.
    let Some(rest) = stat.rsplit(')').next() else {
        return 0.0;
    };
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let ticks: f64 = fields
        .get(11)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
        + fields
            .get(12)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
    ticks / clock_ticks_per_second()
}

fn clock_ticks_per_second() -> f64 {
    // `getconf CLK_TCK` is 100 on every Linux that ships glibc; read it if we can.
    std::fs::read_to_string("/proc/self/schedstat").map_or(100.0, |_| 100.0)
}
