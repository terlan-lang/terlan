# Release Process Observation

Updated: 2026-09-10. This is scoped V9-1 evidence, not release certification.

## Contract

`terlan-process-owner` records actual owned launches and direct-child outcomes
when `TERLAN_PROCESS_ACTIVITY_LOG` is set. Records contain an attempt identity,
hashed command declaration, program category, actual owner/child PIDs, elapsed
time and termination outcome. Arguments, environment values and output are not
disclosed. Nonzero exits and failed probes are observations, not automatically
failed validation; missing termination observations remain incomplete.

The typed release validator parses native-build and process events in one pass.
Its `terlan.execution-inventory.v1` summary preserves native work identities and
adds `terlan.process-inventory.v1` counters. Preparation owners retain the exact
inspected bytes alongside their atomic output-set checkpoints. An owner cannot
pass without a recorded launch and reap. Native-cycle recovery checks every
declared output, including the owner/graph commit gap.

Nested preparation must preserve both the caller's log and its private log.
The bounded additive scope encoding is `terlan-process-scopes-v1:` followed by
a JSON array of paths/scopes. Rust resolves paths before changing directories,
deduplicates them, flattens inheritance, and admits at most eight destinations.
Preparation propagates the scope through its `env -i` boundary. Invalid scopes
or log-write failures fail the operation; they never silently drop a destination.
Single paths preserve native OS encoding; multiple paths require UTF-8.

## Verified Evidence

- Initial mixed-stream validator build and complete promotion adversarial tests
  passed: `target/v9-observer-final-acceptance.log`.
- A real signal-shutdown log passed inspection: 18 events, six launches/reaps,
  including three expected nonzero exits. A separate fault-injection log with
  two missing terminal observations was rejected as incomplete, not malformed.
  Logs: `target/v9-observer-actual-log-acceptance.log` and
  `target/v9-observer-incomplete-acceptance.log`.
- Actual Cargo recovery passed cold execution, zero-launch warm reuse, dependency
  invalidation, intentional termination, and equivalent resumed output:
  `target/v9-observer-cargo-acceptance.log`.
- Actual proof-image recovery passed cold/warm execution, repair of each of
  three reports, termination and resume: `target/v9-observer-proof-reviewed-acceptance.log`.
  The reviewed baseline change is solely the Cargo lockfile addition of
  `serde_json` and `sha2` to `terlan-process-owner`; recomputing the candidate
  without those entries exactly recovers the prior candidate identity. All
  fourteen slice traces remain unchanged. This is not a fresh execution of
  the underlying proof/runtime tiers.
- The enclosing-log probe exposed the nested-scope bug:
  `target/v9-nested-observer-probe.log`. The corrective Rust implementation
  passes 42 process-owner tests, including nested inheritance and bounded scope
  failures, plus strict all-target Clippy. The orchestrator's 221 unit tests
  and integration suites pass, as do its strict Clippy checks and production
  compiler/VM Clippy: `target/v9-scope-rust.log`, `target/v9-scope-bootstrap.log`,
  and `target/v9-scope-consumers.log`.
- Rebuilt additive-scope AOT acceptance passed with enclosing observation enabled:
  `target/v9-scope-acceptance.log` records 1,173 launches and reaps, 3,519 events,
  23 expected nonzero exits and no pending/abandoned observations. This includes
  the complete promotion self-test, not just the previously failing owner probe.
- Nested real proof recovery also passes: `target/v9-scope-proof.log` records
  199 launches/reaps across six observing processes, including ten rustc probes.
  No observer is detached by the sanitized producer environment.
- Post-fix structural audit, 72-file headroom check, build/release contract,
  Rust quality/docs, Rust formatting and whitespace checks pass:
  `target/v9-scope-closeout.log`. The image is 21,516,448 bytes.

## Continuation Metadata Cost

Sampling the unchanged validator under GDB located a runtime-wide allocation
defect: 22 of 24 snapshots were in `handle_reply`, predominantly cloning or
dropping the entire continuation-signature vector after a capability transition.
The snapshots are in `target/v9-inventory-samples.log`. Sampling used a disposable
container and did not change host profiling permissions.

Admitted continuation metadata now uses one immutable, reference-counted table
shared by image forks, call caches, and parked actors. Sparse stable IDs use
binary lookup. Admission still rejects duplicate IDs; the runtime index does not
change sealed descriptors, ABI formats, or per-transition type validation.

The exact same AOT image and two input logs were measured before and after the VM
change. Image/input hashes and both JSON summaries match byte-for-byte.

