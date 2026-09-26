# Terlan 0.0.9 Roadmap

Updated: 2026-09-10. Baseline: 0.0.8 is published. This file contains remaining
work, not an engineering activity log. The previous long-form record is retained
in the [historical snapshot](../archive/ROADMAP_0_0_9_PRE_CONSOLIDATION_2026_09_07.md).
Legacy `V8-1` references now map to V9-1 and V9-2 below.

## Scope And Execution Order

Stabilize and shorten build/publication first, then resume staged self-hosting.
ABI compatibility and the runtime inspector, durable checkpoint/migration, and
HTTP/3 requirements carried from 0.0.7 are explicit work below, not silently
deferred or assumed complete. Follow unchecked items in document order.

The [self-hosting plan](../ROADMAP_SELF_HOSTING.md) owns compiler migration phases.
The [accelerator roadmap](../ROADMAP_0_0_10_CUDA.md) owns accelerator compiler/VM
contracts; external packages own CUDA implementation and package integration.
Do not duplicate their suites inside the core release aggregate.

All active items remain unchecked. Recent publication/cache fixes exist in the
release worktree and have focused tests, but are not committed or evidence of
complete end-to-end preparation recovery. The self-hosting implementation is in
a separate checkout and must be integrated explicitly.

V9-1 has validated managed-list/multicore/AOT report checkpoints, bootstrap
process-crash recovery, proof-output isolation, hosted-download retention, and
supporting compiler/runtime fixes. Bootstrap and native caches now share the
compiler-owned host/linker identity. Native linking and CLI tool capture share
bounded process ownership, with Linux process and WebAssembly regression evidence.
Shared cache publication now reserves owned scratch and preserves old artifacts
when replacement fails.
Native execution-point inventory distinguishes emission attempts, actual linker
spawns, and outcomes; cold/warm/concurrent fixture counts pass. Preparation-wide
cycle ownership and Cargo/self-host accounting are still required. Its Terlan
consumer rejects incomplete/replayed work. Report owners now reserve, validate,
retain, and hash their native logs; graph dependencies include those logs.
Graph-local accounting now passes duplicate-work rejection, failed-owner native
retention, and interrupted owner/graph commit-gap recovery. Global cycle
accounting and the complete interruption protocol remain open. Report owners
now bind retained logs to their inspected bytes and recover partially published
outputs from prepared checkpoints without replaying the producer. Supporting AOT
Result construction, branch-local continuations, and preservation of effects
across yields pass focused regressions. Script assertions now have linear
lowering and verified fail-fast behavior.
Managed `Option` results now retain their representation after resumed grouped
guards, with focused execution and broader NativeIR regression evidence.
Publication's four platform/sanitizer/release contract self-tests now have
candidate-scoped owners, with measured zero-launch warm reuse and interrupted
publication recovery. This does not complete ownership of the remaining gates.
Reusable native module objects now have byte/count/age limits and a lease held
through linking. Focused eviction/recovery tests and actual cold/warm/concurrent
builds pass; relinking reuses unchanged module objects. Checked implementations
now share bounded retention with independently leased hash buckets; cold/warm
fixture evidence preserves payloads and avoids new native work. Final images and
Cargo artifacts still require their own retention ownership. A separate Linux
bootstrap now audits and retires redundant Rust incremental sessions under rustc
leases, preserving each crate's newest session; broader Cargo storage is not yet
covered. File-backed and embedded
interfaces now share bounded, exact-content parsing. Focused and broader consumer
tests pass; the 65-module fixture reduces warm frontend time from 8.891 s to
0.436 s with byte-identical native output. Full turnaround/ownership closeout
remains separate from these measured improvements.
Private hard-linked inputs also preserve a surviving linker's dependencies after
compiler SIGKILL and canonical-object eviction, verified in a real Linux
rehearsal. Orphan-reference cleanup and other process-tree exit paths remain open.
The portable-list boundary defect
it exposed is fixed. Nested-Boolean exhaustiveness and nested generic alias
expansion now pass typechecker and actual AOT execution regressions.
Detailed scope, limitations, commands, and
test evidence belong in [release preparation notes](../../quality/RELEASE_PREPARATION.md).

