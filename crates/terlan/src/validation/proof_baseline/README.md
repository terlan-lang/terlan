# Proof Baseline Validation Internals

This directory owns the static LP8 proof-baseline contract used by CLI formal
gates. `mod.rs` exposes the small immutable records and reusable validators;
`contracts.rs` and `manifests.rs` hold the static fixture tables that describe
which phase-contract fixtures are trusted as compiler-side Lean handoff inputs.

## Responsibilities

- Name the gate-backed LP8 phase-contract fixtures.
- Name the pinned LP8 next-model candidate fixtures that are not Lean-covered
  yet.
- Record required CoreIR contract snippets for each fixture.
- Record required phase-manifest `core_proof_coverage` counters for each
  fixture.
- Keep proof-baseline expectations out of the CLI entrypoint test code.
- Keep large static baseline tables out of `mod.rs` so the public validation
  surface remains reviewable.

## Public Surface

- `ContractBaseline`: fixture name plus required CoreIR contract snippets.
- `ManifestBaseline`: fixture name plus required manifest proof counters.
- `ManifestCount`: expected `core_proof_coverage` field/value pair.
- `contract_baselines`: returns all gate-backed contract-text expectations.
- `manifest_baselines`: returns all gate-backed manifest counter expectations.
- `next_lean_model_candidate_baselines`: returns contract-text expectations
  for compiler-pinned fixtures that must remain `proof-model-required`.
- `next_lean_model_candidate_manifest_baselines`: returns manifest counter
  expectations for compiler-pinned next-model fixtures.
- `validate_contract_baseline`: checks actual CoreIR contract text against one
  contract baseline.
- `validate_manifest_baseline_counts`: checks actual manifest counters against
  one manifest baseline through a field lookup closure.
- `validate_manifest_baseline_artifact`: checks manifest CoreIR hash,
  `lean-covered` readiness, and baseline counters together.
- `proof_baseline_phase_trait_pins_remote_dispatch_contract`: static
  regression coverage that keeps the `phase_trait` compiler-data blocker pinned
  while it remains a next-model candidate.

## Files

- `mod.rs`: public model types and reusable validation helpers.
- `contracts.rs`: CoreIR contract snippet baselines and next-model contract
  candidates.
- `manifests.rs`: phase-manifest proof-counter baselines and constructor
  counter field groups.
- `proof_baseline_test.rs`: static table shape and validator regression tests.

Public methods or values exposed to callers include the Lean-ready baseline
tables, the next-model candidate baseline tables, and the reusable validation
helpers.

## Core Model

The core model is a static contract table. It does not execute compiler phases
or parse artifacts. Callers supply actual CoreIR contract text or decoded phase
manifest JSON and compare those artifacts against the expected fixture
snippets/counts.

The main flow is:

1. A formal gate calls `contract_baselines` or `manifest_baselines`.
2. The gate locates each named phase-contract fixture.
3. The gate produces actual compiler artifacts through the normal formal path.
4. The gate compares actual artifacts against the baseline contract.
5. Future proof-export preflight code can call the same validation helpers
   before accepting compiler artifacts as theorem inputs.

Important invariants:

- Every listed fixture is protected by `formal-core-proof-gate`.
- Every Lean-ready manifest baseline requires `lean-covered` readiness in the
  caller.
- Every next-model candidate manifest baseline requires
  `proof-model-required` readiness in the caller.
- Every next-model candidate contract baseline must pin at least one concrete
  `:proof=proof-model-required` Core form, so Lean-covered child expressions do
  not hide the blocked expression that still needs promotion.
- Every next-model candidate's concrete `:proof=proof-model-required` contract
  snippets must match its manifest `proof_model_required` counter.
- A next-model candidate may have partial Lean anchors already. It must still
  remain `proof-model-required` until the roadmap explicitly promotes the
  compiler fixture and the expected CoreIR contract snippets/counters move into
  the Lean-ready baseline tables.
- Proof-coverage baselines include every expression and pattern coverage bucket,
  using explicit zeroes for non-covered categories.
- Constructor-resolution baselines include zero unresolved constructor
  candidate counters.
- Constructor-resolution baselines include the full resolved/unresolved
  call/chain/pattern bucket set, using explicit zeroes when a fixture does not
  exercise a constructor shape.
- Type-alias constructor identity coverage for local, directly imported, and
  aliased-imported calls, chain bases, and patterns is currently protected by
  focused CoreIR lowering and command-level manifest regressions. Do not add a
  new Lean phase-contract fixture for alias constructors while LP8 fixture
  scope is paused; promote one only after the roadmap explicitly resumes proof
  export for that exact CoreIR subset.
- Checked-preservation baselines include structural evidence-kind counters and
  explicit freshness partition counters for both expressions and patterns.
- While `phase_trait` is a next-model candidate, its contract baseline must
  still pin a `RemoteCall(...)` form with `:proof=proof-model-required`, so the
  selected blocker cannot drift away from remote-dispatch readiness.
- Handoff documentation consistency is validated by
  `docs/compiler/scripts/check_proof_baseline_docs.sh`, not by Rust crate tests. Release
  compiler crates must not include roadmap or proof prose with `include_str!`.

## Integration Points

- `crates/terlan/src/main.rs`: CLI regression tests iterate these tables
  while keeping command execution in the test harness.
- `docs/compiler/CORE_IR_LEAN_CONFORMANCE.md`: documents the same gate-backed
  baseline and next-model candidate sets for LP8 restart planning.
- `proofs/lean/Terlan/Core/Baselines.lean`: contains the matching Lean terms
  and theorem anchors for these compiler fixtures.

For `phase_trait`, the current Lean anchors cover the selected remote/scoped
call syntax, remote signature typing, staging-to-main typing bridge, argument
stepping, and value-ready runtime-boundary progress. The fixture remains a
next-model candidate until the compiler-side remote-dispatch readiness policy
decides whether that boundary is promoted to Lean-covered evidence.

## Edge Cases

- The module intentionally stores snippets instead of whole golden files because
  full CoreIR snapshots already live under `tests/fixtures/phase_contract`.
- The module does not validate fixture existence; callers own fixture lookup so
  they can report command-specific failure messages.
- Manifest readiness is asserted by callers because readiness is a string field
  outside the numeric counter table.

## Types And Interfaces

`ContractBaseline`
: Static CoreIR contract expectation for one fixture.

`ManifestBaseline`
: Static phase-manifest proof-counter expectation for one fixture.

`ManifestCount`
: One expected numeric field in `core_proof_coverage`.

## Testing Notes

- `run_phase_contract_lean_conformance_baselines_are_lean_covered` protects
  Lean-ready contract snippets.
- `run_check_phase_contract_lean_conformance_baselines_emit_manifest_evidence`
  protects Lean-ready manifest counters.
- `run_phase_contract_next_lean_model_candidates_are_pinned` protects
  next-model candidate contract snippets.
- `run_check_phase_contract_next_lean_model_candidates_emit_manifest_evidence`
  protects next-model candidate manifest counters.
- Module-local `proof_baseline_` tests protect the static table shape and
  reusable validator error paths. They also reject duplicate manifest counter
  fields in a baseline.
- Add a new baseline here before expanding LP8 proof export to additional
  compiler fixtures.
