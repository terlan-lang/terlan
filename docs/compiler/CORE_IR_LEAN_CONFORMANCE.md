# CoreIR Lean Conformance

This release-owned snapshot records the Lean anchors that the core typing spec
may reference from `docs/compiler/type_spec/terlan_core_typing_spec.toml`.

The full proof tree lives under `proofs/lean/Terlan/Core` in the development
workspace. Release gates should not depend on files outside the repository
checkout, so this document keeps the anchor names stable for CI.

## Lean Covered Subset

Lean currently models the following initial Core forms:

- `Ty.int`
- `Ty.bool`
- `Ty.atom`
- `Ty.binary`
- `Ty.never`
- `Ty.tuple`
- `Ty.list`
- `Ty.struct`
- `Ty.arrow`
- `Ty.constructor`
- `Pattern.wildcard`
- `Pattern.var`
- `Pattern.int`
- `Pattern.atom`
- `Pattern.tuple`
- `Pattern.list`
- `Pattern.constructor`
- `Expr.int`
- `Expr.bool`
- `Expr.atom`
- `Expr.binary`
- `Expr.var`
- `Expr.tuple`
- `Expr.list`
- `Expr.listCons`
- `Expr.lam`
- `Expr.call`
- `Expr.caseOf`
- `Expr.ifThen`
- `Expr.fieldAccess`
- `Expr.constructor`
- `Expr.unaryOp`
- `Expr.binaryOp`

The Lean track proves typing, progress, and checked preservation for this
subset. Forms outside this list must be marked `proof-model-required`,
`runtime-boundary`, or `artifact-only` in the machine-readable type-spec index
until their Lean model and proof status are promoted.
