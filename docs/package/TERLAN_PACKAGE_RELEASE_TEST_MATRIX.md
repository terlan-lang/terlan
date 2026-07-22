# Terlan Package Release Test Matrix Contract

Every first-party package intended for a Terlan release declares:

- package type
- target support
- capability contract
- tests
- examples
- docs
- generated artifacts
- native artifacts
- publish readiness state

Package test matrices run from clean temporary workspaces. They use:

- installed compiler
- installed stdlib
- package lockfile
- local registry mirror
- VM default runtime
- verified alternate artifact when explicitly declared

The package command matrix covers:

- build
- test
- docs generation
- example execution
- formatter checks
- lint checks
- capability denial paths
- package resolver behavior
- lockfile behavior
- support-bundle output on failure

Native package rows also cover:

- binding generation
- native artifact discovery
- target compatibility diagnostics
- stale handle diagnostics
- cancellation behavior
- missing native dependency skips

The adversarial package matrix covers:

- packages with no examples
- packages with docs that do not compile
- packages that pass only from workspace paths
- missing capability tests
- stale generated bindings
- broken lockfiles
- missing target metadata
- publish-ready packages without tests

The package release matrix report is
`package-release-test-matrix-report.json`. It records:

- package rows
- target rows
- command results
- docs/examples coverage
- capability coverage
- native coverage
- skipped rows
- publish readiness status
