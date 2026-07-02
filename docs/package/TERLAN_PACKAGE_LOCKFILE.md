# Terlan Package Lockfile

Status: 0.0.7 baseline contract.

`terlan.lock` is the Terlan-owned dependency resolution artifact. It is the
compiler contract for reproducible package resolution and must not be replaced
by Hex, npm, Cargo, Rebar, or another target package-manager lockfile.

## Contract

The lockfile records resolved Terlan package sources:

- local path dependencies with normalized package identity
- Git dependencies with URL and immutable `rev`
- optional static index entries when a Terlan package index exists
- target metadata for Hex, npm, and Cargo dependencies without making those
  package managers authoritative for Terlan source resolution

The lockfile must be deterministic, checked before release builds, and updated
by Terlan tooling rather than hand-edited as normal workflow.

Target package manager lockfiles may exist as target adapter artifacts, but
they are secondary to `terlan.lock`.

## Gate

The contract is guarded by:

```bash
make terlan-package-lockfile-check
```
