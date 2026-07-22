# OTP Runtime Exit

This document is the checked 0.0.7 removal inventory for stock OTP runtime
dependency. The 0.0.7 exit condition is no active stock OTP runtime dependency
in the default Terlan build, run, test, serve, package, or VM execution path.

## Contract

- `terlan-vm` is the runtime direction for Terlan-owned execution.
- `std.vm` is the public runtime abstraction surface.
- Generated Erlang and BEAM may remain only as reference-only material outside
  release execution.
- OTP tests may remain reference-only material, but not compatibility gates.
- The compiler must remove the generated Erlang default path.
- The compiler must remove the `erlc` execution path from default flows.
- The compiler must remove the `erl` runtime invocation from default flows.

## Active Removal Lanes

| Lane | Current shape | Required replacement |
| --- | --- | --- |
| `terlc run --target erlang` | removed public spelling; retained only as rejection coverage | `terlan-vm run` over VM artifacts |
| `terlc test --target erlang` | removed public spelling; retained only as rejection coverage | Terlan test runner on `terlan-vm` |
| `terlc repl --runtime beam` | removed public spelling; retained only as rejection coverage | VM REPL runtime |
| `terlc serve` dynamic handler execution | VM-handler-runtime-unavailable diagnostic; no BEAM handler execution | Terlan VM/native HTTP handler dispatch |
| `erlc` bridge | migration bridge | direct VM/native artifact emission |
| `erl` runtime invocation | migration bridge | Terlan VM process/runtime execution |

## Closeout Blockers

There are currently no active OTP/BEAM/Erlang-shaped closeout blockers. The
quality gates must fail if a new active backend path appears instead of
classifying it as accepted migration debt.

## Completed Removal Slices

- Removed the test-only native-boundary runtime command and its helper module.
  VM-owned native-boundary worker coverage is now tracked outside the old BEAM
  test launcher path.
- Removed `commands/test/command_runner.rs` by moving generic bounded process
  execution into `process_runner.rs` and leaving BEAM-specific crash-dump and
  atom quoting helpers inside the remaining legacy BEAM runner.
- Removed generated-Erlang phase-contract emit snapshots and backend parity
  tests from `crates/terlan/src/tests/mod.rs`; the module now keeps
  parse/resolve/type/CoreIR/Lean manifest coverage without OTP lowering. The
  unused `tests/fixtures/phase_contract/*.emit.golden` snapshots were deleted.
- Removed `commands/test/release_support.rs` and its embedded std BEAM support
  inventory tests; `terlc test` now routes through the VM/JS paths without
  compiling release-support BEAM modules.
- Removed `commands/test/beam_runner.rs` and its EUnit/export-injection tests;
  `terlc test` now has no BEAM/EUnit reference runner module.
- Removed the public `terlc emit` command path and its Erlang output tests.
  Phase determinism now compares VM build artifacts, SQL runtime coverage uses
  CoreIR SQL payload tests, and doctest validation stops lowering examples
  through the Erlang backend.
- Removed `crates/terlan/src/backends/erlang` and its backend-local tests. The
  compiler no longer registers an Erlang backend module, the Erlang backend
  classification gate now accepts zero active backend paths, and the OTP
  runtime closeout blocker count is zero.

## Deferred Reference Material

- OTP source and tests may be mined into Terlan-owned runtime conformance
  cases.
- BEAM opcode behavior may be used as reference-only material when it directly
  maps to a Terlan language feature.
- OTP compatibility must not be restored as a release gate.
