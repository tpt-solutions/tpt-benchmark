# Dependency shortlist and licences

Every dependency is checked by `cargo deny check licenses` against `deny.toml`.
Apache-2.0-only, GPL, LGPL, AGPL and other copyleft licences are rejected.
Dual `MIT OR Apache-2.0` crates pass through their MIT arm.

| Crate | Used by | Licence |
|-------|---------|---------|
| serde, serde_json | core (baselines, JSON export) | MIT OR Apache-2.0 |
| plotters 0.3 (svg_backend, line_series) | core (HTML charts) | MIT |
| serde, serde_json | tpt-site (not published) | MIT OR Apache-2.0 |
| pulldown-cmark 0.13 (no default features) | tpt-site (not published), docs rendering | MIT |
| libc 0.2 | core, Linux only (perf_event_open) | MIT OR Apache-2.0 |
| proc-macro2, quote, syn 2 | macros | MIT OR Apache-2.0 |
| trybuild | macros (dev only) | MIT OR Apache-2.0 |

Transitive dependencies of the above (num-traits, memchr, termcolor, the toml
family, windows-sys, unicode-ident and others) were all checked by
`cargo metadata` and are MIT, MIT OR Apache-2.0, Unlicense OR MIT, or
(MIT OR Apache-2.0) AND Unicode-3.0.

Not added: Criterion and Divan (planned as dev-only comparison benchmarks,
still to do).

Verify at any time with:

```sh
cargo deny check licenses
```
