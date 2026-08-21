# GitHub Workflows

Terlan uses separate docs, website, compiler, and release-validation flows so focused
documentation checks return early while compiler-facing source changes are
checked continuously. Compiler package publication remains outside the website
deployment contract.

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
Pull requests also trigger it for compiler and website implementation changes.
After merge, those paths are owned by `pages.yml`, avoiding a second identical
compiler/site build on `main`; documentation-content pushes continue to run
Docs CI directly. Shared Rust setup-action pushes are likewise owned by Pages
after merge while both consumers remain validated on pull requests.

## Terlan.io Pages

`pages.yml` builds the repository-owned `sites/terlan.io` module after relevant
changes reach `main` or on explicit dispatch. The build job has read-only
repository access and uploads a validated root-base static artifact. A separate
deployment job receives only `pages: write` and `id-token: write`, targets the
protected `github-pages` environment, and publishes through GitHub's official
Pages actions pinned to immutable revisions. The site artifact carries the
`terlan.io` CNAME; repository Pages settings must also select GitHub Actions as
the source and verify the custom domain before the first production run.
The exact Playwright browser payload is cached by operating system,
architecture, and website lockfile. System browser dependencies are still
verified on every runner, while unchanged deployments avoid downloading the
same Chromium archive again.

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

The release-candidate entry point first measures the canonical artifact budget
from a clean Cargo output tree, then bootstraps validators and runs the reduced
AOT gate on the same runner. CI and local validation use this same entry point,
so neither path can omit or duplicate the destructive pre-build measurement:

```sh
make release-candidate-check
```

Workflow syntax is checked in that same canonical CI job before release
validation begins. The repository build/release contract remains part of
`release-candidate-check`; it is not run in a second job that would rebuild the
compiler and typed repository validator from an empty runner.

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
1.96.0, installs only the Rustfmt and Clippy components exercised by the stable
workflow lanes, and caches only Cargo registry inputs; compiled `target/` trees
are not shared between jobs or retained in GitHub caches. ThreadSanitizer owns
its separate pinned nightly `rust-src` installation. Developer coverage support
remains declared in `rust-toolchain.toml` without making every hosted matrix
lane download `rust-src` and LLVM tools it never executes.
Compiler and release validation both build the exact locked `cargo-audit 0.22.2`
source and run it against `Cargo.lock`, denying vulnerabilities, unsound
dependencies, and unmaintained dependencies. Registry inputs are cached, but
the security decision never trusts an opaque cached audit executable; its
advisory database is refreshed on every run. `security.yml` also runs the same
gate every Monday and on demand, so a newly published advisory is detected even
when the repository has not changed.

Typed validation images use a content-addressed local build wrapper. The
post-measure validation set hashes the compiler and standard library once, then
combines that common fingerprint with each validator package. The artifact
measurement owner binds its image directly because it spans the intentional
`cargo clean` boundary. Unchanged images are reused, while any
compiler, standard-library, validator-source, command, or cache-implementation
change invalidates the exact image. Every seal also hashes the finished image,
so post-build mutation invalidates reuse. An atomic single-writer handoff keeps
parallel Make processes from compiling the same validator twice; interrupted
writers remove their output and seal, and cleanup owns any remaining lifecycle
marker. Each typed-validator build is bounded to thirty minutes by default;
timeout and signal termination stop the child and remove its partial output.
Repository-wide `.terlan/` compiler caches are ignored and rejected if tracked.

After its clean artifact measurement, the release-candidate entry point runs the
canonical Rust suite before spawning the reusable gate graph. That child graph
replaces already-proven Rust selectors with no-ops, so a release cycle cannot
silently rerun hundreds of exact tests.
Default, quality, editor, benchmark, and cross-feature library tests compile as
one union-feature harness. The orchestrator partitions that same binary into
disjoint fast-unit and integration executions with libtest include/exclude
filters, so no second feature-profile artifact or repeated test is required.
Cargo's JSON artifact message identifies the exact test executable after one
bounded `--no-run` build; all four Terlan partitions then execute that sealed
harness directly. Only the separate workspace-support phase invokes Cargo
again, reducing the orchestrator from five Cargo launches to two.
The same orchestrator owns ignored, evidence-producing integration contracts,
including the generated C++ package proof, so later gates consume reports
without replaying tests or accidentally suppressing their producers.
Every orchestrated build or test phase receives closed stdin and a bounded deadline,
preventing an accidental interactive read from stalling release validation.
The orchestrator atomically seals `target/quality/rust-test-suite-report.json`
with one explicit tier, ordered outcome, and wall time for every phase; a
missing report is a gate failure.
Make recipes invoke freshness-checked prebuilt `terlc`, `terlan-quality`, and
`terlan-benchmark` executables instead of repeatedly paying Cargo metadata and
feature-resolution startup through `cargo run`. The build/release contract
expands the release plan and rejects regressions to duplicate Cargo test or
`cargo run` execution. Multicore gates consume the shared compiler bootstrap
instead of each issuing equivalent `cargo check` calls, and repository
validation rejects direct Cargo recipes that bypass the locked wrapper.
Repeated exact `terlc test --name` selectors compile one source and native
application closure for their union, so grouped validation does not pay one
compiler process per selected test. Tree-sitter validation bootstraps its exact locked npm
dependency on demand and keeps npm, home, and parser caches under `target/tmp`,
so clean checkouts and cleanup use the same reproducible path.
Artifact-budget measurement reuses a prior sealed report only while source and
manifest inputs, policy, pinned toolchain identity, profile fingerprints, and
all required prebuilt executables still match. Otherwise it runs the complete
clean/warm/invalidation/profile measurement. This makes repeated no-op
preflights cheap without allowing stale clean-build evidence to pass.