Lean replay now uses private, bounded-process workspaces without deleting shared
source build state. Linux retains independently validated replicas, reuses all
26 on an unchanged run, and resumes after actual Lean interruption. Warm replay
takes about eight seconds locally; full preparation ownership and orphan cleanup
remain open.
Focused proof consumers now share that owner, batch repeated proof paths, and
preserve complete reports. Rust oracle checks reject empty or ignored test runs;
one stale semantic-kernel selector was corrected.
The Rust orchestrator and VM now share child lifetime ownership. Bounded Cargo
capture and process/pipe deadlines pass focused real-process checks; nested
groups, Windows jobs, and producer-wide resume accounting remain open.
Its live report now counts observed direct-child spawns, retains incomplete
states across interruption, and rejects duplicate phase owners. Compiled-harness
inventory checks reject stale/empty selectors and overlapping test ownership
before execution. These observations are not reusable successful-test receipts.
The standalone Unix Rust runner now handles INT/TERM/HUP through owned
cancellation, with real signal/reaping evidence. Shared cleanup also terminates
a direct child that changes groups without targeting its new peer group. VM
signal policy, escaped descendants, and SIGKILL recovery remain open.
Rust suite closeout now compares content-bound working-tree admission/closeout
snapshots and retains changed-input evidence. Real-tree admission and isolated
changed-source rehearsals pass; tool binding and reusable phase receipts remain
required before replacing the existing skip flags.
The suite now retains bounded, run-bound exact main-harness selections in a
companion document and reconciles their identities with every owned completion.
Missing, failed, duplicate, forged, or modified selection evidence fails closeout;
externally owned coverage is not certified. This adds no test-discovery or Cargo
launches. A current-input coverage consumer and reusable receipts remain open;
the legacy skip flags are not yet replaced. The v2 selection companion also
retains the complete compiled inventory and original ignore classification.
Historical selector coverage now distinguishes locally passed tests from
delegated and unexecuted ignored tests, using the same selector implementation
as execution planning. This does not yet authorize current-input reuse.
Selected Git/Cargo entry points, prebuilt runtimes, the driver, and its compiled
harness now have byte identities. The driver invokes verified paths, rejects
changed harnesses before another phase, and checks all selected executables at
closeout. The runner now captures one bounded OS environment, explicitly applies
it to direct children, and records effective phase-environment digests without
plaintext values. Cargo/Rustup configuration files outside Git are also bound;
changed configuration fails closeout even when source and executables match.
That inventory now follows active Cargo includes from the hashed bytes, including
optional-file absence; bounded traversal and real Cargo precedence probes pass.
The runner now resolves native Cargo/rustc through observed Rustup probes,
checks the compiler pin, and binds installed bin/lib bytes across execution.
Explicit Cargo compiler/rustdoc/wrapper selections now bind executable bytes,
effective subprocess environments, and PATH shadowing across closeout, with
actual Cargo precedence and wrapper-order probes.
Rustup proxy PATH and custom-toolchain symlink retargeting now have direct
regression coverage. Default compiler/rustdoc entry points now bind Cargo's
actual selection rules, including named versus path-based Rustup toolchains,
with real Cargo/Rustup differential probes and a passing production rehearsal.
Admission now checks the selected workspace/dependency wrapper chains, compiler
pin, and default sysroots, reusing the installed tree's identity when shared.
Real wrapper-environment and wrong-version-before-build rehearsals pass.
Target/rustflag-specific inputs, wrapper-internal tools, external SDK/library
closure, and reusable receipts remain open; this is not yet complete build-input
provenance.
The nine direct Terlan phases and native workspace targets now verify exact
per-test completion against admitted inventories, independently of test stdout.
The workspace runner consumes streamed Cargo artifacts, shares package manifest
snapshots, checks executable identity, preserves Cargo's package environment,
and records its helper/inventory/test launches. Early exits, forged summaries,
and bypassed runners fail. Ordinary failures retain available launch records
before disposable files are cleaned; those partial records are not success.
Linux/macOS helpers join the enclosing Cargo process group. Generic runners
change Rust 2024 doctest isolation, so doctests retain an independent Cargo
invocation and the original Rustdoc test runner. The reviewed allowance is three orchestrator Cargo calls
(seven preparation-wide), with fixture evidence of unchanged compiler-unit and
test-body counts. Doctest runs now reconcile each emitted name and outcome with
its harness's declared count and summary, without another discovery/compile pass.
Mixed standalone/merged/compile-only/compile-fail/ignored fixtures preserve
isolation and reject forged test-body output. Individual test cases remain
emitted-harness evidence; library targets now have independent metadata admission.
Nested compiler/link/VM inventory, Windows job ownership,
interrupted-file recovery, and reusable successful test receipts remain open.
A six-configuration real Cargo differential now establishes how a Rustdoc
observation hook can preserve child environments, isolated doctests, and
nonduplicated compiler units. The production hook now consumes the shared
metadata's doctest-enabled library targets and requires each target's exact
completion and observer/Rustdoc process records. Missing completions fail even
when Cargo exits successfully. Cross-filesystem observer installation is supported.
The full tiny-workspace rehearsal executes all 13 expected bodies once without
adding Cargo calls. Strict Clippy, 173 unit tests, and two integration tests pass.
Rust-quality Make consumers now share one locked metadata producer per Make
invocation; a real repository rehearsal rejects any second query. Exact Cargo
member IDs replace SourceInventory's private metadata subprocess. Failure-safe
refresh and membership tests pass. The orchestrator and SBOM now share the fresh
Make-owned handoff; complete preparation-wide input binding and reuse remain
open. The stricter file-growth check's
newly exposed violations have been refactored without raised limits; headroom,
structural, documentation, and strict library Clippy checks pass.
Supply-chain license admission now shares the exact member projector and enforces
its stated Apache-2.0 workspace policy. Duplicate checksum reads and failed
extraction's temporary-workspace leak are corrected. The supply-chain check now
shares the fresh Make-owned Cargo query with Rust-quality consumers, with actual
repository provenance and failed-refresh rejection evidence. The Rust suite
compares that generation with its own source, configuration, selected tools,
environment, and resolver-cache observations before building. Native harness
packages must belong to its admitted workspace. A complete tiny-workspace run
and changed-source rejection pass; reusable successful-test receipts remain open.
The compiler bootstrap now also owns the native worker and supplies the suite,
removing its separate bootstrap invocation. The release-plan gate distinguishes
metadata mode from suite execution and passes without raised budgets.
The canonical workspace selection now includes product integration and binary
tests, delegating only the exact main library to its existing direct phases.
Bootstrap snapshots its already-built driver outside Cargo's mutable outputs;
metadata and suite consumers hold reader leases that exclude replacement.
An actual workspace rebuild changes Cargo's driver without changing this
snapshot, with no additional compilation. The main Make check path now owns
current-input coverage rather than trusting an unchecked test-skip flag.
The batch verification boundary now covers explicit library, binary and
integration requests using one current-input scan. Native completions retain
exact names reconciled with their result logs. A real mixed-target batch passes
with zero Cargo/test replay and rejects changed native bytes. The live Make owner
now matches requests to completed phase environments and shares its admitted
metadata with nested consumers. Real nested Make tests reject missing selections,
changed environments, absent owners and swallowed failures without test replay.
Dry runs remain plans, not coverage evidence. Local release refresh now extends
that same Make graph, sharing prerequisites and rejecting ambiguous correctness
entry points. Coverage retains the exact suite digest, suite ID and Make command
identity under a reader lease. CI is configured to retain these records under
run-and-attempt-specific artifact names. Hosted producer coordinates remain
unauthenticated context until a consumer verifies them. A bounded historical
bundle reader now checks the exact three exported files, producer run/attempt
and Linux x86-64 scope, suite/Make byte bindings, all eight matching closed input
bindings, and requested names against the existing main/native inventories.
The real Make fixture rejects altered provenance, inputs, invocation, requests,
missing/extra files and symlinks without replaying test bodies. This reader
explicitly does not authenticate GitHub origin or authorize current-input reuse.
The downloader now requires signed coverage records and checks the successful
compiler job's producing attempt separately from a signing-only retry. A
main-only signing job consumes the compiler artifact's immutable ID without
rerunning tests. The rebuilt promotion validator's adversarial tests pass,
including failed signatures/records and signing-only retry without download
replay. Publication refresh now enters a live source-coverage owner which admits
the authenticated download checkpoint, matches exact source bytes and canonical
test selections, and rejects dirty source, changed request environments, corrupt
subjects and concurrent checkpoint writers. Real Make/Cargo fixtures pass without
replaying their 15 test bodies. Overlapping multicore/AOT source prerequisites
share one Make graph; the default-feature Cargo check and local report production
remain actual local work. Hosted coverage explicitly does not certify locally
rebuilt binaries or supply current Cargo metadata. An actual hosted run and the
full clean-candidate rehearsal are still required. The legacy flag alone fails
closed. This does not close preparation-wide ownership or recovery.
Publication's source gates and release composition now share one Make graph.
The staged-input producer orders local reports, AOT records, restored artifacts
and their matrix; distribution consumers explicitly depend on that producer.
Real parallel Make/Cargo fixtures preserve single execution of shared gates and
block consumers/composition after source or staging failure, including with
`make -k`. The fixture substitutes report/download services; remaining nested
producer ownership and full candidate recovery are still required.
Local report staging now shares one managed-list/multicore/AOT dependency graph
and one declaration-time source/tool admission. Its real Cargo/composer rehearsal
passes cold/warm reuse, upstream receipt binding, selective failure and killed
producer recovery. Refresh-plan accounting rejects missing or duplicated graph
owners. This does not yet cover all prerequisites or nested subprocess launches.
Readiness/preflight now admit parsed report schemas, decisions, exact seals and
designated archive hashes instead of matching arbitrary report text. Adversarial
and real-consumer fixtures pass, including generic candidate-version binding.
Cold preparation no longer repeats readiness/staged-distribution work after its
successful full graph. Parallel Make fixtures preserve warm repair and reject
failure continuation. Cold and warm publication now share a staged-distribution
checkpoint. The real public-installer fixture verifies zero installer replay on
unchanged reuse, failed-checksum preservation, changed-readiness invalidation,
recovery and scratch cleanup. It exposed and now passes a missing inferred-list
descriptor regression, also covered by linked native execution and 460 NativeIR
tests. Candidate seals now exclude exact preparation-bookkeeping paths while
retaining genuine gate reports. Readiness requires each mandatory report to
have a unique sealed entry matching its actual bytes; omitted, duplicate,
malformed and changed-but-passing evidence is rejected. Final AOT consumer,
promotion-contract, formatting and new-module lint checks pass. Publication now
checkpoints sealing and readiness as dependent producers with one shared input
inventory. A real-process fixture passes zero-launch warm reuse, actual readiness
SIGKILL/resume, generated-component invalidation and corrupt-output repair;
ordered inventory merging passes against all 11,228 tracked paths. Standalone
checks retain direct execution. Preparation-wide ownership and full candidate
acceptance remain open.
Generated-artifact freshness now shares five ordinary Make prerequisites, with
the snapshot ordered before shared regeneration and bound to the inventory's
exact bytes. Parallel fault-injection fixtures reject early producers, failed
tools/gates and changed outputs. Node/npm execution is checked before snapshot
admission. Full preparation-wide receipts and recovery remain open.
Proof evidence now has a focused image bootstrap shared with the full validator
aggregate. Parallel fixtures verify single ownership and failure propagation;
the actual focused command rebuilds only that validator and passes all 14
proof-evidence records without changing the accepted baseline.
Proof aggregates now share ordered Make prerequisites; real parallel Make
fixtures verify single execution and failure propagation. Smoke consumers reject
retained passing reports after a failed or interrupted current attempt. The
semantic-kernel script requires a live owner for Rust coverage reuse; a skip flag
alone fails actual AOT execution. Reviewed grammar/dispatch evidence now passes
the 14-record proof-release check. Native-boundary and smoke scripts now also
request exact coverage from the live owner, retain standalone Rust execution,
and label its provenance. The Rust request bridge and compiled Terlan callers
have focused passing evidence. Three actual AOT-compiled proof callers now pass
live-owner acceptance against a completed tiny Rust suite, rejecting missing
selections and changed environments without replaying test bodies. Prior hosted
authentication is a fixture boundary, not a new GitHub attestation. Full producer
ownership and clean-candidate recovery remain open. These focused
checks do not close V9-1.
The acceptance work exposed a structured-control gap in nested eager operands.
Calls, lists, casts and field projections now give case-valued operands ordered
lexical owners; mixed-effect conditions use the existing join-continuation
lowering. Focused script/native-object tests and 454 additional NativeIR tests
pass, including observable effect order and failure propagation. This is a
compiler correctness fix, not evidence of complete preparation recovery.
Publication preflight now queries remote tag identity once and fails closed on
transport or local reference errors. Disposable real-Git fixtures verify tag
absence, matching retries, fetch failure and conflicting tags; upload recovery
and complete publication acceptance remain separate requirements.
The shared metadata producer now uses bounded native process ownership and
records failed attempts separately from the last successful document. Production
Make integration and real repository consumers pass; transitive resolver input
binding and preparation-wide reuse remain open.
Native Cargo behind Rustup is now byte-bound using shared tool resolution, and
successful metadata attempts bind their exact output digest. Changed native tools
and malformed resolution fail focused tests without replacing prior success.
The typed metadata consumer now checks completed attempt identity and exact
output bytes. A real producer/consumer fixture accepts success and rejects the
retained document after a failed locked refresh. This is handoff integrity,
not cross-cycle freshness or preparation-wide metadata reuse.
Bounded Cargo-home observations now detect resolver-index, manifest, and target
discovery changes during production. Cache mutation/cancellation tests and the
real repository query pass. The suite now supplements these observations with
resolved package contents, including ignored, vendored and external sources.
The shared query selects all features so optional dependencies are included.
The bounded tree walker is shared with resolver observations; Git/cache-owned
files are referenced instead of hashed twice. Actual ignored `include_str!`
input admission passes without additional Cargo or inventory launches.
Workspace discovery outside declared packages, arbitrary build-script inputs,
external SDKs and wrapper internals remain incompletely bound.
The exposed final-call script result gap is fixed without changing the checked
source contract or wrapping its control flow. Actual AOT execution verifies
successful and failing Boolean results, including the grouped-guard fallback.
Closed-image admission now shares the exhaustive call-analysis traversal;
454 NativeIR tests and the scoped structural/documentation gates pass.

