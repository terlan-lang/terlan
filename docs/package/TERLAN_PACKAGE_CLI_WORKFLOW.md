# Terlan Package CLI Workflow Contract

Terlan package commands are deterministic release-artifact workflows. They run
from installed release artifacts in clean temporary workspaces and preserve user files
unless an explicit write action is requested.

The package CLI surface is:

- `terlc package add`
- `terlc package remove`
- `terlc package update`
- `terlc package tree`
- `terlc package audit`
- `terlc package publish --dry-run`
- `terlc package cache clean --check`

Every package command validates or updates manifests and lockfiles
deterministically. Text output and JSON output describe the same operation and
diagnostics. Network access is disabled unless the command explicitly requests a
live registry.

`package tree` and `package audit` report:

- target constraints
- capabilities
- native artifacts
- generated bindings
- yanked packages
- duplicate versions
- security warnings
- provenance warnings

The adversarial package CLI corpus covers:

- adding incompatible packages
- removing transitive dependencies
- update conflicts
- stale lockfiles
- yanked packages
- malformed package specs
- cache poisoning
- source-path leakage
- JSON/text output drift
- write operations without explicit consent

The package CLI workflow report is
`package-cli-workflow-report.json`. It records:

- command matrix
- before/after manifest hashes
- lockfile hashes
- output snapshots
- diagnostics
- cache behavior