| Input | Before wall time | After wall time | Before/after peak RSS |
| --- | ---: | ---: | ---: |
| 597 events | 13.79 s | 2.40 s | 62,208 / 61,632 KiB |
| 3,519 events | 90.73 s | 20.18 s | 110,888 / 109,352 KiB |

Logs: `target/v9-inventory-baseline.log` and `target/v9-inventory-shared.log`.
These are single-run diagnostics on the same ordinary host with concurrent Rust
work, not an idle-host requirement, statistical throughput claim, or measurement
of total release-cycle improvement. No validator image rebuild was needed for
this runtime-only comparison. Significant validator cost remains to investigate.

All 73 focused native-runtime tests pass, including sparse lookup, shared storage
lifetime, actual image fork/first suspension, and thread handoff. The production
compiler/VM build and strict production Clippy pass. Complete promotion
adversarial tests also pass with enclosing observation: 1,173 launches/reaps,
3,519 events, and no pending or abandoned observations. Logs:
`target/v9-metadata-tests-retry.log`, `target/v9-metadata-bootstrap-serial.log`,
`target/v9-metadata-clippy.log`, and `target/v9-metadata-acceptance.log`.
Real proof-image cold/warm, output-repair, interrupted and resumed execution also
passes with 199 launches/reaps and no missing outcomes:
`target/v9-metadata-proof.log`. Fresh structural, file-headroom, build/release
contract, Rust quality/docs, formatting and whitespace gates pass in
`target/v9-metadata-closeout.log`.

Low disk space interrupted linking before these successful runs. Cleanup removed
only obsolete compiled test executables, lease-approved old incremental sessions,
and the disposable profiling container; source and retained evidence were
preserved. The successful production retry used one Cargo job to bound peak disk
usage. This does not establish preparation-wide resource-budget acceptance.

## Typed Validator Build Ownership

The typed-validator bootstrap runs its explicit producer through the installed
Rust driver's `--run-owned` mode. This mode does not bootstrap tools or dispatch
the correctness suite. It closes stdin, records the actual producer lifecycle,
and uses the shared deadline/cancellation owner. The driver retains its snapshot
read lease until cleanup finishes. Cache fingerprints include the owner's bytes;
even warm admission rejects a missing owner or a failed owner hash read.

The wrapper permits fifteen seconds for cancellation cleanup before forced
termination, rather than killing the owner inside its bounded five-second
activity-log lock wait. Ordinary descendants sharing the producer's process
group are terminated on Linux timeout/cancellation. This does not cover nested
owners that create separate groups, deliberate group escape, or uncatchable
termination of the owner itself. Windows executable suffix resolution is tested
as a path-selection contract; Linux tests do not prove Windows job containment.

Evidence:

- `target/v9-owned-build-tests-retry.log`: all three real lifecycle tests and
  invalid-command dispatch pass, followed by strict all-target Clippy and driver
  installation. The trailing shell invocation used the wrong `--self-test`
  spelling; it is not shell-cache acceptance evidence.
- `target/v9-typed-cache-self-test.log`: the correct `self-test` invocation
  passes cold/warm reuse, equivalent-writer serialization, failure, timeout,
  handled interruption, input mutation, and interrupted-publication recovery.
- `target/v9-typed-owner-integration.log`: the real wrapper records one cold
  producer and none on warm reuse; missing-owner admission fails without
  changing the seal; changed owner bytes invalidate the cache.
- `target/v9-owner-regression.log`: 221 orchestrator unit tests plus snapshot,
  Make coverage and focused proof-bootstrap integration tests pass. Expected
  failing child fixtures exercise fail-closed behavior.
- `target/v9-owned-actual-aot.log`: the real Make-recipe validator is built and
  sealed, then reused unchanged, and its Make-thinness check passes. The cold
  observation log has two launches/reaps (compiler and one owned tool); the warm
  pass produces no owned-launch events. This is a receipt-cold build with
  existing compiler/native caches, not a clean-machine compilation baseline.

The suffix-resolution follow-up and structural closeout evidence are recorded
in `target/v9-owner-closeout.log`. Preparation-wide launch accounting, nested
process containment and the full candidate rehearsal remain required.

## Remaining Compiler Tool Boundaries

Five previously unbounded compiler subprocess call sites now use the shared
capturing process owner: Windows Rust target-library discovery, Git clone and
checkout inspection, and both managed browser-bundling paths. Browser bundlers
share one execution policy. Probes/inspection have a thirty-second deadline;
clone/bundling have five minutes. Captured stdout/stderr are jointly bounded
(one MiB for the Rust probe, sixteen MiB for Git/bundling), and stdin is closed.
Git terminal prompting is disabled. These limits include pipe drainage.

