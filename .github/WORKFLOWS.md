# GitHub Workflows

Terlan uses separate docs, compiler, and release flows so lightweight
documentation checks do not run compiler builds, compiler-facing source changes
are checked continuously, and release artifacts are built and published from the
local release command, not by GitHub Actions.

## Docs CI

`docs.yml` runs on pull requests and `main` pushes when documentation-facing
files change:

- `README.md`
- `CHANGELOG.md`
- `docs/**`
- `.github/WORKFLOWS.md`
- docs workflow configuration

It performs lightweight Markdown hygiene only. It does not install Rust or run
compiler release gates.

## Compiler CI

`ci.yml` runs on pull requests and `main` pushes when compiler-facing sources
change:

- Cargo workspace files
- `crates/**`
- `std/**`
- `editors/**`
- `tree-sitter-terlan/**`
- `tests/**`
- `docs/grammar/**`
- `tools/**`
- `install.sh`
- `Makefile`
- compiler workflow configuration

The direct-AOT matrix runs target-native validation on Linux, macOS, and Windows
for x86-64 and AArch64. Each runner compiles, packages, installs, executes,
reloads, crashes, and rejects incompatible native images before uploading one
attestation. The aggregate accepts only the complete six-target set from one
official GitHub workflow run, attempt, and commit, then retains the aggregate
report for 90 days.

The independent compiler job runs the reduced AOT release-candidate gate:

```sh
make release-candidate-check
```

Non-AOT feature jobs remain paused during the hard AOT cutover.

## Release Validation

`release.yml` runs manually or when a version tag is pushed:

```text
v0.0.4
```

It runs the same six target-native AOT attestations and strict aggregate before
the release validation job can execute. The validation job runs:

```sh
make release-candidate-check
```

It does not build release artifacts and it does not publish GitHub releases.
Publication is owned by the local release command:

```sh
make publish VERSION=0.0.7
```

`make publish` runs the local preflight, builds the current-platform artifact
into `dist/`, smoke-tests the artifact and installer, and seals the exact upload
set in `dist/release-candidate.json`. The command verifies that manifest before
pushing `main` and the tag, then creates or updates the GitHub release using only
the checksummed files named by the manifest. Publication never discovers extra
`dist/` files or rebuilds after the candidate is sealed.

Review the offline upload plan without contacting GitHub with:

```sh
make release-promotion-dry-run VERSION=0.0.7
```

The dry run writes `target/quality/release-promotion-pipeline-report.json` with
the candidate seal, artifact checksums, and exact upload list.

If a tag validation workflow fails after publication, fixing `main` no longer
depends on a CI artifact rebuild. A release upload can be retried locally as
long as the remote tag still points at `HEAD`; `make publish VERSION=<version>`
updates the release notes and clobbers matching uploaded assets.
