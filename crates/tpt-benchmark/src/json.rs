//! JSON export of raw statistical data to `target/tpt-benchmark/tpt-benchmark.json`.
//!
//! The document is versioned by `schema_version`. See `docs/json-schema.md`
//! for the field-by-field description.

use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use crate::environment::Environment;
use crate::runner::BenchResult;

/// Version of the export schema. Bump on any incompatible change.
pub const SCHEMA_VERSION: u32 = 2;

/// File name of the export inside the output directory.
pub const FILE_NAME: &str = "tpt-benchmark.json";

#[derive(Serialize)]
struct Export<'a> {
    schema_version: u32,
    tool: &'static str,
    tool_version: &'static str,
    environment: &'a Environment,
    benchmarks: &'a [BenchResult],
}

/// Renders the export document.
///
/// # Errors
///
/// Returns an error if serialisation fails.
pub(crate) fn to_string(results: &[BenchResult], environment: &Environment) -> io::Result<String> {
    let doc = Export {
        schema_version: SCHEMA_VERSION,
        tool: "tpt-benchmark",
        tool_version: env!("CARGO_PKG_VERSION"),
        environment,
        benchmarks: results,
    };
    serde_json::to_string_pretty(&doc).map_err(io::Error::other)
}

/// Writes the export document into `dir`, creating it if needed.
///
/// # Errors
///
/// Returns an error if the directory or file cannot be written.
pub(crate) fn write(
    dir: &Path,
    results: &[BenchResult],
    environment: &Environment,
) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(FILE_NAME), to_string(results, environment)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_env() -> Environment {
        Environment {
            commit: Some("abc123".to_owned()),
            timestamp: "2026-01-02T03:04:05Z".to_owned(),
            os: "linux",
            arch: "x86_64",
            cpu: None,
            cores: Some(8),
            rustc: Some("rustc 1.85.0".to_owned()),
            clock_source: "tsc",
            runner: "local",
        }
    }

    #[test]
    fn export_has_schema_version_and_benchmarks() {
        let text = to_string(&[], &sample_env()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["schema_version"], SCHEMA_VERSION);
        assert_eq!(value["tool"], "tpt-benchmark");
        assert!(value["benchmarks"].as_array().unwrap().is_empty());
    }

    /// Golden test: the exact layout is part of the public schema. Changing
    /// it means bumping `SCHEMA_VERSION` and updating docs/json-schema.md.
    #[test]
    fn golden_layout_is_stable() {
        use crate::runner::BenchResult;
        use crate::stats::{StopReason, Summary};

        let results = [BenchResult {
            name: "g::f/2".to_owned(),
            layers: vec![Summary {
                layer: "WallClock".to_owned(),
                mean: 2.5,
                std_dev: 0.5,
                median: 2.0,
                ci_low: 2.25,
                ci_high: 2.75,
                samples: 2,
                values: vec![2.0, 3.0],
                outliers: 1,
                iters_per_batch: 16,
                stop_reason: StopReason::MaxSamples,
                noisy: true,
                throughput: None,
            }],
        }];
        let expected = r#"{
  "schema_version": 2,
  "tool": "tpt-benchmark",
  "tool_version": "0.1.0",
  "environment": {
    "commit": "abc123",
    "timestamp": "2026-01-02T03:04:05Z",
    "os": "linux",
    "arch": "x86_64",
    "cpu": null,
    "cores": 8,
    "rustc": "rustc 1.85.0",
    "clock_source": "tsc",
    "runner": "local"
  },
  "benchmarks": [
    {
      "name": "g::f/2",
      "layers": [
        {
          "layer": "WallClock",
          "mean": 2.5,
          "std_dev": 0.5,
          "median": 2.0,
          "ci_low": 2.25,
          "ci_high": 2.75,
          "samples": 2,
          "values": [
            2.0,
            3.0
          ],
          "outliers": 1,
          "iters_per_batch": 16,
          "stop_reason": "max_samples",
          "noisy": true
        }
      ]
    }
  ]
}"#;
        assert_eq!(to_string(&results, &sample_env()).unwrap(), expected);
    }
}
