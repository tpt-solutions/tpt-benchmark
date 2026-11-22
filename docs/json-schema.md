# JSON export schema

`tpt-benchmark` writes its raw statistical data to
`target/tpt-benchmark/tpt-benchmark.json` after every run. The document is
versioned, so CI tooling can check it before reading it.

Current schema version: **2**. The matching Rust constant is
`tpt_benchmark::json::SCHEMA_VERSION`.

## Top level

| Field            | Type    | Meaning                                                 |
|------------------|---------|---------------------------------------------------------|
| `schema_version` | integer | Schema version. Increases on any incompatible change.   |
| `tool`           | string  | Always `"tpt-benchmark"`.                               |
| `tool_version`   | string  | Version of the crate that wrote the file.               |
| `environment`    | object  | Machine and build the run was measured on (new in v2). See below. |
| `benchmarks`     | array   | One entry per benchmark that ran. See below.            |

## Environment

Added in schema 2. Every probe is best effort, so a field that could not be
determined is `null`.

| Field          | Type           | Meaning                                                      |
|----------------|----------------|--------------------------------------------------------------|
| `commit`       | string or null | Git commit hash. `GITHUB_SHA` if set, else `git rev-parse HEAD`. |
| `timestamp`    | string         | UTC time the run finished, RFC 3339 (`2026-11-23T04:05:06Z`). |
| `os`           | string         | `linux`, `macos`, `windows`, ... (`std::env::consts::OS`).   |
| `arch`         | string         | `x86_64`, `aarch64`, ... (`std::env::consts::ARCH`).         |
| `cpu`          | string or null | CPU model name (`/proc/cpuinfo`, `sysctl`, or `PROCESSOR_IDENTIFIER`). |
| `cores`        | integer or null| Logical cores available to the process.                      |
| `rustc`        | string or null | Output of `rustc --version` at run time.                     |
| `clock_source` | string         | Wall-clock source: `tsc`, `cntvct` or `instant`.             |
| `runner`       | string         | `github-actions` when `GITHUB_ACTIONS` is set, else `local`. |

## Benchmark entry

| Field    | Type   | Meaning                                                             |
|----------|--------|---------------------------------------------------------------------|
| `name`   | string | Fully qualified name, `group::name` or just `name`. Arguments appear as `name/<arg>`. |
| `layers` | array  | One entry per layer that produced a result. Layers that were unavailable are omitted. |

## Layer entry

| Field             | Type    | Meaning                                                              |
|-------------------|---------|----------------------------------------------------------------------|
| `layer`           | string  | `WallClock`, `Allocations`, `AllocBytes`, `HwInstructions`, `HwL1Misses`, `HwL2Misses` or `HwBranchMisses`. |
| `mean`            | number  | Mean per-iteration value. Nanoseconds for `WallClock`, counts for the others, bytes for `AllocBytes`. |
| `std_dev`         | number  | Sample standard deviation of the accepted values.                    |
| `median`          | number  | Median of the accepted values.                                       |
| `ci_low`          | number  | Lower bound of the 95 % confidence interval for the mean.            |
| `ci_high`         | number  | Upper bound of the 95 % confidence interval for the mean.            |
| `samples`         | integer | Number of accepted samples.                                          |
| `values`          | array   | The accepted per-iteration values, in arrival order.                 |
| `outliers`        | integer | Number of samples rejected by the MAD test.                          |
| `iters_per_batch` | integer | Iterations executed in each measured batch.                          |
| `stop_reason`     | string  | `converged`, `max_samples` or `max_time`.                            |
| `noisy`           | boolean | `true` when the noise-regime SPRT did not accept the clean hypothesis. |

Non-finite numbers (`NaN`, infinities) are written as `null`. In practice
this appears only for degenerate inputs such as a benchmark whose mean is zero.

## Example

```json
{
  "schema_version": 2,
  "tool": "tpt-benchmark",
  "tool_version": "0.1.0",
  "environment": {
    "commit": "9f2c1ab0d4e5",
    "timestamp": "2026-11-23T04:05:06Z",
    "os": "linux",
    "arch": "x86_64",
    "cpu": "AMD EPYC 7763 64-Core Processor",
    "cores": 4,
    "rustc": "rustc 1.91.0 (f1e2d3c4b 2026-10-01)",
    "clock_source": "tsc",
    "runner": "github-actions"
  },
  "benchmarks": [
    {
      "name": "sums::sum/128",
      "layers": [
        {
          "layer": "WallClock",
          "mean": 412.7,
          "std_dev": 3.1,
          "median": 411.9,
          "ci_low": 411.8,
          "ci_high": 413.6,
          "samples": 37,
          "values": [412.0, 409.5, 415.2],
          "outliers": 2,
          "iters_per_batch": 2560,
          "stop_reason": "converged",
          "noisy": false
        }
      ]
    }
  ]
}
```

(`values` is shortened here. A real export lists every accepted value.)

## Compatibility

- Schema 2 is schema 1 plus the `environment` object. Nothing in `benchmarks` changed,
  so a reader written for schema 1 can read schema 2 files by ignoring the new field.
  A reader of schema 2 must treat a missing `environment` (a schema 1 file) as unknown.

- Adding an optional field is not a breaking change, and does not bump the version.
- Renaming or removing a field, or changing a unit, bumps `schema_version`.
- Golden-file tests in `crates/tpt-benchmark/src/json.rs` guard the layout.
