//! Reading exports and the run history kept under `data/`.

use std::fs;
use std::io;
use std::path::Path;

use serde::Deserialize;
use tpt_benchmark::runner::BenchResult;

/// Machine and build a run was measured on. Every field is optional so a
/// schema 1 file (no environment) and future additions still load.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Env {
    pub commit: Option<String>,
    pub timestamp: String,
    pub os: String,
    pub arch: String,
    pub cpu: Option<String>,
    pub cores: Option<usize>,
    pub rustc: Option<String>,
    pub clock_source: String,
    pub runner: String,
}

impl Env {
    /// Runs on different machines are never plotted on one trend line.
    pub fn machine_key(&self) -> String {
        format!(
            "{} {} | {} | {}",
            self.os,
            self.arch,
            self.cpu.as_deref().unwrap_or("unknown cpu"),
            self.runner
        )
    }

    pub fn short_commit(&self) -> String {
        self.commit
            .as_deref()
            .map_or_else(|| "unknown".to_owned(), |c| c.chars().take(8).collect())
    }
}

#[derive(Debug, Deserialize)]
struct Document {
    schema_version: u32,
    #[serde(default)]
    environment: Option<Env>,
    benchmarks: Vec<BenchResult>,
}

/// One saved run.
#[derive(Clone, Debug)]
pub struct Run {
    /// File name under `data/`, used for the raw JSON link.
    pub file: String,
    pub env: Env,
    pub benchmarks: Vec<BenchResult>,
}

/// Parses an export. Schema 1 files have no environment, so `fallback_time`
/// stands in for the timestamp.
pub fn parse(text: &str, file: &str, fallback_time: &str) -> Result<Run, String> {
    let doc: Document = serde_json::from_str(text).map_err(|e| format!("{file}: {e}"))?;
    if doc.schema_version > 2 {
        return Err(format!(
            "{file}: schema version {} is newer than this tpt-site understands (2)",
            doc.schema_version
        ));
    }
    let mut env = doc.environment.unwrap_or_default();
    if env.timestamp.is_empty() {
        fallback_time.clone_into(&mut env.timestamp);
    }
    Ok(Run {
        file: file.to_owned(),
        env,
        benchmarks: doc.benchmarks,
    })
}

/// File name a run is stored under: sortable timestamp, commit, OS.
pub fn data_file_name(env: &Env) -> String {
    let stamp: String = env
        .timestamp
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let os = if env.os.is_empty() {
        "unknown"
    } else {
        &env.os
    };
    format!("{stamp}-{}-{os}.json", env.short_commit())
}

/// Loads every run in `dir`, oldest first. Unreadable files are reported on
/// stderr and skipped, so one bad file cannot take the site down.
pub fn load_history(dir: &Path) -> io::Result<Vec<Run>> {
    let mut runs = Vec::new();
    if !dir.exists() {
        return Ok(runs);
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".json") || name == "index.json" {
            continue;
        }
        match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| parse(&text, name, "1970-01-01T00:00:00Z"))
        {
            Ok(run) => runs.push(run),
            Err(err) => eprintln!("warning: skipping {err}"),
        }
    }
    runs.sort_by(|a, b| {
        a.env
            .timestamp
            .cmp(&b.env.timestamp)
            .then_with(|| a.file.cmp(&b.file))
    });
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2: &str = r#"{"schema_version":2,"tool":"tpt-benchmark","tool_version":"0.1.0",
      "environment":{"commit":"0123456789abcdef","timestamp":"2026-01-02T03:04:05Z","os":"linux",
      "arch":"x86_64","cpu":null,"cores":4,"rustc":null,"clock_source":"tsc","runner":"local"},
      "benchmarks":[]}"#;

    #[test]
    fn reads_schema_2_and_names_the_file() {
        let run = parse(V2, "x.json", "").unwrap();
        assert_eq!(run.env.short_commit(), "01234567");
        assert_eq!(
            data_file_name(&run.env),
            "20260102T030405Z-01234567-linux.json"
        );
        assert!(run.env.machine_key().contains("unknown cpu"));
    }

    #[test]
    fn schema_1_gets_the_fallback_time() {
        let v1 = r#"{"schema_version":1,"tool":"t","tool_version":"0","benchmarks":[]}"#;
        let run = parse(v1, "old.json", "2020-05-06T07:08:09Z").unwrap();
        assert_eq!(run.env.timestamp, "2020-05-06T07:08:09Z");
        assert_eq!(run.env.short_commit(), "unknown");
    }

    #[test]
    fn newer_schema_is_refused() {
        let v9 = r#"{"schema_version":9,"benchmarks":[]}"#;
        assert!(parse(v9, "f.json", "").is_err());
    }
}
