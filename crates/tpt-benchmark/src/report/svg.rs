//! Charts drawn with plotters' SVG backend, returned as inline SVG strings.
//!
//! Colours are fixed mid-tones that read on both light and dark pages. Axis
//! text is recoloured to `currentColor` by the page CSS, so labels follow the
//! theme. Tooltips are SVG `<title>` children added after drawing.

use plotters::prelude::*;

use super::escape_html;
use crate::baseline::{Comparison, Verdict};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 280;
const BLUE: RGBColor = RGBColor(37, 99, 235);
const RED: RGBColor = RGBColor(220, 38, 38);
const GREEN: RGBColor = RGBColor(22, 163, 74);
const GREY: RGBColor = RGBColor(139, 148, 158);

/// Distribution of the accepted per-iteration values as a histogram.
pub(super) fn histogram(values: &[f64], fmt: &dyn Fn(f64) -> String) -> String {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return note("no samples to plot");
    }
    let min = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let max = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (lo, hi) = if max > min {
        (min, max)
    } else {
        (min - 0.5, max + 0.5)
    };
    let bins = ((finite.len() as f64).sqrt().round() as usize).clamp(1, 30);
    let bin_width = (hi - lo) / bins as f64;
    let mut counts = vec![0_u64; bins];
    for &v in &finite {
        let idx = (((v - lo) / bin_width) as usize).min(bins - 1);
        counts[idx] += 1;
    }
    let y_max = counts.iter().copied().max().unwrap_or(0) as f64 * 1.1 + 1.0;

    let svg = render_svg(|root| {
        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .x_label_area_size(44)
            .y_label_area_size(52)
            .build_cartesian_2d(lo..hi, 0.0..y_max)
            .map_err(|e| e.to_string())?;
        chart
            .configure_mesh()
            .x_desc("time per iteration")
            .y_desc("samples")
            .x_label_formatter(&|v: &f64| fmt(*v))
            .y_label_formatter(&|v: &f64| format!("{v:.0}"))
            .axis_style(GREY)
            .light_line_style(GREY.mix(0.25))
            .draw()
            .map_err(|e| e.to_string())?;
        chart
            .draw_series(counts.iter().enumerate().map(|(i, &count)| {
                let x0 = lo + i as f64 * bin_width;
                Rectangle::new(
                    [(x0, 0.0), (x0 + bin_width, count as f64)],
                    BLUE.mix(0.75).filled(),
                )
            }))
            .map_err(|e| e.to_string())?;
        root.present().map_err(|e| e.to_string())
    });

    let titles: Vec<String> = counts
        .iter()
        .enumerate()
        .map(|(i, count)| {
            let x0 = lo + i as f64 * bin_width;
            format!("{} to {}: {count} samples", fmt(x0), fmt(x0 + bin_width))
        })
        .collect();
    add_titles(&svg, "rect", &titles)
}

/// Accepted values plotted against arrival index, with a least-squares line.
pub(super) fn trend(values: &[f64], fmt: &dyn Fn(f64) -> String) -> String {
    if values.is_empty() {
        return note("no samples to plot");
    }
    let n = values.len();
    let (slope, intercept) = least_squares(values);
    let x_max = (n - 1).max(1) as f64;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let pad = if max > min { (max - min) * 0.1 } else { 0.5 };
    let (y_lo, y_hi) = (min - pad, max + pad);

    let svg = render_svg(|root| {
        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .x_label_area_size(44)
            .y_label_area_size(60)
            .build_cartesian_2d(0.0..x_max, y_lo..y_hi)
            .map_err(|e| e.to_string())?;
        chart
            .configure_mesh()
            .x_desc("sample (arrival order)")
            .y_desc("time per iteration")
            .x_label_formatter(&|v: &f64| format!("{v:.0}"))
            .y_label_formatter(&|v: &f64| fmt(*v))
            .axis_style(GREY)
            .light_line_style(GREY.mix(0.25))
            .draw()
            .map_err(|e| e.to_string())?;
        chart
            .draw_series(
                values
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| Circle::new((i as f64, v), 3, BLUE.mix(0.8).filled())),
            )
            .map_err(|e| e.to_string())?;
        chart
            .draw_series(LineSeries::new(
                [(0.0, intercept), (x_max, intercept + slope * x_max)],
                RED.stroke_width(2),
            ))
            .map_err(|e| e.to_string())?;
        root.present().map_err(|e| e.to_string())
    });

    let titles: Vec<String> = values
        .iter()
        .enumerate()
        .map(|(i, &v)| format!("sample {}: {}", i + 1, fmt(v)))
        .collect();
    add_titles(&svg, "circle", &titles)
}

/// Slope of the least-squares line through `values` against their index, in
/// value units per sample. Zero when there are fewer than two values.
pub(super) fn trend_slope(values: &[f64]) -> f64 {
    least_squares(values).0
}

