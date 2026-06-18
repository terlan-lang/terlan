# Core Proof Internals

This directory owns small proof-support modules used by CoreIR conformance
checks. The implementation records compiler facts and evidence without making
normal compilation depend on an external proof runner.

## Responsibilities

- Build proof evidence from checked CoreIR modules.
- Keep module-level proof facts separate from lowering logic.
- Preserve stable data shapes for future Lean or Aeneas integration.
- Avoid backend-specific assumptions in proof metadata.

## Public Surface

- `evidence`: proof evidence records.
- `module_facts`: module-level facts derived from checked CoreIR.

## Core Model

Proof support is a side channel over the compiler pipeline. It observes
checked structures, records facts, and leaves runtime artifact emission to
normal backends.

The main flow is:

1. Receive checked module/CoreIR data.
2. Derive evidence and facts from compiler-owned structures.
3. Return stable payloads to validation or proof-facing code.

Important invariants:

- Proof helpers must not alter typechecking results.
- Evidence must be deterministic for the same source input.
- Backend details stay outside proof facts unless explicitly modeled.

## Integration Points

- `crate::core_proof`: root proof support API.
- `crate::core_ir`: source of checked backend-neutral structures.
- Validation commands consume proof payloads for release gates.

## Edge Cases

- Missing proof coverage is reported as validation debt, not a compile crash.
- Unsupported proof concepts should be explicit diagnostics.
- Future proof tools may consume these shapes without changing compiler output.

## Types And Interfaces

`evidence`
: Evidence values derived from CoreIR and typechecking.

`module_facts`
: Per-module facts used by proof and conformance checks.

## Testing Notes

- Proof support is covered by CoreIR and proof baseline tests.
- Any payload-shape change needs a validation fixture update.
- Backend-specific facts require a design note before implementation.
