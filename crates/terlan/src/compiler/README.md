# Compiler Internals

This directory owns Terlan front-end compilation and compiler-owned contracts.

## Responsibilities

- Parse Terlan source into syntax output.
- Resolve module and interface metadata.
- Typecheck modules and lower checked programs into CoreIR.
- Lower supported checked CoreIR through Terlan-owned NativeIR into relocatable
  target-native objects with Cranelift.
- Own compiler-level contracts such as API extraction and router syntax helpers.
- Expose explicit integration hooks when runtime or delivery modules need
  compiler diagnostics over typed declarations.

## Public Surface

- `syntax`: lexer, parser, formatter, and syntax contracts.
- `hir`: module and interface resolution.
- `typeck`: type checking and CoreIR lowering.
- `native_ir`: compiler-owned native values and operations plus the Cranelift
  object emitter. NativeIR is not a runtime serialization format. Fixed
  algebraic constructors retain the shared managed aggregate descriptor,
  deterministic union discriminant, exact field kinds, and semantic result
  identity; no parallel compiler-only layout model is permitted. Application
  lowering retains one canonical constructor table per module and uses it for
  both ordinary function bodies and suspended continuations. Reverse lexical
  liveness removes a dead constructor graph only when every field expression is
  allocation-only and effect-free; unknown calls, intrinsics, and unsupported
  expression shapes retain source evaluation and allocation. A live fixed
  constructor whose aggregate identity never escapes and whose uses are only
  direct named-field projections is scalar-replaced into ordered field locals.
  Every field expression is still evaluated exactly once in source order;
  aggregate uses, unknown fields, patterns, indexes, and unsupported control
  forms conservatively retain the managed allocation.
- `api_contract`: typed API contract extraction.

## Integration Points

- `commands`: invokes compiler phases for CLI operations.
- `backends`: consumes checked compiler output.
- `mobile`: consumes compiler contracts to emit mobile planning and shell
  metadata.
- `validation`: checks target and release contracts over compiler artifacts.

## Testing Notes

- Keep tests adjacent to the compiler phase they validate.
- Add adversarial tests for ambiguous syntax, unresolved symbols, and target
  profile mismatches.