/// Horizontal bars of the relative change against the baseline.
pub(super) fn comparison(comparison: &[Comparison]) -> String {
    let entries: Vec<&Comparison> = comparison
        .iter()
        .filter(|c| c.delta_pct.is_some_and(f64::is_finite))
        .collect();
    if entries.is_empty() {
        return note("no baseline deltas to plot");
    }
    let n = entries.len();
    let bound = entries
        .iter()
        .filter_map(|c| c.delta_pct)
        .map(f64::abs)
        .fold(0.0_f64, f64::max)
        .max(1.0)
        * 1.1;
    // The first entry is drawn at the top.
    let bars: Vec<(f64, f64, RGBColor)> = entries
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let delta = c.delta_pct.unwrap_or(0.0);
            let colour = match c.verdict {
                Verdict::Improved => GREEN,
                Verdict::Regressed => RED,
                _ => GREY,
            };
            ((n - 1 - i) as f64, delta, colour)
        })
        .collect();

    let svg = render_svg(|root| {
        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .x_label_area_size(44)
            .build_cartesian_2d(-bound..bound, -0.5..(n as f64 - 0.5))
            .map_err(|e| e.to_string())?;
        chart
            .configure_mesh()
            .disable_y_mesh()
            .disable_y_axis()
            .x_desc("change in mean vs baseline")
            .x_label_formatter(&|v: &f64| format!("{v:+.0}%"))
            .axis_style(GREY)
            .light_line_style(GREY.mix(0.25))
            .draw()
            .map_err(|e| e.to_string())?;
        chart
            .draw_series(bars.iter().map(|&(y, delta, colour)| {
                Rectangle::new(
                    [(delta.min(0.0), y - 0.35), (delta.max(0.0), y + 0.35)],
                    colour.mix(0.85).filled(),
                )
            }))
            .map_err(|e| e.to_string())?;
        root.present().map_err(|e| e.to_string())
    });

    // Same order as `bars`, which is the order the rectangles are drawn in.
    let titles: Vec<String> = entries
        .iter()
        .map(|c| {
            let delta = c.delta_pct.unwrap_or(0.0);
            format!("{} [{}]: {delta:+.1}%", c.name, c.layer)
        })
        .collect();
    add_titles(&svg, "rect", &titles)
}

/// Draws into a fresh SVG string. Falls back to a note if drawing fails.
fn render_svg<F>(draw: F) -> String
where
    F: for<'a> FnOnce(DrawingArea<SVGBackend<'a>, plotters::coord::Shift>) -> Result<(), String>,
{
    let mut buf = String::new();
    let outcome = {
        let root = SVGBackend::with_string(&mut buf, (WIDTH, HEIGHT)).into_drawing_area();
        draw(root)
    };
    match outcome {
        Ok(()) => buf,
        Err(message) => note(&format!("chart unavailable: {message}")),
    }
}

fn note(message: &str) -> String {
    format!("<p class=\"note\">{}</p>", escape_html(message))
}

/// Inserts a `<title>` child into the first `titles.len()` elements named
/// `tag` (for example `rect` or `circle`), in document order. Elements beyond
/// the list are left unchanged. Titles are escaped.
fn add_titles(svg: &str, tag: &str, titles: &[String]) -> String {
    let open = format!("<{tag}");
    let mut out = String::with_capacity(svg.len() + titles.len() * 48);
    let mut rest = svg;
    let mut index = 0;
    while let Some(start) = rest.find(&open) {
        let (before, from_tag) = rest.split_at(start);
        out.push_str(before);
        let Some(end) = from_tag.find("/>") else {
            break;
        };
        out.push_str(&from_tag[..end]);
        match titles.get(index) {
            Some(title) => {
                out.push('>');
                out.push_str("<title>");
                out.push_str(&escape_html(title));
                out.push_str("</title></");
                out.push_str(tag);
                out.push('>');
            }
            None => out.push_str("/>"),
        }
        index += 1;
        rest = &from_tag[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Least-squares slope and intercept of `values` against their index.
fn least_squares(values: &[f64]) -> (f64, f64) {
    let n = values.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let count = n as f64;
    let mean_x = (count - 1.0) / 2.0;
    let mean_y = values.iter().sum::<f64>() / count;
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    for (i, &y) in values.iter().enumerate() {
        let dx = i as f64 - mean_x;
        sxy += dx * (y - mean_y);
        sxx += dx * dx;
    }
    let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
    (slope, mean_y - slope * mean_x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slope_of_a_straight_line_is_exact() {
        let values = [10.0, 12.0, 14.0, 16.0];
        assert!((trend_slope(&values) - 2.0).abs() < 1e-12);
        assert_eq!(trend_slope(&[5.0]), 0.0);
    }

    #[test]
    fn add_titles_targets_elements_in_order() {
        let svg = "<g><rect x=\"1\"/><path d=\"M\"/><rect x=\"2\"/></g>";
        let titles = vec!["a & b".to_owned()];
        let out = add_titles(svg, "rect", &titles);
        assert_eq!(
            out,
            "<g><rect x=\"1\"><title>a &amp; b</title></rect><path d=\"M\"/><rect x=\"2\"/></g>"
        );
    }

    #[test]
    fn histogram_has_one_title_per_bin() {
        let values = [100.0, 101.0, 102.0, 103.0, 150.0, 151.0, 152.0, 300.0];
        let svg = histogram(&values, &|v| format!("{v:.0} ns"));
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("<title>"));
        assert!(svg.contains("samples</title>"));
    }

    #[test]
    fn empty_inputs_produce_notes_not_charts() {
        assert!(histogram(&[], &|v| format!("{v}")).contains("no samples"));
        assert!(trend(&[], &|v| format!("{v}")).contains("no samples"));
        assert!(comparison(&[]).contains("no baseline deltas"));
    }
}
