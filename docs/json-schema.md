# JSON export schema

`tpt-benchmark` writes its raw statistical data to
`target/tpt-benchmark/tpt-benchmark.json` after every run. The document is
versioned, so CI tooling can check it before reading it.

Current schema version: **1**. The matching Rust constant is
`tpt_benchmark::json::SCHEMA_VERSION`.

## Top level

| Field            | Type    | Meaning                                                 |
|------------------|---------|---------------------------------------------------------|
| `schema_version` | integer | Schema version. Increases on any incompatible change.   |
| `tool`           | string  | Always `"tpt-benchmark"`.                               |
| `tool_version`   | string  | Version of the crate that wrote the file.               |
| `benchmarks`     | array   | One entry per benchmark that ran. See below.            |

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
  "schema_version": 1,
  "tool": "tpt-benchmark",
  "tool_version": "0.1.0",
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

- Adding an optional field is not a breaking change, and does not bump the version.
- Renaming or removing a field, or changing a unit, bumps `schema_version`.
- Golden-file tests in `crates/tpt-benchmark/src/json.rs` guard the layout.
