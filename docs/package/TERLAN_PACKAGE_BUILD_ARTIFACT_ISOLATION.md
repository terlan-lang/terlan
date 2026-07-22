# Terlan Package Build Artifact Isolation Contract

Terlan package and workspace build artifacts must be isolated by package
identity, version/source hash, target, and capability set.
Stale package artifacts must not affect build, test, docs, editor, package CLI,
or VM runtime results.

The package/workspace build artifact layout records:

- compiled modules
- VM artifacts
- generated docs
- generated bindings
- native artifacts
- test binaries
- diagnostics snapshots
- per-package caches

Incremental builds invalidate on:

- source hash
- package manifest hash
- lockfile hash
- target/capability hash
- stdlib hash
- compiler version
- generated binding hash
- native artifact hash
- environment/config inputs

Package artifacts are namespaced by:

- package identity
- version/source hash
- target
- capability set

Clean/check behavior covers package artifact directories and
workspace artifact directories. Clean/check dry-run output protects source,
lockfiles, package
caches, and live registry mirrors.

The adversarial artifact isolation corpus covers:

- stale module output
- stale generated binding output
- changed stdlib hash
- changed compiler version
- target drift
- package rename collisions
- concurrent builds
- partial failed builds
- clean commands deleting the wrong package artifacts

The package build artifact isolation report is
`package-build-artifact-isolation-report.json`. It records:

- artifact roots
- invalidation matrix
- stale-artifact fixtures
- clean dry-run output
- concurrency result
- diagnostics
