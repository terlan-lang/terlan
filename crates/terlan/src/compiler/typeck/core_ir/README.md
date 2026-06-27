# CoreIR Typecheck Internals

This directory owns focused CoreIR helper modules used by the typechecker.
The implementation keeps CoreIR patterns, proof payloads, and type helpers
separate from the top-level CoreIR model.

## Responsibilities

- Hold CoreIR helper logic that would otherwise bloat `core_ir.rs`.
- Keep pattern and type representations backend agnostic.
- Preserve proof payload shapes consumed by formal and release checks.
- Provide small, testable transformations for CoreIR construction.

## Public Surface

- `intrinsics`: CoreIR intrinsic identities, runtime capabilities, and effect
  sets.
- `module`: Core module payload, metadata, contract rendering, and runtime
  boundary discovery.
- `patterns`: CoreIR pattern helpers.
- `proof_payloads`: proof-facing CoreIR payload structures.
- `types`: CoreIR type helpers.

## Core Model

CoreIR is the backend-neutral compiler representation after parsing and
typechecking. This directory contains submodels that are part of CoreIR but
are easier to reason about outside the root module.

The main flow is:

1. Typechecked source constructs are lowered into CoreIR data.
2. Helper modules model repeated CoreIR pieces such as patterns and types.
3. Backends and proof checks consume the resulting CoreIR structures.

Important invariants:

- CoreIR helpers cannot depend on a concrete backend.
- Proof payloads must stay deterministic and serializable where required.
- Type helpers must not reintroduce parser-level syntax decisions.

## Integration Points

- `crate::terlan_typeck::core_ir`: root CoreIR model and exports.
- `crate::terlan_typeck::core_lowering`: constructs CoreIR from checked modules.
- Backend crates consume CoreIR through public typecheck APIs.

## Edge Cases

- Unsupported source forms must fail before backend emission.
- Proof payload changes need matching proof/check updates.
- Pattern lowering must preserve constructor and atom distinctions.

## Types And Interfaces

`patterns`
: Pattern-focused CoreIR helpers.

`intrinsics`
: Backend-neutral intrinsic and runtime capability identifiers.

`module`
: Core module metadata, contract snapshots, and module-wide runtime boundary
discovery.

`proof_payloads`
: Structures emitted for proof and conformance checks.

`types`
: Type-focused CoreIR helpers.

## Testing Notes

- CoreIR tests live in adjacent `*_test.rs` modules under `terlan_typeck`.
- Changes to proof payloads require conformance or snapshot-style coverage.
- Backend-neutrality regressions should be tested before backend emission.