Rust validation ownership is explicit in
`docs/quality/RUST_VALIDATION_TIERS.tsv`. The canonical orchestrator verifies
that every ignored Rust test has exactly one inventory row and records each
executed phase under one of six tiers. Controlled-host, performance, and
concurrency evidence remain under their named external Make owners. All
orchestrated children receive closed stdin and a phase timeout.

The direct-AOT cache identity binds the dependency lock, profile, enabled Cargo
features, target, codegen policy, and bytes of the resolved linker. Set
`TERLAN_NATIVE_CACHE_MISS_POLICY=error` in a warm-cache validation step to make
an unexpected miss fail instead of rebuilding. Editor and documentation parity
share one sealed AOT validator image, and runtime benchmark gates share one
release `terlan-benchmark` executable.

Every typed-validator AOT recipe uses `terlc build --incremental`. Standalone
scripts receive an output-local compiler cache just like project builds, and
checked IR is accepted only when its source, dependency manifests, frontend
implementation digest, Cargo feature/profile policy, and syntax contract all
match. Embedded std summaries are parsed lazily for the source module's import
and fully-qualified-call closure, then reused across the remaining modules in
that compiler process.
After the shared Rust tools are sealed, cold typed-validator misses run through
two bounded Make lanes. Validator outputs and locks remain independent, and
the release dry plan must remain byte-deterministic; `TERLAN_VALIDATOR_BUILD_JOBS`
can reduce the width on constrained hosts.
Target-local serial sequences use explicit one-job recursive Make calls. The
GNU Make 4.3 global-serialization special target is forbidden because it also
silently serializes otherwise-independent validator builds.
Workflow syntax validation pins Go 1.25.0 for actionlint 1.7.12 and caches only
the actionlint module/build inputs, avoiding both runner-image toolchain drift
and repeated compilation without caching repository build outputs.
The accelerator CPU boundary scan is part of the canonical reusable gate graph.
Its Rust semantics execute in the union-feature suite, so CI no longer starts a
separate runner that recompiles Terlan and repeats an equivalent Cargo check.
Audit, platform, aggregation, and sanitizer jobs install the minimal Rust
compiler profile with no Rustfmt/Clippy payload; those components are owned only
by the canonical compiler-check and final release-validation jobs that execute
their gates. This ownership also covers Docs, Pages, scheduled security, and
the controlled multicore evidence jobs.

`target/quality/validation-build-plan-report.json` records the expanded release
plan and enforces the current ratchets: no `cargo run`, no duplicate equivalent
build, at most six Cargo invocations, and at most seventeen typed-validator
requests. It also caps direct Terlan test-process launches at forty-three after
grouping the twelve exact String capability selectors, and caps the sixteen unique
typed-validator AOT builds. The report seals the expanded plan digest so later
timing evidence can be attributed to the exact command graph. Plan expansion
resets inherited warm-cycle flags in the child environment, so a reused gate
cannot overwrite the baseline with an artificially smaller subgraph.
Budget increases require an explicit contract change rather than
silently lengthening release validation.
The exhaustive entry point rejects typed-validator partials immediately before
and after the reusable gate graph, so interrupted builds are attributed at the
cycle boundary and cannot silently survive a successful closeout.

Cargo retention keeps reusable debug dependencies while bounding regenerable
incremental state. Incremental artifacts are capped at 16 GiB and the complete
debug tree warns at 32 GiB. Hashed test/tool executables are grouped by canonical
workspace stem; retention preserves the two newest generations and every
generation younger than five minutes, then removes superseded binaries and
their dep-info sidecars. The grace period avoids racing an active Cargo writer,
while the generation cap prevents feature/profile churn from accumulating
hundreds of megabytes per executable. The explicit shared-debug maintenance
target removes only `target/debug/incremental`; it does not discard compiled
dependencies or the prebuilt validation executables and therefore does not
force a full rebuild.
Both Cargo dev and test profiles retain file/line tables but omit bulky
variable-level debug metadata. Compiler and VM backtraces remain attributable,
while routine local binaries, rlibs, and link steps no longer pay for full Rust
debug information that Terlan's source debugger does not consume.

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