Git cloning claims a fresh empty scratch directory rather than deleting a
colliding path. Bounded capture errors now clean that owned partial checkout,
as ordinary clone failures already did; no failed fetch publishes a lockfile.
This is handled-error cleanup, not a claim of SIGKILL-safe scratch recovery.

`target/v9-build-tool-harness.jsonl` identifies the one compiled test harness;
`target/v9-build-tool-compile.log` records its build. Direct execution of that
harness passes 41 selected Git package and browser tests in
`target/v9-build-tool-acceptance.log`, followed by strict production Clippy and
compiler/VM/native-worker bootstrap. Added behavioral cases exercise real failed
Git cloning and bundler EOF, nonzero status, separate diagnostics and excessive
output. Bootstrap admission reclaimed 2,102,349,824 bytes across fifteen
lease-approved obsolete incremental sessions; these are regenerable caches,
not source or candidate evidence.

The attempted process-runner selector in that run did not match its actual
module path; those ten cases were not part of the 41-test result. Subsequent
optional-tool validation uses the enumerated `commands::process_runner::tests`
path and verifies nonzero selected counts.

Fresh structural, size, documentation, formatting and build/release contract
checks pass in `target/v9-build-tool-closeout.log`, along with the unchanged
fourteen-slice proof-evidence check. Linux execution does
not certify the Windows-only probe or Windows descendant containment. The
optional Node syntax-smoke path is covered by the subsequent checkpoint below.
Nested owners in separate process groups remain outside outer-group cleanup.

## Optional Node Tool Admission

Node syntax smoke now uses the same capturing owner with a thirty-second
deadline, one-MiB combined output cap and closed stdin. Its existing optional
runtime policy is unchanged: an OS spawn `NotFound` result becomes
`skipped:node_unavailable`; a running Node rejecting syntax remains an error.
Timeouts, output overflow and permission/observation errors are not skips.

`OwnedChild::spawn_optional_command` preserves the distinction between OS spawn
errors and observation I/O errors before converting either to diagnostics. A
missing observation directory therefore fails even though its I/O error kind
is also `NotFound`. Successful optional children retain ordinary group cleanup
and reaping. Required process callers still report missing programs as errors.

Evidence:

- `target/v9-optional-tool-compile.log`: 44 process-owner tests and strict
  all-target Clippy pass, including absence accounting, observer `NotFound`,
  permission errors and successful-child ownership.
- `target/v9-optional-observer-probe.log` matched zero tests and is not evidence.
  The corrected exact selector in `target/v9-optional-observer-exact.log`
  exposed an incomplete enclosing observation from the deliberate failure
  fixture. The fixture now runs in a private observer environment; its parent
  still records and waits for that subprocess.
- `target/v9-optional-tool-acceptance.log`: enumerated selections execute all
  32 cases (12 compiler-tool, one real Node and 19 VM process cases), with zero
  failures. Node actually accepts valid syntax and rejects invalid syntax in
  this environment. Strict production Clippy and compiler/VM/worker bootstrap
  also pass. The repaired enclosing log has 105 events: 33 reaped launches,
  three expected spawn failures, and no pending/abandoned attempts.
- `target/v9-optional-tool-closeout.log` records the typed log validator's
  rejection of the incomplete probe and acceptance of the repaired run. Its
  headroom check rejected two lines of growth in `dispatch.rs`; this run is not
  passing closeout evidence.

The headroom failure was fixed by extracting the unchanged neutral/bridge value
definitions into `dispatch/value.rs`, preserving the public re-exports. The
dispatcher shrinks from 906 to 815 lines; its now-obsolete warning-band row is
removed, not increased or exempted. The broader post-refactor suite passes all
102 selected dispatch, process and Node cases, followed by strict production
Clippy and bootstrap in `target/v9-dispatch-value-acceptance.log`. The final
structural closeout is recorded in `target/v9-optional-final-closeout.log`;
that run then rejected stale native-boundary proof inputs. It is not a passing
proof closeout. `target/v9-value-ownership-check.log` passes the fresh AST's type
ownership checks. The proof refresh and successful closeout are recorded below.

No direct unbounded subprocess `.output()` calls remain in the inspected
production `commands/build` tree. This is not a repository-wide subprocess or
cross-platform containment certificate. Bootstrap admission also reclaimed
3,098,484,736 bytes of obsolete, lease-approved incremental cache; no source or
candidate evidence was deleted.

## Native-Boundary Proof Refresh and Oracle Batching

