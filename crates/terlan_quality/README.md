# Terlan Quality Internals

This crate owns permanent repository quality checks. The implementation is
centered on stable Rust scanners for rules that should remain part of the
compiler codebase over time. Its most important boundary is that release-only
or one-off policy probes can remain scripts, while durable engineering rules
belong here.

## Responsibilities

- Enforce permanent Rust source quality rules.
- Keep repository-maintenance checks out of user-facing `terlc` commands.
- Preserve stable diagnostics for Makefile and CI gates.
- Provide normal Rust tests for quality-rule behavior.

## Public Surface

- `terlan-quality rust-quality`: checks Rust file-size and inline-test
  baselines.
- `terlan-quality rust-docs`: checks Rustdoc coverage for implementation
  functions and types.
- `terlan-quality module-readmes`: checks README coverage for source-owning
  module directories.
- `terlan-quality cli-exact-selectors`: checks CLI Makefile exact-test
  selectors against Cargo's current test list.
- `terlan-quality test-hierarchy`: checks that Makefile script gates remain
  release-owned policy, drift, generator, or orchestration checks.
- `terlan-quality internal-docs`: checks that published docs do not contain
  scratch roadmap or research packets.
- `terlan-quality oxc-boundary`: checks that Oxc usage stays behind JavaScript
  backend and binding-generator ownership boundaries.
- `run_rust_quality`: library entrypoint used by the command wrapper and tests.
- `run_rustdoc`: library entrypoint for Rustdoc coverage validation.
- `run_module_readmes`: library entrypoint for README coverage validation.
- `run_cli_exact_selectors`: library entrypoint for exact-selector validation.
- `run_test_hierarchy`: library entrypoint for Makefile script-gate
  validation.
- `run_internal_docs`: library entrypoint for published-doc leakage validation.
- `run_oxc_boundary`: library entrypoint for Oxc ownership validation.

## Core Model

The crate scans repository files, compares the current state with checked-in
baselines, and reports growth or stale baseline rows.

The main flow is:

1. Discover Rust files under `crates/`.
2. Load quality baselines from `tools/quality`.
3. Report oversized files, missing docs, stale rows, missing READMEs, or
   disallowed inline test modules.
4. Compare checked-in exact-test selector recipes with Cargo's current test
   list when validating CLI Makefile gates.
5. Classify Makefile script gates so durable behavioral checks keep moving into
   Rust tests instead of hidden shell or Python scripts.
6. Reject internal roadmap, baseline, checkpoint, scratch, or research packets
   under published `docs/`.
7. Keep Oxc dependencies and symbols confined to the approved JS backend and
   binding-generator ownership boundary.

Important invariants:

- New tests should live in adjacent `*_test.rs` modules.
- Large Rust files must be split instead of silently growing.
- Existing debt must be explicit in reviewed baselines until it is removed.

## Integration Points

- `Makefile`: invokes `terlan-quality rust-quality`,
  `terlan-quality rust-docs`, and `terlan-quality module-readmes`.
- `tools/quality`: stores migration baselines.
- `crates/*`: scanned source tree.

## Edge Cases

- Rustdoc mentioning `#[cfg(test)]` is ignored; only actual attribute lines are
  checked.
- Adjacent `#[path = "*_test.rs"]` modules are allowed.
- Missing baseline files are surfaced as hard check failures.

## Types And Interfaces

`RustFile`
: Repository-relative Rust file measurement.

`RustQualitySummary`
: Success metrics rendered by the command-line wrapper.

## Testing Notes

- `src/lib_test.rs` covers clean fixtures, oversized files, stale baselines,
  inline tests, and doc-comment false positives.
- Add focused fixture tests whenever a new permanent quality rule is added.
