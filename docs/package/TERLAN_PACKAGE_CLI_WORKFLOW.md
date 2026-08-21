# Terlan Package CLI Workflow Contract

Terlan package commands are deterministic release-artifact workflows. They run
from installed release artifacts in clean temporary workspaces and preserve user files
unless an explicit write action is requested.

The implemented Registry dependency surface is:

- `terlc package add`
- `terlc package remove`
- `terlc package update`
- `terlc package tree`
- `terlc package audit`
- `terlc package publish --dry-run`
- `terlc package cache clean --check`

Every implemented Registry command validates or updates manifests and
lockfiles deterministically. Live resolution requires an explicit Registry
origin and trust pin; offline resolution is explicit and reads only the
verified cache.

`package tree` reports the locked Registry graph. `terlc package audit` checks
the lockfile's Registry provenance, duplicate versions, and content-addressed
archive cache without modifying the workspace. Text output is intended for
people, while JSON output is deterministic for CI consumers. Network access is disabled
for this command; commands that access a live registry require an
explicit Registry origin and trust pin. A future signed advisory source can
extend the audit without weakening these local integrity checks.
Registry lock entries retain:

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
