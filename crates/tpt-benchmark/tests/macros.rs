//! End-to-end checks for the `#[tpt_bench]`, `#[tpt_bench_group]` and
//! `#[layers]` macros.

use std::time::Duration;

use tpt_benchmark::engine::SequentialEngine;
use tpt_benchmark::{
    BenchDef, Bencher, Config, Counter, HardwareCounter, LayerKind, black_box, layers, tpt_bench,
    tpt_bench_group,
};

#[tpt_bench_group]
#[layers(WallClock)]
mod matrix {
    use super::*;

    #[tpt_bench(args = [1, 2])]
    pub fn add(b: &mut Bencher, n: u64) {
        b.iter(|| black_box(n) + 1);
    }

    #[tpt_bench]
    #[layers(WallClock, Allocations)]
    pub fn noop(b: &mut Bencher) {
        b.iter(|| black_box(0_u64));
    }

    #[tpt_bench]
    #[layers(HwInstructions)]
    pub fn counted(b: &mut Bencher) {
        b.iter(|| black_box(0_u64));
    }
}

#[layers(HwBranchMisses)]
#[tpt_bench_group]
mod layered_above {
    use super::*;

    #[tpt_bench]
    pub fn plain(b: &mut Bencher) {
        b.iter(|| black_box(0_u8));
    }
}

#[test]
fn layers_above_the_group_apply_to_it() {
    let defs = layered_above::__tpt_benches();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].full_name(), "layered_above::plain");
    assert_eq!(defs[0].layers, &[LayerKind::HwBranchMisses]);
}

/// The function is kept, so it can still be called directly.
#[test]
fn bench_functions_remain_callable() {
    let config = quick_config();
    let summary = tpt_benchmark::measure(&config, tpt_benchmark::WallClock::new(), |b| {
        matrix::add(b, 7);
    });
    assert_eq!(summary.layer, "WallClock");
}

fn quick_config() -> Config {
    Config::default()
        .with_warmup(Duration::from_millis(1))
        .with_engine(SequentialEngine::default().with_max_time(Duration::from_millis(200)))
}

fn find<'a>(defs: &'a [BenchDef], name: &str) -> &'a BenchDef {
    defs.iter()
        .find(|def| def.full_name() == name)
        .unwrap_or_else(|| panic!("no benchmark named {name}"))
}

#[test]
fn group_registers_every_argument() {
    let defs = matrix::__tpt_benches();
    let mut names: Vec<String> = defs.iter().map(BenchDef::full_name).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "matrix::add/1",
            "matrix::add/2",
            "matrix::counted",
            "matrix::noop"
        ]
    );
    assert!(defs.iter().all(|def| def.group == "matrix"));
}

#[test]
fn layers_are_taken_from_attributes() {
    let defs = matrix::__tpt_benches();
    assert_eq!(find(&defs, "matrix::add/1").layers, &[LayerKind::WallClock]);
    assert_eq!(
        find(&defs, "matrix::noop").layers,
        &[LayerKind::WallClock, LayerKind::Allocations]
    );
    assert_eq!(
        find(&defs, "matrix::counted").layers,
        &[LayerKind::HwInstructions]
    );
}

#[test]
fn run_produces_a_summary_for_an_active_layer() {
    let defs = matrix::__tpt_benches();
    let config = quick_config();
    let def = find(&defs, "matrix::add/2");
    let summary = (def.run)(LayerKind::WallClock, &config).expect("WallClock is always available");
    assert_eq!(summary.layer, "WallClock");
    assert!(summary.samples >= config.engine.min_samples);
}

#[test]
fn run_returns_none_for_a_layer_outside_the_benchmark() {
    let defs = matrix::__tpt_benches();
    let config = quick_config();
    let def = find(&defs, "matrix::add/1");
    assert!((def.run)(LayerKind::AllocBytes, &config).is_none());
}

#[test]
fn unavailable_hardware_counter_returns_none() {
    // Only meaningful where the counter cannot be opened. Where it can, the
    // layer really runs, so there is nothing to assert.
    if HardwareCounter::try_new(Counter::Instructions).is_ok() {
        return;
    }
    let defs = matrix::__tpt_benches();
    let config = quick_config();
    let def = find(&defs, "matrix::counted");
    assert!((def.run)(LayerKind::HwInstructions, &config).is_none());
}
