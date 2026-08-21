# ABI 1 Release Evidence

ABI 1 optimization and compatibility promotion uses executable gates. Missing,
malformed, stale, or failing evidence is a hard failure; a gate never converts
missing evidence into a skipped success.

## Commands

| Gate | Command | Input |
|---|---|---|
| Continuous fuzz | `make abi1-continuous-fuzz-check` | `target/abi1-evidence/continuous-fuzz.json` |
| Cross-target conformance | `make abi1-cross-target-conformance-check` | `target/abi1-evidence/cross-target-conformance.json` |
| Tail latency | `make abi1-tail-latency-check` | `target/abi1-evidence/tail-latency.json` |
| Zero-copy conformance | `make abi1-zero-copy-conformance-check` | Current implementation and behavioral-test owners |
| Specialization equivalence | `make abi1-specialization-equivalence-check` | `target/abi1-evidence/specialization-equivalence.json` |
| Trusted-adapter audit | `make abi1-trusted-adapter-audit-check` | Current NativeBoundary and worker sources |
| Release candidate | `make abi1-release-candidate-check` | All six prerequisite reports |
| Compatibility freeze | `make abi1-compatibility-freeze-check` | Candidate report and frozen compatibility baseline |

`TERLAN_ABI1_REVISION` is mandatory for measured producers. Cross-target
production additionally requires `TERLAN_ABI1_AARCH64_RUNNER`; the configured
runner must execute the aarch64 test binary rather than merely compile it.

## Common measured-evidence envelope

Measured gates consume JSON with this common envelope:

```json
{
  "schema": "terlan.abi1.gate-evidence.v1",
  "gate": "continuous-fuzz",
  "abi_version": 1,
  "managed_layout_profile": 1,
  "status": "passed",
  "revision": "source revision tested by the producer",
  "runs": []
}
```

The Make targets produce these envelopes before invoking their validators.
Descriptor fuzzing, actor-heap latency, and generic/specialized binary extraction
run as release-mode Rust integration tests. Cross-target evidence is composed
only after the same ABI probe executes successfully on both target binaries.

Continuous-fuzz runs contain `seed`, `cases`, `failures`, and a SHA-256
`corpus_digest`. At least three distinct seeds and 10,000 total cases are
required, with zero failures.

Cross-target runs contain `target`, `architecture`, `pointer_width`, `endian`,
`failures`, and `status`. Distinct little-endian 64-bit `x86_64` and `aarch64`
targets are mandatory.

Tail-latency runs contain `workload`, `samples`, `p95_ns`, `p99_ns`,
`p95_limit_ns`, and `p99_limit_ns`. Every workload requires at least 1,000
samples and must satisfy both declared limits.
Limits may tighten the repository policy of 100,000 ns p95 and 200,000 ns p99,
but evidence cannot raise either ceiling.

Specialization-equivalence runs contain `semantic_case`, `generic_digest`,
`specialized_digest`, `generic_status`, and `specialized_status`. Both paths
must pass and produce the same SHA-256 semantic-result digest.

## Local structural gates

Zero-copy conformance requires the checked borrowed binary view and its bounds,
relocation, semantic-type, and ownership tests. Trusted-adapter audit recursively
checks NativeBoundary, VM boundary, capability-worker, and native-worker Rust
sources and rejects in-process `unsafe`, raw C entry points, and trusted-in-shard
shortcuts.

## Promotion and freeze

The release-candidate gate accepts only normalized `validated` reports for all
six prerequisite gates. The compatibility-freeze gate additionally requires
`docs/runtime/ABI1_COMPATIBILITY_BASELINE.json` with schema
`terlan.abi1.compatibility-baseline.v1`, status `frozen`, ABI and managed-layout
version 1, the candidate's exact `release_revision`, and a non-empty
`contract_terms` array. Every frozen term must remain present in the normative
ABI specification. All measured prerequisite reports must identify the same
source revision before the candidate can be formed.

The compatibility baseline is intentionally absent while ABI 1 remains
`current-pre-freeze`. Adding that file is the explicit, reviewable freeze act;
the freeze gate cannot pass before it.
