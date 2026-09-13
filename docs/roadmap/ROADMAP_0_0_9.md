# Terlan 0.0.9 Release Optimization Roadmap

Updated: 2026-09-13. Baseline: 0.0.8 is published.

## Scope

By explicit user decision, 0.0.9 is limited to build and release optimization,
reliable validation, and verified publication. The self-hosted frontend slice,
new native compatibility policy, runtime inspector, durable checkpoint/restore,
HTTP/3, and accelerator follow-ups move to the
[0.0.10 roadmap](ROADMAP_0_0_10.md). They are postponed, not completed.

Preserve existing compiler/runtime correctness and supported installed artifacts.
Do not use this scope change to drop correctness tests, weaken evidence, or
claim new features. CPU quietness is not a publication prerequisite.

Prepare the focused 0.0.9 release, but obtain explicit user authorization before
tagging or publicly publishing it. The exact candidate and its artifacts must
pass verification first. This scope does not cover publishing 0.0.10.

## Current Status

Draft PR #22 remains unready for merge or publication. Candidate `36a3d8d6`
passes the complete hosted release-validation workflow and Docs CI. Its six
native platform jobs, both sanitizer families, matrix aggregation, final artifact
validation, and distribution attestation pass. Compiler CI now
passes 6,225 core tests and 1,160 integration-tier tests, confirming the cache
fixture and grammar-fingerprint fixes. It then fails four workspace ABI tests
because ordinary correctness execution required release-producer metadata.
The correction retains every workload and assertion while making report writing
an explicit, fail-closed evidence mode. All nine focused tests pass locally;
actual report-writing probes also pass and reject missing revision metadata.
These changes and the preparation-lease changes still need hosted confirmation.
The separate CodeQL check reports two numeric-count logging alerts awaiting
documented false-positive review; the redaction diagnostic alert is resolved.
Full production cold/warm/resume acceptance, the V9-2 harness/dispatcher split
and measurements, and V9-3 remain open. Active versions are still 0.0.8.
Further V9-1 review extends the candidate lease over distribution restoration
and compiler probing, preserves that descriptor through nested download tools,
and rejects redirected lock directories before writing. Real-process
reproductions and 42 surrounding contract tests pass; hosted confirmation of
these additional changes is still required.
Final matrix aggregation and artifact/contract validation now share one hosted
job and Make graph, removing a duplicate cold bootstrap. Serial/parallel graph
tests verify single execution of every leaf and failure propagation. Review also
reproduced a stale six-build report expectation despite the reviewed eight-build
policy; the consumer now agrees with the validator, and its regression test
rejects mismatched budgets. These fixes still need hosted confirmation.
The completed hosted baseline took 80m45s; matrix aggregation and final artifact
validation occupied separate 9m49s and 8m11s jobs. The shared-job change has not
yet produced a hosted timing comparison and is not a full-cycle speedup claim.

### Earlier implementation checkpoints

Draft PR #22 is running hosted validation. Its first run exposed two clean-build
defects: a relative-output contract violation on Linux and a missing compiler
invocation on non-Linux hosts. Both have local reproductions and tested fixes;
native hosted reruns are still required. Required acceptance remains open, and
the draft must not be merged or published on the strength of component tests.
The next run exposed a lock-wait timeout incorrectly wrapping entire builds;
separate acquisition/execution bounds now pass real Make tests. Output/receipt
parent-path checks also reject reproduced symlink redirects before launch.
Docs CI now passes. Both macOS compiler builds finish but their process-group
cleanup encounters Darwin's zombie-only EPERM behavior. The scoped correction
requires a retained exited leader and a kernel inventory containing only that
leader; live/other-member permission failures remain errors. All 47 process-owner
tests pass on Linux; native macOS tests and full hosted validation remain open.
The separate multicore sanitizer and CodeQL pass. The AOT sanitizer's previous
green result used the same obsolete binary selector and does not count as
executed coverage. Its correction inventories and byte-binds the actual library
harness before running and verifying the full filtered selection under the
pinned instrumented toolchain. Local filter and sanitizer-admission tests pass;
hosted sanitizer execution remains required. Linux archive/installer checks pass, but review
found obsolete binary test selectors that ran zero library tests; those statuses
do not establish the required reload/recovery/stress coverage. The correction
batches five exact library tests through private-result verification. Compiler
CI also exposed metadata downloading dependencies after cache admission; a
bounded fetch now precedes an offline, cache-stable query. Focused regressions
and the complete orchestrator suite pass. Hosted reruns remain required.

