//! The `main!` entry point.

/// Generates `fn main` for a benchmark binary declared with `harness = false`.
///
/// Each argument is the name of a module annotated with `#[tpt_bench_group]`.
/// The generated `main` collects every group's benchmarks and hands them to
/// the command-line runner.
///
/// ```ignore
/// tpt_benchmark::main!(matrix_ops, string_ops);
/// ```
#[macro_export]
macro_rules! main {
    ($($group:ident),+ $(,)?) => {
        fn main() -> ::std::process::ExitCode {
            let mut __tpt_defs: ::std::vec::Vec<$crate::BenchDef> = ::std::vec::Vec::new();
            $(
                __tpt_defs.extend($group::__tpt_benches());
            )+
            $crate::runner::main(&__tpt_defs)
        }
    };
}
