# Release Gate Shard Resume

This document is the contract for making release gates shardable, resumable, and non-redundant in 0.0.7.

The release gate manifest must record every check, inputs, output artifacts,
dependency gates, expected reports, estimated cost, shard assignment, and
whether a gate may be skipped from a valid cache.

## Run Semantics

Release runs must stop at first failure by default, support an explicit
collect-all mode, and print the exact resume command for the next unchecked gate
without re-running completed gates.

`make release-evidence-refresh` owns expensive evidence production.
Evidence refresh and preflight are separate commands:
`make release-preflight` performs candidate-bound composition and final
integration validation, and preflight never executes completed gates. A late
failure therefore cannot replay the entire successful prefix.

`make release-check` is the version-neutral end-to-end entry point and resolves
the candidate version from workspace metadata.

## Cache Semantics

Gate caching must be content-addressed by:

- source files
- lock files
- generated artifacts
- tool versions
- environment contracts
- declared external dependencies

Cache hits must be invalidated when any declared input changes.

The candidate-bound composition records whether evidence was refreshed or
reused. Reuse is permitted across process and session boundaries only when the
input, gate-definition, toolchain, environment, output, and candidate
fingerprints still match.

## Shard Semantics

Shard execution must preserve:

- deterministic output ordering
- stable JSON summaries
- stable support-bundle layout
- identical final release decisions compared with a single-process serial run

## Adversarial Cases

The adversarial matrix must include:

- interrupted release runs
- stale cached reports
- reordered shards
- missing gate artifacts
- changed toolchain versions
- partial support bundles
- resume commands after failure

## Report Evidence

The executable gate persists release-gate-shard-resume-report.json with:

- gate DAG
- cache keys
- skipped gates
- executed gates
- shard timings
- resume command
- first-failure decision
- collect-all decision

The report is release evidence that resumed and sharded release runs preserve
the same pass/fail result, diagnostics, report contents, benchmark inclusion,
and support-bundle paths as the canonical serial run.
