# Proof exceptions for Terlan 0.10.0

On 2026-10-27 the repository owner explicitly approved carrying the six existing
release-scoped proof exceptions from 0.0.9 into 0.10.0:

- Typed CoreIR preservation (compiler).
- Target-profile inference (compiler).
- VM execution subset (vm).
- Pattern and operator coverage (compiler).
- Wasm CoreIR lowering (compiler).
- Aeneas Rust verification bridge (formal).

This renewal changes only the release association in the six gap manifests, the
gap registry, and the feature-slice index. All six obligations remain blocked.
Their reasons, remediation owners, planned gates, blocker dates, and evidence
hashes remain unchanged. The historical blocker reviews are preserved.

The renewal does not establish additional proof coverage or replace any proof,
runtime oracle, test, artifact check, or publication gate. Current-candidate proof
evidence must still be reproduced and its baseline changes reviewed before the
release can pass validation.

The candidate replay subsequently validated all 26 proof replicas, the six
semantic-kernel families, and eight ordered proof lanes. The baseline proposal
preserved all 14 slice trace hashes, with no added or removed slices. Its changed
candidate materials were limited to the workspace version in Cargo.toml and
Cargo.lock and the approved exception release associations in the feature-slice
index. The reviewed baseline refresh therefore updates only candidate identity
and the normalized evidence digest; proof coverage and semantic traces remain
unchanged.
