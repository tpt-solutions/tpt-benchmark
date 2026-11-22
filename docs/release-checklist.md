# Release checklist: v1.0.0

This is the maintainer's checklist for releasing `tpt-benchmark` and
`tpt-benchmark-macros` to crates.io. Work through it in order. Each step
assumes the previous ones passed. Nothing here is automated, and the publish
steps are irreversible once a version is on crates.io, so do not start them
until every check above them is green.

The two crates are published as a pair. The macros crate goes first, because
`tpt-benchmark` depends on it by version.

## 1. Before the release branch

- [ ] Every Phase 0 to Phase 5 item in `todo.md` is either checked or
      explicitly moved to the post-1.0 backlog.
- [ ] `CHANGELOG.md`: move the `[Unreleased]` entries under a new version
      heading with the release date, and add a fresh empty `[Unreleased]`.
- [ ] `README.md` status line is updated (it still says pre-1.0).
- [ ] The JSON export `schema_version` and the baseline `format_version`
      are unchanged, or the change is recorded in the changelog as breaking
      (see `docs/json-schema.md`).
- [ ] Public API review for 1.0 is done (Phase 5). Trait signatures and
      `#[non_exhaustive]` choices are final.

## 2. Name and metadata

- [ ] crates.io name check: `tpt-benchmark` and `tpt-benchmark-macros` are
      free, or already owned by the TPT Solutions account. Check on
      <https://crates.io/crates/tpt-benchmark> and
      <https://crates.io/crates/tpt-benchmark-macros>.
- [ ] Both `Cargo.toml` files have `description`, `keywords`, `categories`,
      `readme`, `documentation` and `repository`.
- [ ] `[package.metadata.docs.rs]` is set in both crates (`all-features`, and
      the targets `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`,
      `aarch64-apple-darwin`).
- [ ] Versions: both crates are at the release version, and the
      `tpt-benchmark` dependency on `tpt-benchmark-macros` names that version.

## 3. Quality gates (on every OS in CI)

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`, including the ignored hardware tests on a
      machine that has the counters (`-- --ignored`)
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] MSRV 1.85 job passes
- [ ] `cargo deny check` passes, with the licence audit of the full
      dependency tree (Phase 6)
- [ ] Cross-compile checks pass for `aarch64-unknown-linux-gnu` and
      `x86_64-unknown-linux-gnu`
- [ ] docs.rs build is verified locally: `cargo doc` with `--all-features`
      for each target in the metadata table

## 4. Dry run (no upload)

- [ ] `cargo publish --dry-run -p tpt-benchmark-macros`
- [ ] `cargo publish --dry-run -p tpt-benchmark`
      The library dry run can fail until the macros crate is on crates.io,
      because cargo resolves the path dependency against the registry. If it
      does, record the failure and rely on step 5 ordering.
- [ ] Inspect the packaged file list (`cargo package --list`) for both crates.
      Confirm that no benchmark output, baseline files or `target/` content
      is included.

## 5. Publish (irreversible)

- [ ] `cargo publish -p tpt-benchmark-macros`
- [ ] Wait until the new version appears in the crates.io index.
- [ ] `cargo publish -p tpt-benchmark`
- [ ] Check both pages on crates.io and the docs.rs builds. A failed docs.rs
      build is fixed with a patch release, not by re-publishing.

## 6. Tag and release

- [ ] Commit the release changes on `master`, with the maintainer's own
      commit message.
- [ ] Tag: `git tag -a v1.0.0 -m "tpt-benchmark v1.0.0"`, then push the tag.
- [ ] Create the GitHub release from the tag, and paste the `CHANGELOG.md`
      section into it.
- [ ] Confirm the release workflow, if one exists, ran on the tag.

## 7. Announce

- [ ] Update the README badges and install instructions to the released
      version.
- [ ] Post the announcement, linking the crates.io page and the changelog.
- [ ] Mark the Phase 6 milestone done in `todo.md`.

## Rollback

A published version cannot be removed, only yanked (`cargo yank`). If a
release is broken, yank it, then publish a fixed patch version.
