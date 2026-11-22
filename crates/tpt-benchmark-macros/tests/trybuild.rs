//! Compile-time checks: valid usage compiles, invalid usage reports a spanned
//! error. Snapshots live next to each case as `.stderr` files.

#[test]
fn ui() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/*.rs");
    cases.compile_fail("tests/ui/fail/*.rs");
}
