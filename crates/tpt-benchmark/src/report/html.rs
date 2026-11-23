//! The single-file HTML report: contents, summary table, baseline chart and
//! one collapsible section per benchmark. All CSS, script and SVG are inline.

use std::fmt::Write as _;

use super::{ci_pct, escape_html, find_comparison, format_value, svg, unit_label};
use crate::baseline::{Comparison, Verdict};
use crate::runner::BenchResult;
use crate::stats::{StopReason, Summary};

pub(super) const STYLE: &str = r#"
:root {
  --bg: #ffffff; --fg: #1f2328; --muted: #59636e; --line: #d0d7de; --card: #f6f8fa;
  --good: #1a7f37; --bad: #cf222e; --accent: #0969da;
  color-scheme: light;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #0d1117; --fg: #e6edf3; --muted: #9198a1; --line: #30363d; --card: #161b22;
    --good: #3fb950; --bad: #f85149; --accent: #58a6ff; color-scheme: dark;
  }
}
:root[data-theme="dark"] {
  --bg: #0d1117; --fg: #e6edf3; --muted: #9198a1; --line: #30363d; --card: #161b22;
  --good: #3fb950; --bad: #f85149; --accent: #58a6ff; color-scheme: dark;
}
* { box-sizing: border-box; }
[hidden] { display: none !important; }
body {
  margin: 0; padding: 16px; background: var(--bg); color: var(--fg);
  font: 15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif;
}
header.page, main { max-width: 1100px; margin: 0 auto; }
h1 { font-size: 1.6rem; margin: 0.5rem 0 0.2rem; }
h2 { font-size: 1.25rem; border-bottom: 1px solid var(--line); padding-bottom: 4px; margin-top: 2rem; }
h3 { font-size: 1.05rem; margin: 0 0 0.4rem; }
a { color: var(--accent); }
.muted { color: var(--muted); }
.note { color: var(--muted); font-style: italic; }
.good { color: var(--good); font-weight: 600; }
.bad { color: var(--bad); font-weight: 600; }
.unit { color: var(--muted); font-weight: normal; font-size: 0.9em; }
.table-wrap { overflow-x: auto; }
table { border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; }
th, td { padding: 6px 10px; border-bottom: 1px solid var(--line); text-align: left; }
th { background: var(--card); font-weight: 600; }
.num { text-align: right; }
.toc ul { columns: 2 260px; padding-left: 1.2rem; }
.toggle {
  all: unset; cursor: pointer; font: inherit; font-weight: 600; color: var(--fg);
}
.toggle::before { content: "\25BE  "; color: var(--muted); }
.toggle[aria-expanded="false"]::before { content: "\25B8  "; }
.toggle:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.layer {
  background: var(--card); border: 1px solid var(--line); border-radius: 8px;
  padding: 12px 16px; margin: 12px 0;
}
.stats {
  display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 8px 16px; margin: 10px 0 0;
}
.stats dt { color: var(--muted); font-size: 0.85rem; }
.stats dd { margin: 0; font-variant-numeric: tabular-nums; }
.charts { display: flex; flex-wrap: wrap; gap: 16px; margin-top: 12px; }
.chart { margin: 0; flex: 1 1 320px; max-width: 100%; color: var(--fg); }
.chart figcaption { color: var(--muted); font-size: 0.9rem; margin-bottom: 4px; }
.chart svg { display: block; width: 100%; height: auto; max-width: 640px; }
.chart text { fill: currentColor; }
"#;

const SCRIPT: &str = r#"
(function () {
  "use strict";
  function setOpen(button, open) {
    var body = document.getElementById(button.getAttribute("aria-controls"));
    button.setAttribute("aria-expanded", open ? "true" : "false");
    if (body) { body.hidden = !open; }
  }
  document.querySelectorAll("[data-toggle]").forEach(function (button) {
    button.addEventListener("click", function () {
      setOpen(button, button.getAttribute("aria-expanded") !== "true");
    });
  });
  function revealHash() {
    var section = document.getElementById(window.location.hash.slice(1));
    var button = section && section.querySelector("[data-toggle]");
    if (button) { setOpen(button, true); }
  }
  window.addEventListener("hashchange", revealHash);
  revealHash();
})();
"#;

