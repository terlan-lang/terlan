# Proof exceptions for Terlan 0.0.10

On 2026-10-27 the repository owner explicitly approved carrying the six existing
release-scoped proof exceptions from 0.0.9 into 0.0.10:

- Typed CoreIR preservation (compiler).
- Target-profile inference (compiler).
- VM execution subset (vm).
- Pattern and operator coverage (compiler).
- Wasm CoreIR lowering (compiler).
- Aeneas Rust verification bridge (formal).

This renewal changes only the release association in the six gap manifests, the
gap registry, and the feature-slice index. All six obligations remain blocked.
Their reasons, remediation owners, planned gates, and blocker dates remain
unchanged. The historical blocker reviews are preserved.

The renewal does not establish additional proof coverage or replace any proof,
runtime oracle, test, artifact check, or publication gate. Current-candidate proof
sources must still execute successfully before the release can pass validation.

The candidate replay validates every current proof family directly. Candidate
materials are the current source tree plus the approved exception release
associations; no stored evidence identity needs refreshing.
