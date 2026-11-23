//! `tpt-site`: turns tpt-benchmark JSON exports into a static trend site.
//!
//! ```text
//! tpt-site build [--input DIR] [--out DIR] [--title T] [--repo URL] [--base-url URL] [--threshold PCT] [--fail-on-regression] [--docs DIR]
//! tpt-site serve [--out DIR] [--port N]
//! ```
//!
//! `build` adds the newest export to the history in `<out>/data/` (existing
//! files are kept, so history accumulates across builds) and regenerates the
//! site.

mod docs;
mod model;
mod project;
mod serve;
mod site;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage:
  tpt-site build [--input DIR] [--out DIR] [--title T] [--repo URL] [--base-url URL] [--threshold PCT] [--fail-on-regression] [--docs DIR]
  tpt-site serve [--out DIR] [--port N]

--docs renders the Markdown files in DIR as pages under docs/ (default: ./docs when it exists).
--fail-on-regression exits with status 1 (after writing the site) when the newest run is
slower than the previous run on the same machine by more than --threshold.

defaults: --input target/tpt-benchmark, --out site, --threshold 5, title from Cargo.toml,
repo from the git remote `origin`, base URL derived from a github.com repo.";

struct Args {
    input: PathBuf,
    out: PathBuf,
    title: Option<String>,
    repo: Option<String>,
    base_url: Option<String>,
    threshold: f64,
    fail_on_regression: bool,
    docs: Option<PathBuf>,
    port: u16,
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut a = Args {
        input: PathBuf::from("target/tpt-benchmark"),
        out: PathBuf::from("site"),
        title: None,
        repo: None,
        base_url: None,
        threshold: 5.0,
        fail_on_regression: false,
        docs: None,
        port: 8000,
    };
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--input" => a.input = value()?.into(),
            "--out" => a.out = value()?.into(),
            "--title" => a.title = Some(value()?),
            "--repo" => a.repo = Some(value()?),
            "--base-url" => a.base_url = Some(value()?),
            "--threshold" => {
                a.threshold = value()?
                    .parse::<f64>()
                    .ok()
                    .filter(|t| t.is_finite() && *t >= 0.0)
                    .ok_or("--threshold must be a non-negative number")?;
            }
            "--docs" => a.docs = Some(value()?.into()),
            "--fail-on-regression" => a.fail_on_regression = true,
            "--port" => a.port = value()?.parse().map_err(|_| "--port must be 0-65535")?,
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(a)
}

/// Builds the site. Returns the number of regressions found.
fn build(a: &Args) -> Result<usize, String> {
    let source = a.input.join("tpt-benchmark.json");
    let text = fs::read_to_string(&source).map_err(|e| {
        format!(
            "cannot read {}: {e}\nRun `cargo bench` first. Output is written to `target/tpt-benchmark/` \
             relative to the directory the bench runs in (for a workspace member, that is the crate's \
             directory). Use --input to point elsewhere.",
            source.display()
        )
    })?;
    let now = "1970-01-01T00:00:00Z";
    let latest = model::parse(&text, "tpt-benchmark.json", now)?;
    let file = model::data_file_name(&latest.env);

    let data = a.out.join("data");
    fs::create_dir_all(&data).map_err(|e| format!("{}: {e}", data.display()))?;
    fs::write(data.join(&file), &text).map_err(|e| format!("{file}: {e}"))?;
    let runs = model::load_history(&data).map_err(|e| format!("{}: {e}", data.display()))?;

    let repo = a.repo.clone().or_else(project::default_repo_url);
    let docs_dir = a.docs.clone().unwrap_or_else(|| PathBuf::from("docs"));
    let docs = if docs_dir.is_dir() {
        docs::collect(&docs_dir, repo.as_deref())
            .map_err(|e| format!("{}: {e}", docs_dir.display()))?
    } else if a.docs.is_some() {
        return Err(format!("--docs {} is not a directory", docs_dir.display()));
    } else {
        Vec::new()
    };
    let cfg = site::Config {
        has_docs: !docs.is_empty(),
        title: a
            .title
            .clone()
            .unwrap_or_else(|| project::default_title(std::path::Path::new("."))),
        base_url: a
            .base_url
            .clone()
            .or_else(|| repo.as_deref().and_then(project::pages_url))
            .map(|u| if u.ends_with('/') { u } else { format!("{u}/") }),
        repo,
        threshold_pct: a.threshold,
    };
    let regressions = site::build(&a.out, &runs, &docs, &cfg).map_err(|e| e.to_string())?;
    println!("Wrote {} ({} runs in history)", a.out.display(), runs.len());
    for r in &regressions {
        println!("regression vs previous run: {r}");
    }
    Ok(regressions.len())
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    let result = match command.as_deref() {
        Some("build") => parse(args).and_then(|a| {
            let regressions = build(&a)?;
            if a.fail_on_regression && regressions > 0 {
                eprintln!("{regressions} regression(s) beyond {}%", a.threshold);
                return Err(String::new());
            }
            Ok(())
        }),
        Some("serve") => {
            parse(args).and_then(|a| serve::serve(&a.out, a.port).map_err(|e| e.to_string()))
        }
        Some("--help" | "-h" | "help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => Err("expected `build` or `serve`".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) if message.is_empty() => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