/// Builds the complete HTML document.
pub(super) fn render(results: &[BenchResult], comparison: Option<&[Comparison]>) -> String {
    let mut out = String::with_capacity(64 * 1024);
    let benchmarks = results.len();
    let measurements: usize = results.iter().map(|r| r.layers.len()).sum();
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>tpt-benchmark report</title>\n<style>{STYLE}</style>\n</head>\n<body>\n\
         <header class=\"page\">\n<h1>tpt-benchmark report</h1>\n\
         <p class=\"muted\">{benchmarks} benchmarks, {measurements} measurements</p>\n\
         </header>\n<main>\n"
    );
    write_contents(&mut out, results);
    write_summary(&mut out, results, comparison);
    if let Some(comparison) = comparison {
        write_comparison(&mut out, comparison);
    }
    for (index, result) in results.iter().enumerate() {
        write_benchmark(&mut out, index, result, comparison);
    }
    let _ = write!(
        out,
        "</main>\n<script>{SCRIPT}</script>\n</body>\n</html>\n"
    );
    out
}

fn write_contents(out: &mut String, results: &[BenchResult]) {
    out.push_str("<nav class=\"toc\" aria-label=\"Contents\">\n<h2>Contents</h2>\n<ul>\n");
    for (index, result) in results.iter().enumerate() {
        let _ = writeln!(
            out,
            "<li><a href=\"#bench-{index}\">{}</a></li>",
            escape_html(&result.name)
        );
    }
    out.push_str("</ul>\n</nav>\n");
}

fn write_summary(out: &mut String, results: &[BenchResult], comparison: Option<&[Comparison]>) {
    out.push_str(
        "<section id=\"summary\">\n<h2>Summary</h2>\n<div class=\"table-wrap\">\n<table>\n<thead><tr>\
         <th>Benchmark</th><th>Layer</th><th class=\"num\">Mean</th>\
         <th class=\"num\">95% CI</th><th class=\"num\">Samples</th><th>Stopped</th>",
    );
    if comparison.is_some() {
        out.push_str("<th class=\"num\">vs baseline</th>");
    }
    out.push_str("</tr></thead>\n<tbody>\n");
    for (index, result) in results.iter().enumerate() {
        for summary in &result.layers {
            let _ = write!(
                out,
                "<tr><td><a href=\"#bench-{index}\">{name}</a></td><td>{layer}</td>\
                 <td class=\"num\">{mean}</td><td class=\"num\">{spread}</td>\
                 <td class=\"num\">{samples}</td><td>{stop}{noisy}</td>",
                name = escape_html(&result.name),
                layer = escape_html(&summary.layer),
                mean = format_value(&summary.layer, summary.mean),
                spread = spread_text(summary),
                samples = summary.samples,
                stop = stop_label(summary.stop_reason),
                noisy = if summary.noisy { " (noisy)" } else { "" },
            );
            if comparison.is_some() {
                let entry = find_comparison(comparison, &result.name, &summary.layer);
                let _ = write!(out, "<td class=\"num\">{}</td>", delta_html(entry));
            }
            out.push_str("</tr>\n");
        }
    }
    out.push_str("</tbody>\n</table>\n</div>\n</section>\n");
}

