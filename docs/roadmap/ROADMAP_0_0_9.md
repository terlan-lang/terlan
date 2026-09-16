# Terlan 0.0.9 Release Optimization Roadmap

Updated: 2026-09-16. Baseline: 0.0.8 is published.

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

Candidate `9e8a7be2` exposes a shared validator-bootstrap regression in hosted
release and Docs CI: Unit expression/signature spellings incorrectly join as
two atom variants. The local normalization fix passes 485 NativeIR tests, both
strict workspace-binary Clippy profiles, and actual rebuilds of the package
consumer and Rust-quality validators. Package installation and invalid-command
rejection, docs, API boundaries, module structure, headroom and dependency checks
pass. The required `Ordering.compare` lowering gap now has a local correction:
concrete implementation bodies retain canonical trait identities, and existing
typed overload selection resolves their calls without a runtime dictionary.
All 489 NativeIR tests and 38 Core-lowering tests pass; the Unit, Int, Float and
Comparison suites pass 52 tests, including the previously failing Unit trait test.
The Boolean suite now passes all 17 tests. Typed primitive receivers retain their
identity through contextual specialization, including shadowed callback parameters;
string relational operators reuse checked UTF-8 comparison. Native string equality,
conversions, Unicode uppercase and scalar reversal are wired through the managed
ABI. The String suite passes all 46 tests, including two added Unicode/empty-value
regressions. All 494 NativeIR tests and 27 managed-operation ABI tests pass locally.
Both canonical Clippy profiles, the rebuilt quality validator, API boundaries,
module structure, file headroom, refreshed dependency impact and documentation
checks pass for this correction, without increased budgets. Committed-candidate
validation is still required; logs use the `primitive-family-`, `string-family-`,
and `string-transforms-` prefixes. The subsequent table-suite probe passes the
15 assertion and six lifecycle tests. The TableTest Iterator mismatch is traced
to the production embedded loader omitting transitive dependencies, unlike the
test loader. A local correction follows the canonical dependency manifests,
handles namespace indexes separately, and passes all 38 formal-pipeline tests,
821 other type-checker tests and two dependency/type-contract probes. TableTest
now reaches native lowering, where a higher-order tail call has no converged
continuation profile; Base64/Md5 callbacks require non-tail suspension lowering.
Those remain required baseline fixes. Both strict Clippy profiles and the rebuilt
API/module/headroom/dependency gates pass for the loader correction, with unchanged
budgets; evidence uses `embedded-dependencies-`. No required tests are waived.
For the preceding trait correction, the rebuilt quality validator,
canonical Clippy profiles, API boundaries, module structure, headroom, dependency
report and documentation checks pass locally. No candidate-wide success is claimed.
Evidence uses the `concrete-trait-`, `unit-result-`, `unit-package-consumer-` and `unit-rust-quality-`
prefixes under `target/quality/release-diagnostics/`. All checklist items stay open.

The coverage-anchor audit found stale operator and pattern source paths and
executable AOT gaps. The local correction retains aggregate operand/scrutinee
types, constructs structural maps in source evaluation order, normalizes ordered
function heads, and implements bounded native String/Int/Float/Bool captures.
Singleton aliases now match according to the scrutinee's representation rather
than whether an unrelated union contains the alias. Concrete union construction
supports zero-field atom variants beyond `None`. Transparent aliases are resolved
in lambda, let, comprehension and try patterns; inlined aggregate lambda arguments
retain the types needed by structured result inference. No evaluator or dynamic
compatibility path is restored, and no coverage rows or assertions are dropped.

The rebuilt compiler now passes all 57 PatternMatchingTest tests. The other
affected source suites pass: language features 9, operators 12, comparisons 29,
string-pattern long-tail 5, Option 17, Result 14 and Binary 72 (215 tests total).
The current default-feature library harness passes 484 NativeIR tests and 39
managed-operation ABI tests. Both configured workspace-binary Clippy profiles
and the runtime-only binary profile pass, as do formatting, API boundaries
(3,082 internal string-error sites), module structure, file headroom and the
refreshed dependency-impact report, without increased budgets. All 26 affected
coverage-validator tests and source-anchor inventories pass independently of the
execution tests. Logs use `pattern-lambda-`, `pattern-final-` and `pattern-closeout-` prefixes
under `target/quality/release-diagnostics/`.

