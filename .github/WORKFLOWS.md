# GitHub Workflows

Terlan uses separate docs, website, compiler, and release-validation flows so focused
documentation checks return early while compiler-facing source changes are
checked continuously. Compiler package publication remains outside the website
deployment contract.

## Tagged releases

Run the publication command from a clean `main` checkout:

```sh
make publish VERSION=0.0.10
```

The command requires successful Compiler CI and explicit Release validation for
the exact commit before it creates the annotated `vMAJOR.MINOR.PATCH` tag. A
successful release-validation status means that every archive, proof/runtime
record, editor artifact, installer smoke, publication plan, and checksum is
already present in one attested, sealed publication bundle. The pushed tag starts
`publish.yml`, which verifies and uploads that bundle without compiling or
testing. It must match `workspace.package.version`.

The tag workflow never creates or moves tags and never pushes `main`. Failed
checks prevent publication. Retry a failed workflow on the same tag after fixing
an infrastructure issue; source changes require a new reviewed candidate and
tag. Only the publication job receives release-write permission, and concurrent
runs for the same tag are serialized without cancelling an active publisher.

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

Compiler CI calls `source-preflight.yml` once. A change-classification job
distinguishes product inputs from workflow and documentation contracts.
The classifier is Terlan source with 100% declaration coverage. A
content-addressed script-runner cache restores `terlc` and `terlan-vm` when
compiler inputs are unchanged; otherwise the candidate compiler self-bootstraps
that runner once. Source preflight reuses the same compiler for classifier,
release-payload, and readability checks instead of rebuilding it. Script
capability is deliberately separate from release
eligibility: candidate tooling may orchestrate a red build, while only complete
independent evidence permits publication.
Contract-only changes run formatting, actionlint, and publication/tooling
regressions without starting the compiler suite. Product changes additionally
run source inventory, ownership, canonicalization, and the compiler gates.
Release control passes the successful exact-revision Compiler CI run ID into
Release validation. A lightweight GitHub API gate verifies its repository,
workflow path, commit, branch, event, attempt, status, and conclusion instead of
recompiling and rerunning source preflight. That same guarded job publishes the
pending release identity, avoiding a second runner whose only purpose was to
repeat the already-established source binding.

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

On `main`, Compiler CI runs the general compiler/release-candidate job only when
product inputs changed. Release validation never starts from a push: release
control dispatches it once, with the expected full commit SHA, after Compiler CI
for that SHA succeeds. The dispatch also binds the successful Compiler CI run
ID, which Release validation verifies before starting any matrix job. It is the
sole owner of the release AOT matrix, sanitizer families, candidate preparation,
and publication bundle.
On `main`, a separate short attestation job signs the three completed compiler
coverage records. Only this job receives attestation/OIDC write permissions; it
does not execute downloaded code or rebuild/retest the compiler. It consumes the
successful compiler job's immutable artifact ID, including on a signing-only
retry. Preparation verifies the records' signatures, source commit/ref and
workflow, and checks the compiler job's actual producing attempt through the
GitHub job API before admitting its coverage bundle. The workflow's latest
attempt is not substituted for the completed compiler job's attempt.

The shared native-dependency action installs the exact Lean channel named by
`proofs/lean/lean-toolchain` before any executable proof gate runs. It first
uses `elan`, then falls back to the authenticated GitHub asset API with a
pinned SHA-256, bounded retries, an explicit `elan` link and a version smoke
test. Compiler CI and publication therefore share one fail-closed installer; a
missing host toolchain is an infrastructure failure, not a failed theorem.

The direct-AOT portable matrix contract covers Linux, macOS, and Windows on
x86-64 and AArch64. Each available native runner may compile, package, install,
execute, reload, crash, and reject incompatible images, while local closeout
always executes the complete cycle on the current host. The strict contract
self-test rejects incomplete target rows and malformed aggregate data without
making artifact upload or hosted retention a completion requirement.
For an actual release-validation run, every native runner also builds and
installer-smokes its own archive. Native library tests and the optimized release
tools share the workspace Cargo target; the platform validator does not build a
second isolated debug compiler. The final job rejects anything other than the
exact six archives and six matching checksum sidecars, then retains that joined
distribution under the successful run.
Matrix aggregation and final archive/contract validation share that job and
`make release-hosted-validation-check`. The single Make graph executes their
common compiler and validator prerequisites once. It retains the matrix report,
hosted evidence bundle, and distribution uploads; any prerequisite or validation
failure prevents distribution attestation and promotion.
Final AOT roadmap retirement runs
`make tvm-aot-roadmap-reconciliation-check` only after every owned AOT slice is
complete. Ordinary compiler CI keeps running the implementation gates while an
AOT slice is deliberately open.

