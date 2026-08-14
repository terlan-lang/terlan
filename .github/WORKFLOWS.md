# GitHub Workflows

Terlan uses separate docs, compiler, and release-validation flows so focused
documentation checks return early while compiler-facing source changes are
checked continuously. External publication is outside the 0.0.7 roadmap
completion contract.

## Docs CI

`docs.yml` runs on pull requests and `main` pushes when documentation-facing
files change:

- `README.md`
- `CHANGELOG.md`
- `docs/**`
- `.github/WORKFLOWS.md`
- docs workflow configuration

It installs the pinned Rust 1.96.0 workspace toolchain and builds the typed Terlan
documentation validator. It does not run the full compiler release graph.

## Compiler CI

`ci.yml` runs on pull requests, `main` pushes, and dedicated `agent/aot-*`
branch pushes when compiler-facing sources change. It can also be dispatched
manually against an exact revision so the native matrix can validate a release
candidate before merge:

- Cargo workspace files
- pinned toolchain, formatter, and ignore policy
- `benchmarks/**`
- `build/**`
- `crates/**`
- `mk/**`
- `proofs/**`
- `std/**`
- `editors/**`
- `tree-sitter-terlan/**`
- `tests/**`
- `docs/**`
- `tools/**`
- `scripts/**`
- `install.sh`
- `install.ps1`
- `Makefile`
- workflow and shared action configuration

The direct-AOT portable matrix contract covers Linux, macOS, and Windows on
x86-64 and AArch64. Each available native runner may compile, package, install,
execute, reload, crash, and reject incompatible images, while local closeout
always executes the complete cycle on the current host. The strict contract
self-test rejects incomplete target rows and malformed aggregate data without
making artifact upload or hosted retention a completion requirement.
For an actual release-validation run, every native runner also builds and
installer-smokes its own archive. The final job rejects anything other than the
exact six archives and six matching checksum sidecars, then retains that joined
distribution under the successful run.
Final AOT roadmap retirement runs
`make tvm-aot-roadmap-reconciliation-check` only after every owned AOT slice is
complete. Ordinary compiler CI keeps running the implementation gates while an
AOT slice is deliberately open.

A separate Linux x86-64 job installs the pinned
`nightly-2026-07-16` toolchain with `rust-src` and runs:

```sh
make tvm-aot-thread-sanitizer-check
```

This job rebuilds the standard library for `x86_64-unknown-linux-gnu` with
`-Zsanitizer=thread` and detects data races in the thread-neutral AOT runtime
independently of the deterministic schedule models in
`make tvm-aot-multicore-readiness-check`. The ordinary GNU target keeps
Cranelift dependencies on a target triple they support.

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
the release-profile benchmark with hardware policy enforcement. A hosted
watchdog monitors that job and cancels the run with a failing commit status if
the controlled lane remains queued for more than ten minutes. This also works
for organization-level runners, which a repository token cannot reliably list.
The final release job runs:

```sh
make vm-multicore-mc9-evidence-check
```

The join accepts passing controlled performance and pinned ThreadSanitizer
artifacts from local execution, one GitHub attempt, or independently rerun
producers when they describe the same source revision. The report preserves
each producer's provenance and classifies it as local, single-attempt, or
distributed; provenance is descriptive rather than a technical pass
condition. Record-only performance, unpinned sanitizer execution, and
GitHub-hosted substitution for the controlled performance runner still fail
closed.

The complete technical evidence can be produced without Actions:

```sh
make vm-multicore-mc9-local-evidence-check
```

This requires the Rust 1.96.0 `x86_64-unknown-linux-gnutsan` target locally,
runs the controlled performance policy, records local source state, and seals
the two revision-matched reports.

The compiler check job first measures the canonical artifact budget from a
clean Cargo output tree, then bootstraps validators and runs the reduced AOT
release-candidate gate on the same runner. This preserves measurement honesty
without paying for a second runner setup or transferring a large build tree:

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

Workflow actions are pinned to immutable commits and updated by Dependabot.
Checkout credentials are never persisted. The shared setup action pins Rust
1.96.0 and caches only Cargo registry inputs; compiled `target/` trees are not
shared between jobs or retained in GitHub caches.
Compiler and release validation both build the exact locked `cargo-audit 0.22.2`
source and run it against `Cargo.lock`, denying vulnerabilities, unsound
dependencies, and unmaintained dependencies. Registry inputs are cached, but
the security decision never trusts an opaque cached audit executable; its
advisory database is refreshed on every run. `security.yml` also runs the same
gate every Monday and on demand, so a newly published advisory is detected even
when the repository has not changed.

Typed validation images use a content-addressed local build wrapper. A validation
cycle hashes the compiler and standard library once, then combines that common
fingerprint with each validator package. Unchanged images are reused, while any
compiler, standard-library, validator-source, command, or cache-implementation
change invalidates the exact image. Repository-wide `.terlan/` compiler caches
are ignored and rejected if tracked.

`make clean` removes Cargo outputs and every repository-owned generated tree:
release archives, AOT and `_build` caches, proof/editor outputs, generated
Erlang summaries, and installed JavaScript dependencies. Audit the exact cleanup
set without deleting anything with `bash scripts/clean_build_outputs.sh --dry-run`.

Review the staged local distribution plan without contacting GitHub with:

```sh
make release-promotion-dry-run VERSION=0.0.7
```

The dry run writes `target/quality/release-promotion-pipeline-report.json` with
the candidate seal, artifact checksums, and exact staged file list. It performs
no publication and is not an AOT completion dependency.

## Publication

Publication is an explicit local promotion after the exact commit has passed
the hosted `release-validation/run` status. From a clean `main` checkout:

```sh
make publish VERSION=0.0.7
```

The preflight rejects a non-fast-forward `main`, mismatched local or remote
tags, stale candidate evidence, and a missing or non-successful validation
status. It reseals the existing artifacts, refreshes the staged-distribution
binding, and runs the offline 203-gate composition before pushing anything.

The publisher downloads the exact six-platform distribution from the successful
status-bearing run, verifies its workflow identity, archive checksums, and
Sigstore-backed GitHub build-provenance attestations, then seals those immutable
inputs into the local candidate evidence. It creates an annotated release tag
and uploads every archive, detached checksum, and the sealed candidate manifest
to a draft release. It verifies that GitHub contains exactly
the sealed asset names and only then makes the release public. Interrupted
uploads remain drafts; rerunning is safe, while an already-public release is
never mutated unless it already exactly matches the candidate (in which case
the command exits successfully without changes).