These are local correction results, not new committed-candidate CI or full-cycle
acceptance. An additional noncanonical test-target Clippy probe failed on 467
diagnostics outside the modified files; that broader audit is not claimed green.
The pattern family remains partial for its separately inventoried unsupported
contexts. V9-1 production acceptance, V9-2 and V9-3 remain unchecked.

Candidate `d4b85fe4` passes the complete hosted release workflow in 67m01s,
including all six platforms and distribution attestation. Compiler CI's owned
Rust harnesses pass in 1,289.63 seconds, and the previously failing BinaryTest
now passes. The next gate fails because the language-feature coverage matrix
still references an obsolete lambda fragment. The correction points to the
current syntax-to-Core lambda construction, preserving every feature row,
test reference and coverage requirement. The repository coverage check and all
nine language-feature execution tests pass locally; the unchanged five coverage
validator tests already passed in CI. This metadata correction still requires
committed-candidate verification. Evidence uses the `language-anchor-` prefix
under `target/quality/release-diagnostics/`. V9-1 production acceptance, V9-2 and
V9-3 remain open; no merge or publication is authorized by the green release run.

Candidate `c68f80bb` passes the complete hosted release workflow in 71m29s,
including all six native platforms, both sanitizer families, the dependency
audit, consolidated artifact validation and distribution attestation. Compiler
CI's owned Rust harnesses pass in 1,403.53 seconds, but the later
shape-implications gate rejects Binary.decode_protocol_integer's complete
Boolean/endian tuple case. Finite coverage incorrectly treats constructor alias
spelling as its runtime atom, so BigEndian does not match Atom["big"] during
disjointness analysis. A focused regression reproduces the failure; the local
correction reuses the existing alias-aware matcher. All seven finite-coverage
tests, the other 814 type-checker tests, the separately executed release
collection contract, and the exact failing BinaryTest case pass. This is local
correction evidence, not passing CI for a new candidate. Logs use the
`finite-alias-` prefix under `target/quality/release-diagnostics/`.
Both strict workspace Clippy profiles, formatting, API boundaries, module
structure, file headroom and the refreshed dependency-impact report also pass.
No analysis, size, dependency or lint allowance budget is increased.
V9-1 production acceptance, V9-2 and V9-3 remain open; no merge or publication
is implied by the successful release workflow.

Candidate `53fd1a47` passes the complete hosted release workflow: six native
platforms, both sanitizer families, the patched dependency audit, consolidated
artifact validation and attestation. Its duration is 75m05s including scheduling
and bookkeeping, versus `f7602c02`'s 83m40s; these individual observations do not
establish the required reproducible speedup. Compiler CI's canonical Rust report
passes in 1,421.870 seconds, but release-promotion validator compilation fails
because imported optional struct fields lose dependency type information.

The local correction shares dependency-alias resolution with interface signature
loading. Negative tests also expose union return checking that accepted any
matching branch and Boolean literals inferred as Dynamic. Return checking now
requires every alternative, preserves complete generic unions and diagnostic
details, and does not commit substitutions on failure; literals retain Bool.
The stronger check reveals imported constructors returning unqualified nominal
types, a missing Option type import in std.test.Test, and two HTTP response
helpers returning Unit on their enabled branches. Constructors retain provider
identity, Test imports its report dependency, and the HTTP helpers use the
existing response-returning header builder. Their generated summaries are fresh.
Direct calls to the optional HTTP helpers remain outside the existing native
managed profile; this change does not introduce a new native surface.