## CodeQL

`codeql.yml` and `codeql-rust.yml` select security analysis through declarative
GitHub path filters. GitHub Actions analysis runs for `.github/**`; Rust
analysis runs for Rust sources, Cargo manifests, the lockfile, toolchain input,
or its shared CodeQL implementation. Both entry workflows call
`codeql-language.yml`, so checkout, initialization, and analysis stay defined
once. Documentation-only and Terlan-only pushes no longer start an 18-minute
Rust extraction. Manual and scheduled runs still cover both maintained
languages. Default setup is disabled so one commit cannot receive duplicate
automatic and repository-owned analyses.

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

Compiler and release platform matrices share
`.github/actions/setup-native-dependencies/action.yml`. Windows validation
installs `libpq` and `pkgconf` from the same vcpkg target triplet, exports the
exact `pkgconf` executable to Cargo, and checks every native installer exit
status. The Windows action pre-seeds vcpkg's pinned, checksum-verified
`win_flex_bison` archive through the authenticated GitHub asset API when the
public release download edge is unavailable, then retries the complete
idempotent vcpkg install with bounded backoff. A failed package download
therefore cannot be hidden by a later successful PowerShell command or turn a
short-lived edge failure into an immediate matrix failure.

Release validation uses hosted runners only. It proves portable multicore
correctness and retains the pinned multicore ThreadSanitizer report. Throughput
benchmarks that depend on CPU quietness are diagnostic-only: they are never
part of `check`, release closeout, or publication evidence. This keeps release
correctness reproducible on shared hosts while preserving the benchmark
commands for deliberate performance investigations.

The release-candidate entry point first measures the canonical artifact budget
from a clean Cargo output tree, then bootstraps validators and runs the reduced
AOT gate on the same runner. CI and local validation use this same entry point,
so neither path can omit or duplicate the destructive pre-build measurement:

```sh
make release-candidate-check
```

Workflow syntax and source-level readability are checked before release
validation begins. The preflight compiler is disposable and never counts as
candidate evidence. The deterministic multicore release contract and
repository build/release contract remain part of `release-candidate-check`;
they are not duplicated in preliminary steps that would build typed validators
only to discard them during the candidate's clean artifact measurement.

The shared native-dependency action installs bubblewrap and enables unprivileged
user namespaces on the ephemeral Ubuntu 24.04 runners. Both validation-tool
bootstrap and capability-worker execution require the actual sandbox; failure
to create it stops the build. Local Make never changes the host's namespace policy.

On Linux, the first validation-tool build runs through
`mk/hermetic-support.mk`: Git-listed working files are frozen into a read-only
snapshot, the numeric workspace Rust toolchain is selected explicitly, and
Cargo receives a fresh configuration home and a cleared environment. Checkout
and ambient Cargo configuration, credentials, wrappers, and flags are not
imported. Compiler/runtime builds continue using their normal configuration.
Only locked registry/git caches and the dedicated `target/hermetic-support`
output tree persist. Incremental compilation is disabled for these small tools.
The sandbox uses the host's read-only system SDK, not a pinned OS image; selected
compiler/linker executable identities are bound into the input digest.

One bootstrap lease covers snapshot creation, Cargo, receipt verification, and
installation. The first successful build seals its actual Cargo artifact log;
unchanged retries validate output hashes without another Cargo invocation.
Working-source bytes, including changes hidden from Git status, are checked
again before installation. Failed/interrupted builds cannot install new tools.
Reserved scratch is recovered under that same lease, and the lifecycle check
reports any surviving scratch. No build receipt substitutes for release evidence.

Non-AOT feature jobs remain paused during the hard AOT cutover.

## Release Validation

`release.yml` is an explicit, exact-revision workflow. It is never restarted by
an unrelated push. Its final successful status is withheld until the complete
publication bundle has been prepared, attested, and retained. The local AOT
validation command remains available for focused development:

