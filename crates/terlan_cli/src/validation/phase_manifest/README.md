# Phase Manifest Validation Internals

This directory owns phase-manifest construction and validation for CLI compiler
checks. The implementation in `mod.rs` is centered on deterministic JSON
serialization tied to the current canonical syntax contract identity.

## Responsibilities

- Build stable phase output records for parse, resolve, typecheck, and CoreIR
  phases.
- Serialize phase manifests with syntax contract identity, dependency hashes,
  CoreIR contract hashes, CoreIR proof readiness, and CoreIR proof-coverage
  counts.
- Serialize source-to-artifact debug identity so diagnostics, CoreIR contracts,
  and generated artifacts can later be correlated back to Terlan source.
- Validate generated or loaded phase-manifest JSON before it is trusted.
- Provide best-effort manifest emission for error paths without masking the
  original command exit code.

## Public Surface

- `PhaseManifestDiagnostic`: diagnostic record emitted in phase manifests.
- `PhaseManifestSnapshot`: decoded manifest record used by tests.
- `create_phase`: constructs sorted phase output entries.
- `emit_phase_manifest`: writes a full validated manifest.
- `emit_or_log_phase_manifest_error`: writes an error manifest when possible.
- `validate_phase_manifest_contents`: validates serialized manifest JSON.
- `current_syntax_contract_identity`: loads the compiler's active syntax
  contract identity.

Public methods or values exposed to callers include `create_phase`,
`emit_phase_manifest`, `emit_or_log_phase_manifest_error`,
`validate_phase_manifest_contents`, and `current_syntax_contract_identity`.

## Core Model

A phase manifest is a deterministic JSON artifact that records which compiler
phases ran, their diagnostics, the source hash, interface hashes, CoreIR hash,
CoreIR proof readiness/counts, source-to-artifact debug identity, dependency
hashes, and the canonical syntax contract identity used by the compiler.

The main flow is:

1. Command code collects phase diagnostics.
2. `create_phase` sorts diagnostics and creates phase records.
3. `emit_phase_manifest` attaches syntax identity and serializes JSON.
4. `validate_phase_manifest_contents` verifies the serialized artifact before
   it is written or accepted.

Important invariants:

- Every emitted manifest carries the current syntax contract identity.
- Every emitted manifest carries a `debug_trace` block whose module, source
  path, and CoreIR hash agree with the top-level manifest identity.
- A manifest whose `core` phase is `ok` carries a non-zero CoreIR contract hash.
- Successful CoreIR manifests carry proof readiness and aggregate
  proof-coverage counts copied from `CoreModuleMetadata`.
- `none` proof readiness is reserved for empty skipped/error-path coverage
  placeholders.
- `no-expressions` proof readiness is valid for CoreIR artifacts that carry
  declarations or type payloads but no expression or pattern summaries.
- Checked-preservation totals are paired with structural evidence-kind counts
  so stale manifests cannot claim preservation evidence without naming the Core
  evidence category.
- Checked-preservation freshness buckets must partition preservation totals,
  making runtime-binding freshness obligations visible without reading full
  CoreIR contract text.
- CoreIR proof readiness must match the combined expression and pattern
  coverage buckets using the same precedence as compiler metadata construction.
- Constructor-resolution buckets must report zero unresolved constructor calls,
  chains, and patterns for manifests accepted by formal validation.
- Diagnostics are sorted before serialization for stable artifacts.
- Unparsed sources use `<unparsed>` as the module name.
- Manifest validation rejects empty phase names and empty diagnostic codes.

## Integration Points

- `commands/check`: emits manifests for single-file and directory checks.
- Formal compile path: fills `PhaseManifestDiagnostic` from parse, resolve, and
  typecheck diagnostics, then records the deterministic CoreIR contract hash,
  proof readiness, and proof-coverage counts when CoreIR is produced.
- `terlan_syntax`: supplies syntax contract identity validation.

## Edge Cases

- Error-path manifest emission is best effort and does not replace the original
  exit code.
- A manifest path with no extension may be treated by callers as a directory for
  per-module manifests.
- Syntax contract mismatches invalidate decoded manifests.

## Testing Notes

- Existing phase-manifest tests decode generated JSON through
  `validate_phase_manifest_contents`.
- Add focused tests when adding fields, because validation should reject
  malformed or stale artifacts before downstream tools consume them.
- Constructor-resolution tests cover unresolved call, chain, and pattern
  counters separately so future manifest fields cannot accidentally protect
  only one constructor surface. They assert the exact shared
  `phase manifest constructor candidates must resolve before formal validation`
  error string.
- Debug-trace tests cover source/module/CoreIR identity coherence so future
  backend artifact metadata cannot drift from the top-level manifest.
