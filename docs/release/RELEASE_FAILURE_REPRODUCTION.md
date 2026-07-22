# Release Failure Reproduction

This document is the contract for exact local reproduction for release failures
in 0.0.7.

Every release gate failure must emit:

- exact reproduction command
- required environment variables
- input fixture path
- random seed
- target profile
- cache mode
- shard ID
- relevant report/support-bundle paths

## Command Stability

Reproduction commands must be stable across local and CI runs.
They must not depend on absolute checkout paths.
They must work after support bundles are unpacked into a fresh temporary directory.

## Narrow And Broad Reproduction

Failed gates must provide narrow reproduction commands for the failing test/case.
Failed gates must also provide broader reproduction commands for the owning suite.
Each report must include clear guidance on when each is valid.

## Adversarial Cases

The adversarial matrix must include:

- stale reproduction commands
- missing seeds
- path-dependent fixtures
- deleted temp directories
- sharded failures
- cached failures
- benchmark failures
- VM runtime failures with captured source maps

## Report Evidence

The executable gate persists release-failure-reproduction-report.json with:

- failure samples
- reproduction commands
- fixture digests
- support-bundle replay results
- path-redaction decisions
- command success status

## Release Decision

Release cannot pass without a working reproduction command for the failing case.

The gate fails if reproduction depends on local checkout paths.
The gate fails if reproduction depends on stale caches.
The gate fails if reproduction depends on untracked files.
The gate fails if reproduction depends on CI-only state.
The gate fails if reproduction depends on hidden environment assumptions.