An authorized detached local candidate now exists, with no changes to existing
branches or public releases. Its cold-checkout plan check exposed and now fixes
an unbuilt-orchestrator dependency; actual execution retains live coverage
admission. The refreshed parallel proof graph, retry/lease tests, three plan
tests, native-owner rehearsal and strict Clippy pass. Full hosted acceptance is
not implied: no exact-commit hosted artifacts exist for this local candidate,
active versions remain 0.0.8, and V9-1/V9-2/V9-3 stay open.

The enclosing preparation lease now works with nested producers: real tests
reproduce and fix the previous self-conflict, preserve the outer lock, serialize
sibling owners, and reject invalid inherited descriptors. Publication/proof Make
tests, actual resource admission, and strict Clippy pass. The rebuilt repository
build/release contract also passes after its bootstrap matcher was brought up to
date; it distinguishes command-hash text from planned Cargo work and retains the
reviewed budgets. Clean-candidate acceptance and V9-2/V9-3 remain open; these are
verified blocker fixes, not a release-ready declaration.

V9-1 remains open. Scoped cache/checkpoint recovery, nested Rust subprocess
observation and typed checkpoint integration have passing component evidence.
Shared continuation metadata removes repeated whole-image copies: identical
observation logs validate 4.5–5.7 times faster in focused before/after runs.
Further validator cost and preparation-wide producer coverage remain open.
Abandoned Rust cache retention passes real interrupted-writer recovery. Linux
compiler bootstrap now checks cache budgets and disk headroom before launch;
standalone admission remains point-in-time, while canonical `publish-prepare`
holds its lease across the selected preparation branch.
Superseded Rust build configurations now have a 72-hour retirement policy under
rustc leases; tested cleanup reclaimed stale incremental generations while
preserving compiler/runtime binaries and evidence, restoring build headroom.
Typed-validator builds now use the shared process owner; scoped cancellation,
cache invalidation and zero-producer warm reuse have passing evidence.
Git fetching, browser bundling and Windows linker discovery now share bounded
tool execution; scoped Linux package/process tests pass. Full platform and
preparation-wide acceptance are not implied by those component results.
Optional Node smoke is bounded too, preserving absence versus execution failure;
the repaired fault-injection fixture leaves enclosing observations complete.
Native-boundary proof inputs cover the extracted value definitions; affected
proof/runtime checks and proof-evidence closeout pass. Standalone oracle checks
now use one Cargo invocation instead of four, preserving exact test coverage.
The root compiler build now uses the bounded process owner, with real Make
failure/timeout and production-bootstrap evidence; Linux shares the owner and
cache-tool support build without an extra Cargo invocation. Root-build reusable
receipts now cover clean-candidate support and compiler outputs; complete
preparation-wide containment and production-candidate acceptance remain open.
Cold/warm publication preparation now shares one Make bootstrap graph and one
locked resource-admission node per branch. Fault-injection tests and production
plans verify one compiler producer per branch; cross-invocation zero-build reuse
is still not established.
The canonical `publish-prepare` entry point now holds a bounded preparation lease
across its selected branch and final preflight, while nested admission reuses
that lease instead of reopening a competing lock.
Clean candidates now also persist atomic, output-hash-checked receipts for the
support-owner and root compiler bootstrap. Matching toolchain, dependency,
command and revision inputs reuse all four bootstrap binaries without a Cargo
launch; dirty worktrees deliberately retain the owned Cargo fallback.
The cache-admission producer itself passes 30 unit tests and three real Make
admission tests, including insufficient-space and unsafe-cache fail-closed paths.
Prepared-output recovery now rebuilds changed inputs in the same invocation;
fault-injection tests verify failed replacements preserve all recovered outputs
and interrupted graph recovery retains duplicate-native-work detection.
Proof track production and final lane sealing now own separate reports, removing
read/modify/write reuse of stale fields. Rust proof tests, cold/warm replica reuse,
smoke/lane consumers and proof closeout pass; upstream graph ownership is still open.
Proof-track execution now has fresh content-bound tool admission, complete
private three-output staging and an ordered preparation owner in Make. All 140
Rust proof tests pass; actual cold execution completes 26 replicas and warm
execution verifies reuse of all 26 without modifying final evidence. Owner tests
cover changed inputs/tools, failed replacement and missing-parent recovery
without producer replay. Runtime policy, ownership and regression reports now
also have independent preparation owners: 142 Rust proof tests, parallel Make
failure tests, actual cold/warm report reuse and interrupted-midnight recovery
pass. Warm policy reuse preserves report/ledger/native-log bytes and launches no
policy producer. The three policies now share one source/tool preflight, with
passing graph fault-injection and actual reuse evidence. Verified warm owners
now avoid unnecessary journal transitions: actual policy reuse drops from 20
to 11 subprocesses while preserving every report and owner receipt. Linux typed
builds also use inherited kernel writer leases after real container PID reuse
exposed a false stale-lock wait; contention and interrupted-publication tests
pass. The focused admission test also passes corruption/residue rejection,
changed-input execution and post-producer mutation checks; the corrected test
fixture is now included in the refreshed sealed promotion image.
Broader upstream preflight ownership still needs integration. The proof-kernel
owner graph is integrated and its canonical
promotion image, selective-reuse/parent-recovery self-tests, and full proof
preparation rehearsal pass. The proof baseline proposal was accepted after all
14 slice traces remained unchanged; only candidate identity and its derived
digest changed. Repository contract, fresh Rust boundary AST, structure,
headroom, Rust-quality/docs, focused lint, formatting, and whitespace gates
also pass. This is still not preparation-wide acceptance: upstream
native-boundary ownership and full-candidate orchestration remain open.
Automatic cache cleanup is now wired into both preparation branches under the
publication lock. Nested process containment is exercised by the combined
candidate fixture; full cold/warm/interrupted candidate acceptance and the
upstream report-owner integration remain open.
The hosted proof-smoke producer is now represented by a typed `proof-smoke`
owner with private smoke, blocker, and attempt outputs. Its declaration
self-test is part of the explicit owner-graph closeout and passes in the
refreshed promotion image (`target/v9-proof-smoke-owner-self-test-20260912.log`).
The prior isolated-candidate smoke result is withdrawn as acceptance evidence:
its command removed the native and process observation variables. Observation
has been restored, and empty blocker tables now use an explicit output policy
without weakening the nonempty proof-baseline contract.
The corrected isolated candidate `0c382f2091f00c6ad07e02b024f9a60497592342`
now passes cold and unchanged warm smoke preparation with observation enabled
(`target/v9-proof-smoke-observed-cold-user-scratch.log` and
`target/v9-proof-smoke-observed-warm.log`). Cold execution records 11 completed
native work units, two native links and 17 reaped subprocess launches with no
pending work; all three semantic families, eight lanes and seven script tests
pass. Warm execution reports `completed=0 reused=1`; all three report hashes,
the native/process log and owner receipt remain byte-for-byte unchanged.
The owner/table contract and observation-inheriting smoke rehearsal also pass.
An earlier attempt failed before compilation on root-owned disposable build
storage; the successful rehearsal uses separate user-owned scratch without
deleting that failed attempt.
Root and nested smoke commands now request verified incremental native reuse.
The rebuilt image and command-contract test pass. Candidate
`06cd4617c086cd6a5042be6845a294024b6ab669` was terminated after sealing its native
image and resumed without clearing its interrupted ledger or cache
(`target/v9-proof-smoke-interruption-trigger.log`,
`target/v9-proof-smoke-interrupted.log`, `target/v9-proof-smoke-resumed.log`).
Resume preserves nine completed native-work records and performs zero native
compilations/links; all eight script tests, three semantic families and eight
lanes pass. A subsequent warm run reports `completed=0 reused=1` and preserves
all report, log and owner-receipt hashes
(`target/v9-proof-smoke-resumed-warm.log`). This verifies the post-seal interruption boundary, not every
interruption point or preparation-wide release acceptance. Focused script lint
still reports readability and complexity warnings and is not counted as passing.
The lane producer now owns both lane and gate reports, with a hash-bound,
candidate-local snapshot of the previous lane report as its comparison input.
This avoids feeding newly published output back into its own reuse key. Source,
upstream reports, compiler bytes and reported native toolchain identity are
bound; the producer uses private output paths and verified incremental AOT reuse.
The real recovery fixture passes partial-output failure preservation, unchanged
warm reuse, changed-input execution, output repair and corrupt-history rejection
(`target/v9-proof-lanes-owner-self-test.log`). Candidate
`6ecec52a32bd0ddedfe7b4e155b2b5ba3bb05758` passes all 15 script tests and eight
lane-policy checks in `target/v9-proof-lanes-cold.log`. Its warm invocation
reports `completed=0 reused=1` (`target/v9-proof-lanes-warm.log`); report,
history, execution-log and owner-receipt hashes are unchanged. Both focused
Make contract tests, strict Clippy, full promotion source check and formatting
pass. This closes this producer's integration, not V9-1's full-candidate boundary.
Directory-generation recovery now has a passing foundation: owners bind exact
JSON members, schemas and content hashes, retain the old directory until commit,
and recover interrupted publication without producer replay. Directory-member
consumers require explicit producer edges. The corrected promotion image seals
successfully (`target/v9-directory-owner-build-row-fix.log`); the owner, graph
and candidate suites pass in `target/v9-directory-{owner,graph,candidate}-self-test.log`.
This includes 15 directory fault/recovery scenarios and warm dependent reuse.
The initial AOT build exposed a grouped-let lowering limitation; the helper
separates JSON construction from its fallible read, without claiming a general
compiler fix. The native-boundary producer is now wired to the directory owner:
its compiled recovery fixture, three focused AOT tests, four exact Rust oracles,
and real Lean cold/cached checks pass. Hosted execution preserves coverage and
observation, requires completed proofs, and publishes only a complete private
generation. Parallel Make failure tests verify that proof failure blocks
distribution staging. Clean-candidate execution of the complete native producer
and full-candidate acceptance remain open; this component evidence does not
close V9-1. See the native-boundary generation-owner section in the release
preparation notes for logs and limits.
The platform-contract owner now emits revision-scoped receipts outside the
private preparation subtree, and its validator matches the declarative Make
publish prerequisite graph. The complete preparation target set passes in one
run (`target/v9-preparation-targets-green.log`), covering recovery, multicore,
local reports, AOT, release reports, staged distribution, readiness, and
platform contracts. Readiness uses the immutable VM by absolute path instead
of copying the large VM binary; its production-shaped owner rehearsal passes
(`target/v9-readiness-final8.log`). These close concrete owner and Make wiring
gaps, including shared resource admission for both publication branches, but do
not close the remaining nested-containment or end-to-end candidate-acceptance
requirements. Automatic cache cleanup is now wired into both preparation
branches under the publication lock and covered by the bounded recipe contract.
The refreshed consolidated owner graph, including the proof-smoke owner,
passes in `target/v9-owner-graph-smoke-owner-20260912.log`.
The latest host rerun reaches staged-distribution verification but the retained
0.0.8 archive requires glibc 2.39 while the host provides 2.35. The Ubuntu 24
validation container provides 2.39; the earlier claim that this container was
incompatible was incorrect. The staged-distribution rehearsal passes there:
real installation, warm reuse, checksum-failure preservation, recovery, changed
readiness inputs and scratch cleanup. This is retained-artifact rehearsal
evidence, not validation of a new 0.0.9 release archive.
The AOT closeout graph no longer honors the legacy
`TERLAN_MULTICORE_CLOSEOUT_ALREADY_RUN` environment bypass: every local AOT
correctness gate is admitted on each closeout invocation, while Make's graph
deduplicates shared nodes within that invocation. The refreshed matrix contract
self-test and publication-preparation graph tests pass, so a caller cannot turn
a stale multicore receipt into a correctness skip.
The legacy suite/check bypass removal is covered by focused orchestrator and
release-graph tests plus the refreshed promotion contract image; canonical
Terlan formatting and Rust diff checks remain green.
The refreshed closeout image passes its local self-test and the preparation
contract rehearsal (`target/v9-preparation-contract-final.log`) records the
same four cold launches, selective repair, interruption recovery, and candidate
isolation without a warm-build replay.
The shared Rust fixture now also runs unchanged cold then warm preparation in
one candidate root and asserts zero additional producer launches
(`target/v9-same-candidate-warm.log`). The same fixture now resumes an
interrupted final owner and verifies that only that owner launches again
(`target/v9-release-preparation-resume-final.log`).
An interrupted-upload fixture now retries the `publish` target and records only
read-only verification plus promotion operations on the second attempt; no
preparation producer, Cargo invocation, download, or evidence refresh is
allowed (`target/v9-release-acceptance-final.log`).
That acceptance log now contains five passing publication-preflight tests and
twelve passing preparation-graph tests, including static assertions that the
retry recipe cannot select preparation, Cargo, or evidence-refresh targets.
The unchanged-warm case also compares every successful owner output byte-for-
byte, not just launch counts, so an apparently reused candidate cannot hide a
changed decision payload.
The same acceptance run verifies outer 900-second deadlines for the staged
release-report owner, artifact-matrix verification, and publication upload;
the preparation lock therefore has a finite failure window.
The preparation graph test `preparation_branches_share_one_resource_admission`
also verifies that cold and warm publication branches converge on the same
locked resource-admission owner, with a bounded wait and no duplicate admission
recipe.
The admission command now waits on the shared lock under its 120-second outer
deadline instead of failing immediately on transient contention.
The combined candidate rehearsal additionally executes cold preparation,
unchanged warm reuse, interrupted-owner resume, and a two-attempt upload retry
in one fixture, proving that publication retry does not relaunch preparation.
The `terlan-process-owner` containment suite also passes all 44 tests, including
real nested-owner membership, timeout/cancellation termination of grandchildren,
reaping, and inherited-scope inventory. This is component evidence; the
full-candidate rehearsal still needs to exercise that containment boundary.
The recovery preparation gate now runs that suite before the candidate fixture,
so nested-owner regressions fail the recovery path instead of being reported
only by an unrelated workspace test.
It also runs the combined candidate acceptance fixture as a prerequisite,
covering cold/warm/resume and publication retry in one recovery gate.
The combined candidate fixture now also starts a deliberately hanging
descendant during preparation and verifies that the enclosing owner times out,
reaps the descendant, and leaves no live process residue. This joins the
containment assertion to the candidate cold path instead of relying only on the
standalone process-owner suite; the refreshed gate log is
`target/v9-owner-final-candidate-rehearsal-20260912.log`. Hosted clean-candidate
acceptance remains open.
The complete `terlan-test-orchestrator` crate also exits successfully in
`target/v9-orchestrator-full-final.log`; embedded `FAILED` lines are expected
fault-injection child processes, while every outer Cargo test result passes.
The live publication-plan contracts pass in
`target/v9-publish-plan-final.log`: verification schedules zero build/test
replays, and refresh plans five Cargo invocations, one isolated selector, and
zero duplicate builds.
The real Cargo-backed candidate-owner cold/warm/invalidation/kill/resume
rehearsal also passes (`target/v9-candidate-owner-final.log`).
The named Make recovery gate passes against the refreshed image as well
(`target/v9-candidate-owner-make-final.log`), running process containment and
the combined candidate acceptance fixture before the Terlan rehearsal.
Publication preparation now admits the pinned Rust channel and executable
paths before downloading hosted inputs or entering build/test owners; the same
fixture covers this preflight with a pinned `1.96.0` toolchain.
The hosted-input download and verified compiler probe also have explicit outer
deadlines, with their contract covered by the focused preparation tests. The
hosted coverage refresh is bounded to 1,800 seconds as well. Focused publication
retry validation and the combined fixture pass; full production cold/warm/
interrupted-candidate acceptance still remains required. Component timing gains
do not establish end-to-end release time.
Local `publish-prepare` source preflight is now network- and GitHub-CLI-free;
authentication, branch, ancestry and remote-tag checks are isolated in
`publish-remote-preflight`, which is required only by promotion. This keeps
offline preparation deterministic while retaining the complete remote policy
before publication.
The native-boundary proof input split was re-proved with four exact Rust
oracles and Lean, then its accepted Slice 14 baseline was updated after the
trace change was reviewed. Readiness self-tests now bind the candidate root
explicitly; mounted-worktree rehearsals must expose Git metadata and mark the
checkout safe so inventory cannot resolve an external worktree path.

