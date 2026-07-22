# Release Gate Report Schema

This document defines the versioned schema family for all 0.0.7 gate reports.

Every release gate report schema must include:

- gate identity
- input digests
- tool versions
- environment contract
- diagnostics
- coverage deltas
- benchmark data
- support-bundle references
- pass/fail decision
- release-blocking rationale

Every required `*-report.json` file in the roadmap must declare:

- schema version
- producing gate
- generation timestamp policy
- stable ordering rules
- path redaction rules
- compatibility policy

## Validation

Schema validation must run before release readiness.

The validator must reject reports that are missing required sections.
It must also reject reports that contain unstable absolute paths, contain
unredacted local user data, or use undocumented ad hoc fields.

## Adversarial Cases

The adversarial matrix must include:

- malformed reports
- unknown schema versions
- duplicated gate IDs
- missing input digests
- unstable object order
- path leakage
- partially written JSON
- stale reports from previous runs

## Report Evidence

The executable gate persists release-gate-report-schema-report.json with:

- schema inventory
- validated reports
- rejected reports
- compatibility matrix
- redaction decisions
- schema migration notes

## Release Decision

Release cannot pass when any planned gate emits an unversioned report.
Release cannot pass when any planned gate emits a malformed report.
Release cannot pass when any planned gate emits a stale report.
Release cannot pass when any planned gate emits a path-leaking report.
Release cannot pass when any planned gate emits a schema-incompatible report.

The gate fails if a new roadmap-required report is added without a schema entry
and validation fixture.
