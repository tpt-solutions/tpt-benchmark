//! Terminal and HTML report rendering.
//!
//! The terminal output is one table with a column per active layer, plus
//! optional baseline deltas. The HTML report is a single self-contained file:
//! CSS, a small script and inline SVG charts drawn with plotters' SVG backend.

mod html;
mod svg;
mod terminal;
#[cfg(windows)]
#[allow(unsafe_code)]
mod win_console;

use std::fs;
use std::io;
use std::path::Path;

use crate::baseline::Comparison;
use crate::runner::BenchResult;
use crate::stats::Summary;

/// Renders results as a table, with baseline deltas when a comparison is given.
///
/// When `color` is `true`, deltas are coloured with ANSI escapes (green for
/// improvements, red for regressions). When `color` is `false` the output
/// contains no escape sequences. On Windows, `color = true` also enables
/// ANSI processing on the stdout console, if stdout is a console.
#[must_use]
pub(crate) fn render_terminal(
    results: &[BenchResult],
    comparison: Option<&[Comparison]>,
    color: bool,
) -> String {
    prepare_terminal(color);
    terminal::render(results, comparison, color)
}

/// Writes the HTML report into `dir` as `index.html`.
///
/// # Errors
///
/// Returns an error if the directory cannot be created or the file cannot be
/// written.
pub(crate) fn write_html(
    dir: &Path,
    results: &[BenchResult],
    comparison: Option<&[Comparison]>,
) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join("index.html"), html::render(results, comparison))
}

/// Building blocks for `tpt-site`. Not part of the stable API.
#[doc(hidden)]
pub mod site_support {
    /// The report's CSS, including the light and dark themes.
    #[must_use]
    pub fn css() -> &'static str {
        super::html::STYLE
    }

    /// Histogram of per-iteration values for `layer`, as inline SVG.
    #[must_use]
    pub fn histogram_svg(layer: &str, values: &[f64]) -> String {
        super::svg::histogram(values, &|v| super::format_value(layer, v))
    }

    /// Mean and 95 % interval per run, oldest first. Each point is
    /// `(label, mean, ci_low, ci_high)`.
    #[must_use]
    pub fn history_svg(layer: &str, points: &[(String, f64, f64, f64)]) -> String {
        super::svg::history(points, &|v| super::format_value(layer, v))
    }

    /// Formats a per-iteration value with a unit scaled to its magnitude.
    #[must_use]
    pub fn format_value(layer: &str, value: f64) -> String {
        super::format_value(layer, value)
    }

    /// Display unit for a layer, such as `time/iter`.
    #[must_use]
    pub fn unit_label(layer: &str) -> &'static str {
        super::unit_label(layer)
    }

    /// Escapes text for HTML.
    #[must_use]
    pub fn escape_html(text: &str) -> String {
        super::escape_html(text)
    }
}

#[cfg(windows)]
fn prepare_terminal(color: bool) {
    if color {
        // Best effort: when stdout is not a console this simply fails.
        let _ = win_console::enable_virtual_terminal();
    }
}

#[cfg(not(windows))]
fn prepare_terminal(_color: bool) {}

/// Layer names in order of first appearance across all results.
pub(crate) fn active_layers(results: &[BenchResult]) -> Vec<String> {
    let mut layers: Vec<String> = Vec::new();
    for result in results {
        for summary in &result.layers {
            if !layers.contains(&summary.layer) {
                layers.push(summary.layer.clone());
            }
        }
    }
    layers
}

/// Display unit for a layer's per-iteration values.
pub(crate) fn unit_label(layer: &str) -> &'static str {
    match layer {
        "WallClock" => "time/iter",
        "AllocBytes" => "bytes/iter",
        "Allocations" => "allocs/iter",
        "HwInstructions" => "instructions/iter",
        "HwL1Misses" => "L1 misses/iter",
        "HwL2Misses" => "L2 misses/iter",
        "HwBranchMisses" => "branch misses/iter",
        _ => "value/iter",
    }
}

/// Formats a per-iteration value with a unit scaled to its magnitude.
pub(crate) fn format_value(layer: &str, value: f64) -> String {
    if !value.is_finite() {
        return "n/a".to_owned();
    }
    match layer {
        "WallClock" => format_nanos(value),
        "AllocBytes" => format_bytes(value),
        _ => format_count(value),
    }
}

fn format_nanos(ns: f64) -> String {
    let abs = ns.abs();
    if abs < 1e3 {
        format!("{ns:.2} ns")
    } else if abs < 1e6 {
        format!("{:.2} µs", ns / 1e3)
    } else if abs < 1e9 {
        format!("{:.2} ms", ns / 1e6)
    } else {
        format!("{:.2} s", ns / 1e9)
    }
}