fn write_comparison(out: &mut String, comparison: &[Comparison]) {
    out.push_str(
        "<section id=\"baseline\">\n<h2>Baseline comparison</h2>\n\
         <p class=\"muted\">Bars show the change in mean against the baseline; positive is \
         slower. Green bars improved, red bars regressed beyond the threshold, grey bars are \
         unchanged, new or missing. Hover a bar for its value.</p>\n\
         <figure class=\"chart\">\n",
    );
    out.push_str(&svg::comparison(comparison));
    out.push_str("\n</figure>\n");
    let missing: Vec<&Comparison> = comparison
        .iter()
        .filter(|c| c.verdict == Verdict::Missing)
        .collect();
    if !missing.is_empty() {
        out.push_str("<p class=\"muted\">In the baseline but not run:</p>\n<ul>\n");
        for c in missing {
            let _ = writeln!(
                out,
                "<li>{} [{}]</li>",
                escape_html(&c.name),
                escape_html(&c.layer)
            );
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</section>\n");
}

fn write_benchmark(
    out: &mut String,
    index: usize,
    result: &BenchResult,
    comparison: Option<&[Comparison]>,
) {
    let _ = write!(
        out,
        "<section class=\"bench\" id=\"bench-{index}\">\n\
         <h2><button type=\"button\" class=\"toggle\" data-toggle aria-expanded=\"true\" \
         aria-controls=\"body-{index}\">{name}</button></h2>\n\
         <div class=\"bench-body\" id=\"body-{index}\">\n",
        name = escape_html(&result.name)
    );
    for summary in &result.layers {
        write_layer(out, summary, &result.name, comparison);
    }
    out.push_str("</div>\n</section>\n");
}

fn write_layer(out: &mut String, summary: &Summary, name: &str, comparison: Option<&[Comparison]>) {
    let layer = summary.layer.as_str();
    let fmt = |v: f64| format_value(layer, v);
    let _ = write!(
        out,
        "<div class=\"layer\">\n<h3>{layer} <span class=\"unit\">({unit})</span></h3>\n\
         <p>Mean <strong>{mean}</strong> ±{spread} (95% CI {low} to {high})</p>\n",
        layer = escape_html(layer),
        unit = unit_label(layer),
        mean = fmt(summary.mean),
        spread = spread_percent(summary),
        low = fmt(summary.ci_low),
        high = fmt(summary.ci_high),
    );
    if let Some(entry) = find_comparison(comparison, name, layer) {
        let base = entry.baseline_mean.map_or_else(|| "n/a".to_owned(), fmt);
        let _ = writeln!(
            out,
            "<p>vs baseline: {} (baseline mean {base})</p>",
            delta_html(Some(entry))
        );
    }
    out.push_str("<dl class=\"stats\">\n");
    let rows = [
        ("Std dev", fmt(summary.std_dev)),
        ("Median", fmt(summary.median)),
        ("Samples", summary.samples.to_string()),
        ("Outliers rejected", summary.outliers.to_string()),
        ("Iterations per batch", summary.iters_per_batch.to_string()),
        ("Stopped", stop_label(summary.stop_reason).to_owned()),
    ];
    for (label, value) in rows {
        let _ = writeln!(out, "<div><dt>{label}</dt><dd>{value}</dd></div>");
    }
    out.push_str("</dl>\n");
    if summary.noisy {
        out.push_str("<p class=\"note\">Noisy environment detected for this layer.</p>\n");
    }
    let _ = write!(
        out,
        "<div class=\"charts\">\n\
         <figure class=\"chart\"><figcaption>Distribution of samples</figcaption>{hist}</figure>\n\
         <figure class=\"chart\"><figcaption>Sample trend, least-squares line</figcaption>{trend}</figure>\n\
         </div>\n\
         <p class=\"muted\">Sample trend slope: {slope} per sample.</p>\n</div>\n",
        hist = svg::histogram(&summary.values, &fmt),
        trend = svg::trend(&summary.values, &fmt),
        slope = fmt(svg::trend_slope(&summary.values)),
    );
}

/// `±x.x%` for the table, or `±n/a` when the CI is not meaningful.
fn spread_text(summary: &Summary) -> String {
    format!("±{}", spread_percent(summary))
}

fn spread_percent(summary: &Summary) -> String {
    ci_pct(summary).map_or_else(|| "n/a".to_owned(), |pct| format!("{pct:.1}%"))
}

fn delta_html(entry: Option<&Comparison>) -> String {
    let Some(entry) = entry else {
        return "-".to_owned();
    };
    match (entry.verdict, entry.delta_pct) {
        (Verdict::New, _) => "new".to_owned(),
        (verdict, Some(delta)) => {
            let class = match verdict {
                Verdict::Improved => " class=\"good\"",
                Verdict::Regressed => " class=\"bad\"",
                _ => "",
            };
            format!("<span{class}>{delta:+.1}%</span>")
        }
        (_, None) => "-".to_owned(),
    }
}

const fn stop_label(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Converged => "converged",
        StopReason::MaxSamples => "sample cap",
        StopReason::MaxTime => "time cap",
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::summary;
    use super::*;

    const SVG_NS: &str = "http://www.w3.org/2000/svg";

    fn results() -> Vec<BenchResult> {
        vec![
            BenchResult {
                name: "matrix::multiply/128".into(),
                layers: vec![
                    summary("WallClock", 1_500.0, 15.0),
                    summary("Allocations", 2.0, 0.0),
                ],
            },
            BenchResult {
                name: "<b>&\"quoted\"</b>".into(),
                layers: vec![summary("WallClock", 40.0, 1.0)],
            },
        ]
    }

    fn comparison() -> Vec<Comparison> {
        vec![
            Comparison {
                name: "matrix::multiply/128".into(),
                layer: "WallClock".into(),
                baseline_mean: Some(1_700.0),
                current_mean: Some(1_500.0),
                delta_pct: Some(-11.8),
                verdict: Verdict::Improved,
            },
            Comparison {
                name: "gone".into(),
                layer: "WallClock".into(),
                baseline_mean: Some(3.0),
                current_mean: None,
                delta_pct: None,
                verdict: Verdict::Missing,
            },
        ]
    }

    /// Every `http`/`https` URL in the document.
    fn urls(html: &str) -> Vec<&str> {
        html.match_indices("http")
            .map(|(start, _)| {
                let rest = &html[start..];
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '<')
                    .unwrap_or(rest.len());
                &rest[..end]
            })
            .collect()
    }

    #[test]
    fn has_no_external_references() {
        let html = render(&results(), Some(&comparison()));
        for url in urls(&html) {
            assert_eq!(url, SVG_NS, "unexpected URL in report: {url}");
        }
        assert!(!html.contains("src="));
        assert!(!html.contains("<link"));
    }

    #[test]
    fn escapes_benchmark_names() {
        let html = render(&results(), None);
        assert!(html.contains("&lt;b&gt;&amp;&quot;quoted&quot;&lt;/b&gt;"));
        assert!(!html.contains("<b>&\"quoted\""));
    }

    #[test]
    fn has_contents_summary_and_anchored_sections() {
        let html = render(&results(), Some(&comparison()));
        assert!(html.contains("href=\"#bench-0\""));
        assert!(html.contains("id=\"bench-0\""));
        assert!(html.contains("id=\"bench-1\""));
        assert!(html.contains("id=\"summary\""));
        assert!(html.contains("id=\"baseline\""));
        assert!(html.contains("<span class=\"good\">-11.8%</span>"));
        assert!(html.contains("data-toggle"));
        assert!(html.contains("prefers-color-scheme: dark"));
        assert!(html.contains("In the baseline but not run"));
    }

    #[test]
    fn rendering_is_deterministic() {
        let first = render(&results(), Some(&comparison()));
        let second = render(&results(), Some(&comparison()));
        assert_eq!(first, second);
    }

    #[test]
    fn writes_index_html_into_directory() {
        let dir = std::env::temp_dir().join(format!("tpt-report-test-{}", std::process::id()));
        super::super::write_html(&dir, &results(), None).unwrap();
        let written = std::fs::read_to_string(dir.join("index.html")).unwrap();
        assert!(written.starts_with("<!doctype html>"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