Proof closeout now shares one canonical evidence construction across its
adversarial tests, baseline comparison and release-mode schedule. Actual AOT
execution preserves all 14 slice traces and records one set of tool probes;
parallel Make failure-order tests pass. Proof composition now checkpoints all
three reports together, including their schemas and metadata-selected inputs.
The real-image fixture passes cold/warm reuse, individual report repair and
killed-producer resume with identical final evidence. Prepared transactions
recover after partial publication without replay; publication orders staged
inputs before proof composition and candidate readiness after it. Upstream proof
execution and preparation-wide acceptance remain open.

The shared Rust process owner now records opt-in, bounded real launch/reap
observations with hashed command metadata. Forty tests and strict consumer Clippy
pass, including nested environment-reset and cleanup cases. Release-owner
retention/schema integration and rebuilt-VM script execution remain open; these
observations are not equivalent-build identities or complete OS-tree coverage.

Still required before V9-1 closes: preparation-wide ownership (including
prerequisite checks, proofs, and distributions), actual nested subprocess
inventory, remaining cache budgets, transitive tool identity, nested-process/VM
signal and Windows cleanup, and a full clean-candidate interruption/resume
rehearsal. Passing focused fixtures does not imply this end-to-end acceptance.

## Active Checklist

- [ ] V9-1: Make release preparation resumable and publication retry-safe.
  - Separate immutable source, candidate-specific evidence, reusable caches,
    and disposable workspaces. Generated proof metadata must not dirty tracked
    source or change the candidate identity during preparation.
  - Give every build and evidence producer one owner. Persist an atomic resume
    ledger containing its input fingerprint, tool/profile/target identity,
    dependencies, output hashes, and outcome. Reuse only successful matching
    entries; invalidate affected dependents when inputs change.
  - Record actual subprocess launches, not only Make dry-run text. Equivalent
    Cargo, Terlan AOT, native-link, and self-host builds must not run twice in a
    cycle. Assign every correctness test to exactly one execution tier.
  - Keep `make publish-prepare` as the preparation entry point and `make publish`
    as promotion of an already prepared candidate. Publication must never
    compile, test, download distributions, or refresh evidence on a retry.
  - Reuse hosted downloads only after checking producer identity and cached
    bytes. Preserve checkpoints across local failures; reject corrupt, stale,
    cross-target, or changed-attempt inputs. Reuse matching uploads and leave
    incomplete releases as drafts. Network failure must not imply absence.
  - Check required tool versions and executable compatibility before expensive
    work. Isolate container build caches from incompatible host daemons.
  - Bound subprocesses, close undeclared interactive stdin, and attribute hangs.
    Register temporary outputs with their owner and clean them on success,
    failure, panic, timeout, cancellation, and signals. Interrupted residue
    must be identified before reuse. Never delete active evidence or source.
  - Apply byte/entry/age budgets and generation-safe cleanup to reusable caches;
    retain dependencies without accumulating obsolete candidate payloads.
  - Acceptance: cold preparation, unchanged warm preparation, and interrupted
    preparation followed by resume produce equivalent decisions. A verified
    completed owner is not replayed; changed inputs rerun only affected owners.
    Interrupted upload retries do no preparation work. Exercise these paths
    with deterministic fault injection before an end-to-end candidate rehearsal.

