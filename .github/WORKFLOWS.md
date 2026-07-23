# GitHub Workflows

Terlan uses separate docs, compiler, and release-validation flows so lightweight
documentation checks do not run compiler builds and compiler-facing source
changes are checked continuously. External publication is outside the 0.0.7
roadmap completion contract.

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

The direct-AOT portable matrix contract covers Linux, macOS, and Windows on
x86-64 and AArch64. Each available native runner may compile, package, install,
execute, reload, crash, and reject incompatible images, while local closeout
always executes the complete cycle on the current host. The strict contract
self-test rejects incomplete target rows and malformed aggregate data without
making artifact upload or hosted retention a completion requirement.
Compiler CI also runs `make tvm-aot-roadmap-reconciliation-check` so the
completed AOT boundary cannot drift through documentation-only validation.

A separate Linux x86-64 job installs Rust's fully instrumented
`x86_64-unknown-linux-gnutsan` standard library target and runs:

```sh
make tvm-aot-thread-sanitizer-check
```

This job detects data races in the thread-neutral AOT runtime independently of
the deterministic schedule models in `make tvm-aot-multicore-readiness-check`.

Every native matrix runner also executes the bounded, reproducibly seeded VM
memory-model stress. A separate Linux x86-64 release-candidate lane installs
the exact Rust 1.96.0 `x86_64-unknown-linux-gnutsan` target and runs:

```sh
make vm-multicore-thread-sanitizer-check
```

That gate runs the same seeds through isolated child processes under a
deadlock watchdog. CI fails when the pinned sanitizer target or its evidence
is absent; unsupported local hosts may still run the portable memory-model
gate.

Release validation also requires a Linux x86-64 self-hosted runner carrying
the `terlan-linux-x86_64-multicore-v1` label. That controlled runner executes
the release-profile benchmark with hardware policy enforcement. The final
release job runs:

```sh
make vm-multicore-mc9-evidence-check
```

The join accepts only passing performance and pinned ThreadSanitizer artifacts
from the same official workflow run, attempt, repository, and source revision.
Record-only performance and GitHub-hosted runner substitution fail closed.

The independent compiler job runs the reduced AOT release-candidate gate:

```sh
make release-candidate-check
```

Non-AOT feature jobs remain paused during the hard AOT cutover.

## Release Validation

`release.yml` may run manually as an additional validation surface. The local
validation command is authoritative:

```sh
make tvm-aot-release-closeout-check
```

The closeout gate reruns the complete local AOT gate set, executes locked Cargo
validation with `RUSTFLAGS` unset, and records the current-host platform,
compilation, HTTP, managed-list, inventory, and semantic evidence in:

```text
target/quality/tvm-aot-release-closeout-report.json
```

`make tvm-aot-roadmap-reconciliation-check` separately prevents Slices 100 and
101A through 101F from being checked without a valid repository-local closeout
report. Neither command requires a clean commit, push, upload, tag, external
account, or retained hosted artifact.

Review the staged local distribution plan without contacting GitHub with:

```sh
make release-promotion-dry-run VERSION=0.0.7
```

The dry run writes `target/quality/release-promotion-pipeline-report.json` with
the candidate seal, artifact checksums, and exact staged file list. It performs
no publication and is not an AOT completion dependency.
