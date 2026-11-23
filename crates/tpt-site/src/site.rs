//! Static site generation: index, per-benchmark pages, sitemap and robots.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use tpt_benchmark::Summary;
use tpt_benchmark::report::site_support::{
    css, escape_html, format_value, histogram_svg, history_svg, unit_label,
};
use tpt_benchmark::runner::BenchResult;

use crate::model::Run;

const WALL: &str = "WallClock";

const EXTRA_CSS: &str = r"
nav.site { max-width: 1100px; margin: 0 auto 8px; display: flex; gap: 16px; flex-wrap: wrap; }
.caveat { background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 8px 14px; }
.machine { margin-top: 2.5rem; }
footer { max-width: 1100px; margin: 3rem auto 0; color: var(--muted); font-size: 0.85rem; }
";

/// Shown on every index page. Keeps the known limits of the measurements in
/// view instead of presenting the numbers as settled.
const ABOUT_NUMBERS: &str = r#"<section class="caveat">
<h2>About these numbers</h2>
<ul>
<li>Hosted CI runners share hardware, so timings are noisy. Compare a benchmark with its own
earlier runs on the same machine. Runs on different machines are charted separately.</li>
<li>Hosted runners usually expose no hardware counters, so instruction and cache figures are
normally missing here.</li>
<li><strong>Open question: agreement with Criterion.</strong> On identical workloads on a quiet
machine, tpt-benchmark's 95% intervals overlapped Criterion's for two of four workloads. They did
not overlap for the dot product, where tpt-benchmark read 2.6% higher, or for a sub-nanosecond
multiply at the timer floor, where it read lower. An earlier 2x difference did not reproduce and
its cause is not established. Treat absolute values as unvalidated. Details are in the
<a href="https://github.com/tpt-solutions/tpt-benchmark/blob/master/docs/comparison-criterion.md">comparison write-up</a>.</li>
</ul>
</section>
"#;

pub struct Config {
    pub title: String,
    pub repo: Option<String>,
    /// Absolute site address ending in `/`. Needed for canonical URLs and the sitemap.
    pub base_url: Option<String>,
    /// Minimum change, in percent, for a non-overlapping interval to count as a regression.
    pub threshold_pct: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Same,
    Regressed,
    Improved,
}

/// Compares two runs of one layer. A change only counts when the 95 %
/// intervals do not overlap and the mean moved by at least `threshold_pct`.
/// Lower is better for every layer.
pub fn compare(prev: &Summary, cur: &Summary, threshold_pct: f64) -> Option<(f64, Verdict)> {
    if prev.mean == 0.0 || !prev.mean.is_finite() || !cur.mean.is_finite() {
        return None;
    }
    let pct = (cur.mean - prev.mean) / prev.mean * 100.0;
    let overlap = cur.ci_low <= prev.ci_high && prev.ci_low <= cur.ci_high;
    let verdict = if overlap || pct.abs() < threshold_pct {
        Verdict::Same
    } else if pct > 0.0 {
        Verdict::Regressed
    } else {
        Verdict::Improved
    };
    Some((pct, verdict))
}

pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "bench".to_owned()
    } else {
        slug.to_owned()
    }
}

/// Benchmark names (newest run first, then first appearance) with unique slugs.
fn benchmark_index(runs: &[Run]) -> Vec<(String, String)> {
    let mut names: Vec<String> = Vec::new();
    for run in runs.iter().rev() {
        for b in &run.benchmarks {
            if !names.contains(&b.name) {
                names.push(b.name.clone());
            }
        }
    }
    let mut used: Vec<String> = Vec::new();
    names
        .into_iter()
        .map(|name| {
            let base = slugify(&name);
            let mut slug = base.clone();
            let mut n = 2;
            while used.contains(&slug) {
                slug = format!("{base}-{n}");
                n += 1;
            }
            used.push(slug.clone());
            (name, slug)
        })
        .collect()
}

fn find<'a>(run: &'a Run, name: &str) -> Option<&'a BenchResult> {
    run.benchmarks.iter().find(|b| b.name == name)
}

fn layer<'a>(bench: &'a BenchResult, layer: &str) -> Option<&'a Summary> {
    bench.layers.iter().find(|s| s.layer == layer)
}

struct Page<'a> {
    title: &'a str,
    description: &'a str,
    /// Path of the page relative to the site root.
    path: &'a str,
    body: String,
    jsonld: serde_json::Value,
}

