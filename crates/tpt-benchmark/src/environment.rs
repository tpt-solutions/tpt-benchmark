//! Description of the machine and build a result set was measured on.
//!
//! Written into the JSON export (schema v2) so a site or CI job can tell runs
//! apart. Every probe is best effort: a field that cannot be determined is
//! `null` rather than an error, and nothing here ever fails a benchmark run.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::clock::{Clock, ClockSource};

/// Where and when a run happened.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Environment {
    /// Git commit hash (`GITHUB_SHA`, else `git rev-parse HEAD`).
    pub commit: Option<String>,
    /// UTC time the run finished, RFC 3339 (`2026-11-23T04:05:06Z`).
    pub timestamp: String,
    /// Operating system, as in `std::env::consts::OS`.
    pub os: &'static str,
    /// CPU architecture, as in `std::env::consts::ARCH`.
    pub arch: &'static str,
    /// CPU model name, when the OS reports one.
    pub cpu: Option<String>,
    /// Logical cores available to the process.
    pub cores: Option<usize>,
    /// `rustc --version` output.
    pub rustc: Option<String>,
    /// Time source used by the wall-clock layer: `tsc`, `cntvct` or `instant`.
    pub clock_source: &'static str,
    /// `github-actions` on Actions runners, else `local`.
    pub runner: &'static str,
}

impl Environment {
    /// Probes the current machine.
    #[must_use]
    pub fn capture() -> Self {
        Self {
            commit: commit(),
            timestamp: rfc3339_utc(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
            ),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            cpu: cpu_model(),
            cores: std::thread::available_parallelism().ok().map(Into::into),
            rustc: run("rustc", &["--version"]),
            clock_source: match Clock::best().source() {
                ClockSource::Tsc => "tsc",
                ClockSource::CntVct => "cntvct",
                _ => "instant",
            },
            runner: if std::env::var_os("GITHUB_ACTIONS").is_some() {
                "github-actions"
            } else {
                "local"
            },
        }
    }
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn commit() -> Option<String> {
    std::env::var("GITHUB_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| run("git", &["rev-parse", "HEAD"]))
}

fn cpu_model() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        info.lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_owned())
    }
    #[cfg(target_os = "macos")]
    {
        run("sysctl", &["-n", "machdep.cpu.brand_string"])
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("PROCESSOR_IDENTIFIER").ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Formats Unix seconds as `YYYY-MM-DDTHH:MM:SSZ` (proleptic Gregorian, UTC).
fn rfc3339_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil-from-days (Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instants() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn capture_fills_the_always_known_fields() {
        let env = Environment::capture();
        assert_eq!(env.os, std::env::consts::OS);
        assert!(env.timestamp.ends_with('Z') && env.timestamp.len() == 20);
        assert!(["tsc", "cntvct", "instant"].contains(&env.clock_source));
    }
}
