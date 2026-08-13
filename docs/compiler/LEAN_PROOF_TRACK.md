# Terlan Lean Proof Track

This document records the release-owned formal proof boundary for Terlan
0.0.7. The repository ships executable CoreIR arithmetic, checked-lowering,
shape implication, NativeBoundary, semantic smoke, and feature-cull rejection
families while keeping every broader obligation explicit in the gap manifest.

The gate is `make lean-proof-track-check`.

## Inventories

- `docs/compiler/proof_track/lean_proof_inventory.tsv` classifies every Lean
  proof source or records that no proof tree is present.
- `proofs/lean/gaps/*.toml` is the strict per-gap lifecycle authority;
  `docs/compiler/proof_track/lean_proof_gaps.tsv` is its exact release index
  and classifies every accepted
  proof gap for the 0.0.7 language, CoreIR, VM, Wasm, and native-boundary
  contract surface.
- `docs/compiler/proof_track/lean_proof_gap_transitions.tsv` records the
  ordered, content-addressed lifecycle history of every accepted gap.
- `proofs/lean/ci/lean-proof-artifacts.tsv` classifies every executable proof
  by theorem scope, targeted manifests, expected process result, and digest.
- `proofs/lean/artifacts/*.json` records replay commands, pinned toolchains,
  dependency-set hashes, manifest fingerprints, timestamp policy, and expected
  output classes for each current proof family.
- `proofs/lean/smoke/smoke-manifest.tsv` binds the concrete parser-to-CoreIR,
  VM, NativeBoundary, and unsupported-target smoke properties to proof
  families, roadmap ownership, and runtime counterparts.
- `proofs/lean/smoke/0.0.7-signatures.json` is the content-addressed
  compatibility baseline shared by local and CI execution.

## Rules

- A Lean proof row must name the source contract it proves, the Terlan version
  it targets, its status, and the gate that owns it.
- Non-absent Lean proof rows must correspond to real files under `proofs/lean`.
- Every `current` row must have executable artifact metadata, a matching source
  digest, and a successful Lean invocation under the clean proof environment.
- `make proof_repro_check` deletes local proof build output, executes each
  current family twice through pinned `lake env lean` flags, normalizes paths
  and line endings, and requires identical output signatures.
- Artifact, manifest, dependency, and output drift becomes an explicit
  `proof_gap`; a `nondeterministic` family must carry a remediation plan and
  cannot remain current.
- `make lean-proof-smoke-check` discharges every semantic smoke theorem, runs
  the matching VM/native runtime behavior and unsupported-target rejection,
  compares normalized signatures with the 0.0.7 baseline, and emits
  `lean-proof-smoke.json`. A divergence emits a synchronized blocker row and
  fails; an exception must be strict, owner-reviewed, expiring, and carry both
  a hard blocker and an executable remediation gate.
- Every ordered proof lane carries a smoke-health score of 100. Core lanes earn
  it from executable smoke coverage; an accepted-gap lane earns it only from
  its separately validated classified-gap evidence. Missing or lower scores
  fail both lane sealing and release closeout.
- `make lean-proof-track-release-closeout-check` validates normalized family
  records and the ordered eight-class `lean-proof-baseline.tsv`, then rejects
  missing proofs, toolchain/lockfile drift, non-current status, blockers, or a
  reproducibility verdict other than `pass` with stable
  `error[lean_proof_closeout_*]` identifiers.
- A current baseline class records every current family digest as a sorted,
  unique semicolon-separated SHA-256 set.
- `release-0-0-7-preflight` and `publish-preflight` require Lean proof closeout.
- A proof gap row must name the missing feature, lifecycle status, category,
  reason, remediation owner, planned gate, deadline or exception, blocker
  update date, blocker hash, and the concrete manifest or spec files covered
  by that accepted gap.
- Lifecycle status is one of `open`, `triaged`, `blocked`, `remediated`, or
  `closed`; committed `open` rows fail until triaged. Categories are
  `not_started`, `resource`, `model_gap`, `performance`, or `toolchain`.
- Lifecycle history starts at `none -> open`, advances exactly one state at a
  time, uses nondecreasing ISO dates and SHA-256 evidence, and must end at the
  status in the live gap manifest. Non-closed histories must end with the
  current blocker hash.
- A released exception uses `exception:<lane>@YYYY-MM-DD`, must name a fixed
  proof lane, must be approved by the remediation owner in the TOML record,
  and fails after its expiry. The blocker update date is ISO `YYYY-MM-DD` and cannot
  exceed the TTL in `lean_proof_gap_policy.toml`. The blocker hash is SHA-256
  over the UTF-8 feature, category, reason, and update date fields, each
  terminated by a zero byte. Editing the blocker or refreshing its timestamp
  without renewing the hash therefore fails the gate.
- `lean-proof-gate.json` records maximum `gap_staleness_days`, aggregate
  `gap_classification_confidence`, unresolved-open count, and per-gap metrics.
- A proof gap planned gate must be an existing `*-check` Make target, and the
  owner must be one of the accepted proof-track owner groups.
- `make lean-proof-track-gap-hygiene-check` rejects exact feature overlap
  between active gap rows and current proof inventory rows, non-executable
  planned gates, blockers that exceed the policy TTL, and closure notes that do
  not resolve to a current executable proof digest.
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
`- Proof-gap closure: \`feature\` restored by \`sha256:...\`: rationale`. The
hygiene gate requires exactly one entry for every `closed` gap, verifies the
digest against the current proof artifact inventory, and rejects entries for
gaps that are not closed.

- Proof-gap closure: `EBNF syntax preservation` restored by `sha256:b1af86ef1a14129efe0e7497472d71ee23fb68fb356539de82cfb2c4335e2789`: the generated grammar is fingerprint-bound to canonical EBNF and executable theorems cover the stable SyntaxOutput-to-checked-CoreIR boundary.
- Proof-gap closure: `native-boundary contracts` restored by `sha256:3671cd9f8b63956f45f40d20e76106b933cad57f5079d3c0285aaa734368ddc2`: executable theorems cover typed callsites, handle ownership and linearity, async policy, side-effect denial, and fail-closed usage, with row-level generated-manifest binding and VM runtime oracles.