fn render_page(cfg: &Config, page: &Page<'_>) -> String {
    let root = "../".repeat(page.path.matches('/').count());
    let title = escape_html(page.title);
    let description = escape_html(page.description);
    let mut head = format!(
        "<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<meta name=\"description\" content=\"{description}\">\n\
         <meta property=\"og:title\" content=\"{title}\">\n\
         <meta property=\"og:description\" content=\"{description}\">\n\
         <meta property=\"og:type\" content=\"website\">\n\
         <meta property=\"og:site_name\" content=\"{}\">\n",
        escape_html(&cfg.title)
    );
    if let Some(base) = &cfg.base_url {
        let url = escape_html(&format!(
            "{base}{}",
            page.path.trim_end_matches("index.html")
        ));
        let _ = write!(
            head,
            "<link rel=\"canonical\" href=\"{url}\">\n<meta property=\"og:url\" content=\"{url}\">\n"
        );
    }
    // `</` inside JSON-LD would end the script element early.
    let jsonld = page.jsonld.to_string().replace("</", "<\\/");
    let repo_link = cfg.repo.as_ref().map_or_else(String::new, |r| {
        format!("<a href=\"{}\">Repository</a>", escape_html(r))
    });
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n{head}<style>{}{EXTRA_CSS}</style>\n\
         <script type=\"application/ld+json\">{jsonld}</script>\n</head>\n<body>\n\
         <nav class=\"site\"><a href=\"{root}index.html\">{}</a>{repo_link}\
         <a href=\"{root}data/index.json\">Raw data</a></nav>\n\
         <header class=\"page\"><h1>{title}</h1></header>\n<main>\n{}</main>\n\
         <footer>Built with tpt-benchmark. Hosted CI runners are noisy and usually have no \
         hardware counters, so compare trends on one machine rather than absolute numbers.</footer>\n\
         </body>\n</html>\n",
        css(),
        escape_html(&cfg.title),
        page.body
    )
}

fn commit_cell(cfg: &Config, run: &Run) -> String {
    let short = escape_html(&run.env.short_commit());
    match (&cfg.repo, &run.env.commit) {
        (Some(repo), Some(full)) if repo.starts_with("https://github.com/") => format!(
            "<a href=\"{}/commit/{}\">{short}</a>",
            escape_html(repo),
            escape_html(full)
        ),
        _ => short,
    }
}

fn delta_cell(delta: Option<(f64, Verdict)>) -> String {
    match delta {
        None => "<span class=\"muted\">n/a</span>".to_owned(),
        Some((pct, Verdict::Regressed)) => format!("<span class=\"bad\">{pct:+.1}%</span>"),
        Some((pct, Verdict::Improved)) => format!("<span class=\"good\">{pct:+.1}%</span>"),
        Some((pct, Verdict::Same)) => format!("<span class=\"muted\">{pct:+.1}%</span>"),
    }
}

/// The run before `index` on the same machine that contains `name`.
fn previous_on_machine<'a>(runs: &'a [Run], index: usize, name: &str) -> Option<&'a Run> {
    let key = runs[index].env.machine_key();
    runs[..index]
        .iter()
        .rev()
        .find(|r| r.env.machine_key() == key && find(r, name).is_some())
}

