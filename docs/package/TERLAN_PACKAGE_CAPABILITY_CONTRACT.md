# Terlan Package Capability Contract

Every Terlan package declares the privileged capabilities it needs before the
package can be installed, built, typechecked, executed by the VM runtime,
included in release packaging, or exported in a support bundle.

Package capability declarations cover:

- filesystem
- network
- HTTP listener
- database
- NativeBoundary resources
- generated bindings
- native artifacts
- environment variables
- process spawning
- debugger hooks
- release-time hooks

Capability checks run at:

- install
- build
- typecheck
- VM runtime
- release packaging
- support-bundle generation

No ambient permissions are inherited from the host process.

Native packages declare:

- resource handle types
- blocking policy
- cancellation behavior
- target compatibility
- generated binding hash
- native artifact hash
- security review status

Package consumers receive a deterministic capability summary before privileged
behavior is used. The summary appears in:

- lockfiles
- diagnostics
- generated docs
- release reports

The package capability contract report is
`package-capability-contract-report.json`. It records:

- package capability matrix
- denied operation fixtures
- native resource inventory
- lockfile capability hashes
- diagnostic coverage

The adversarial capability corpus covers:

- undeclared filesystem access
- undeclared network access
- hidden NativeBoundary calls
- stale native artifact hashes
- capability drift between manifest and lockfile
- package import aliases bypassing checks
- generated bindings requesting extra capabilities
- runtime handles reused across package boundaries
