# Terlan Package API Compatibility Contract

Each release package publishes a public API manifest. The manifest records:

- package names
- modules
- exported functions
- types
- constructors
- shapes
- capabilities
- generated bindings
- docs anchors
- examples
- diagnostics
- target support

Compatibility checking compares the new package API manifest against the
previous published package version. Every changed item receives one explicit
diff classification:

- additive
- compatible tightening
- deprecated
- breaking
- private
- target-only
- generated-binding-only

Every package follows semantic version policy:

- patch releases cannot remove public APIs
- patch releases cannot break public APIs
- minor releases document additive surfaces
- breaking changes require major/pre-1 compatibility annotation
- breaking changes require migration guidance

Package-level diagnostics point to migration guidance when these public surfaces
change:

- imports
- symbols
- capabilities
- generated bindings
- target support

The adversarial API compatibility corpus covers:

- removed exports without version bump
- changed function arity
- changed type shape
- stale docs anchors
- target support drift
- generated binding drift
- capability drift
- package examples importing removed APIs

The package API compatibility report is
`package-api-compatibility-report.json`. It records:

- old manifest hashes
- new manifest hashes
- diff classifications
- required version bump
- migration coverage
- rejected unclassified changes