fn format_count(value: f64) -> String {
    const SCALES: [(f64, &str); 4] = [(1e12, "T"), (1e9, "G"), (1e6, "M"), (1e3, "k")];
    for (scale, suffix) in SCALES {
        if value.abs() >= scale {
            return format!("{:.2}{suffix}", value / scale);
        }
    }
    format!("{value:.2}")
}

fn format_bytes(bytes: f64) -> String {
    const SCALES: [(f64, &str); 3] = [
        (1024.0 * 1024.0 * 1024.0, "GiB"),
        (1024.0 * 1024.0, "MiB"),
        (1024.0, "KiB"),
    ];
    for (scale, suffix) in SCALES {
        if bytes.abs() >= scale {
            return format!("{:.2} {suffix}", bytes / scale);
        }
    }
    format!("{bytes:.1} B")
}

/// Half-width of the 95 % confidence interval as a percentage of the mean.
pub(crate) fn ci_pct(summary: &Summary) -> Option<f64> {
    let half = (summary.ci_high - summary.ci_low) / 2.0;
    if half.is_finite() && summary.mean.is_finite() && summary.mean != 0.0 {
        Some(half.abs() / summary.mean.abs() * 100.0)
    } else {
        None
    }
}

/// The comparison entry for a benchmark and layer, if any.
pub(crate) fn find_comparison<'a>(
    comparison: Option<&'a [Comparison]>,
    name: &str,
    layer: &str,
) -> Option<&'a Comparison> {
    comparison?
        .iter()
        .find(|c| c.name == name && c.layer == layer)
}

/// Escapes text for use in HTML or XML element content and attribute values.
pub(crate) fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::StopReason;

    /// A synthetic summary with a symmetric CI of `half` around `mean`.
    pub(crate) fn summary(layer: &str, mean: f64, half: f64) -> Summary {
        Summary {
            layer: layer.to_owned(),
            mean,
            std_dev: half,
            median: mean,
            ci_low: mean - half,
            ci_high: mean + half,
            samples: 4,
            values: vec![
                mean - half,
                mean - half / 2.0,
                mean + half / 2.0,
                mean + half,
            ],
            outliers: 0,
            iters_per_batch: 1000,
            stop_reason: StopReason::Converged,
            noisy: false,
            throughput: None,
        }
    }

    #[test]
    fn scales_nanoseconds() {
        assert_eq!(format_value("WallClock", 12.345), "12.35 ns");
        assert_eq!(format_value("WallClock", 1_500.0), "1.50 µs");
        assert_eq!(format_value("WallClock", 2_500_000.0), "2.50 ms");
        assert_eq!(format_value("WallClock", 3_000_000_000.0), "3.00 s");
    }

    #[test]
    fn scales_counts_and_bytes() {
        assert_eq!(format_value("Allocations", 2_000.0), "2.00k");
        assert_eq!(format_value("HwInstructions", 4_500_000.0), "4.50M");
        assert_eq!(format_value("AllocBytes", 2048.0), "2.00 KiB");
        assert_eq!(format_value("AllocBytes", 10.0), "10.0 B");
        assert_eq!(format_value("WallClock", f64::NAN), "n/a");
    }

    #[test]
    fn ci_percentage_is_relative_half_width() {
        let s = summary("WallClock", 100.0, 5.0);
        let pct = ci_pct(&s).unwrap();
        assert!((pct - 5.0).abs() < 1e-9);
    }

    #[test]
    fn escapes_markup_characters() {
        assert_eq!(
            escape_html("<a href=\"x\">&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn render_terminal_without_color_has_no_escapes() {
        let results = vec![BenchResult {
            name: "bench".into(),
            layers: vec![summary("WallClock", 100.0, 1.0)],
        }];
        let baseline = vec![Comparison {
            name: "bench".into(),
            layer: "WallClock".into(),
            baseline_mean: Some(120.0),
            current_mean: Some(100.0),
            delta_pct: Some(-14.5),
            verdict: crate::baseline::Verdict::Improved,
        }];
        let text = render_terminal(&results, Some(&baseline), false);
        assert!(!text.contains('\x1b'));
        assert!(text.contains("-14.5%"));
    }

    #[test]
    fn active_layers_keep_first_seen_order() {
        let results = vec![
            BenchResult {
                name: "a".into(),
                layers: vec![
                    summary("WallClock", 1.0, 0.1),
                    summary("Allocations", 0.0, 0.0),
                ],
            },
            BenchResult {
                name: "b".into(),
                layers: vec![summary("Allocations", 1.0, 0.0)],
            },
        ];
        assert_eq!(active_layers(&results), vec!["WallClock", "Allocations"]);
    }
}
