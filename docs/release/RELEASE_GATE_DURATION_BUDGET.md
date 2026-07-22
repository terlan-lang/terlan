# Release Gate Duration Budget

This document is the contract for enforcing release gate duration budgets and
slow-test regression tracking in 0.0.7.

The release budget manifest must define per-gate and per-suite duration budgets
for these lanes:

- local development
- CI
- release preflight
- benchmark lanes
- stdlib checks
- VM semantics checks
- package checks
- editor/tooling checks

## Baseline Comparison

Duration budgets must compare against committed baselines using reports that are
stable machine-readable reports, not ad hoc console timing.
Every baseline row records:

- warmup
- cache state
- sharding mode
- hardware class
- explicit slow test labels

## Slow Test Labels

Slow tests must declare:

- why they are slow
- whether they are permanent release coverage
- whether they are one-off gate probes
- which faster unit tests protect the same behavior
- which fixture tests protect the same behavior

## Adversarial Cases

The adversarial matrix must include:

- timing report drift
- missing slow-test labels
- hidden sleeps
- accidental network waits
- repeated full builds
- benchmark lanes counted as correctness gates
- budget bypasses under sharded or resumed release runs

## Report Evidence

The executable gate persists release-gate-duration-budget-report.json with:

- gate timings
- baseline deltas
- slow-test labels
- hardware class
- cache mode
- shard mode
- budget decisions
- recommended split points

## Release Decision

Release cannot pass when gate duration regresses past the accepted threshold
without an explicit baseline update and rationale.

The gate fails when slow tests are unlabelled.
The gate also fails when correctness gates include accidental benchmark work.
The gate blocks any report where resumed/sharded runs hide repeated expensive work.