Release validation first reuses successful exact-revision Compiler CI evidence,
then audits the locked Rust and Node dependency graphs. Platform and sanitizer
jobs cannot start until both gates pass. `make publish` performs the same locked
dependency audit locally before dispatching hosted release validation, so a
new advisory fails before consuming platform runners.

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
1.96.0 and installs only the components exercised by each lane. Long validation
jobs cache Cargo registry inputs plus compiled dependency, build-script, and
fingerprint directories; incremental state and final executables are excluded.
The cache key binds the host ABI, toolchain, compatible build profile,
manifests, and commit. Sequential preflight, compiler, and preparation jobs with
the same profile reuse compiled dependencies; sanitizer and platform profiles
remain isolated. The compiler job explicitly
saves reusable compiled outputs after a failed late gate as well as after
success; a cache hit only avoids unchanged compilation and never substitutes
for executing a validation gate. ThreadSanitizer owns its separate pinned
nightly `rust-src` installation. Developer coverage support remains declared
in `rust-toolchain.toml` without making every hosted matrix lane download
`rust-src` and LLVM tools it never executes.
Compiler and release validation run the exact locked `cargo-audit 0.22.2`
against `Cargo.lock`, denying vulnerabilities, unsound dependencies, and
unmaintained dependencies. Its platform- and toolchain-bound executable is
cached under an immutable versioned key and its reported version is checked
before use; a cache miss builds the exact locked source. The advisory database
is refreshed on every run, so the cache never substitutes for the security
decision. `security.yml` also runs the same gate every Monday and on demand, so
a newly published advisory is detected even when the repository has not
changed.

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
The encompassing `check-gates` Make graph has its own 3,600-second deadline,
selected by `TERLAN_CHECK_GRAPH_TIMEOUT_SECONDS`; individual Rust phase deadlines
remain 1,800 seconds. The graph option accepts only 1–7,200 seconds and its actual
value is recorded with the Make execution evidence. Cancellation still stops the
owned process tree. This separates aggregate runtime from a single phase's limit:
the September 25 hosted run exhausted 30 minutes after 115 coverage requests
without a failing assertion, while processing the Lean proof gates.
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
compiler process per selected test. An explicit `terlc test <file>` performs the
signature-only source-root prepass and lowers only that test's transitive import
closure; directory and coverage runs retain complete project validation.
Tree-sitter validation bootstraps its exact locked npm
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
executed phase under one of six tiers. Host-sensitive performance measurements
remain manual diagnostics outside validation; concurrency correctness retains
its named Make owner. All orchestrated children receive closed stdin and a
phase timeout.

The normal library inventory is partitioned into commands, native IR, type
checking, the remaining compiler modules, runtime, and a remainder that owns new
top-level modules. These use the same compiled harness. Inventory admission
rejects omissions, overlaps, and empty owners before execution. Each partition
remains serial because tests change process-wide compiler paths and environment;
the partitions run sequentially until shared external resources are proven
safe for concurrent execution. Each partition has its own deadline and timing.

Suite phase records include `wall_time_ms`, `timeout_seconds`, `near_deadline`
(at least 80% of the limit), and `recommended_timeout_seconds`. Recommendations
are twice the observed duration, rounded up to minutes and bounded between one
minute and two hours. They are diagnostics, never substitute test evidence, and
do not automatically raise limits. Review completed-run measurements and timed
out lower bounds before updating CI deadlines. The existing one-hour hosted
phase limit follows the observed 30-minute serialized-suite timeout; the Intel
macOS compiler-build limit follows its observed 30-minute optimized-build timeout.
Both retain outer workflow deadlines.

`make release-status` combines the exact candidate revision, source cleanliness,
both canonical workflows and their jobs, retained artifact metadata, publication
state, and the next action in `target/quality/release-status.json`. The tag
workflow runs `make release-tag` to wait, prepare, and publish under one lease.
It never transfers green evidence between commits. Existing attestation,
checksum, candidate, annotated-tag, and upload verification remain authoritative.

`make release-predict VERSION=0.0.10` is the strict read-only admission check.
It validates both local Make execution plans, observes the exact hosted revision,
and succeeds only when the retained candidate can enter preparation without
discovering another required validation step. It neither builds nor publishes.
`make release-plan VERSION=0.0.10` uses the already-built local compiler to show
the same state without bootstrapping a second release-control artifact.
`make release-wait` and `make release-tag` use that same working compiler and VM
to refresh only the content-addressed release-control image. Candidate Rust or
lockfile changes never trigger a local compiler rebuild merely to orchestrate
hosted validation; the exact-revision Compiler CI evidence owns that decision.

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
the hosted multicore sanitizer job.

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

`make clean` currently refuses blanket deletion: a directory name cannot prove
that its build is inactive or its recovery evidence disposable. Inspect generated
outputs without deleting them with `bash scripts/clean_build_outputs.sh --dry-run`.
Use `make rust-incremental-cache-prune` for lease-checked Rust cache retention;
resume interrupted preparation owners to recover their staged publications.
Do not delete writer-lease files or pending publication journals manually.