- [ ] V9-2: Reduce compiler, test, and validator turnaround costs.
  - Split the monolithic Rust test harness along stable implementation,
    reusable-support, and independently linked test-tier boundaries. Preserve
    exact test inventory, direct sealed-harness execution, and useful line tables.
  - Split the large Rust-quality AOT dispatcher into independently sealed
    validator families sharing compiled support. Do not replace one large
    build with repeated equivalent compilation or extra compiler startups.
  - Complete content-addressed AOT/native-link reuse. Keys cover normalized
    typed inputs, compiler/frontend/runtime ABI, target, dependency lock,
    profile/features, linker identity, and relevant environment policy.
  - Parallelize independent deterministic tiers with bounded workers and stable
    reports. Preserve isolation for concurrency tests; performance comparisons
    remain separate diagnostics, not CPU-quietness release prerequisites.
  - Emit one report with actual compile/test counts, cache hits and misses,
    wall/CPU time, peak memory, artifact bytes, and dominant costs. Record a
    reproducible cold, warm, and localized-edit baseline with machine identity.
  - Preserve reviewed count budgets (seven Cargo invocations, seventeen validator
    requests, sixteen validator AOT builds, thirty-two Terlan test processes)
    until measurements justify an explicit reviewed change. Lower costs without
    dropping tests, hiding skips, loosening assertions, or disabling debug data.
    The native/doctest isolation split accounts for the one added Cargo
    coordination call; its differential evidence preserves compiler-unit and
    test-body counts. See the release preparation notes for the budget review.
  - Acceptance: compare baseline and revised runs, including a test-only edit
    and a validator-only edit. Show reduced affected rebuild/link scope and no
    duplicate work. Timing regressions require investigation; noisy-host timing
    alone must not block publication. Subprocess timeouts remain enforced.