/// Writes the whole site into `out`. Returns descriptions of regressions
/// found between the newest run and its predecessor on the same machine.
pub fn build(out: &Path, runs: &[Run], cfg: &Config) -> io::Result<Vec<String>> {
    fs::create_dir_all(out.join("b"))?;
    let index = benchmark_index(runs);
    let mut regressions = Vec::new();
    let latest_index = runs.len().checked_sub(1);

    // Index page.
    let mut body = String::new();
    if let Some(latest) = runs.last() {
        let _ = writeln!(
            body,
            "<p class=\"muted\">{} benchmarks, {} runs. Latest: {} on {} at {}.</p>",
            index.len(),
            runs.len(),
            commit_cell(cfg, latest),
            escape_html(&latest.env.machine_key()),
            escape_html(&latest.env.timestamp)
        );
        body.push_str(
            "<div class=\"table-wrap\"><table>\n<tr><th>Benchmark</th><th class=\"num\">Time/iter</th>\
             <th class=\"num\">vs previous</th><th class=\"num\">Allocs/iter</th><th>Noisy</th></tr>\n",
        );
        for (name, slug) in &index {
            let Some(li) = latest_index else { break };
            let Some(bench) = find(&runs[li], name) else {
                continue;
            };
            let wall = layer(bench, WALL);
            let delta = wall.and_then(|w| {
                let prev = previous_on_machine(runs, li, name)?;
                compare(layer(find(prev, name)?, WALL)?, w, cfg.threshold_pct)
            });
            if let Some((pct, Verdict::Regressed)) = delta {
                regressions.push(format!("{name}: {pct:+.1}% time/iter"));
            }
            let allocs = layer(bench, "Allocations")
                .map_or_else(|| "-".to_owned(), |s| format_value("Allocations", s.mean));
            let time = wall.map_or_else(|| "-".to_owned(), |s| format_value(WALL, s.mean));
            let noisy = if bench.layers.iter().any(|s| s.noisy) {
                "yes"
            } else {
                "no"
            };
            let _ = writeln!(
                body,
                "<tr><td><a href=\"b/{slug}.html\">{}</a></td><td class=\"num\">{time}</td>\
                 <td class=\"num\">{}</td><td class=\"num\">{allocs}</td><td>{noisy}</td></tr>",
                escape_html(name),
                delta_cell(delta)
            );
        }
        body.push_str("</table></div>\n<h2>Recent runs</h2>\n<div class=\"table-wrap\"><table>\n");
        body.push_str("<tr><th>Time (UTC)</th><th>Commit</th><th>Machine</th><th>Raw</th></tr>\n");
        for run in runs.iter().rev().take(20) {
            let _ = writeln!(
                body,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td><a href=\"data/{}\">json</a></td></tr>",
                escape_html(&run.env.timestamp),
                commit_cell(cfg, run),
                escape_html(&run.env.machine_key()),
                escape_html(&run.file)
            );
        }
        body.push_str("</table></div>\n");
    } else {
        body.push_str("<p class=\"note\">No benchmark runs yet.</p>\n");
    }
    body.push_str(ABOUT_NUMBERS);
    let description = format!(
        "Benchmark results and performance trends for {}, measured with tpt-benchmark.",
        cfg.title
    );
    let page = Page {
        title: &format!("{} benchmarks", cfg.title),
        description: &description,
        path: "index.html",
        body,
        jsonld: serde_json::json!({
            "@context": "https://schema.org",
            "@type": "WebSite",
            "name": format!("{} benchmarks", cfg.title),
            "description": description,
        }),
    };
    fs::write(out.join("index.html"), render_page(cfg, &page))?;

    // Per-benchmark pages.
    for (name, slug) in &index {
        write_benchmark_page(out, runs, cfg, name, slug)?;
    }

    // Machine-readable run list, robots and sitemap.
    let listing: Vec<serde_json::Value> = runs
        .iter()
        .map(|r| {
            serde_json::json!({
                "file": r.file,
                "timestamp": r.env.timestamp,
                "commit": r.env.commit,
                "machine": r.env.machine_key(),
            })
        })
        .collect();
    fs::write(
        out.join("data").join("index.json"),
        serde_json::to_string_pretty(&listing).map_err(io::Error::other)?,
    )?;
    fs::write(out.join(".nojekyll"), "")?;
    let mut robots = String::from("User-agent: *\nAllow: /\n");
    if let Some(base) = &cfg.base_url {
        let _ = writeln!(robots, "Sitemap: {base}sitemap.xml");
        let mut sitemap = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
        );
        let _ = writeln!(sitemap, "  <url><loc>{}</loc></url>", escape_html(base));
        for (_, slug) in &index {
            let _ = writeln!(
                sitemap,
                "  <url><loc>{}b/{slug}.html</loc></url>",
                escape_html(base)
            );
        }
        sitemap.push_str("</urlset>\n");
        fs::write(out.join("sitemap.xml"), sitemap)?;
    }
    fs::write(out.join("robots.txt"), robots)?;
    Ok(regressions)
}