The extracted `dispatch/value.rs` is now declared in the Lean artifact input
inventory, replay fingerprints, script inputs and runtime-oracle signatures.
Moving the definitions therefore does not remove them from proof input coverage.
The Lean source digest and all 22 theorem IDs are unchanged.

Standalone validation now submits the four exact runtime-oracle selectors to
one Cargo invocation and one library harness, instead of four competing Cargo
invocations. Acceptance requires exit success, exactly four passed tests with
zero failures/ignored tests, and the successful output line for each exact
selector. Hosted reuse still submits individual requests to the authenticated
coverage owner; no environment flag manufactures passing test evidence.

`target/v9-native-proof-gate.log` records seven passing Terlan tests, including
negative tests for missing/wrong names, ignored tests, zero matches, nonzero
exit and incorrect counts, plus CRLF handling. The repository test executes
the affected Lean proof and all four runtime oracles: 24 manifests, 184 rows,
22 theorems. `target/v9-native-proof-gate.jsonl` records 12 process events,
four successful reaped launches (including one Cargo launch), no spawn failures
and no abandoned/pending attempts. The outer operation took 40.90 seconds;
the Cargo operation took 20.57 seconds. These are scoped timings, not a release
cycle baseline or a complete inventory of Cargo's internal subprocesses.

The proposal in `target/v9-native-proof-proposal.log` was reviewed by material
path, not array position. Only the dispatcher, extracted values, proof input
inventory and native-boundary script change. Slice 14's trace changes; the
other 13 slice records, proof source and theorem IDs remain unchanged. The
accepted baseline was updated only after the affected execution passed.
`target/v9-native-proof-closeout.log` then passes the adversarial proof-evidence
self-test, six-stage readiness release mode, Terlan formatting and diff checks.
This does not seal a publication candidate or close V9-1.

Build admission reclaimed 1,154,240,512 bytes from four obsolete, lease-approved
incremental-cache sessions before the runtime-oracle build. These caches are
regenerable; no source, current cache generation or candidate evidence was removed.

## Root Compiler Build Ownership

`terlan-compiler-bootstrap` now runs its Cargo producer through the installed
`terlan-test-orchestrator --run-owned` snapshot. The compiler build has closed
stdin, the existing process-observation hooks and a default 3,600-second deadline
(`TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS`). Invalid deadlines fail before compiler
Cargo starts. The snapshot's reader lease remains held throughout the build;
the build no longer requests the orchestrator package or replaces its owner.

`terlan-build-owner-bootstrap` builds and installs that small owner first.
On Linux it shares the existing cache-tool Cargo invocation, so this does not
add a Cargo coordination step or compile the owner twice. Disk admission still
precedes the compiler and remains shared under parallel Make consumers. Other
platforms build only the process owner at this prerequisite, not the Linux cache
tool. Cross-platform runtime validation is still required.

Evidence:

- `target/v9-compiler-owner-tests.log`: eight focused integration tests pass.
  The new real-Make/real-owner test exercises success, compiler failure, timeout
  with an ordinary descendant, closed stdin, support-build failure and two
  parallel consumers sharing a single compiler producer. Existing real disk
  admission, owner cancellation and adversarial refresh-plan tests also pass.
- `target/v9-compiler-owner-closeout.log`: strict Clippy on the affected test
  targets, workspace Rustfmt, whitespace and repository build/release contract
  pass. `target/v9-compiler-owner-thinness.log` passes Make recipe thinness.
- `target/v9-compiler-owner-plan.log`: the publication check has zero build/test
  replay; the refresh dry-run records five Cargo invocations, one isolated report
  graph and no equivalent duplicate build commands. These are plan counts,
  not claims about the full preparation process tree.
- `target/v9-compiler-bootstrap-owned.log`: the real support Cargo step succeeds
  in 0.11 seconds; the compiler/VM/worker build succeeds in 14.05 seconds.
  `target/v9-compiler-bootstrap-processes.jsonl` records exactly one compiler
  Cargo lifecycle (started, spawned, successful reap), observed at 14.14 seconds.

Admission retired six obsolete, lease-approved incremental sessions, reclaiming
1,320,808,448 bytes of regenerable cache. No source or candidate evidence was
removed. The initial native support build is still the bootstrap boundary;
this is not a reusable root-build receipt, a warm-cycle zero-build certificate,
or containment of escaped/nested process groups. Those V9-1 requirements remain.