- [ ] V9-3: Integrate and close the first self-hosted frontend vertical slice.
  - Bring the separate self-hosting work into the normal build deliberately;
    preserve unrelated development changes and do not infer readiness from a
    successful standalone package build.
  - Complete Phases 0 and 1 of the [self-hosting plan](../ROADMAP_SELF_HOSTING.md):
    versioned boundaries, bootstrap provenance, spans/tokens, module/import and
    public-interface extraction, and a maintained positive/negative corpus.
  - Connect canonical Rust frontend evidence and run the named Phase 0/1 gates.
    Compare tokens, interface identity, diagnostics, and byte spans; minimize
    disagreements and retain actionable reproducers.
  - Register each gate with the V9-1 execution owner. Proposed targets must be
    implemented and wired into the normal tier, not merely listed in a document.
  - Acceptance: the complete maintained corpus agrees across Rust and Terlan,
    including malformed inputs, and clean bootstrap/recovery is demonstrated.
    Rust remains authoritative at this milestone. Full parsing, typechecking,
    lowering, driver migration, and `terlc1`/`terlc2` convergence remain explicit
    later phases; scheduling them requires the preceding gates to pass. This
    item must not be presented as completion of full compiler self-hosting.

- [ ] V9-4: Define and verify the native compatibility policy.
  - Resolve the ABI stability work carried from 0.0.7. Publish an explicit
    compiler/runtime/image and native-package compatibility matrix, including
    C ABI, C++, and Rust consumers and supported target combinations.
  - Separate runtime ABI identity from the self-host compiler/backend protocol.
    Define supported upgrades and deterministic rejection of incompatible
    images, layouts, ownership contracts, and package artifacts.
  - Use installed producer/consumer fixtures across supported versions, with
    positive, incompatible-version, and malformed-input coverage.
  - Acceptance: document exactly which compatibility is supported and what
    requires rebuilding. Keep ABI 1 `current-pre-freeze` unless a specific
    compatibility promise is backed by the matrix; do not imply an ABI freeze
    merely because self-hosted compilation or a current-version test succeeds.