Review the staged local distribution plan without contacting GitHub with:

```sh
make release-promotion-dry-run VERSION=0.0.8
```

The dry run writes `target/quality/release-promotion-pipeline-report.json` with
the candidate seal, artifact checksums, and exact staged file list. It performs
no publication and is not an AOT completion dependency.

## Publication

Run publication from a clean `main` checkout after committing the release contents:

```sh
make publish VERSION=0.0.10
```

This entry point runs lightweight local publication-contract checks, dispatches
missing Compiler CI and Release validation sequentially, and waits until the
exact commit has an unexpired sealed publication bundle. Only then does it
atomically push the commit and annotated tag. The tag workflow performs sealed
upload only. Working files must be clean; the command never commits, stashes, or
resets them. The Make clean-tree preflight runs once; the Terlan controller then
guards the exact clean revision before dispatch and again before atomic tag
submission without replaying that Make target. `make release` aliases the same
command.

The command holds a lease in Git's common directory for the complete local
preflight, hosted-validation wait, and submission interval. Installed commit
hooks in every worktree acquire that same lease, so another local task cannot
replace `main` and invalidate an in-flight candidate. The final upload reuses
the lease; non-hook remote pushes are still rejected by the exact-revision
guard rather than published as stale evidence.

The preflight rejects tags outside `main` history, mismatched local or remote
tags, stale candidate evidence, and a missing or non-successful validation
status. It requires both the exhaustive compiler check and release validation
to be successful for the exact candidate commit. Candidate validation happens
once in those hosted workflows; publication does not replay the full Rust test
suite.

Release validation downloads the exact six-platform distribution, verifies its
workflow identity, archive checksums, and Sigstore-backed GitHub provenance,
then prepares all remaining candidate-bound evidence. It exports an immutable
publication plan and attests the complete bundle. The tag workflow downloads
that bundle, verifies its manifest and attestations, and uploads it. Missing or
stale evidence prevents the green readiness status; an interrupted upload can
be retried without compilation or tests.

Verified downloads have an atomic checkpoint in `target/publication-downloads/`.
The key binds the repository, commit, both successful workflow runs and attempts,
and the download verifier implementation. A preparation retry checks live workflow
status and hashes the entire cached inventory, then reuses the archives, extracted
payload, and hosted evidence without downloading or rechecking attestations.
Changed producer attempts require a new checkpoint; corruption stops before
replacing the active distribution. The checkpoint is saved before local checks,
so a later local failure does not discard successful hosted verification.
Concurrent download/restore owners are rejected with a nonblocking worktree lock.
These local caches are not independently signed evidence and never replace the
candidate seal or final distribution checks. They retain full payloads on disk;
retire obsolete checkpoints after publication when reclaiming disk space.

The publisher uses the existing annotated release tag
and uploads every archive, detached checksum, and the sealed candidate manifest
to a draft release. One verified publication plan supplies the asset inventory,
sizes, hashes, and candidate seal. Draft asset lookup uses the numeric release
ID, not GitHub's unreliable draft tag endpoint. Matching uploads are reused.
Failed tag lookups require a successful authenticated release-list lookup;
network or authentication failures stop publication instead of implying that
a release is absent.
It verifies that GitHub contains exactly the sealed asset names, sizes, and
SHA-256 digests before making the release public. Interrupted
uploads remain drafts; rerunning is safe, while an already-public release is
never mutated unless it already exactly matches the candidate (in which case
the command exits successfully without changes).

Release announcements use the title `Terlan <version>` and the matching curated
`CHANGELOG.md` section. Include only what changes for users, who is affected,
and any action required when upgrading. Describe observable behavior, useful new
capabilities, compatibility, and security impact—not the implementation work
performed. Omit unchanged platform recaps, progress reports, refactoring stories,
workflow logs, candidate hashes, gate counts, and internal build repairs. A
release with few user-facing changes should have short notes, not filler.
Cryptographic identity belongs in `release-candidate.json`; publication
verifies its uploaded digest, independently of the announcement's wording.

Remaining preparation hardening is separate from upload retry correctness:

- Move generated proof/evidence metadata out of tracked source paths into a
  candidate-specific output root. Preparation is not yet transactional and
  must not be described as fully resumable.
- Record completed build/evidence owners and their input fingerprints in a
  durable execution ledger. Recipe dry-runs cannot detect subprocess builds
  hidden inside validators or safely resume an interrupted preparation.
- Isolate container Gradle caches from host daemons; do not share writable
  caches across environments that cannot coordinate daemon locks.
