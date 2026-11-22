//! Baseline save/load under `target/tpt-benchmark/baselines/`, and regression detection.
//!
//! A baseline is a versioned JSON document holding the results of one run.
//! Regressions are judged statistically: a change counts only when the 95 %
//! confidence intervals of the two runs do not overlap and the mean moved by
//! more than the configured threshold.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::runner::BenchResult;

/// Version of the baseline file format. Bump on any incompatible change.
pub const FORMAT_VERSION: u32 = 1;

/// A named, saved set of results.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    /// Format version the file was written with.
    pub format_version: u32,
    /// The name the baseline was saved under.
    pub name: String,
    /// Version of `tpt-benchmark` that wrote the file.
    pub tool_version: String,
    /// The saved results.
    pub results: Vec<BenchResult>,
}

/// How a benchmark's layer compares with the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Significantly faster (lower) than the baseline.
    Improved,
    /// Significantly slower (higher) than the baseline, beyond the threshold.
    Regressed,
    /// No significant change beyond the threshold.
    Unchanged,
    /// Present now but not in the baseline.
    New,
    /// In the baseline but not produced by this run.
    Missing,
}

/// One benchmark layer compared against the baseline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// Fully qualified benchmark name.
    pub name: String,
    /// Layer name, e.g. `"WallClock"`.
    pub layer: String,
    /// Baseline mean, or `None` when the benchmark is new.
    pub baseline_mean: Option<f64>,
    /// Current mean, or `None` when the benchmark is missing from this run.
    pub current_mean: Option<f64>,
    /// Relative change in percent (`+5.2` means slower), when both sides exist.
    pub delta_pct: Option<f64>,
    /// The outcome.
    pub verdict: Verdict,
}

/// The file that holds baseline `name` under `dir`.
///
/// # Panics
///
/// Never panics. Names containing path separators are rejected by [`save`]
/// and [`load`] before this path is used.
#[must_use]
pub fn path(dir: &Path, name: &str) -> PathBuf {
    dir.join("baselines").join(format!("{name}.json"))
}

fn validate_name(name: &str) -> io::Result<()> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid baseline name `{name}`: use letters, digits, `-`, `_` or `.`"),
        ))
    }
}

/// Saves `results` as baseline `name`, creating directories as needed.
///
/// # Errors
///
/// Returns an error for an invalid name or when the file cannot be written.
pub fn save(dir: &Path, name: &str, results: &[BenchResult]) -> io::Result<()> {
    validate_name(name)?;
    let baseline = Baseline {
        format_version: FORMAT_VERSION,
        name: name.to_owned(),
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        results: results.to_vec(),
    };
    let target = path(dir, name);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&baseline).map_err(io::Error::other)?;
    fs::write(target, json)
}

/// Loads baseline `name` from `dir`.
///
/// # Errors
///
/// Returns an error when the name is invalid, the file is missing or not
/// valid JSON, or its format version is not [`FORMAT_VERSION`].
pub fn load(dir: &Path, name: &str) -> io::Result<Baseline> {
    validate_name(name)?;
    let text = fs::read_to_string(path(dir, name))?;
    let baseline: Baseline =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if baseline.format_version != FORMAT_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "baseline format version {} is not supported (expected {FORMAT_VERSION})",
                baseline.format_version
            ),
        ));
    }
    Ok(baseline)
}