fn write_benchmark_page(
    out: &Path,
    runs: &[Run],
    cfg: &Config,
    name: &str,
    slug: &str,
) -> io::Result<()> {
    // Group the run indices that contain this benchmark by machine.
    let mut machines: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, run) in runs.iter().enumerate() {
        if find(run, name).is_some() {
            machines.entry(run.env.machine_key()).or_default().push(i);
        }
    }

    let mut body = String::new();
    let mut latest_wall = None;
    for (machine, indices) in &machines {
        let _ = write!(
            body,
            "<section class=\"machine\"><h2>{}</h2>\n<p class=\"muted\">{} runs</p>\n",
            escape_html(machine),
            indices.len()
        );
        let newest = &runs[*indices.last().unwrap_or(&0)];
        let Some(newest_bench) = find(newest, name) else {
            continue;
        };
        if let Some(w) = layer(newest_bench, WALL) {
            latest_wall = Some(w.mean);
        }
        body.push_str("<div class=\"charts\">\n");
        for s in &newest_bench.layers {
            let points: Vec<(String, f64, f64, f64)> = indices
                .iter()
                .filter_map(|&i| {
                    let l = layer(find(&runs[i], name)?, &s.layer)?;
                    Some((runs[i].env.short_commit(), l.mean, l.ci_low, l.ci_high))
                })
                .collect();
            let _ = writeln!(
                body,
                "<figure class=\"chart\"><figcaption>{} ({}) across runs, mean with 95% interval</figcaption>{}</figure>",
                escape_html(&s.layer),
                unit_label(&s.layer),
                history_svg(&s.layer, &points)
            );
        }
        if let Some(w) = layer(newest_bench, WALL) {
            let _ = writeln!(
                body,
                "<figure class=\"chart\"><figcaption>Latest run: distribution of per-iteration time</figcaption>{}</figure>",
                histogram_svg(WALL, &w.values)
            );
        }
        body.push_str("</div>\n<div class=\"table-wrap\"><table>\n");
        body.push_str(
            "<tr><th>Time (UTC)</th><th>Commit</th><th class=\"num\">Mean</th><th class=\"num\">95% interval</th>\
             <th class=\"num\">vs previous</th><th class=\"num\">Samples</th><th>Noisy</th><th>Raw</th></tr>\n",
        );
        for &i in indices.iter().rev() {
            let run = &runs[i];
            let Some(s) = find(run, name).and_then(|b| layer(b, WALL)) else {
                continue;
            };
            let delta = previous_on_machine(runs, i, name)
                .and_then(|p| layer(find(p, name)?, WALL))
                .and_then(|p| compare(p, s, cfg.threshold_pct));
            let _ = writeln!(
                body,
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{} to {}</td>\
                 <td class=\"num\">{}</td><td class=\"num\">{}</td><td>{}</td><td><a href=\"../data/{}\">json</a></td></tr>",
                escape_html(&run.env.timestamp),
                commit_cell(cfg, run),
                format_value(WALL, s.mean),
                format_value(WALL, s.ci_low),
                format_value(WALL, s.ci_high),
                delta_cell(delta),
                s.samples,
                if s.noisy { "yes" } else { "no" },
                escape_html(&run.file)
            );
        }
        body.push_str("</table></div></section>\n");
    }

    let description = match latest_wall {
        Some(mean) => format!(
            "Performance history of {name} in {}: latest mean {} per iteration, with 95% confidence intervals per run.",
            cfg.title,
            format_value(WALL, mean)
        ),
        None => format!("Performance history of {name} in {}.", cfg.title),
    };
    let path = format!("b/{slug}.html");
    let page = Page {
        title: &format!("{name} benchmark"),
        description: &description,
        path: &path,
        body,
        jsonld: serde_json::json!({
            "@context": "https://schema.org",
            "@type": "Dataset",
            "name": format!("{name} benchmark history"),
            "description": description,
            "isPartOf": { "@type": "WebSite", "name": format!("{} benchmarks", cfg.title) },
        }),
    };
    fs::write(
        out.join("b").join(format!("{slug}.html")),
        render_page(cfg, &page),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_benchmark::StopReason;

    fn summary(mean: f64, lo: f64, hi: f64) -> Summary {
        Summary {
            layer: WALL.to_owned(),
            mean,
            std_dev: 1.0,
            median: mean,
            ci_low: lo,
            ci_high: hi,
            samples: 10,
            values: vec![mean],
            outliers: 0,
            iters_per_batch: 1,
            stop_reason: StopReason::Converged,
            noisy: false,
            throughput: None,
        }
    }

    #[test]
    fn overlapping_intervals_are_never_a_change() {
        let v = compare(
            &summary(100.0, 90.0, 110.0),
            &summary(108.0, 100.0, 116.0),
            1.0,
        );
        assert_eq!(v.map(|x| x.1), Some(Verdict::Same));
    }

    #[test]
    fn separated_intervals_beyond_threshold_regress_or_improve() {
        let slow = compare(
            &summary(100.0, 99.0, 101.0),
            &summary(120.0, 119.0, 121.0),
            5.0,
        );
        assert_eq!(slow.map(|x| x.1), Some(Verdict::Regressed));
        let fast = compare(
            &summary(100.0, 99.0, 101.0),
            &summary(80.0, 79.0, 81.0),
            5.0,
        );
        assert_eq!(fast.map(|x| x.1), Some(Verdict::Improved));
        let small = compare(
            &summary(100.0, 99.9, 100.1),
            &summary(101.0, 100.9, 101.1),
            5.0,
        );
        assert_eq!(small.map(|x| x.1), Some(Verdict::Same));
    }

    #[test]
    fn index_page_carries_the_caveats() {
        let dir = std::env::temp_dir().join(format!("tpt-site-test-{}", std::process::id()));
        let cfg = Config {
            title: "demo".to_owned(),
            repo: None,
            base_url: None,
            threshold_pct: 5.0,
        };
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("data")).unwrap();
        build(&dir, &[], &cfg).unwrap();
        let index = fs::read_to_string(dir.join("index.html")).unwrap();
        assert!(index.contains("Open question: agreement with Criterion"));
        assert!(index.contains("comparison-criterion.md"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn slugs_are_safe_and_unique() {
        assert_eq!(slugify("matrix_ops::dot/128"), "matrix-ops-dot-128");
        assert_eq!(slugify("../../etc"), "etc");
        assert_eq!(slugify("***"), "bench");
    }
}
