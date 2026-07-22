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

Every resolved Git source record includes:

- dependency name
- repository URL
- immutable revision
- resolved revision checksum
- lockfile entry
- resolver version

The compiler may fetch Git sources when explicitly asked to resolve or update
dependencies. Normal build and release flows must consume the checked-in
`terlan.lock` data.

The initial executable workflow is:

```bash
terlc package fetch <project-dir>
terlc build <project-dir> --target terlan-vm
```

`package fetch` clones the declared immutable revision into
`<project>/.terlan/packages/git/<rev>` (or `TERLAN_PACKAGE_CACHE_DIR`), verifies
its origin, revision, clean worktree, and Git tree checksum, then writes
`terlan.lock`. Normal builds never fetch and fail with
`error[package_git_not_locked]` when the lock/cache entry is absent.

## Gate

The contract is guarded by:

```bash
make terlan-package-git-source-check
```