All 821 type-checker tests (including the release collection sweep and complete
Test/Response source contracts) pass. The earlier 1,129 other compiler tests,
22 managed-HTTP lowering tests, three managed-HTTP runtime tests, both strict
workspace Clippy profiles and runtime-only Clippy pass. Both validators now
build and seal; promotion/preflight adversarial tests, repository contract,
native-boundary ownership and candidate fixture cold/warm/interrupted-resume
rehearsals pass. This is not production preparation acceptance. All 206 summaries,
embedded interfaces, the 85-module release manifest, native artifacts, Rust-backed
contracts and negative APIs pass. API, module, dependency, documentation and lint
checks pass without increased budgets or allowances; the declarations no-growth
ceiling decreases from 993 to 992 lines. Local logs use the `typeck-` prefix under
`target/quality/release-diagnostics/`. The API follow-up repeated the six manifest
fixture tests from the earlier local closeout; these diagnostic runs do not
establish the required single-owner/no-replay production acceptance.
Exact-candidate hosted verification remains required after these corrections.
Production cold/warm/resume acceptance, V9-2 and V9-3 remain open. No merge or
publication is implied.

Candidate `f6f6dc4d` commits the dependency corrections below. Its hosted
security audit discovers newly published RUSTSEC-2026-0285 affecting locked
Rustls 0.23.42; the earlier passing audit predates this advisory. The local
correction raises the Rustls minimum to patched 0.23.45 and updates its required
crypto/certificate dependencies. The unchanged warnings-denied security audit
passes, as do 358 selected transport/related tests, strict default/all-feature
workspace and runtime-only Clippy, and the refreshed dependency-impact check.
The dependency budgets remain 56 direct dependencies and 34 duplicate families.
Committed-candidate hosted verification remains required.

Review also invalidates the combined preparation rehearsal's old interruption
claim: a second cold run against an already-completed fixture failed at the
source preflight, not the intended final owner. A strengthened assertion
reproduces that false positive. The correction uses an independent cold fixture,
proves the final owner actually failed, compares completed outputs, and requires
resume to launch only that failed owner. The corrected rehearsal and the other
23 preparation tests pass, as does strict test-target Clippy. These remain
fixture results, not production cold/warm/resume acceptance. Evidence uses the
`preparation-interruption-` and `rustls-` log prefixes under
`target/quality/release-diagnostics/`.