- [ ] V9-5: Deliver the runtime inspector TUI.
  - Use the existing debugger/runtime inspection boundary to show actors,
    shard ownership, mailbox pressure, timers, supervision, and pending native
    operations. Do not create a parallel debugger or scheduler state model.
  - Provide bounded snapshots/streams, disconnect handling, and access control.
    Start with inspection; any state-changing controls need explicit semantics.
  - Acceptance: exercise a running multicore application, actor exit/restart,
    stale identities, overload, and client disconnect. Inspection must not block
    shard owners or expose unsafe memory. Include CLI help and user examples.

- [ ] V9-6: Deliver an explicit durable checkpoint/restore contract.
  - Define persistence semantics before implementation: application-visible
    state, mailbox/timer treatment, message delivery, snapshot consistency,
    schema evolution, crash recovery, and authorization.
  - Keep logical durable state distinct from live heaps and continuations.
    Do not serialize native pointers, open handles, or in-flight capability
    calls as portable state; unsupported resources require explicit rejection.
  - Define whether restore creates new actors/identities and how supervised
    services coordinate replay, duplicates, draining, and version migration.
    Respect fixed ownership of live actors; no implicit live-shard migration.
  - Acceptance: demonstrate documented recovery and supported version migration
    across a process restart. Test interrupted writes, corruption, incompatible
    schemas, and duplicate delivery. Do not claim transparent process migration
    or exactly-once effects beyond what the persistence contract proves.

