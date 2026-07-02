# Terlan Package Git Sources

Status: 0.0.7 baseline contract.

Git source dependencies are supported package inputs, but releases must never
depend on floating repository state. A Git dependency must declare a URL and an
immutable `rev` before it can participate in release builds.

## Contract

Git dependency resolution follows these rules:

- `url` identifies the repository source.
- `rev` is the immutable commit identity used by the compiler.
- floating branches and tags may be accepted only as resolution input before
  they are converted into an immutable `rev`.
- `terlan.lock` records the resolved immutable revision.
- release builds must be deterministic and must not perform implicit network
  resolution for Git sources.
- local path dependencies remain separate from Git dependencies.
- target package manager metadata is secondary and cannot make Cargo, npm, Hex,
  Rebar, or another adapter authoritative for Terlan source resolution.

The compiler may fetch Git sources when explicitly asked to resolve or update
dependencies. Normal build and release flows must consume the checked-in
`terlan.lock` data.

## Gate

The contract is guarded by:

```bash
make terlan-package-git-source-check
```