/// Compares the current results with a baseline, layer by layer.
///
/// A change is significant when the 95 % confidence intervals of the two
/// runs do not overlap. It is reported as a regression or improvement only
/// when, in addition, the mean moved by more than `threshold_pct` percent.
/// Higher values are treated as worse, which holds for every layer
/// (time, allocations, bytes and misses alike).
#[must_use]
pub fn compare(current: &[BenchResult], base: &Baseline, threshold_pct: f64) -> Vec<Comparison> {
    let mut out = Vec::new();

    for result in current {
        let old = base.results.iter().find(|b| b.name == result.name);
        for summary in &result.layers {
            let old_summary = old.and_then(|o| o.layers.iter().find(|l| l.layer == summary.layer));
            let Some(old_summary) = old_summary else {
                out.push(Comparison {
                    name: result.name.clone(),
                    layer: summary.layer.clone(),
                    baseline_mean: None,
                    current_mean: Some(summary.mean),
                    delta_pct: None,
                    verdict: Verdict::New,
                });
                continue;
            };

            let delta_pct = if old_summary.mean == 0.0 {
                None
            } else {
                Some((summary.mean - old_summary.mean) / old_summary.mean * 100.0)
            };
            let disjoint =
                summary.ci_high < old_summary.ci_low || summary.ci_low > old_summary.ci_high;
            let verdict = match delta_pct {
                Some(d) if disjoint && d.abs() > threshold_pct => {
                    if d > 0.0 {
                        Verdict::Regressed
                    } else {
                        Verdict::Improved
                    }
                }
                _ => Verdict::Unchanged,
            };
            out.push(Comparison {
                name: result.name.clone(),
                layer: summary.layer.clone(),
                baseline_mean: Some(old_summary.mean),
                current_mean: Some(summary.mean),
                delta_pct,
                verdict,
            });
        }
    }

    // Anything in the baseline that this run did not produce.
    for old in &base.results {
        for old_summary in &old.layers {
            let produced = current.iter().any(|r| {
                r.name == old.name && r.layers.iter().any(|l| l.layer == old_summary.layer)
            });
            if !produced {
                out.push(Comparison {
                    name: old.name.clone(),
                    layer: old_summary.layer.clone(),
                    baseline_mean: Some(old_summary.mean),
                    current_mean: None,
                    delta_pct: None,
                    verdict: Verdict::Missing,
                });
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{StopReason, Summary};

    fn summary(layer: &str, mean: f64, half: f64) -> Summary {
        Summary {
            layer: layer.to_owned(),
            mean,
            std_dev: 0.0,
            median: mean,
            ci_low: mean - half,
            ci_high: mean + half,
            samples: 10,
            values: vec![mean; 10],
            outliers: 0,
            iters_per_batch: 1,
            stop_reason: StopReason::Converged,
            noisy: false,
        }
    }

    fn one(name: &str, s: Summary) -> BenchResult {
        BenchResult {
            name: name.to_owned(),
            layers: vec![s],
        }
    }

    fn baseline_with(results: Vec<BenchResult>) -> Baseline {
        Baseline {
            format_version: FORMAT_VERSION,
            name: "main".to_owned(),
            tool_version: "test".to_owned(),
            results,
        }
    }

    #[test]
    fn significant_slowdown_is_a_regression() {
        let base = baseline_with(vec![one("f", summary("WallClock", 100.0, 1.0))]);
        let cur = [one("f", summary("WallClock", 120.0, 1.0))];
        let c = compare(&cur, &base, 5.0);
        assert_eq!(c[0].verdict, Verdict::Regressed);
        assert!((c[0].delta_pct.unwrap() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn overlapping_intervals_are_not_significant() {
        let base = baseline_with(vec![one("f", summary("WallClock", 100.0, 30.0))]);
        let cur = [one("f", summary("WallClock", 120.0, 30.0))];
        assert_eq!(compare(&cur, &base, 5.0)[0].verdict, Verdict::Unchanged);
    }

    #[test]
    fn small_change_below_threshold_is_unchanged() {
        let base = baseline_with(vec![one("f", summary("WallClock", 100.0, 0.1))]);
        let cur = [one("f", summary("WallClock", 102.0, 0.1))];
        assert_eq!(compare(&cur, &base, 5.0)[0].verdict, Verdict::Unchanged);
    }

    #[test]
    fn speedup_is_an_improvement() {
        let base = baseline_with(vec![one("f", summary("WallClock", 100.0, 1.0))]);
        let cur = [one("f", summary("WallClock", 50.0, 1.0))];
        assert_eq!(compare(&cur, &base, 5.0)[0].verdict, Verdict::Improved);
    }

    #[test]
    fn new_and_missing_benchmarks_are_reported() {
        let base = baseline_with(vec![one("old", summary("WallClock", 1.0, 0.1))]);
        let cur = [one("new", summary("WallClock", 1.0, 0.1))];
        let c = compare(&cur, &base, 5.0);
        assert!(
            c.iter()
                .any(|x| x.name == "new" && x.verdict == Verdict::New)
        );
        assert!(
            c.iter()
                .any(|x| x.name == "old" && x.verdict == Verdict::Missing)
        );
    }

    #[test]
    fn invalid_names_are_rejected() {
        assert!(validate_name("../evil").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name("main-2.0_rc").is_ok());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("tpt-bl-{}", std::process::id()));
        let results = vec![one("f", summary("WallClock", 42.0, 1.0))];
        save(&dir, "rt", &results).unwrap();
        let loaded = load(&dir, "rt").unwrap();
        assert_eq!(loaded.results, results);
        assert_eq!(loaded.format_version, FORMAT_VERSION);
        let _ = fs::remove_dir_all(&dir);
    }
}
