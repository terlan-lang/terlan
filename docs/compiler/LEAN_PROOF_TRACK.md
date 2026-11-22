# Terlan Lean Proof Track

This document records the release-owned formal proof boundary for Terlan
0.0.7. The repository ships executable CoreIR arithmetic, checked-lowering,
shape implication, NativeBoundary, semantic smoke, and feature-cull rejection
families while keeping every broader obligation explicit in the gap manifest.

The gate is `make lean-proof-track-check`.

Focused consumers use `target/debug/terlan-quality lean-proof-replay` followed by
one or more exact `proofs/lean/*.lean` paths. This shares the track's isolated
replicas and Linux checkpoints; it never overwrites complete track reports with
a partial selection. Smoke and semantic-kernel checks batch their proof paths,
then run their distinct runtime oracles. Rust oracles must execute one passing,
non-ignored test: a successful process with zero matching tests is not evidence.

## Inventories

- `docs/compiler/proof_track/lean_proof_inventory.tsv` classifies every Lean
  proof source or records that no proof tree is present.
- `proofs/lean/gaps/*.toml` is the strict per-gap lifecycle authority;
  `docs/compiler/proof_track/lean_proof_gaps.tsv` is its exact release index
  and classifies every accepted
  proof gap for the 0.0.7 language, CoreIR, VM, Wasm, and native-boundary
  contract surface.
- `docs/compiler/proof_track/lean_proof_gap_transitions.tsv` records the
  ordered lifecycle history of every accepted gap.
- `proofs/lean/ci/lean-proof-artifacts.tsv` classifies every executable proof
  by theorem scope, targeted manifests, and expected process result.
- `proofs/lean/artifacts/*.json` records replay commands, pinned toolchains,
  dependency files, timestamp policy, and expected output classes for each
  current proof family.
- `proofs/lean/smoke/smoke-manifest.tsv` binds the concrete parser-to-CoreIR,
  VM, NativeBoundary, and unsupported-target smoke properties to proof
  families, roadmap ownership, and runtime counterparts.

## Rules

- A Lean proof row must name the source contract it proves, the Terlan version
  it targets, its status, and the gate that owns it.
- Non-absent Lean proof rows must correspond to real files under `proofs/lean`.
- Every `current` row must have executable artifact metadata and a successful
  Lean invocation under the clean proof environment.
- `make proof_repro_check` deletes local proof build output, executes each
  current family twice through pinned `lake env lean` flags, normalizes paths
  and line endings, and requires identical semantic outcomes.
- Artifact, manifest, dependency, and output failures become an explicit
  `proof_gap`; a `nondeterministic` family must carry a remediation plan and
  cannot remain current.
- `make lean-proof-smoke-check` shares the direct replay node. Runtime behavior
  and unsupported-target rejection remain independently executable checks.
- Every ordered proof lane carries a smoke-health score of 100. Core lanes earn
  it from executable smoke coverage; an accepted-gap lane earns it only from
  its separately validated classified-gap evidence. Missing or lower scores
  fail both lane sealing and release closeout.
- `make lean-proof-track-release-closeout-check` rejects missing proofs,
  toolchain failures, non-current status, unresolved policy violations, or a
  reproducibility verdict other than `pass`.
- `release-preflight` and `publish-preflight` require Lean proof closeout.
- A proof gap row must name the missing feature, lifecycle status, category,
  reason, remediation owner, planned gate, deadline or exception, blocker
  update date, and the concrete manifest or spec files covered by that accepted
  gap.
- Lifecycle status is one of `open`, `triaged`, `blocked`, `remediated`, or
  `closed`; committed `open` rows fail until triaged. Categories are
  `not_started`, `resource`, `model_gap`, `performance`, or `toolchain`.
- Lifecycle history starts at `none -> open`, advances exactly one state at a
  time, uses nondecreasing ISO dates, and must end at the status in the live gap
  manifest.
- A blocked obligation may append a `blocked -> blocked` review without
  rewriting its earlier history. It must have a later date and a non-empty
  `Review:` rationale; the latest review date must match the
  blocker update date. Review is not remediation or closure and does not approve
  an exception for another release.
- A released exception uses `exception:<lane>@release-<version>`, must name a fixed
  proof lane, must be approved by the remediation owner in the TOML record,
  and must match `workspace.package.version` in the source `Cargo.toml` while
  the gap is unresolved. A version change requires fresh owner approval.
  Approvals do not expire with calendar time, and blocker age is informational.
  `lean_proof_gap_policy.toml` requires release-scoped approval. Closed rows may
  retain their historical dated exceptions. The blocker update date is ISO
  `YYYY-MM-DD`.
- `lean-proof-gate.json` records maximum unresolved `gap_staleness_days`, aggregate
  `gap_classification_confidence`, unresolved-open count, and per-gap metrics.
- `lean-proof-runtime-policy.json` separately records validated runner
  configuration. Its limits are policy, not measured CPU, memory, or I/O usage;
  running that check does not replace completed proof verdicts.
- A proof gap planned gate must be an existing `*-check` Make target, and the
  owner must be one of the accepted proof-track owner groups.
- `make lean-proof-track-gap-hygiene-check` rejects exact feature overlap
  between active gap rows and current proof inventory rows, non-executable
  planned gates, approvals for a different release, and closure notes that do
  not resolve to a current executable proof path.
- Every current language/CoreIR/protocol proof-status manifest must be linked
  from an accepted proof gap until the matching Lean artifact is restored.
- Stale proof terminology for removed runtime and backend product contracts is
  rejected inside proof-track files and Lean proof files.
- `make lean-proof-feature-cull-check` binds every retired assumption to an
  explicit pre-VM rejection theorem and a current replacement gate. Its
  machine-readable map rejects stale active proof/matrix terms and restored
  fallback Make aliases.
- Aeneas/Rust verification is not treated as proof of Terlan semantics unless a
  bridge artifact explicitly connects the Rust property to the Terlan formal
  model.

The arithmetic and checked-lowering seeds do not prove complete CoreIR
preservation. The checked-lowering family proves deterministic lowering,
typing/evaluation preservation, and VM/JS/Wasm admission for its modeled
literal/addition/process seed only. The shape implication family proves the
closed structural-evidence model, not every language pattern or the full
compiler-to-Lean refinement. The feature-cull family proves only the one-way
removal boundary named by its map. The remaining language and runtime gaps stay
release-visible until their own executable proof families exist.

## Proof-gap closure changelog

Every accepted closure is appended here in the canonical form
`- Proof-gap closure: \`feature\` restored by \`proof/path.lean\`: rationale`.
The hygiene gate requires exactly one entry for every `closed` gap, verifies
the path against the current proof inventory, and rejects entries for gaps that
are not closed.

- Proof-gap closure: `EBNF syntax preservation` restored by `proofs/lean/parser_shape/ParserShape.lean`: the generated grammar is bound to canonical EBNF and executable theorems cover the stable SyntaxOutput-to-checked-CoreIR boundary.
- Proof-gap closure: `native-boundary contracts` restored by `proofs/lean/native_boundary/NativeBoundary.lean`: executable theorems cover typed callsites, handle ownership and linearity, async policy, side-effect denial, and fail-closed usage, with generated-manifest binding and VM runtime oracles.
