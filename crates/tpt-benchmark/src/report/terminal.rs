//! The terminal table: one row per benchmark, one column per active layer.

use super::{active_layers, ci_pct, find_comparison, format_value, unit_label};
use crate::baseline::{Comparison, Verdict};
use crate::runner::BenchResult;

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

struct Cell {
    text: String,
    style: Option<&'static str>,
}

impl Cell {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
        }
    }
}

pub(super) fn render(
    results: &[BenchResult],
    comparison: Option<&[Comparison]>,
    color: bool,
) -> String {
    if results.is_empty() {
        return "no benchmarks were run\n".to_owned();
    }
    let layers = active_layers(results);

    let mut headers = vec![Cell::plain("Benchmark")];
    for layer in &layers {
        headers.push(Cell::plain(format!("{layer} ({})", unit_label(layer))));
        if comparison.is_some() {
            headers.push(Cell::plain("vs baseline"));
        }
    }

    let mut rows = Vec::with_capacity(results.len());
    for result in results {
        let mut row = vec![Cell::plain(result.name.as_str())];
        for layer in &layers {
            row.push(value_cell(result, layer));
            if comparison.is_some() {
                row.push(delta_cell(comparison, &result.name, layer));
            }
        }
        rows.push(row);
    }

    let mut widths: Vec<usize> = headers.iter().map(|c| width(&c.text)).collect();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(width(&cell.text));
        }
    }
    let rule_len = widths.iter().sum::<usize>() + 2 * (widths.len() - 1);

    let mut out = String::new();
    write_row(&mut out, &headers, &widths, color, true);
    out.push_str(&"-".repeat(rule_len));
    out.push('\n');
    for row in &rows {
        write_row(&mut out, row, &widths, color, false);
    }

    let mut footnotes = Vec::new();
    if results.iter().flat_map(|r| &r.layers).any(|s| s.noisy) {
        footnotes.push("* noisy environment detected: treat these numbers with care".to_owned());
    }
    if let Some(comparison) = comparison {
        for c in comparison.iter().filter(|c| c.verdict == Verdict::Missing) {
            footnotes.push(format!(
                "{} [{}] is in the baseline but was not run",
                c.name, c.layer
            ));
        }
    }
    if !footnotes.is_empty() {
        out.push('\n');
        for note in footnotes {
            out.push_str(&note);
            out.push('\n');
        }
    }
    out
}

fn value_cell(result: &BenchResult, layer: &str) -> Cell {
    let Some(summary) = result.layers.iter().find(|s| s.layer == layer) else {
        return Cell::plain("-");
    };
    let spread = match ci_pct(summary) {
        Some(pct) => format!("±{pct:.1}%"),
        None => "±n/a".to_owned(),
    };
    let noisy = if summary.noisy { " *" } else { "" };
    Cell::plain(format!(
        "{} {spread}{noisy}",
        format_value(layer, summary.mean)
    ))
}

fn delta_cell(comparison: Option<&[Comparison]>, name: &str, layer: &str) -> Cell {
    let Some(entry) = find_comparison(comparison, name, layer) else {
        return Cell::plain("-");
    };
    match (entry.verdict, entry.delta_pct) {
        (Verdict::New, _) => Cell::plain("new"),
        (verdict, Some(delta)) => {
            let style = match verdict {
                Verdict::Improved => Some(GREEN),
                Verdict::Regressed => Some(RED),
                _ => None,
            };
            Cell {
                text: format!("{delta:+.1}%"),
                style,
            }
        }
        (_, None) => Cell::plain("-"),
    }
}

/// Display width, counted in characters (all table text is single-width).
fn width(text: &str) -> usize {
    text.chars().count()
}

fn write_row(out: &mut String, cells: &[Cell], widths: &[usize], color: bool, header: bool) {
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        let padding = " ".repeat(widths[i] - width(&cell.text));
        // The first column holds names and is left-aligned; the rest are numbers.
        let right_aligned = i > 0;
        if right_aligned {
            out.push_str(&padding);
        }
        let style = if !color {
            None
        } else if header {
            Some(BOLD)
        } else {
            cell.style
        };
        match style {
            Some(code) => {
                out.push_str(code);
                out.push_str(&cell.text);
                out.push_str(RESET);
            }
            None => out.push_str(&cell.text),
        }
        if !right_aligned {
            out.push_str(&padding);
        }
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::super::tests::summary;
    use super::*;

    fn results() -> Vec<BenchResult> {
        vec![BenchResult {
            name: "matrix::multiply/128".into(),
            layers: vec![
                summary("WallClock", 1_500.0, 15.0),
                summary("Allocations", 2.0, 0.0),
            ],
        }]
    }

    fn comparison(verdict: Verdict, delta: Option<f64>) -> Vec<Comparison> {
        vec![Comparison {
            name: "matrix::multiply/128".into(),
            layer: "WallClock".into(),
            baseline_mean: Some(1_700.0),
            current_mean: Some(1_500.0),
            delta_pct: delta,
            verdict,
        }]
    }

    #[test]
    fn table_has_a_column_per_layer_and_units() {
        let text = render(&results(), None, false);
        assert!(text.contains("Benchmark"));
        assert!(text.contains("WallClock (time/iter)"));
        assert!(text.contains("Allocations (allocs/iter)"));
        assert!(text.contains("1.50 µs ±1.0%"));
        assert!(text.contains("2.00 ±0.0%"));
    }

    #[test]
    fn no_color_means_no_escape_sequences() {
        let cmp = comparison(Verdict::Improved, Some(-14.5));
        let text = render(&results(), Some(&cmp), false);
        assert!(!text.contains('\x1b'));
        assert!(text.contains("-14.5%"));
    }

    #[test]
    fn deltas_are_coloured_by_verdict() {
        let improved = comparison(Verdict::Improved, Some(-14.5));
        let text = render(&results(), Some(&improved), true);
        assert!(text.contains("\x1b[32m-14.5%\x1b[0m"));

        let regressed = comparison(Verdict::Regressed, Some(5.2));
        let text = render(&results(), Some(&regressed), true);
        assert!(text.contains("\x1b[31m+5.2%\x1b[0m"));

        let unchanged = comparison(Verdict::Unchanged, Some(0.4));
        let text = render(&results(), Some(&unchanged), true);
        assert!(text.contains("+0.4%"));
        assert!(!text.contains("\x1b[31m+0.4%"));
        assert!(!text.contains("\x1b[32m+0.4%"));
    }

    #[test]
    fn missing_benchmarks_are_listed_as_footnotes() {
        let cmp = vec![Comparison {
            name: "gone".into(),
            layer: "WallClock".into(),
            baseline_mean: Some(1.0),
            current_mean: None,
            delta_pct: None,
            verdict: Verdict::Missing,
        }];
        let text = render(&results(), Some(&cmp), false);
        assert!(text.contains("gone [WallClock] is in the baseline but was not run"));
    }
}