- [ ] V9-7: Add HTTP/3 through maintained protocol libraries.
  - Define the supported QUIC/HTTP/3 library, TLS/ALPN behavior, target matrix,
    configuration, resource limits, cancellation, and graceful shutdown.
    No handwritten protocol implementation.
  - Keep scheduling, readiness, actor lifecycle, and backpressure under VM
    ownership through protocol-agnostic adapters. Do not introduce an independent
    application scheduler or block shard owners on protocol work.
  - Preserve HTTP/1.1 and HTTP/2, and make unsupported targets/configurations
    explicit. Document advertised HTTP/3 support and fallback behavior.
  - Acceptance: test installed client/server interoperability, stream multiplexing,
    flow control, cancellation, malformed traffic, and shutdown. Publish only
    the support level actually exercised; throughput measurements are diagnostic.

- [ ] V9-8: Close and publish 0.0.9 with accurate user-facing support claims.
  - Review every item above and its owned tests, supported targets, compatibility
    promises, and limitations. No checkbox closes on prose, fixture disposition,
    or a historical report alone; scope changes require an explicit decision.
  - Run the canonical release validation once for the selected commit and use
    its sealed results for preparation and promotion. Verify installed packages,
    exact artifact identity, checksums, provenance, and failure/retry behavior.
  - Reconcile accelerator support with the companion roadmap and package-owned
    evidence. Preserve CPU-only independence and narrow experimental claims;
    unavailable hardware lanes are not passing coverage.
  - Release details contain only changes users will notice, who is affected,
    compatibility/security impact, and upgrade actions. No work summaries,
    unchanged platform recaps, gate counts, hashes, or internal build fixes.
  - Acceptance: all agreed release requirements pass, the installed support
    matrix and notes are accurate, and publication consumes the verified
    candidate without rebuilding it. Publishing requires explicit authorization;
    this roadmap update does not authorize a tag or public release.

## Accelerator Status And Package Follow-Ups

The companion roadmap records AC1–AC10 and hard contracts as closed. This edit
does not rerun or recertify those gates. Its recorded experimental promotion is
limited to Linux x86-64, the tested RTX A4500/compute capability 8.6, driver
580.173.02, LLVM 14.0.0, and local package snapshots; it is not general CUDA
availability or evidence that the package repository has been published.

Package-owned remaining work includes one-shot transferred-resource packets,
CUDA-enabled OpenCV exchange, compiler/VM trace identities for NVTX, and another
executed compute-capability lane. Keep their implementation and checks in the
external package, as recorded in the companion roadmap. CPU applications and
core release validation must not acquire a CUDA or self-hosted-runner dependency.

## Gate And Status Discipline

Reuse the existing repository build/release contract, promotion self-tests,
canonical Rust/Terlan test tiers, and package gates. Add missing behavioral cases
to the appropriate owner; do not add checks proving that old files were deleted
or new roadmap prose exists. Roadmaps are planning documents, not build inputs.
Use fresh implementation evidence before changing `[ ]` to `[x]`; preserve old
reports as history without treating them as current candidate evidence.
