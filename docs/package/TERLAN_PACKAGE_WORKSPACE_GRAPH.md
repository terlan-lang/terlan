# Terlan Package Workspace Graph Contract

Terlan multi-package workspaces must be deterministic across build, test, lint,
format-check, package tree, package audit, docs generation, and release dry-run
workflows. Workspace behavior must not depend on package discovery order,
stale build artifacts, implicit local paths, or ambient registry state.

The workspace manifest records:

- multiple local packages
- shared lockfile
- shared registry mirror
- package graph roots
- local path dependencies
- package-level capabilities
- per-package target support

Workspace commands run all packages in deterministic topological order.

Local path dependencies are explicit, cannot escape the workspace root unless
configured, and are represented in lockfiles with path, package hash,
target metadata, and capability summary.

Workspace diagnostics cover:

- package cycles
- duplicate package names
- conflicting versions
- conflicting capabilities
- stale local path hashes
- mismatched target support
- cross-package generated binding drift

The adversarial workspace corpus covers:

- cyclic workspaces
- duplicate local packages
- path traversal
- hidden source-checkout dependencies
- stale shared lockfiles
- nondeterministic graph order
- package-specific target mismatch
- one package passing only because another package left build artifacts

The package workspace graph report is `package-workspace-graph-report.json`. It
records:

- workspace fixture paths
- package graph
- topological order
- lockfile hash
- per-package command results
- diagnostics
- artifact isolation checks