Implementation evidence and limitations live in
[release preparation notes](../quality/RELEASE_PREPARATION.md) and the
[process-observation checkpoint](../quality/RELEASE_PROCESS_OBSERVATION.md) and
[cache-retention evidence](../quality/RELEASE_CACHE_RETENTION.md).
The [pre-scope-change snapshot](archive/ROADMAP_0_0_9_PRE_RELEASE_SCOPE_2026_09_10.md)
preserves the previous requirements and detailed status. Follow unchecked items
in order; close them only with passing implementation evidence.

## Active Checklist

- [ ] V9-1: Make release preparation resumable and publication retry-safe.
  - Separate immutable source, candidate-specific evidence, reusable caches,
    and disposable workspaces. Generated proof metadata must not dirty tracked
    source or change the candidate identity during preparation.
  - Give every build and evidence producer one owner. Persist an atomic resume
    ledger containing its input fingerprint, tool/profile/target identity,
    dependencies, output hashes, and outcome. Reuse only successful matching
    entries; invalidate affected dependents when inputs change.
    The shared support, compiler, quality-tool, release-benchmark, HTTP
    benchmark, and serve-runtime Cargo bootstraps now use this receipt owner;
    each batch remains a single producer with all emitted outputs bound.
    Typed-AOT images retain their existing process-owner and atomic cache
    contract, and the default-feature AOT release check now records its Cargo
    launch through the shared process owner. Remaining report/proof producers
    and full-candidate acceptance must adopt/verify the same contract before
    this item can close.
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
  - Preserve reviewed count budgets (eight Cargo invocations, seventeen validator
    requests, sixteen validator AOT builds, thirty-two Terlan test processes)
    until measurements justify an explicit reviewed change. Lower costs without
    dropping tests, hiding skips, loosening assertions, or disabling debug data.
    The native/doctest isolation split accounts for the one added Cargo
    coordination call; its differential evidence preserves compiler-unit and
    test-body counts. A second added call builds the small resource-admission
    tool before the compiler; its warm bootstrap plus admission measured 0.44 s.
    See the release preparation and cache-retention notes for these reviews.
  - Acceptance: compare baseline and revised runs, including a test-only edit
    and a validator-only edit. Show reduced affected rebuild/link scope and no
    duplicate work. Timing regressions require investigation; noisy-host timing
    alone must not block publication. Subprocess timeouts remain enforced.

