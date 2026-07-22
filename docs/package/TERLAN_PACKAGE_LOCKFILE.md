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

Every resolved package entry records:

- package name
- package version
- resolved source
- source checksum
- target/capability constraints
- generated binding hashes when bindings are generated
- native artifact hashes when native artifacts are required
- resolver version

The lockfile must be deterministic, checked before release builds, and updated
by Terlan tooling rather than hand-edited as normal workflow.

For the 0.0.7 Git-source slice, `terlc package fetch` writes lockfile version 1
with the resolver version and deterministic Git entries containing alias,
package identity, URL, immutable revision, Git tree checksum, and native
capabilities. Local path hashing and registry package entries remain later
extensions of this format.

Target package manager lockfiles may exist as target adapter artifacts, but
they are secondary to `terlan.lock`.

## Gate

The contract is guarded by:

```bash
make terlan-package-lockfile-check
```
