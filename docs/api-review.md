# Public API review for 1.0

This records the pre-1.0 review of the `tpt-benchmark` public API (2026-11-23):
what was changed, what was left open on purpose, and a proposed stability
policy. The public surface was listed from rustdoc's JSON output, so it covers
every item reachable from outside the crate, not only the ones in the README.

## Principle

After 1.0, removing or changing a public item, or adding a required method or a
public field to a constructible type, needs a major version. Adding things does
not. So the review favours a small surface now: anything that can be added later
without breaking anyone is left out until it is needed.

## Changes made

| Change | Why |
|--------|-----|
| `#[non_exhaustive]` on every struct the library produces: `Summary`, `BenchResult`, `Comparison`, `Baseline`, `Environment`, `Observations`, `Unavailable`, `HwSample`, `WallSample`, `AllocSample`, `AllocBytesSample` | New fields (a new statistic, a new environment probe) would otherwise be breaking. Users read these types, so nothing is lost. |
| `#[non_exhaustive]` on the settings structs `Config`, `SequentialEngine`, `Sprt` and `Options`, plus `with_*` setters on the first three | These are where new knobs will arrive, for example per-benchmark settings, which the Criterion migration guide lists as missing. `non_exhaustive` forbids `Config { .. }` expressions outside the crate, and assigning fields after `default()` trips clippy's `field_reassign_with_default`, so the setters are the idiomatic way: `Config::default().with_warmup(..)`. Fields stay `pub`, so reading them and assigning them still work. |
| `#[non_exhaustive]` on `BenchDef`, with `BenchDef::new` | The macros build it inside user crates. The macros now call `new`. |
| `StatisticalEngine` is sealed | The sampling loop uses `Config::engine`, a concrete `SequentialEngine`, so a custom engine could never be used. Freezing its three method signatures at 1.0 would limit later work for no benefit. Unsealing later is not breaking. |
| `MeasurementLayer` no longer requires `Send + Sync`; `HardwareCounter` is `!Send + !Sync` | A hardware counter follows the thread that opened it. See `unsafe-policy.md`. |
| `Unavailable` implements `Display` and `Error` | `HardwareCounter::try_new` returns it as an error, so it has to work with `?` and `Box<dyn Error>`. |
| `json` is a public module holding only `SCHEMA_VERSION` and `FILE_NAME`; its functions are crate-private | `json-schema.md` already cited `tpt_benchmark::json::SCHEMA_VERSION`, which was not reachable. `tpt-site` now uses the constant instead of a hard-coded `2`. |

Enums were already `#[non_exhaustive]` (`StopReason`, `Verdict`, `LayerKind`,
`Counter`, `ClockSource`, `SprtDecision`, `Throughput`).

## Resolved after review

These were the two questions that cannot be undone after 1.0, so they were
decided before it.

1. **Root re-exports.** Only re-exports that have another public path were
   dropped: `thread_dealloc_totals`, `AllocSample`, `AllocBytesSample` and
   `HwSample`, which stay under `alloc::` and `hwcounters::`. Nothing in the
   tests, examples, docs or `tpt-site` used the root paths. The rest stay at the
   root, including `WallSample` and `Welford`, because their modules are private
   and the root is their only public path. (An earlier draft of this review said
   all of them were reachable through modules. That was wrong for those two.)
2. **Public plumbing.** Nothing outside the crate used `runner::run_benches`,
   `runner::Options`, `runner::output_dir`, the whole `baseline` module, the
   `environment` module, or `report::render_terminal` and `report::write_html`.
   They are now crate-private. `report` stays public but `#[doc(hidden)]`,
   because it still holds `report::site_support` for `tpt-site`. The unused
   `runner::baseline_path` wrapper was deleted. The public modules are now
   `alloc`, `engine`, `hwcounters`, `json` (two constants) and `runner`
   (`main`, `BenchDef`, `BenchResult`, `LayerKind`). Making any of the hidden
   items public again later is not a breaking change.

## Still open for the maintainer

These are judgement calls, so they were not changed.

1. **`SampleData: Copy + Send + Sync + 'static`.** This constrains custom
   layers. It is probably stricter than needed now that layers are no longer
   `Send`. Loosening it later is not breaking, so it can wait.
2. **Layer units.** Custom layers have only a name, so the report shows their
   values as `value/iter`. A unit method with a default implementation can be
   added later without breaking anyone, so no change is needed now.
3. **`Welford` and `Sprt::decide`.** Both are public building blocks with no
   other use in the API. Keep them only if they are meant to be supported.

## Proposed stability policy

This is a proposal for the maintainer to confirm, not a policy already in force.

- **Semver covers** every public item that is not `#[doc(hidden)]`. That includes
  `#[non_exhaustive]` types, which may gain fields, variants and methods in a
  minor release.
- **`#[doc(hidden)]` items** (such as `report::site_support` and `__tpt_benches`)
  exist for the macros and `tpt-site` and carry no stability promise.
- **The JSON export** is versioned by `schema_version`. Adding optional fields
  does not bump it; renaming, removing or changing a unit does, and is a major
  release of the crate.
- **MSRV** is 1.85 on stable. Raising it is a minor release and is noted in the
  CHANGELOG.
- **Before 1.0**, minor versions may break the API, as the CHANGELOG says.