The bundled quality-tool bootstrap now follows the same clean-candidate owner
contract. Its single Cargo invocation is receipt-backed and binds all eleven
quality/structural-analysis binaries by hash; release and HTTP benchmark groups
and the profile-specific serve runtime use the same one-owner contract. A
matching receipt therefore cannot be mistaken for independent per-tool builds.
Typed-AOT images retain their existing cache's process-owner and atomic
publication contract. Dirty checkouts retain the owned direct-Cargo fallback,
and prebuilt validation consumers do not request the support-owner prerequisite.
This closes three more preparation producers while the remaining AOT/report/
proof producers still require equivalent integration.

The default-feature AOT release check is now also launched through the shared
process owner. Its Cargo start, child lifecycle, timeout and reap are therefore
part of the preparation activity record instead of being inferred from Make
output. The fresh `release-preparation-recovery-check` passes the current graph:
44 process-owner tests, the full candidate cold/warm/changed-dependency/killed-
producer/resume rehearsal, and the expected zero-replay warm path. This closes
the previously unowned AOT Cargo check, but does not by itself close the
preparation-wide ownership requirement for every report and proof producer.

## Shared Publication Preparation Graphs

`publish-prepare` no longer starts separate compiler-owning Make processes for
version checking, evidence refresh and final source checks. After validating
hosted artifact executable compatibility and inspecting existing evidence, it
selects one `publish-preparation-cold` or `publish-preparation-warm` graph.
Version validation and the other consumers share the same compiler/bootstrap
nodes inside that Make process. No new prebuilt/skip flag authorizes this reuse.

`publish-evidence-refresh` now declares its bootstrap prerequisites directly,
instead of opening another build graph from its recipe. Cold evidence still
passes post-refresh verification before cache retirement. Independent source
checks can fail early alongside cold refresh; their failure still prevents final
preflight. Warm distribution verification waits for source checks and cache
retirement, and never duplicates the cold graph's installation work. Requesting
both preparation routes together is rejected before any producer starts.

Executable compatibility is checked before the first compiler bootstrap;
version metadata remains mandatory in both branches and before the cold evidence
producer is admitted. `make publish` and its read-only verification are unchanged.

Evidence:

- `target/v9-preparation-branch-tests.log` passes the real preparation recipes
  with fixture-owned services across sixteen cold/warm/failure scenarios. Each
  successful branch observes exactly one compiler producer and one version
  check, even with parallel Make consumers. Failed evidence prevents cache
  retirement; failed source checks prevent warm installation; all failures stop
  final preflight. The first run exposed a fixture extraction boundary broken
  by the new Make rules; its failed log is not acceptance evidence.
- `target/v9-preparation-routing-final.log` verifies the specific competing-route
  diagnostic and absence of producer activity, then passes strict Clippy,
  workspace formatting and whitespace checks. The earlier diagnostic probe
  accidentally inspected a file while the shared capture helper inherited
  stderr; the corrected bounded probe captures Make's diagnostic explicitly.
- `target/v9-preparation-graph-tests.log` also records successful existing root
  compiler, live coverage and distribution/readiness-routing tests. Its combined
  run stops on the fixture extraction error noted above; do not report that
  whole invocation as passing.
- `target/v9-preparation-graph-closeout.log` passes repository build/release and
  Make-thinness contracts, strict Clippy and formatting. The retained production
  cold/warm plans each contain one compiler bootstrap, with four/two Cargo build
  command lines respectively and no equivalent duplicate build commands.
- `target/v9-preparation-graph-plan-container.log` passes publication's zero
  build/test-replay check and the refresh budget (five Cargo invocations, one
  isolated report graph). The initial host-side plan probe cannot execute the
  Ubuntu-24-built driver on Ubuntu 22; it is not passing evidence.

These are recipe/ownership and plan results, not an end-to-end candidate
rehearsal. Warm preparation still requests support/compiler builds across
invocations; successful input-bound root receipts remain required for V9-1.
No network service, public release, or unrelated checkout was modified.

## Remaining Acceptance

This observer covers `OwnedChild`, not arbitrary grandchildren launched internally
by Cargo, a shell, or a third-party tool. Its command-declaration hash is not a
reusable build identity: inherited environment, executable bytes and source
identity require the producer's separate fingerprint. An `env` launch is still
an `env` launch, not an independently observed Cargo/rustc launch.

Preparation-wide launch coverage, explicit treatment of intentionally killed
nested observers, complete producer integration, producer-level cache/resource
admission and cold/warm/interrupted full-candidate acceptance remain open.
[Abandoned Rust cache retention](RELEASE_CACHE_RETENTION.md) now has scoped
implementation and real-writer recovery evidence. Do not replace
these requirements with a passing component test or a Make dry-run count.
