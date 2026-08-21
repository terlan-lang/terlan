# Terlan Registry Protocol v1

`terlan-registry-protocol-v1` is the Terlan-only contract between public Terlan
package tooling and a Registry implementation. The compiler owns the public
models and generates their schemas and canonical fixtures:

```bash
terlc package protocol --out-dir _build/registry-protocol
```

The output is deterministic. `manifest.json` inventories all schema and fixture
files with their byte lengths and SHA-256 digests. Registry implementations
consume this output; they do not maintain a private copy of the models.

## Records

The v1 bundle includes publish request/result, package-version, dependency,
artifact, yank, root, snapshot, and package-index records. Every record has an
exact `schema` discriminator, an explicit required-field list, and rejects
unknown fields. Optional values are represented either by an omitted nullable
field or an explicit Boolean such as `dependency.optional`.

HTTP implementations use deterministic JSON and these media types:

```text
application/vnd.terlan.registry.publish-request.v1+json
application/vnd.terlan.registry.publish-result.v1+json
application/vnd.terlan.registry.package-version.v1+json
application/vnd.terlan.registry.yank.v1+json
application/vnd.terlan.registry.root.v1+json
application/vnd.terlan.registry.snapshot.v1+json
application/vnd.terlan.registry.package-index.v1+json
```

Unknown schema discriminators, unsupported media-type versions, unknown
fields, missing required fields, and malformed digests fail closed. A client
must not silently reinterpret a newer response as v1.

Every package-version and package-index resource contains a required public
HTTPS `repository_url`. Publish tooling derives it from
`[package].repository`; URLs with another scheme or embedded credentials are
rejected before sealing.

## Archive admission

The publish request repeats the server-enforced v1 limits so dry-run and live
publication validate the same values:

| Limit | v1 value |
| --- | ---: |
| Compressed archive | 67,108,864 bytes |
| Unpacked archive | 268,435,456 bytes |
| Files | 4,096 |
| UTF-8 path length | 240 bytes |
| Symlinks | rejected |

Archives are `tar.zst`. Absolute paths, parent traversal, backslash-separated
paths, devices, hard links, and any entry exceeding the declared limits are
rejected before publication. Archive hashes use lowercase SHA-256.

## Trust bootstrap and rotation

An installation distributes one trusted root record out of band with the
compiler/tooling or an explicitly configured Registry. The root contains
Ed25519 public keys, roles, and a signature threshold. A client accepts a new
root only when:

1. its version is exactly the trusted version plus one;
2. `previous_version` names the trusted version;
3. the old root threshold authorizes the new root;
4. the new root satisfies its own threshold; and
5. all signed digests and key identifiers are canonical.

Rotation is staged: publish a root containing old and new keys, publish and
verify a snapshot under the overlap, then publish a later root that removes the
old key. Clients retain their highest verified root and snapshot sequence and
reject rollback, skipped-root, threshold, signature, or digest failures.

Root and snapshot fixtures currently carry the signed payload digest. The
signature envelope and cryptographic verifier are introduced by Registry gate
MR1.4 without changing these versioned payload records.

## Product boundary

This is not a compatibility protocol for another package ecosystem. The
Registry accepts only sealed Terlan package metadata and archives created by
supported Terlan tooling. It does not accept foreign package metadata,
serialization formats, or package archives through this contract.
