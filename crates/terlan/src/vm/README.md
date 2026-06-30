# Terlan VM Binary Internals

This directory owns the standalone `terlan-vm` binary. The binary is the
experimental Rust-native runtime artifact packaged beside `terlc`.

## Responsibilities

- Parse `terlan-vm` command-line arguments.
- Compile one Terlan source file through the normal compiler frontend.
- Load the resulting CoreIR into the Rust VM and execute a zero-arity
  entrypoint.
- Preserve text and test-evaluation output semantics for release validation.

## Public Surface

- `main.rs`: standalone binary entrypoint.
- `commands.rs`: command helpers shared by the VM binary surface.

## Core Model

The binary does not define a separate Terlan-to-VM compiler path. It reuses the
formal compiler pipeline, loads CoreIR into `runtime::vm::TerlanVm`, and runs
the requested function.

Important invariants:

- The VM binary must not bypass compiler validation.
- `--test-eval` accepts only boolean test results.
- User-facing errors must identify whether failure happened during read,
  compile, load, or execution.

## Integration Points

- `formal_pipeline`: source-to-CoreIR compilation.
- `runtime::vm`: CoreIR execution.
- Release packaging: installs `terlan-vm` beside `terlc`.

## Testing Notes

- `main_test.rs` covers argument parsing and source execution.
- Release preflight checks compare `terlc run` output with `terlan-vm run`
  output for the bridge fixture.
