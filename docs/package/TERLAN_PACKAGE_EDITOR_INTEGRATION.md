# Terlan Package Editor Integration Contract

Terlan package metadata must be usable through CLI, compiler, LSP, and editor
workflows from the same lockfile and installed package cache. Editor package
resolution must not reach into source checkout paths when installed package
metadata is available.

LSP package resolution covers package modules, exported functions, types,
shapes, docs, examples, capabilities, diagnostics, and generated binding metadata
from the project lockfile and installed package cache.

Editor completion covers package imports, exported symbols, methods,
constructors, capabilities, and documented examples. Completion results are
derived from package metadata and lockfile state.

Hover documentation shows package version, docs summary, target support,
capability requirements, deprecation status, generated binding provenance, and a
link to generated package docs.

Editor diagnostics match CLI diagnostics for missing package, stale lockfile,
yanked package, incompatible target, missing capability, and missing native artifact
cases. Diagnostics include fix suggestions where possible.

The adversarial editor package corpus covers stale LSP package cache,
package import aliasing, missing docs, yanked packages, generated binding drift,
editor command path leakage, package upgrade while editor is running, and
CLI/LSP diagnostic drift.

The package editor integration report is
`package-editor-integration-report.json`. It records:

- package fixtures
- completion snapshots
- hover snapshots
- diagnostic snapshots
- cache invalidation cases
- installed-tool paths
