# Terlan Package Registry Publish Contract

The first hosted implementation of this contract is the Terlan-authored module
registry used as Terlan Cloud's mandatory reference customer. Registry product
gates live in `../../docs/roadmap/ROADMAP_MODULE_REGISTRY.md`; deployment and
Hetzner operations gates live in
`../../docs/roadmap/ROADMAP_CLOUD_IMPLEMENTATION_PLAN.md`.

Terlan package publication is a promotion of a sealed package archive. It is
not a rebuild from the workspace. The package registry receives immutable
artifacts that were already produced, checked, hashed, and attested by the
release pipeline.

The publish input set is:

- package manifest
- required public HTTPS repository URL from `[package].repository`
- source archive
- generated binding manifest
- native artifact manifest
- docs summary
- checksum file
- compatibility metadata
- target capability metadata
- package provenance

The registry publish operation must be dry-run capable. A dry-run publish
computes the same archive hash, provenance hash, docs hash, target metadata, and
index diff that live publication would submit.

The repository URL is included in the signed package-version metadata and the
signed package index. Registry stores its current value on the stable package
record; each immutable version retains the URL that was submitted with that
release and its exact source provenance digest.

The executable local sealing surface is:

```bash
terlc package publish --dry-run [project-dir] --out-dir <dir>
```

The executable local Registry publication surface uses those exact sealed
bytes:

```bash
terlc package publish --mirror <mirror-dir> [project-dir] --out-dir <staging-dir>
```

It creates immutable archive/metadata objects plus deterministic package index,
root, and snapshot resources. Re-publishing an existing name/version fails.
`terlc package resolve --mirror ...` verifies the complete digest chain, writes
Registry v2 `terlan.lock`, and extracts the verified archive into the
content-addressed project cache used by ordinary offline builds and tests.

It writes a deterministic `<name>-<version>.tar.zst` and a matching versioned
publish request. The request inventories every admitted file with its SHA-256
digest, normalized role, and byte length. It also records the package identity,
dependencies, declared targets/capabilities, producing compiler, provenance,
public API, documentation, archive identity, and exact admission limits.

Sealing uses an allowlist derived from `terlan.toml`: the manifest, source
roots, scripts, web assets, declared native source package, deployment
migrations, root README/license files, and `docs/`. Workspace outputs and
dependency/tool caches (`_build`, `.terlan`, `target`, `node_modules`, `.git`)
are excluded. Included symbolic links and special files are rejected. Tar
entries are path-sorted with uid, gid, and timestamp zeroed and modes normalized
before deterministic zstd compression.

Package versions are immutable. A published version cannot be overwritten with a
new source archive, checksum file, generated binding manifest, native artifact
manifest, compatibility metadata, target capability metadata, docs summary, or
package provenance.

Registry index updates are deterministic. New package versions are represented
as append-only version entries. Package yanks are explicit yanks, not silent
mutation of package metadata. A checksum change for an existing package version
is rejected. Missing provenance, missing docs, missing target metadata, and
hidden native artifacts are rejected before any registry index update is
accepted.

Offline registry mirror validation is required. The publish gate must be able to
validate a local registry mirror without network access and still produce the
exact index diff that a live publish would produce.

The publish integrity report is
`package-registry-publish-report.json`. It records:

- package archive path
- archive hash
- index diff
- provenance hash
- docs hash
- target metadata
- dry-run publish result
- rejected mutation attempts

The adversarial publish corpus must cover:

- duplicate package versions
- overwritten checksums
- missing generated binding hashes
- missing native artifact hashes
- target-incompatible packages
- stale docs
- malformed index entries
- yanked packages resolving silently
- publish commands that rebuild from source
