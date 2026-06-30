# Runtime VM Internals

This directory owns helper modules for the in-process Rust VM evaluator. The VM
executes checked Terlan CoreIR directly for REPL, tests, and experimental
runtime validation without going through Erlang source generation.

## Responsibilities

- Classify VM values and unsupported CoreIR shapes for stable diagnostics.
- Keep intrinsic, pattern, std-remote, and value helpers separate from the root
  VM evaluator.
- Preserve target-neutral CoreIR execution semantics for the supported subset.

## Public Surface

- `value`: renderable VM values, closures, and type classification helpers.
- `intrinsics`: supported CoreIR intrinsic evaluation.
- `patterns`: pattern binding and matching helpers.
- `std_remote`: supported std module remote-call behavior.
- `kind`: compact CoreIR expression and pattern names for diagnostics.

## Core Model

The parent `runtime::vm` module owns the evaluator loop and module table. This
directory owns the helper boundaries needed to keep that evaluator small enough
to reason about.

Important invariants:

- Unsupported CoreIR forms must fail with stable diagnostic text.
- Runtime values must remain Terlan-facing values, not backend-specific terms.
- Helper modules must not introduce a second backend or target-specific
  lowering path.

## Integration Points

- `runtime::vm`: calls these helpers while executing CoreIR.
- `formal_pipeline`: produces the checked CoreIR modules loaded into the VM.
- `vm/main.rs`: packages the same VM into the standalone experimental binary.

## Testing Notes

- `runtime/vm_test.rs` covers source-to-CoreIR execution through the evaluator.
- Add focused tests for every newly supported CoreIR expression or intrinsic.