Draft PR #22 is not ready for merge or publication. Candidate `305902f3` passes
the complete hosted [release workflow](https://github.com/terlan-lang/terlan/actions/runs/34851940995):
all six native platforms, both sanitizer families, the security audit,
consolidated artifact validation, and distribution attestation. Docs and CodeQL
also pass. [Compiler CI](https://github.com/terlan-lang/terlan/actions/runs/34851947641)
passes all owned Rust harnesses in 1,416.05 seconds and confirms the bounded
runtime-history, generated-reentry, module-structure, and lint corrections below.
It then fails the internal string-error inventory gate: command, compiler, and
native-boundary helpers exceed their unchanged site budgets, and moved helpers
have stale inventory locations. The local correction retains typed structural,
coverage-budget, and process failures until their existing diagnostic boundaries;
it refreshes moved inventory rows without raising budgets or adding allowances.
The fresh AST-backed API gate passes, as do 1,303 affected tests, the separately
owned release standard-library contract test, and both configured strict Clippy
profiles. These local changes still require committed-candidate hosted verification.
The runtime-only feature profile additionally exposed unconditional compiler-tool
exports. Their module and exports now follow the existing compiler-presence
condition; strict runtime-only Clippy passes without dead-code allowances.
Bounded lint/module checks, file headroom, Rust documentation, dormant-runtime,
and deterministic-map checks also pass. Local logs use the `api-boundary-`
prefix in `target/quality/release-diagnostics/`.

That correction is committed as `f767592f`; its compiler and release workflows
are running. A subsequent local dependency-impact check exposes the next gate:
58 direct normal dependencies exceed 56, 36 duplicate families exceed 34, and
the report/classifications are stale. The in-progress correction shares Unix
pipe readiness through the process owner, uses Hyper's public body trait, and
migrates the editor to `tower-lsp-server` 0.23.0. Fresh Cargo metadata shows
56 direct dependencies and 34 duplicate families without changing budgets.
All 134 previously executed LSP tests remain in the new compiled inventory;
four URI tests are added. The new virtual-document test exposed that URL path
conversion does not itself require the file scheme. With explicit scheme
admission, the rebuilt harness passes all 141 LSP/command tests. Shared process
ownership (49 tests), runtime process capture (31), and Hyper/SSE (10) pass too.
Cargo-audit 0.22.2 passes with warnings denied, as do strict default/all-feature
workspace Clippy and runtime-only Clippy. The Tokio boundary remains intact.
Disk space briefly fell below 200 MiB; full compilation paused until space
became available again without this session pruning shared caches.
The final source-bound dependency report, dependency-impact gate, API gate,
lint allowance scan and workspace-policy check pass. The build-graph gate also
passes after refreshing the stale same-name Entry classification: proof
tool-admission entries bind executable contracts, distinct from disk retention,
parse-cache and multicore-inventory entries. Module structure and file headroom
also pass; committed-candidate hosted verification remains outstanding. Detailed local evidence uses
the `dependency-` log prefix in `target/quality/release-diagnostics/`.

The preceding `2b018189` candidate exhausted the unchanged 128 MiB address-space
limit in the module-structure validator. Constrained reproduction and
debugger backtraces identify unbounded production retention of test-only actor
ownership history. Removing that retention makes the exact failing gate pass
in 23.80 seconds at 83.4 MiB peak resident memory. A follow-up constrained
lint-allowance run exposes the same defect in scheduler queue-transition history.

The local correction removes both production histories while preserving actor
ownership checks, scheduler behavior, and cumulative counters. Test-only traces
retain a bounded prefix and reject incomplete replay evidence explicitly; no
trace is silently presented as complete. Regression tests cover storage bounds,
concurrent actor-event ordering, and scheduler-history cleanup. The 57 scheduler
tests and 209 actor/profile/control tests pass; the one ignored control helper is
executed by its passing parent with all eight explicit seeds. The scheduler file
is below the warning band and the actor-directory no-growth ceiling is reduced
to its actual size. Both configured Clippy profiles, the rebuilt dormant-runtime
checker, and constrained module/headroom checks pass. The lint and workspace
validators initially remained within their memory limit, but reached the existing
1,048,576-resume limit. Native debugger snapshots show scanner progress between
resumes; generated continuation re-entry forces a yield on almost every scanner
step instead of using the existing reduction budget. A new compiled regression
reproduces that excessive-yield failure before the correction; afterward it
executes one million steps on a 128 KiB stack with periodic yields and bounded
resume counts. All 469 NativeIR tests and 25 direct-backend/handler tests pass,
as do both configured Clippy profiles. The full rebuilt lint scan passes in
129.35 seconds at 81.4 MiB peak resident memory. Workspace validation then exposes
a duplicated `getrandom` version in the orchestrator manifest; switching it to
workspace inheritance preserves the exact dependency and lockfile. The complete
orchestrator tests pass, and workspace validation passes in 128.81 seconds at
42 MiB. Module structure and headroom pass in 22.58 and 20.71 seconds, with
69 near-limit files and zero oversized or inline-test files. No memory, resume,
or timeout limit is increased. The validator image
grew from 7,471,880 to 11,969,232 bytes as generated reentry joins native tail
components. This is a recorded code-size cost for V9-2's pending dispatcher work,
not a claim that every cost improved. Candidate `305902f3` confirms these
runtime and validator corrections in the hosted compiler workflow.
Logs and debugger backtraces are in
`target/quality/release-diagnostics/` (`generated-reentry-*`,
`transition-telemetry-*`, `module-structure-bounded-*`, and
`lint-allowance-bounded-backtrace.log`).

V9-1 still requires complete production cold/warm/resume acceptance. V9-2's
harness/dispatcher splits and comparative measurements, and V9-3's version
update and publication, remain open. Active versions are still 0.0.8. The
latest consolidated hosted final stage measured 10m32s, versus `2b018189`'s
7m03s and the baseline's two stages totaling 18m00s. Total hosted release duration
was 67m56s, versus 63m54s and the 80m45s baseline. Investigation records slower
Rust compilation (250s versus 176s) and platform-matrix AOT compilation
(259.04s versus 147.83s), without a duplicated final bootstrap. The workers use
the same runner image but different regions; this does not establish a causal
explanation or dismiss the regression. Measurements are recorded in
`target/quality/release-diagnostics/hosted-timing-305902f3.md`. These are individual
hosted observations, not the required reproducible cold/warm/localized-edit
comparison.

### Candidate validation checkpoints

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
The two numeric-count CodeQL alerts were dismissed with source-level reasoning
after explicit user approval; CodeQL is now green. The redaction diagnostic
alert is resolved.
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

Candidate `bb7b3401` confirms the ABI correction in hosted Compiler CI: all nine
ABI tests pass after 6,225 core and 1,160 integration-tier tests pass. Three
direct-AOT integration tests then fail. Local reproduction identifies missing
Memory storage variant declarations, a rejection fixture whose deep condition
is now supported, and a consumer fixture still using the removed execution-worker
protocol. Explicit storage aliases preserve the emitted atom identities; native
execution now verifies all three values. The rejection fixture retains the
fail-closed native-admission assertion using an unsupported fixed array, and a
separate positive test executes the former deep-condition case. These three
focused checks pass locally. Migrating the remaining worker fixture's assertions
to the current AOT/shard interfaces is still required; no test is skipped and
the full integration suite is not claimed green. Logs are under
`target/quality/release-diagnostics/direct-aot-{before,after,memory,rejection,clippy}.log`.
The separate hosted release run `34767160245` now passes completely: all six
native targets, both sanitizer families, security audit, consolidated validation,
and distribution attestation. The final validation job took 9m07s, versus 18m00s
combined for the baseline's two final jobs. This is a measured final-stage
reduction, not a full-cycle speedup: the current run took 87m33s versus 80m45s,
with macOS x64 taking 78m02s. Evidence is retained in
`target/quality/release-diagnostics/release-bb7b3401.json` and
`target/quality/release-diagnostics/release-validation-103760690224.log`.
The worker-fixture migration now executes 49 application assertions through the
production shard, using one compiled image with explicit reachable test entries.
It exposed and fixes a reachability defect: a parameter named `timer` was mistaken
for an imported function value, retaining `Process.timer`'s native placeholder
after its actual intrinsic call had already lowered correctly. All three pruning
paths now reuse lexical free-variable analysis, preserving real callback values
while excluding parameters and local pattern bindings. Eleven pruning tests pass,
and the standalone nested-timer build/execution succeeds. The 49 application
assertions return successfully before the retained obsolete-worker assertion
fails; this is migration progress, not a passing full integration test. Evidence:
`target/quality/release-diagnostics/pruning-scopes.log` and
`target/quality/release-diagnostics/direct-aot-shard-migration.log`.
The compiled-image boundary regression exposed another representation error:
bodyless VM tokens such as `Process.ExitReason` were rewritten as native-worker
resource records. Exact canonical VM identities now retain their intrinsic ABI;
similarly named package resources still receive private handle layouts. Six
native-package tests pass. The new compiled-image test builds once and verifies
ten operations both directly and through non-tail calls, ordered/live captures,
repeated suspension, owner/request/continuation authority, duplicate-resume
rejection, and malformed-word rejection by the production capture validator.
It passes with no added public backend API or duplicate runtime validation.
Evidence: `target/quality/release-diagnostics/native-package-token-layouts.log`
and `target/quality/release-diagnostics/direct-boundary-compiled.log`.
The obsolete worker integration assertions remain until their complete coverage
has been migrated; the compiler pipeline and V9-1 are not yet closed.
Strict Clippy passes for the production library and the two touched integration
targets. An additional `--lib --tests` audit fails with 468 diagnostics in the
library test harness; the configured release Clippy gate targets workspace
binaries, not that harness. This broader audit is not a clean result and has not
been suppressed. The multi-stage integration helper now groups its two ordered
capture expectations rather than exceeding the argument-count limit. Logs:
`target/quality/release-diagnostics/direct-boundary-production-clippy.log` and
`target/quality/release-diagnostics/direct-boundary-clippy.log`.

The boundary suite now covers direct, delegated, and composed calls for all ten
transition operations, plus branch selection, capture ordering, repeated
suspension, and native arithmetic failures. All scenarios share one compiled
image; the delegated-call and branch checks pass. Extending the application
fixture to execute collection identities exposed two compiler defects, rather
than merely stale test expectations: scalar replacement looped indefinitely on
an aggregate alias chain, and typed Map/Set construction lost its explicit type
arguments. Scalar replacement now requires actual destructuring progress (not
an iteration limit), with all 30 surrounding tests passing. Collection lowering
preserves all List/Map/Set type arguments; its focused type-check/lowering test
and the compiled-image collection execution both pass. Evidence includes
`target/quality/release-diagnostics/scalar-replacement-aliases.log`,
`target/quality/release-diagnostics/typed-collection-constructors-with-interfaces.log`,
and `target/quality/release-diagnostics/direct-boundary-delegated-fixed.log`.
The main application fixture now executes 53 expressions through the VM and
uses owned, deadline-bounded lifecycle checks. Its former worker-wire assertions
have moved to the actual private direct backend. The remaining condition and
call-composition integration fixtures still require migration; this is not a
passing canonical compiler run or V9-1 closeout.

The next migration step preserves the three condition/expression/tail integration
test names while replacing their execution-worker handshake with one focused
AOT build and a production-VM contract per target. All three pass, in
`target/quality/release-diagnostics/direct-aot-condition-vm.log`,
`target/quality/release-diagnostics/direct-aot-condition-expr-vm.log`, and
`target/quality/release-diagnostics/direct-aot-tail-vm.log` (approximately 1.09 s,
0.94 s, and 0.59 s, excluding Rust compilation).
The same focused source fixtures feed the single compiled-backend image, whose
additional assertions retain ordered captures, exact suspension counts,
short-circuit error timing, Unit/Boolean word validation, callee continuation
identity, stale/foreign ownership, and duplicate-resume rejection. The direct
boundary passes in `target/quality/release-diagnostics/direct-boundary-condition-expr.log`.
Delegated Unit calls retain caller values in VM completion frames, rather than
exposing them as the callee's captures; this matches the current execution-shard
architecture and does not restore the removed worker protocol. Shared test
support owns temporary directories and bounds compiler/VM execution, including
expected failures with checked stderr. No additional public backend API or
production compatibility layer was added. Two legacy call-composition targets
remain, and canonical compiler/production-candidate acceptance is still open.
Stronger inspection of actual VM-owned completion frames then exposed an
unnecessary identity frame on direct tail calls inside conditionals. Prepared
call lowering now forwards unchanged results without allocating that frame,
including branches with local bindings and checked arithmetic. The regression
requires empty caller-frame stacks for tail calls and precise retained caller
values for non-tail Unit helpers; it passes in
`target/quality/release-diagnostics/direct-boundary-tail-prefixes.log`.
The broader native-compiler run caught a reduction-yield regression from this
earlier tail classification. Existing `TailCall` nodes now receive installed
recursive reduction identities too; all 467 native-compiler tests then pass,
including one million recursive edges on a small stack with observed scheduler
yields (`native-ir-tail-yield-fixed.log`). Review also found that generated
continuation annotation tested membership in a set containing every continuation,
making it a no-op. It now excludes only reduction-resume entries, with an
idempotence test proving ordinary re-entry yields while the resume itself does
not immediately yield again. All 468 native-compiler tests pass after that
correction (`native-ir-generated-yields-fixed.log`, 23.94 s excluding Rust
compilation), as does the compiled-backend ownership suite
(`direct-boundary-final-yields.log`). Strict Clippy passes for the production
library and affected integration targets (`direct-aot-call-migration-clippy.log`).
These remain local component results, not V9-1 production acceptance or a green
canonical compiler workflow.

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
