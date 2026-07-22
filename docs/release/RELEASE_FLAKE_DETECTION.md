# Release Flake Detection And Quarantine Policy

This document is the release flake detection and quarantine policy contract for
0.0.7. Release gates must use a deterministic flake-detection policy with
repeat counts, timeout multipliers, allowed nondeterminism, random seeds, temp
path normalization, clock isolation, and network/socket isolation rules.

The deterministic policy explicitly includes temp path normalization.

## Classification

Release gates must classify every nondeterministic failure as one of:

- fixed
- quarantined
- intentionally unstable

Every classification must include an owner, expiry date, linked issue, affected
gate, and explicit release impact.

Every quarantine record explicitly includes affected gate.

## Visibility

Quarantined tests must remain visible in release output and must not silently
reduce coverage, adversarial corpus coverage, benchmark comparability, VM
semantics coverage, or package compatibility coverage.

Quarantine visibility explicitly protects VM semantics coverage.

## Adversarial Cases

The adversarial matrix must include randomized test order, stale temp
directories, clock-dependent diagnostics, port reuse, race-prone watchers,
benchmark warmup variance, file-system ordering, and support-bundle path
leakage.

The adversarial matrix explicitly includes stale temp directories and
support-bundle path leakage.

## Report Evidence

The executable gate persists release-flake-detection-report.json with:

- repeated run summaries
- seeds
- failure signatures
- quarantine records
- expiry validation
- timeout classification
- release-blocking decisions

The report is release evidence that flaky behavior cannot silently pass,
silently reduce coverage, or improve benchmark comparisons by excluding
unstable cases.
