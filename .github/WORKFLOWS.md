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

`ci.yml` runs on pull requests, `main` pushes, and dedicated `agent/aot-*`
branch pushes when compiler-facing sources change. It can also be dispatched
manually against an exact revision so the native matrix can validate a release
candidate before merge:

- Cargo workspace files
- `crates/**`
- `std/**`
- `editors/**`
- `tree-sitter-terlan/**`
- `tests/**`
- `docs/grammar/**`
- `docs/roadmap/**`
- `docs/release/evidence/**`
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
`make tvm-aot-platform-matrix-contract-check` rejects missing manual dispatch,
runner substitutions, incomplete target rows, missing aggregate execution, or
unretained release evidence before a branch is sent to GitHub.
Roadmap and retained-attestation changes deliberately enter Compiler CI, where
the release-candidate graph runs `make tvm-aot-roadmap-reconciliation-check`;
the final AOT-9 checkoff therefore cannot pass through documentation-only CI.

A separate Linux x86-64 job installs Rust's fully instrumented
`x86_64-unknown-linux-gnutsan` standard library target and runs:

```sh
make tvm-aot-thread-sanitizer-check
```

This job detects data races in the thread-neutral AOT runtime independently of
the deterministic schedule models in `make tvm-aot-multicore-readiness-check`.

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

It runs the same six target-native AOT attestations, strict aggregate, and Linux
ThreadSanitizer gate before the release validation job can execute. The
validation job runs:

```sh
make tvm-aot-release-closeout-check
```

The closeout gate reruns the complete local AOT gate set and the full 0.0.7
release preflight from the clean release checkout. The preflight generates its
own release HTTP soak and timer evidence, validates version/channel state, runs
the Lean proof closeout, and checks the release promotion pipeline. Closeout
then validates that the downloaded platform and ThreadSanitizer evidence belong
to the same commit and workflow run, and retains the report, clean-checkout
record, native matrix, ThreadSanitizer record, compilation and HTTP benchmarks,
and managed-list profile as one checksummed artifact bundle for 90 days.
`make tvm-aot-roadmap-reconciliation-check` separately prevents Slice 100 and
101A through 101F from being checked before their owning AOT items and requires
a retained revision-bound closeout attestation before AOT-9 can be checked.
After downloading the successful closeout bundle into `target/quality`, retain
its canonical report with:

```sh
python3 -B tools/check_tvm_aot_roadmap_reconciliation.py attest \
  --report target/quality/tvm-aot-release-closeout-report.json
```

The generated `docs/release/evidence/0.0.7-aot-closeout.json` is committed with
the final AOT-6, Slice 101F, and AOT-9 roadmap checkoffs.

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
