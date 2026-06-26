# Terlan Quality Internals

This crate owns permanent repository quality checks that should be implemented
and tested in Rust.

## Responsibilities

- Enforce durable engineering rules, such as Rust file-size limits and the
  adjacent `*_test.rs` test-layout rule.
- Keep quality checks independent from user-facing `terlc` commands.
- Preserve stable diagnostics for Makefile and CI gates.

## Public Surface

- `terlan-quality rust-quality`: validates Rust source size and inline-test
  baselines.
- `terlan-quality rust-docs`: validates Rustdoc coverage for implementation
  functions and types.
- `terlan-quality module-readmes`: validates README coverage for source-owning
  module directories.
- `terlan-quality cli-exact-selectors`: validates CLI Makefile exact-test
  selectors against Cargo's current test list.
- `terlan-quality test-hierarchy`: validates that Makefile script gates remain
  release-owned policy, drift, generator, or orchestration checks.
- `terlan-quality internal-docs`: validates that published docs do not contain
  scratch roadmap or research packets.
- `terlan-quality oxc-boundary`: validates that Oxc usage stays behind
  JavaScript backend and binding-generator ownership boundaries.

## Testing Notes

Check logic belongs in normal Rust tests next to this crate. One-off release or
policy probes can stay as Python scripts when they are not permanent compiler
engineering rules.
