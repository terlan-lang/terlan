# Terlan Package Registry Publish Contract

Terlan package publication is a promotion of a sealed package archive. It is
not a rebuild from the workspace. The package registry receives immutable
artifacts that were already produced, checked, hashed, and attested by the
release pipeline.

The publish input set is:

- package manifest
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
