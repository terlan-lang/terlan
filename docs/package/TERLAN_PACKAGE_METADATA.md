# Terlan Package Metadata

Status: 0.0.7 baseline contract.

Terlan package metadata is target-neutral. `terlan.toml` is the source
manifest, and `terlan-package-build.json` is the generated build metadata
artifact. Both describe Terlan package identity, dependencies, source roots,
capabilities, target profiles, and generated artifacts without making any
runtime or package manager the language contract.

## Hex

Hex may be used as distribution infrastructure for Terlan packages in the BEAM
ecosystem, but Hex reuse does not imply OTP compatibility, Erlang source
compatibility, BEAM bytecode compatibility, Rebar compatibility, or OTP
application boot compatibility.

Hex-facing metadata must be derived from Terlan package metadata. It must not
be the authoritative source of package identity, dependency shape, runtime
capabilities, native boundary declarations, or compiler target selection.

## Dependencies

Terlan dependency metadata records source kind and target intent explicitly:

- local path dependencies are first-class Terlan package dependencies
- Git dependencies are first-class Terlan package dependencies
- Hex dependencies are distribution/package-source metadata
- npm dependencies are JavaScript target metadata
- Cargo dependencies are Rust/native target metadata

Target package managers may provide fetching or distribution mechanics, but
Terlan-owned manifests and lockfiles define the compiler contract.

## Gate

The contract is guarded by:

```bash
make hex-target-metadata-check
```
