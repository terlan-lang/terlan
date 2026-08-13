# CoreIR Lean Conformance

This release-owned snapshot records the Lean anchors that the core typing spec
may reference from `docs/compiler/type_spec/terlan_core_typing_spec.toml`.

The full proof tree lives under `proofs/lean/Terlan/Core` in the development
workspace. Release gates should not depend on files outside the repository
checkout, so this document keeps the anchor names stable for CI.

## Executable Lean-Covered Subset

The checked-in Lean tree currently proves only the following CoreIR contracts:

- integer literals and integer addition are typed and evaluable;
- lowering integer literals, addition, and the typed process-spawn seed is
  deterministic;
- that lowering preserves the modeled Core type and evaluation result;
- the VM profile admits the process-spawn seed while the shared-JavaScript and
  core-Wasm profiles reject it;
- closed structural-shape implication evidence preserves field access,
  lexical scope, and runtime values without conversion.

The executable families are
`proofs/lean/Terlan/Core/Arithmetic.lean`,
`proofs/lean/Terlan/Core/CheckedLowering.lean`, and
`proofs/lean/Terlan/Type/ShapeImplication.lean`. Their replay metadata records
the exact theorem names and source-contract fingerprints.

Constructors, general patterns, calls, effects, collections, closures,
case/receive semantics, and the rest of the CoreIR surface are not claimed as
Lean-proven in 0.0.7. They remain classified by the `typed CoreIR
preservation`, `target-profile inference`, `VM execution subset`, and
`pattern and operator coverage` rows in
`docs/compiler/proof_track/lean_proof_gaps.tsv` until executable families
replace those gaps.
