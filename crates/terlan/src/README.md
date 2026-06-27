# Terlan CLI Source Internals

This directory owns the `terlc` command-line compiler entry point and command
routing. It coordinates syntax, HIR, type checking, backend emission, standard
library checks, release artifacts, and developer tooling without owning the
language semantics implemented in the compiler crates.

## Responsibilities

- Parse CLI arguments and dispatch to command modules.
- Keep user-facing command behavior separate from compiler phase
  implementations.
- Wire compiler crates together for check, build, test, REPL, docs, bind, and
  serve paths.
- Preserve stable command diagnostics and release preflight behavior.

## Public Surface

- `main.rs`: executable entry point.
- `commands`: command implementations.
- `formal_pipeline`: formal compiler pipeline helpers.
- `validation`: release and compiler contract checks.
- `support`: shared CLI support helpers.

## Core Model

The CLI is orchestration code. It reads project/source inputs, selects target
profiles, invokes compiler phases, writes artifacts, and reports diagnostics.
It should avoid duplicating parser, typechecker, backend, or std behavior.

The main flow is:

1. Parse command-line options.
2. Route to the selected command module.
3. Invoke compiler phases and backend emitters.
4. Write artifacts or print diagnostics.

Important invariants:

- `main.rs` stays small and routes only.
- Internal scratch commands must not leak into top-level user help.
- Command tests stay outside implementation files.

## Integration Points

- `terlan_syntax`, `terlan_hir`, and `terlan_typeck`: compiler frontend.
- `terlan_erlang`, JS emission modules, and Rust/native metadata emitters:
  backend paths.
- `std/`: standard-library sources, summaries, and release tests.
- Makefile/CI release targets.

## Edge Cases

- Missing external tools such as `erlc` should produce stable diagnostics when
  a command actually requires them.
- Target-profile aliases must resolve before imports are validated.
- Release commands must not mutate committed source or generated summaries.

## Types And Interfaces

`commands`
: Public CLI command module tree.

`formal_pipeline`
: Shared phase orchestration used by formal compiler-path tests.

`validation`
: Contract and release gate implementations.

## Testing Notes

- CLI tests live in adjacent `*_test.rs` modules and split `tests/`
  directories.
- Exact cargo tests are used by release preflight for high-value regressions.
- Shell scripts should be minimum validation; prefer Rust or Terlan tests.