- [ ] V9-3: Verify and publish the focused 0.0.9 release.
  - Review V9-1 and V9-2 against their acceptance evidence. Deferred 0.0.10
    features are not release prerequisites or supported-feature claims.
  - Update active compiler, package, editor, and release metadata consistently
    to 0.0.9; preserve historical version references.
  - Run the canonical release validation once for the selected commit and reuse
    its sealed results for preparation and promotion. Verify installed packages,
    exact artifact identity, checksums, provenance, and failure/retry behavior.
  - Verify that baseline compiler/runtime behavior and CPU-only independence are
    preserved. Do not claim additional ABI, self-hosting, or accelerator support.
  - Record measured cold, warm, and interrupted/resumed validation results.
    Distinguish focused measurements from full-cycle results; investigate
    regressions without requiring an idle host.
  - Release notes describe user-visible benefits, compatibility/security impact,
    and upgrade actions—not internal checkpoints, gate counts, hashes, or work logs.
  - Acceptance: the exact candidate has passing required evidence, installed
    artifacts and notes are accurate, and publication promotes those verified
    bytes without rebuilding them. Confirm the public release and assets.

## Gate Discipline

Use the existing build/release contract, promotion self-tests and canonical
Rust/Terlan tiers. Assign every test to one execution owner. Do not add checks
that merely verify roadmap prose or the deletion/renaming of files.
