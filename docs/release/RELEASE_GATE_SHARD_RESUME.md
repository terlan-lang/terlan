# Release Gate Shard Resume

This document is the contract for making release gates shardable, resumable, and non-redundant.

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

The canonical check enters its validation graph through the live Rust coverage
owner, which verifies exact completed selections against current inputs. A
caller-supplied "suite already ran" flag is not evidence. Release refresh exports
its release scope into that same graph; composition uses shared prerequisites
instead of a second recursive Make traversal.

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

The static contract gate persists release-gate-shard-resume-report.json describing
the requirements for:

- gate DAG
- cache keys
- skipped gates
- executed gates
- shard timings
- resume command
- first-failure decision
- collect-all decision

This report checks the documented contract and selected Make wiring. Its fields
describe requirements, not observed launches, timings, cache hits or recovery.
It is not evidence that an interrupted candidate actually resumed successfully.
That claim requires preparation-owner records and executed cold/warm/interrupted
acceptance demonstrating the same pass/fail result, diagnostics, report contents,
benchmark inclusion and support-bundle paths as the canonical serial run.

## Proof-binding preparation

Publication preparation assigns the feature matrix to `proof-feature-binding`
and the ordered diff, impact, review and snapshot chain to
`proof-binding-snapshot`. The latter depends on the matrix receipt and stages
all nine JSON/TSV outputs before committing them. Failed production cannot
replace the last successful snapshot. The compiler identity, source files,
upstream reports and output contracts participate in reuse decisions.

The snapshot owner binds the current UTC date so dated policy decisions are
not reused on a later day; the matrix can still be reused. A UTC rollover
during preparation rejects the invocation and requires a retry. The ordinary
developer targets retain their individual commands, while the owned publication
path executes the snapshot test module once with `TERLAN_LEAN_SNAPSHOT_TASK=all`.
It does not replay that module for each dependent Make target.

Run `make release-preparation-proof-binding-check` for isolated cold/warm,
changed-input, day-change, partial-failure and corrupt-output recovery tests.
These fixture checks do not approve changes to the accepted proof baseline
and cannot substitute for current-candidate proof evidence.
