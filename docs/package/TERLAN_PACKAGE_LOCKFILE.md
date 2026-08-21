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

## Registry resolver v2

The Registry dependency product uses a version 3 `terlan.lock` generated from
one origin-pinned signed snapshot:

```bash
terlc package resolve \
  --registry <url> \
  --trust-root <pin.json> \
  --out-dir <project-dir>
```

The resolver reads direct Registry requirements from the project's
`[dependencies]` table and solves the complete same-origin Registry closure as
one deterministic decision. Each `[[registry]]` entry records `name`,
`version`, Registry URL,
`snapshot_sha256`, source identity, archive and metadata SHA-256 values,
targets, capabilities, resolved dependency edges, and
`terlan-registry-resolver-v2`. The out-of-band trust
pin binds the Registry origin, root key id, algorithm, and public key. The
client verifies origin- and route-bound root, snapshot, and package-index
threshold signatures before parsing their payloads. It retains the highest
accepted root and snapshot, rejects rollback or same-generation replacement,
and verifies the snapshot-to-index, index-to-exact-publication-request, and
index/metadata-to-archive hash chain.

Live GETs have fixed connection/read timeouts, no redirects, bounded bodies,
one safe retry, and exact content ETags. Verified bytes enter a
content-addressed cache through atomic references only after cryptographic and
digest validation. `--offline` is explicit and fails on any verified cache
miss; there is no switch that disables signature, origin, rollback, compiler
compatibility, or archive verification.

Ordinary re-resolution reuses compatible locked versions. `package update`
unlocks all packages, while `package update <name>...` unlocks only the named
packages and any closure changes required for a valid solution. Newly selected
yanked versions are excluded; an already locked yanked release remains
reusable because yanking changes selection policy rather than immutable archive
availability. Optional Registry dependencies activate only when another active
requirement selects their package. Stable diagnostics identify the conflicting
package and requirements when no solution exists.

## Gate

The contract is guarded by:

```bash
make terlan-package-lockfile-check
make registry-trusted-resolution-check
make registry-graph-workflow-check
```
