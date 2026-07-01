# Terlan VM Artifact Format

Status: 0.0.7 baseline contract.

The Terlan VM artifact is the compiler-owned runtime artifact for the post-OTP
default runtime path. It is not Erlang source, not BEAM bytecode, not an OTP
application file, and not a NIF ABI contract. The artifact is derived from
CoreIR or a later runtime IR owned by the Terlan compiler.

## Contract

The artifact must be deterministic. The same checked input program, compiler
version, target profile, and enabled capabilities must produce the same module
records, function records, source map records, native boundary records, and
checksum values.

The first schema must include these required fields:

```text
schema_version
artifact_kind
compiler_version
target_profile
module
exports
functions
types
constants
capabilities
native_boundary
source_map
debug
checksum
```

`artifact_kind` identifies the artifact as a Terlan VM artifact. It must not
identify the artifact as Erlang, BEAM, OTP, or a generated source package.

## Module Records

Each module record describes a checked Terlan module:

- module name
- package name
- imports already resolved by the compiler
- exports visible to other Terlan modules
- public types and constructors
- runtime functions that cannot be compiled away
- source map ranges for diagnostics and debugging

Generated Erlang module names are not part of the default artifact identity.

## Function Records

Each function record describes executable runtime behavior:

- function name
- arity
- parameter types
- return type
- local variable layout
- runtime instructions or lowered runtime IR
- required host capabilities
- source map ranges

Pure behavior that the compiler can safely lower to native code, JavaScript, or
another non-VM target does not have to remain in the VM artifact.

## Type Records

Type records preserve the checked shape needed by the runtime boundary:

- atoms
- structs
- constructors
- generic instantiations
- trait obligations that affect runtime dispatch
- resource handle types

The runtime validates type identity and constructor identity from this metadata;
it does not infer types from Erlang terms or BEAM tags.

## Capabilities

Capabilities describe effects and host access required by the artifact:

- filesystem
- network
- HTTP
- Postgres
- clock
- random
- process
- native module
- WASI worker

The VM must reject an artifact that requires undeclared or unavailable
capabilities. Capability validation is part of artifact loading, not a later
best-effort runtime check.

## Native Boundary

Native boundary records describe typed host calls and resources:

- native module name
- function name
- arity
- argument types
- result type
- blocking policy
- cancellation policy
- cleanup policy
- required host capability
- resource handle ownership rules

Native boundary calls are VM-owned runtime calls. They are not NIF calls and do
not expose raw pointers, BEAM environments, or OTP scheduler assumptions.

## Source Maps

Source map records connect runtime diagnostics back to Terlan source:

- source file
- module
- function
- instruction or runtime IR offset
- line and column range
- generated artifact location

All runtime errors exposed to users should prefer Terlan source locations over
artifact internals.

## Validation

The VM loader must reject artifacts with:

- unknown schema versions
- unsupported artifact kinds
- unresolved imports
- unresolved constructors
- unresolved trait obligations
- missing source map data for user-facing functions
- undeclared host capabilities
- invalid native boundary records
- nondeterministic ordering where the schema requires stable order
- default Erlang source requirements
- default BEAM bytecode requirements

## Non-Goals

The default Terlan VM artifact does not promise OTP compatibility, BEAM opcode
parity, `.erl` emission, `.beam` emission, Rebar compatibility, or NIF ABI
compatibility. OTP and BEAM may be used as reference material while the runtime
transition is underway, but they are not the product runtime contract for the
0.0.7 default path.

## Gate

The contract is guarded by:

```bash
make terlan-vm-artifact-format-check
```
