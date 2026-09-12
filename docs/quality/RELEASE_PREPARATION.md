# Release preparation checkpoints

`make publish-prepare` prepares a candidate. `make publish` only verifies and
promotes prepared evidence; it must not replay builds or tests.

Source preparation checks the local checkout only. GitHub authentication,
remote ancestry and tag consistency are isolated in `publish-remote-preflight`,
so an offline `publish-prepare` remains deterministic while `publish` still
requires the full remote promotion policy.

Publication's local-report graph joins the Linux managed-list producer and
multicore/AOT report composers under one flock and declaration-time source/tool
preflight. AOT depends on the managed-list output and retained receipt/log;
focused standalone entry points remain available. Publication cold and warm
branches now pass through one shared, locked resource-admission node, so
concurrent preparation cannot overcommit the local build budget. This is not yet
preparation-wide resumability. Bootstrap and prerequisite checks, including
proof execution before composition, still need full preparation-wide recovery
integration before V9-1 can close. Proof composition, readiness and staged-distribution
checkpoints are covered below; full candidate recovery remains separate.
The canonical `publish-prepare` entry point holds that lease across its selected
branch and final preflight; nested admission reuses the inherited lease instead
of opening a competing lock.
On clean candidates, the support-owner, root compiler bootstrap, bundled
quality-tool bootstrap, release benchmark binaries, HTTP benchmark comparison
binaries, and the profile-specific serve runtime now use atomic receipts keyed by revision,
toolchain/dependency bytes, command identity, and declared output hashes. A
matching receipt skips the equivalent Cargo launch; a dirty checkout uses the
existing owned Cargo fallback so uncommitted source can never authorize reuse.
The quality tools remain one shared Cargo producer and bind every emitted
binary, while benchmark and serve-runtime groups bind their complete binary
sets. Receipt reuse does not create per-tool rebuilds. Typed-AOT
images retain their existing cache's process-owner and atomic publication
contract. The focused Make contract tests cover clean receipt routing and the
dirty fallback.

## Single-writer proof reports (2026-09-10)

### Policy producer ownership

Runtime policy, ownership policy and regression tracking now support the same
private-workspace output contract as proof replay. Invalid destinations fail
before input evaluation; failed policy validation does not replace published
reports. Ownership and regression JSON gain explicit versioned schemas without
changing their policy decisions. Regression execution checks the declared UTC
day both before validation and before writing, rejecting an expired admission.

`target/v9-proof-policy-rust.log` passes 142 proof-focused Rust tests, strict
production Clippy and the quality binary build. Its two Make coverage tests and
strict integration-test Clippy also pass. Parallel failure injection covers each
policy producer and requires runtime, replay, ownership, regression and report
staging to execute in order; source-test bodies are not replayed.

`target/v9-proof-policy-real-staging.log` runs all three actual policy producers
against copied repository inputs in a disposable Git fixture. They validate four
scheduling groups, 22 ownership rows/eight gaps, and eight regression classes
with zero warnings. Their report decisions match prior verified output; only
explicit schema/date metadata differ. No final report is created by the staged
run, and a stale-date request fails without producing an output. The fixture is
`target/v9-proof-policy-rehearsal.iGOqr5`; it is not a release candidate.

The owned regression destination is the stable `lean-proof-regression.json`,
with the UTC day in the producer's environment and reuse key. Standalone checks
retain their existing dated history. This prevents a date change from stranding
a prepared checkpoint at yesterday's destination.

`target/v9-proof-policy-final.log` passes the complete
`release-preparation-proof-check`: selected-input tests, both proof-track owner
scenarios, all three policy owners and prepared-midnight recovery, followed by
the real proof composer's cold/warm/repair/kill/resume rehearsal. The midnight
fixture seals yesterday's producer before a blocked publication, recovers it,
executes today's producer once, then verifies warm reuse. The first image build
failed to infer a local selector containing an empty-list branch; the selector
now has an explicit function result type. The failed build is not acceptance
evidence and does not establish a general compiler inference fix.

`target/v9-proof-policy-production.log` passes actual cold/warm owner execution
with the real quality binary and copied repository inputs. All three cold
reports equal the directly staged results. Warm execution preserves every report,
ledger and retained native log byte-for-byte. Each retained native log contains
exactly one started/spawned/successfully-reaped producer attempt; warm invocation
launches no policy producer. Source remains clean in the disposable fixture.

Structural/headroom, Rust quality/docs and formatting pass in
`target/v9-proof-policy-quality.log`; scoped Terlan formatting and package
TL0009/TL0010 checks pass in `target/v9-proof-policy-style.log`. The repository
build/release contract and publication plans pass in
`target/v9-proof-policy-closeout.log`: zero publication build/test replay,
five Cargo refresh invocations and zero duplicate builds. Shared preparation
preflight, remaining upstream producers and whole-candidate acceptance still
need completion; this does not close V9-1.

Policy preflight now runs once for the three independent report owners.
`target/v9-proof-policy-shared-preflight.log` passes individual policy recovery,
midnight recovery and the shared graph's cold/warm/failure/selective-repair tests.
Its parallel Make tests and strict Clippy pass in
`target/v9-proof-policy-shared-make.log`. PR/regression Make recipes retain
their exact Rust coverage requests; report production belongs to the successful
shared runtime-policy prerequisite, rather than being repeated by those recipes.

Actual unchanged individual invocations record seven source/tool subprocesses
each (21 total) in
`target/v9-proof-policy-rehearsal.iGOqr5/target/preflight-*.jsonl`.
`target/v9-proof-policy-shared-production.log` then verifies all three existing
owners are reused by the combined graph: all six owner receipt/native-log files
and all three reports remain byte-identical. Comparing declaration hashes in
`preflight-shared.jsonl` finds each of the seven preflight operations exactly
once. However, the graph also performs 13 atomic journal/report updates through
`/usr/bin/mv`: **20 observed subprocesses total, versus 21 before**, all fully
reaped. This is not a two-thirds reduction in total process launches or a claim
about full-release timing. The next shared-owner optimization must avoid needless
warm journal transitions or use native atomic filesystem operations, preserving
interrupted-producer recovery rather than weakening the journal contract.

The subsequent warm-admission change removes the unnecessary active-owner,
clear-owner and per-owner progress writes for already verified clean receipts.
It retains graph open/final records, input/output/native-log validation and
interrupted native-claim recovery. A changed-input miss carries the captured
input snapshot into execution; the producer's post-run fingerprint check remains
mandatory. Pending writes or workspace residue take the ordinary recovery path.
`target/v9-admission-production.log` passes the graph fault-injection cases and
the full proof-owner/composer rehearsal. Actual unchanged policy preparation
now records **11 total subprocesses: seven preflight operations and four journal
updates**, versus the prior 20. All attempts were successfully reaped; all six
owner receipt/native-log files and three reports remain byte-identical. This is
a 45% reduction in scoped subprocess launches, not a full-cycle timing claim.
The compiled promotion image is 25,830,280 bytes versus 24,944,456 previously
(3.6% larger); this includes the added admission implementation and tests and
does not establish improved validator build time. The first admission fixture
incorrectly attempted to create a directory symlink to a missing target. It has
been corrected to create the link before deleting its target.
`target/v9-admission-focused.log` passes the named admission test, covering
read-only byte preservation, input/output/native-log changes, prepared receipts,
pending writes, empty/dangling-link workspaces and post-producer input mutation.
The production graph results do not imply that the corrected test source is
already sealed in the promotion image; that image must be refreshed before
using its complete owner self-test as closeout evidence.
Formatting/lint and publication plans pass in `target/v9-admission-plans.log`:
publication retry still has zero build/test replay and refresh lists five Cargo
invocations with no duplicate builds. The full build/release contract has not
been rerun for this final fixture correction; no roadmap item is closed here.

Scoped Terlan formatting/lint and Rust formatting pass in
`target/v9-proof-policy-shared-style.log`. Build/release contract, cycle-free
Make query and publication plans pass in
`target/v9-proof-policy-shared-closeout.log`: publication still replays no
builds/tests, and the five-Cargo refresh has no duplicate builds.

The next upstream integration must also account for semantic-kernel replay:
`SemanticKernels.terls` invokes selected `lean-proof-replay` before producing its
runtime report, and Make currently schedules it before full proof-track replay.
Those paths share replica caches but not a single tool admission. Reordering
must preserve an explicit, content/tool-bound proof prerequisite for semantic
checks, not substitute an unverified already-run flag or introduce a dependency
cycle. This is a source-level integration finding, not a measured full-cycle
speedup.

### Fresh Lean tool admission

`terlan-quality lean-proof-tool-admission <private-path>` validates all selected
contracts before writing a bounded, stable handoff. It binds the repository,
producer executable, UTC date, effective proof environment, installed Lean/Lake
tree and resolved native-loader inputs. Unknown fields, changed context/contracts,
unsafe paths and oversized records are rejected. Execution requires both the
handoff path and its caller-declared SHA-256; partial or invalid assignments fail
closed rather than falling back to unadmitted execution.

The handoff contains content references, not trusted serialized timestamps.
Execution re-hashes real tool bytes, checks the admission digest and context, and
retains the existing mutation checks around each replica. It does not repeat the
admission's version, resolution or loader probes. Admission must run fresh for
each preparation invocation; this is not a persistent tool-admission cache.

- `target/v9-proof-admission-pin.log`: 140 proof-focused library tests, strict
  production Clippy and quality build pass. Tests verify unchanged handoff bytes,
  no resumed tool probes, changed executable rejection, both-variable admission,
  root/producer/date/environment/contract mismatches, symlinks and size bounds.
  Execution also rechecks the source toolchain pin and exact Lake flags without
  replaying tool probes; a changed source pin is rejected.
- `target/v9-proof-admission-real.log`: actual pinned-tool admission followed by
  26 completed replicas produces all three reports byte-identical to the prior
  verified results. Fresh repeated admission has identical bytes and digest.
- Process logs record seven attempts for admission (one outer owner plus six
  probes) and 27 for replay (one owner plus 26 replicas), all fully reaped. The
  measured admission was 3.138 seconds and replay 26.721 seconds; these are scoped
  observations, not full preparation timings.
- `target/v9-proof-admitted-warm.log`: zero completed and 26 verified reused
  replicas, identical reports. Its process log contains only the outer owner:
  no tool probes or proof processes. Warm verification took 7.867 seconds.
  Recorded cold/warm outputs are under `target/v9-proof-admission-evidence`;
  private demonstration workspaces were removed from the preparation namespace.

The first proof-owner rehearsal exposed a shared cold-publication defect: final
report parent directories were assumed to exist. The producer succeeded and its
prepared checkpoint remained intact. Creating the missing directory recovered
the checkpoint without replay (`target/v9-proof-owner-debug-recover.log`). The
shared commit code now prepares all distinct destination parents after verifying
the complete staged set and before moving any file. The proof-owner, generic
owner/artifact/commit and graph recovery tests pass, together with the actual
proof-composer cold/warm/repair/kill/resume rehearsal, in
`target/v9-proof-owner-closeout.log`. A blocked final parent is preserved; fixing
it recovers the prepared output without another producer launch.

The actual updated quality binary passes another cold/warm handoff in
`target/v9-proof-admission-final.log`: 26 completed, then 26 reused replicas, with
all three reports byte-identical to the prior verified results. Structural,
headroom, build/release, Rust quality/docs and formatting checks were run in
`target/v9-proof-admission-closeout.log`.

The proof-track owner now binds every metadata-selected dependency, including
ignored generated dependencies, without consuming its own reports. Make orders
source prerequisites, owned proof execution and local report staging so parallel
preparation cannot collide on the shared flock. The two Make coverage integration
tests and strict Clippy pass in `target/v9-proof-make-coverage.log`, including
parallel prerequisite failures. `target/v9-proof-publication-plans.log` retains
zero publication build/test replay and a five-Cargo refresh with no duplicate
builds. Full upstream runtime, smoke/lane and preparation-wide ownership remains
open; these results do not close V9-1.

### Complete private output routing

The Rust proof-track producer now accepts either none or all three private-output
variables (`TERLAN_PROOF_REPRO_OUTPUT`, `TERLAN_PROOF_TRACK_OUTPUT`, and
`TERLAN_PROOF_BASELINE_OUTPUT`). Partial assignments fail before proof execution.
All destinations must be distinct, absent files in one canonical `.work`
directory below `target/quality/preparation`; existing outputs, symlinked
workspaces and escaping paths are rejected. Ordinary standalone output paths
are unchanged. Owned execution writes the replay JSON, track JSON and baseline
TSV together into staging without replacing final reports.

`target/v9-proof-output-routing-final.log` passes 136 library proof tests, strict
production Clippy and the quality binary build. Tests cover default/private
routing, every partial-assignment combination, aliases, existing files,
symlinks, and preserving all three previously published outputs. The first run's
Clippy failure was fixed without an allowance; it is not closeout evidence.

`target/v9-proof-staging-real.log` exercises the actual pinned-toolchain producer:
26 proof replicas complete into private staging. All three resulting files are
byte-identical to the prior verified outputs; the five captured final proof/gate
and release-evidence hashes remain unchanged. The staged copies are retained
under `target/v9-proof-staged-output`, and the temporary workspace is removed.
`target/v9-proof-staging.process.jsonl` records 99 events for 33 observed
attempts, each with a complete started/spawned/reaped sequence.
This earlier result validates output routing. Proof-track tool admission and
Make integration are now covered above; preparation-wide ownership remains open.

### Mixed report output contracts

Preparation owners can now declare an exact TSV header as an additional output
contract, alongside JSON schemas. The table header is part of the owner identity;
changing it invalidates reuse. Existing JSON declaration encodings stay unchanged.
Table admission requires a bounded UTF-8 file (1 MiB), a unique/nonempty header
(4 KiB, at most 64 columns), at least one data row, no more than 65,536 rows,
and at most 65,536 bytes per row with the exact declared column count. LF and
CRLF and an optional terminal newline are accepted; blank extra rows, bare CR,
wrong headers and malformed widths are rejected. Field semantics remain the
producer's responsibility rather than an inferred schema.

`target/v9-owner-table-bounds.log` passes the actual mixed-output producer's
cold/warm reuse, malformed replacement preserving both verified files, retry,
and corruption repair. It also exercises header/column, row-count, row-byte and
whole-file limits and runs the enclosing owner/artifact/commit regressions.
The preceding `target/v9-owner-table-build.log` passes graph recovery tests with
the new declaration representation. These output contracts are now used by the
proof-track owner described above; they do not cover every upstream producer.

Closeout: Rust AST/module structure and file headroom pass in
`target/v9-routing-structure.log`; Rust quality/docs and formatting pass in
`target/v9-routing-style.log`. The initial repository lifecycle check correctly
rejected the concurrently active promotion build's `.lock`/`.partial` files.
After that build finished, the repository build/release contract and scoped
Terlan formatting/lint passed in `target/v9-routing-final-contract.log`. No
active build was deleted or restarted to satisfy the check.

### Track and lane writer separation

Replay, gap metrics and lane sealing previously modified the same
`lean-proof-gate.json`. Replay read the previous report before replacing selected
fields, so obsolete runtime-policy and lane fields could survive a new execution.
That mutable shared output also prevented independent producer ownership.

Gap metrics are now a pure value included in replay's complete track result.
The track producer writes `lean-proof-track.json` from current inputs without
reading its previous output or the final gate. Smoke and lane validation require
its distinct `terlan.lean-proof-track.v1` schema. Lane sealing alone writes the
final `lean-proof-gate.json`; downstream feature binding and closeout retain that
interface. This separates producer outputs; recoverable publication of the
entire upstream proof graph is still outstanding.

The corrected library selector passes 131 proof tests, including replacement of
stale/corrupt track output without modifying previously sealed lane evidence
(`target/v9-proof-report-ownership-library.log`). The initial thin-binary selector
selected zero tests and is not test evidence. Strict production Clippy and the
updated quality binary build pass in `target/v9-proof-report-ownership-rust.log`.
Repository track execution passes with 26 completed replicas; the explicit warm
acceptance run reports zero completed and 26 verified reused replicas. Its process
log contains seven observed/reaped tool/owner processes, not proof reexecution
(`target/v9-proof-report-ownership-replay.log`,
`target/v9-proof-track-warm.process.jsonl`).

The smoke consumer initially detected the earlier dispatch/value extraction's
unrefreshed native signature. Independent reconstruction using the recorded
pre-extraction dispatcher hash reproduces the old baseline exactly; all other
bound proof/runtime inputs match (`target/v9-proof-smoke-signature-review.json`).
Smoke now also binds `dispatch/value.rs`, with a real batch-read regression for
changed and missing value definitions. Only the reviewed native-dispatch baseline
changed; semantic-chain and unsupported-fallback baselines remain unchanged.
All seven smoke, fourteen lane and four feature-binding tests pass in
`target/v9-proof-report-ownership-consumers-reviewed.log`.

The replay report remains byte-identical, and all eight semantic lane checksums
match the previously verified evidence. Proof closeout verifies thirteen
families, eight lanes and nine baseline classes; release-evidence adversarial
tests and six-stage readiness pass against the unchanged accepted release-proof
baseline (`target/v9-proof-report-ownership-closeout.log`). Fresh AST structure,
file headroom, build/release contracts, Rust quality/docs, formatting and scoped
Terlan lint pass in the ownership quality/style logs. None of these scoped checks
substitutes for the full-candidate cold/warm/interrupted preparation rehearsal.

## Shared process observations (2026-09-10)

`terlan-process-owner` now implements opt-in `TERLAN_PROCESS_ACTIVITY_LOG`
observations for its actual OS launches and direct-child reaping. The bounded,
locked JSONL stream uses `terlan.process-activity.v1`: attempt identity, hashed
explicit command declaration, allowlisted program category, owner/child PIDs,
state, elapsed time, exit status and OS error category. Raw command arguments,
paths, environment values and child output are not recorded. The declaration
digest is not a source/tool fingerprint or proof of equivalent reusable work.

An active scope is resolved before a child changes directory and survives
command environment clearing or redirection. A missing/unwritable/full log
prevents launch; failed observation after launch retains child cleanup ownership.
Nonzero exit is an observed outcome, not automatically a failed validation:
negative tests and command probes intentionally exercise such outcomes. Abruptly
killed observers can leave incomplete attempts, which must remain distinguishable
from successfully observed reaping. These observations do not cover arbitrary
grandchildren launched internally by uninstrumented tools.

`target/v9-process-inventory-consumers.log` records 40 passing process-owner tests,
strict all-target process-owner/orchestrator Clippy, and production compiler/VM
Clippy. Tests include real nested launches, concurrent appenders, redaction,
relative paths, environment reset, expected nonzero exit, timeout, unwind, full
log rejection before execution, and reap-log failure without a live child leak.
Fault-injection fixtures isolate their log destination through a test-only,
thread-local lookup seam so they cannot corrupt an enclosing validation log;
spawned propagation probes exercise the production lookup without that seam.
The enclosing observed run retained 126 events across 43 attempts, including two
incomplete attempts from intentionally killed nested owners. These are diagnostic
observations, not a successful preparation receipt.

The initial orchestrator shutdown selector selected zero tests. The corrected
exact selector passed all three signal cases in
`target/v9-process-inventory-shutdown.log`. No empty selection is claimed as a
passing execution gate. Release-owner schema/retention integration and actual
rebuilt-VM script execution remain to be completed; the existing native-build
inventory must not silently accept or discard this new schema. No full candidate
rehearsal or preparation-wide inventory completion is claimed here.
The fresh-AST module/headroom, build/release contract, Rust quality/docs and
formatting closeout gates passed in `target/v9-process-inventory-quality.log`.

## Publication tag discovery

Publication source preflight reads checkout status once and stops before network
access on failed inspection or uncommitted changes. It reads the remote tag
object and peeled commit in one `git ls-remote` invocation. A failed query means unknown remote state, never
absence. Tag fetch, local reference inspection and commit resolution failures
also stop preflight. Existing annotated tags must match both the candidate
commit and, when present on the remote, the exact annotation object.

The production Make recipe is exercised against disposable local bare Git
remotes by `tests/publish_preflight.rs` in the Rust orchestrator crate. The
fixture verifies failed/dirty checkout inspection, absent/local-only/remote-only/matching retry states, failed
transport and tag fetches, failed local lookup, lightweight tags, conflicting
annotations and stale candidate commits. Every invocation observes checkout
status once; admitted checkouts make exactly one remote query.
`target/v9-publication-tag-preflight.log` records the passing
fixture; strict all-target Clippy passes in
`target/v9-publication-tag-clippy.log`. Only temporary fixture tags are created;
this is not evidence of a public release or upload-recovery closeout.

## Structural outcome admission

Readiness and offline preflight now share one schema/path inventory and parse
the required JSON fields. Previously they searched report text for passing
decisions, schemas, candidate seals, platform names and archive hashes. A failing
top-level decision with a nested passing decision, or a stale seal/hash copied
into a note, could therefore satisfy admission. Those fields now match exactly
at their specified locations. Platform admission requires an integer count of
six and exactly six distinct supported `target_triple` rows; this establishes
platform contract rows, not six executed cross-host attestations. Supply-chain
and matrix archive bindings use `checksums.installed_archive` and the matching
`host_validation` path/hash/installed-smoke fields respectively.

Readiness now compares the candidate version with parsed release metadata
instead of hard-coding 0.0.8. The final report writer checks that same version
before writing either output, rather than recording a text-search observation.
This does not replace candidate verification or confer trust on unowned reports.

`preflight-self-test` exclusively owns the new admission tests. The rebuilt AOT
validator passes 25 structural assertions covering compact JSON, decoy fields,
malformed/non-object reports, wrong scalar types, stale seals, duplicate/missing/
extra platforms, an incorrect count and misplaced hashes. A disposable-file
rehearsal calls the actual readiness and preflight consumers, accepts matching
0.0.9 fixture metadata, rejects failing reports/stale seals/wrong versions, and
preserves prior readiness output on failure. These constructed reports and
archive bytes are not a certified release candidate. Evidence is in
`target/v9-evidence-admission-test.log`; the corrected build and scoped TL0009/
TL0010 lint are in `target/v9-evidence-admission-build-final.log` and
`target/v9-evidence-admission-lint-final.log`. An initial missing purity annotation
in the extracted constant table was corrected after compiler rejection.
The resulting promotion image measures 15,202,480 bytes (previously 14,201,088);
no compiler optimization profile or artifact limit was changed.

Preparation-wide ownership remains open; the cold preparation duplicate found
during this admission audit is addressed below.

### Candidate evidence and preparation bookkeeping

Candidate sealing previously enumerated every quality JSON file except five
derived release outputs. That also included preparation ledgers, private owner
outputs and Cargo metadata query records. Updating a checkpoint could therefore
invalidate the seal of otherwise unchanged release evidence.

`CandidateEvidence` now excludes the exact `target/quality/preparation/`
namespace and the two root Cargo metadata bookkeeping files, in addition to
the existing derived-output exclusions. Other gate reports remain sealed;
similarly named files and nested directories are not broad exclusions.
Enumeration failures are errors, not an empty successful inventory. Relative
roots account for the filesystem API's path normalization.

The existing promotion self-test mutates bookkeeping within its sealed private
fixture, then verifies the original candidate using the actual payload builder.
It also checks genuine lookalike/nested reports remain in the inventory. Existing
archive and genuine-report mutation/removal checks remain in the same execution
tier. The first AOT run passes in `target/v9-candidate-evidence-test.log`.

Readiness now additionally requires every mandatory report to have exactly one
sealed path/hash entry matching the bytes it parses. A passing report omitted
from the candidate, or changed while retaining a passing decision, cannot satisfy
readiness. This does not replace full candidate verification. The consumer
fixture rejects absent, empty, duplicate, wrong-hash and malformed inventories,
plus changed report bytes that still declare a passing decision. Failed
admission preserves the previous readiness report, and restoring the original
bytes restores admission.

The final AOT promotion and preflight self-tests, repository contract, changed-file
formatter checks and full lint for both new modules pass in
`target/v9-candidate-evidence-acceptance.log`. The earlier closeout passed its
functional tests but exposed deep expressions and an oversized test function;
small helper extraction fixes those without allowances or raised limits.
The final image is 16,110,136 bytes. An interrupted rebuild exited 143; normal
bootstrap subsequently completed in `target/v9-candidate-evidence-resume-build.log`.
This is not the full-candidate interruption/resume acceptance rehearsal.

The bounded Rust cache audit/prune retired two obsolete incremental generations,
reclaiming 1,229,799,424 allocated bytes while preserving 565 current/protected
sessions and release evidence (`target/v9-candidate-evidence-cache-{audit,prune}.json`).
The readiness checkpoint is implemented below; end-to-end candidate acceptance
remains open.

### Readiness checkpoint

Publication's cold hosted graph and warm `publish-staged-distribution-prepare`
now select the same two-owner readiness graph. Standalone readiness checks retain
their direct producer. The first owner seals `dist/release-candidate.json`; the
second verifies and attests that existing seal without resealing it. Both write
to private owner output and use the existing prepared/committed recovery
protocol. The only newly admitted owner output outside `target/quality/` is the
exact candidate-manifest path, not arbitrary distribution files.

`CandidateInputs` is shared by sealing and checkpoint invalidation. It includes
metadata, the documentation selector, archives and sidecars, VM/worker binaries,
release notes, benchmarks, genuine quality evidence, and the actual selected
standard-library/documentation/editor files. Thus ignored generated component
files are inputs too. Candidate metadata must match the requested version and
clean Linux source revision before either producer runs. Traversal failures no
longer become successful empty component inventories.

Combining Git and component inventories uses a linear merge of already sorted,
unique paths, rather than insertion-sorting the entire tracked-source list.
The real AOT fixture checks the merge against all 11,228 tracked paths. The
promotion entry point shrinks from 972 to 668 lines through shared inventory and
fixture extraction; no limits or allowances were changed.

`make release-preparation-readiness-check` runs actual promotion subprocesses
against synthetic artifacts in a disposable Git fixture. A fixture-only wrapper
counts producer launches and kills an actual readiness process; its real VM
bytes are included in tracked fixture inputs. The rehearsal verifies two cold
launches, zero unchanged warm launches, wrong-version rejection before launch,
preservation of the previous report on interruption, one-producer resume, added
ignored-component invalidation, and repair of only the corrupt readiness output.
The fixture is removed afterwards. These constructed reports are not release
certification or the full-candidate acceptance rehearsal.

Final evidence is `target/v9-readiness-owner-acceptance.log`: the owned graph,
promotion and preflight self-tests, repository contract, and changed-file
formatting all pass. Full new-module lint passes before the build in
`target/v9-readiness-owner-final-build.log`; the image is 17,105,648 bytes.
All four real-Make preparation tests and strict orchestrator Clippy pass in
`target/v9-readiness-make.log`, including standalone/owned routing and failure
propagation under `-j8 -k`. Fresh AST/module structure, headroom, repository
build/release contracts, Rust quality/docs and Rustfmt pass in
`target/v9-readiness-owner-quality.log`. Preparation-wide admission, remaining
producer ownership and full clean-candidate/upload recovery remain open.

## Cold preparation distribution ownership

`publish-prepare` no longer repeats readiness sealing and staged installation
after a successful full evidence refresh. The existing-evidence branch still
executes the focused readiness/distribution repair. The choice is initialized
inside the same shell from actual command outcomes, not an environment-provided
skip assertion. A failed refresh or its post-refresh evidence verification stops
before boundary checks and final preflight. Boundary failures also prevent the
focused repair and final preflight.

The production Make recipe passes twelve bounded real-Make scenarios in
`tests/publish_prepare_make.rs`, with external services instrumented rather than
downloaded or published. Readiness and staged-installation producer identities
reject a second invocation. Both successful branches run them exactly once;
failure injection covers source preflight, readiness, installation, full refresh,
post-refresh verification, boundary checks and final preflight under `-j8 -k`.
Opposite caller-supplied branch variables do not change either execution path.
`target/v9-publish-prepare-graph.log` records both tests passing in 0.47 seconds;
strict all-target orchestrator Clippy passes in
`target/v9-publish-prepare-clippy.log`. This verifies recipe sequencing, not real
installer behavior or a full candidate rehearsal.

### Staged distribution checkpoint

Publication's cold hosted graph and `publish-staged-distribution-prepare` now
select the same `staged-distribution` owner. Standalone developer checks retain
their direct producer. Candidate verification and readiness prerequisites are
not bypassed. The owner binds tracked source, the VM and platform-validator
image, candidate and release metadata, host archive and checksum, and readiness
report. Readiness must structurally pass for the candidate seal before admission.
Selected installer executable bytes supplement the runtime-tool fingerprint;
this is not complete dynamic-library or wrapper-input closure.

The installer receives a closed environment with private `HOME` and `TMPDIR`
inside owner scratch. Its report is written to private output and published
through the existing atomic owner protocol; failed attempts must preserve the
last successful report. This uses the existing host archive and does not build
or publish a distribution. Readiness now uses the preceding checkpoint graph.

The explicit acceptance target is `make release-preparation-staged-distribution-check`.
Its disposable Git fixture copies the public installer and real host archive;
candidate/readiness admission records are constructed, not release certification.
Installer launch counters exercise cold execution, unchanged warm reuse,
failed-checksum preservation, recovery, changed-readiness invalidation and owned
scratch cleanup. The first actual rehearsal failed before installer launch:
an inferred `List[File.CopyPlan]` operand lacked its emitted collection
descriptor (`target/v9-staged-owner-actual.log`). A standalone NativeIR
regression reproduces the same omission without installation. Intrinsic
specialization now preserves admitted inferred list operands through the
existing checked-argument annotation path, and visits every list member rather
than stopping at its first type witness. All 460 NativeIR tests pass in
`target/v9-inferred-list-fixed.log`. The strengthened regression also executes
the emitted native object against the managed runtime; it and the repeated-pass
traversal test pass in `target/v9-inferred-list-execution.log`.

Both rebuilt validators pass the real installer rehearsal in
`target/v9-staged-owner-rehearsal.log`: five cold installer launches, zero warm
replay, one deliberately failing checksum launch preserving the previous
report, five recovery launches and five after changed readiness. Failed
readiness is rejected before launching. The final report matches the original
bytes, and the fixture verifies that only its ledger and retained native logs
remain inside the checkpoint directory before removing its private root.
The promotion image is 15,751,736 bytes and the platform image is 9,616,912 bytes.
This is actual public-installer execution against the existing host archive,
not a complete release candidate or publication rehearsal. The separate
Make routing fixture passes direct/owned producer selection and failed-candidate
admission under parallel keep-going Make; all three integration tests and strict
orchestrator Clippy pass in `target/v9-staged-owner-{make,clippy}.log`.

The additional full test-target Clippy diagnostic still exposes previously
documented test debt (605 library-test errors before cleanup), unlike the
passing canonical production Clippy run. Automatic fixes rejected conflicting
suggestions in textually included sources. Reviewed follow-up edits preserve
doc-comment paragraphs, use field shorthand and a byte string, and retain two
successful integration-test cleanups. No lint allowances were added, and these
edits are not a claim that full test-target Clippy passes.

The subsequent size gate caught growth from 902 to 920 lines in the compiler
specialization module. Moving list traversal and operand annotation into its
existing support module reduces the parent to 897 lines; the obsolete headroom
row is removed, with no limit increased. The shared traversal also covers
positional List/Set constructors without stopping at the first type witness.
All 462 NativeIR tests and strict production Clippy pass after this extraction
(`target/v9-list-support-{tests,clippy}.log`). Fresh AST/module structure and
repository-contract checks pass, followed by the corrected headroom, Rust
quality/docs, Rustfmt and diff checks
(`target/v9-staged-owner-closeout-final.log`). Changed Terlan files also pass
formatter checks. Full test-target lint cleanup and the full-candidate
preparation rehearsal are not closed by this evidence.

## Owner contract

### Pre-AOT validator bootstrap

The existing `build_typed_validator.sh` bootstrap now redirects an explicit
`--out-dir` (or a directly declared image argument) into image-local staging.
Compiler failure, timeout, handled interruption, and input mutation before
publication preserve the preceding image and seal. Common/source fingerprints
are rechecked after waiting for another writer and after compilation. Only the
declared executable image is published; temporary package-build side products
are discarded with the staging directory. This does not change the compiler's
reusable native-object cache.

`terlc build --print-toolchain-identity` is a source-free, read-only probe owned
by the same native backend that builds images. It reports the host target,
backend/cache/codegen/build-policy identities, adapter ABI, and exact native
linker identity. The latter now includes a digest of declared linker-relevant
environment values (including absent versus empty settings), not plaintext
environment contents. The inner native cache and this outer bootstrap probe
share that identity; the shell does not implement another linker-selection policy.
Make binds it into the common compiler/stdlib fingerprint once per graph.
The build-plan report counts this one read-only probe separately from the
unchanged 16-AOT-build budget; it is not an extra compilation.

Linux validation includes two environment-identity regressions, strict library
Clippy, missing-linker and malformed-probe-argument rejection, and a real
compiler/bootstrap rehearsal: unchanged settings reuse the image, changed
`SOURCE_DATE_EPOCH` changes the seal and rebuilds, and the new image executes.
Compiler-independent bootstrap self-tests also require changed toolchain identity
to invalidate the common fingerprint. This does not yet inventory transitive
SDK/tool dependencies or all actual compiler/linker subprocess launches.

The bootstrap self-test passes warm reuse, corrupt-image repair, input changes,
concurrent equivalent writers, failed builds, handled interruption, timeout,
mutation during compilation, and a TERM-ignoring builder. Builder stdin is closed;
TERM has a two-second grace before KILL. A real `.terls` compiler build through
staging executes successfully and its warm invocation reuses the sealed image.
No new shell entry point was added: this bootstrap precedes the preparation
validator image that contains the Terlan owner implementation.

Image/seal publication now has a recovery journal under the writer lock. Before
replacement, it retains hashed copies of the preceding files and binds the
pending seal to its image owner and interrupted input fingerprint. If the new
image is intact, recovery finishes sealing it without replaying compilation;
otherwise it validates both backups before restoring the preceding pair.
Interrupted recovery retains its journal and backups. Corrupt or foreign journal
metadata fails closed instead of starting another builder or deleting evidence.
Cache admission never accepts the pending transaction as a completed checkpoint.

The canonical build/release contract passes actual SIGKILL injection after image
replacement and after seal replacement, with no additional builder launch on
resume. It also passes corrupted-image rollback, rejected corrupted seals and
foreign journals, and recovery after repaired metadata. Injection uses a private
test PATH shim, not production fault/skip flags. This is process-crash recovery,
not a filesystem power-loss/fsync durability claim or a simultaneous two-file
replacement for arbitrary readers.

The subsequent [typed build-owner integration](RELEASE_PROCESS_OBSERVATION.md#typed-validator-build-ownership)
adds actual producer launch accounting, closed stdin, deadlines and ordinary
process-group cancellation. Complete nested-process containment, whole-VM
signals, complete tool/environment identity, and integration into the
preparation ledger remain open.

The 2026-09-10 resumed promotion build exposed a Linux container recovery bug:
the terminated writer's PID was 35, and the next container assigned PID 35 to
the new wrapper. The legacy PID-only directory lock therefore made the new
writer wait on itself. The original container and producer were confirmed
terminal before its lock was moved, intact, to
`target/v9-typed-writer-lock-evidence/canceled-legacy-lock`.
Linux writers now use an inherited `flock` on a persistent `.writer-lease`
inode, not a PID as an ownership token. The inode must not be unlinked during
cleanup; live descendants retain the lease after wrapper termination. Legacy
directory locks require verified recovery rather than guessed cross-namespace
PID liveness. Non-Linux locking is unchanged and is not covered by this fix.
`target/v9-typed-writer-lease.log` passes live-lease contention, concurrent
equivalent builders, failed/canceled/timed-out producers, and the existing
image/seal SIGKILL and corrupt-publication recovery cases. The resumed real
promotion compiler built and sealed its image under the new lease. Its open
descriptor confirmed inheritance into the compiler; a competing host-namespace
writer was rejected while the compiler ran and admitted after it terminated.

The generic cleanup audit found a separate unsafe path: `cli-clean` ran Cargo
cleanup first, then deleted generated trees and partial markers solely by name.
That is not proof of exclusive ownership, and a partial marker can be necessary
for journal recovery. `make clean` and the script's default invocation now fail
before any deletion; `--dry-run` is an inventory, not a removal authorization.
Owned pruning remains available through `rust-incremental-cache-prune`, and
interrupted publication is recovered by its existing producer. This conservative
restriction is not a claim that preparation-wide automatic cleanup is complete.
The production-script/Make fixture in `target/v9-cleanup-ownership.log` passes
with a live kernel lease and after its release: archived outputs, native objects,
sealed images, pending journals, prior images and owner receipts remain intact,
and no Cargo-clean command runs. Both tests and strict integration Clippy pass.

### Native cache publication

Checked frontend payloads, native object units, descriptors, images, and their
manifests share the native cache writer. It now reserves temporary files with
exclusive creation before assigning cleanup ownership. Existing temporary paths
are not adopted or removed; generated-name collisions are skipped with a bounded
retry count. Linker output scratch is reserved the same way.

Publication writes and syncs its owned temporary file, then attempts one rename.
A failed rename does not authorize deleting the old destination. The former
delete-and-retry fallback could destroy a verified artifact when replacement
failed. Rust 1.96's inspected Windows implementation already requests replacement
through `MoveFileExW`; a separate destructive fallback is not needed. This does
not claim that Windows execution has been certified here, or that a collection
of artifact files has filesystem power-loss transaction semantics.

On 2026-09-08, six native-cache tests and sixteen other VM artifact tests passed.
The new tests inject replacement failure, preserve a colliding temporary file,
check exclusive scratch cleanup, and reject directory destinations without
damaging their contents. Existing tests retain poisoned-key/target and complete
manifest coverage. Strict library Clippy, Rust size/documentation gates, formatter,
and whitespace checks passed. Evidence is recorded in
`target/v9-cache-ownership-build.log` and
`target/v9-cache-ownership-related.log`; this is cache-slice evidence, not complete
preparation recovery or subprocess accounting.

A real cold AOT build and execution also pass with reserved linker scratch.
Unchanged warm builds succeed with cache misses forbidden and preserve the image
hash. An intentionally failing linker publishes no image and leaves no `.tmp`
files; the original toolchain still reuses and executes its verified image.
The emitted fixture remains 14,952 bytes. This rehearsal is recorded in
`target/v9-cache-ownership-link-recovery.log`.

### Native execution inventory

Setting `TERLAN_BUILD_ACTIVITY_LOG` to a caller-owned log path enables native
execution-point records. Its parent must already exist. The compiler records
module-object emission, application/dispatch-object emission, and native-link
attempts. Records use `terlan.native-build-activity.v1`, an attempt identifier,
the native cache input digest, compiler/child PIDs, outcome, elapsed time, and
emitted object bytes. Arguments, environment values, and source text are not
logged. Link artifact size is currently unmeasured (`null`), not zero.

The `started` event is an execution attempt, not proof that a subprocess exists.
`spawned` is emitted only after the OS returns a child PID. A spawn failure has
no such event; unsuccessful execution has `spawned` followed by `failed`.
Incomplete scope exits are `abandoned`, and process termination can leave a
started attempt without a terminal record. These are not successful checkpoints.
An emission completion records generated bytes, not subsequent cache publication.

Writers serialize individual records with kernel file locks and enforce a five-
second lock wait and 16 MiB log ceiling. Configured inventory failures reject
work rather than silently dropping events. Post-spawn observation failure retains
the process owner's cleanup obligation. Warm native-cache reuse emits no work
events. Logging is opt-in and does not participate in code-generation identity.

On 2026-09-08, four inventory tests, ten CLI process tests, and nineteen VM process
tests passed, along with strict library Clippy. Real AOT rehearsals recorded two
module emissions, one application-object emission, and one linker launch for a
cold fixture; unchanged warm reuse appended no events. Two intentionally
concurrent equivalent invocations also emitted each cache key only once. Real
failed-link and executable-format failures produced distinct execution/spawn
outcomes, with no published image or leaked link scratch. The image remained
14,952 bytes. Evidence: `target/v9-native-inventory-build.log`,
`target/v9-native-inventory-rehearsal.log`, and
`target/v9-native-inventory-counts.log`.

This is execution instrumentation, not completed preparation-wide deduplication.
An object-cache audit also found that module emission embeds the application
name while the unit key omitted it. Native-unit schema v7 includes that input.
The actual object-cache regression verifies distinct paths for renamed
applications, byte agreement with fresh emission, and unchanged-file reuse for
the matching application. All 27 VM artifact tests, strict library Clippy, and
Rust size/documentation/whitespace checks pass in
`target/v9-native-unit-identity.log`. This protects byte identity; it does not
establish normalized cross-application emission equivalence. The ABI helper
documentation now explicitly distinguishes ABI identity from the complete
implementation closure required by cross-module tail emission.

The preparation owner must still assign and retain cycle inventories, validate
closed-cycle ownership before consuming streams, join these records with Cargo/self-host launches,
separate emission equivalence from full-image cache identity, and enforce the
complete execution-tier inventory. A cache-input hash alone is not evidence that
every semantically equivalent operation has the same key. Existing Make dry-run
budgets remain unchanged and are not replaced by this focused fixture.

### Bounded native module-object storage

Development and serve builds now place reusable module objects in
`native-aot/units-v2/entries`, separate from the older unleased `units` namespace.
The default per-root budget is 2 GiB of logical file bytes, 4,096 generations,
and seven days of idle retention. Atomic-replacement staging is included in the
byte reservation. All missing members of the current link set reserve their
entry slots before parallel emission starts.

A stable root lease covers object preparation through final linking. Retention
cannot retire another cooperating build's live linker inputs. Frontend work is
outside that lease, and Cranelift emission remains parallel inside an object set;
different builds sharing one root serialize their object/link window. Lock
acquisition remains bounded by the native-cache lock timeout. Budgets that cannot
hold the pinned inputs plus staging fail explicitly, without selecting pinned
objects for eviction.

Eviction chooses unpinned expired or least-recently-used generations, renames
them to `retired`, and unlinks only validated, flat compiler-owned files.
Interrupted retirement and recognized private scratch are recovered before
reuse. Unknown files, nested directories, and symlinks fail closed. This is a
cooperating compiler cache protocol, not protection against an adversary replacing
filesystem ancestors concurrently. Verified hits refresh an access marker without
rewriting the object bytes.

Focused coverage includes byte/count/age pressure, full-link-set reservations,
over-budget replacement preserving existing inputs, interrupted retirement,
unowned-file and symlink rejection, and a second consumer waiting for the first
link lease to finish. The 35 VM-artifact tests pass in `target/v9-unit-cache.log`;
that run then stopped at three needless-borrow Clippy diagnostics. Those references
were corrected, and strict library Clippy passes in
`target/v9-unit-cache-green.log`.

The real compiler rehearsal passed with two module emissions, one dispatch
emission, and one linker launch on a cold build. Unchanged `--incremental` reuse
with cache misses forbidden appended zero events; two concurrent equivalent
builds also recorded only that single set of work. Moving the private fixture's
image manifest aside required one dispatch emission and link, but no module
emission: both object modification times and sizes were unchanged. The rebuilt
image retained the exact cold-image SHA-256. Cold/concurrent images each measured
15,368 bytes. The native-inventory validator accepted all three actual event logs.
Evidence: `target/v9-unit-cache-rehearsal.log` (cold and quality checks),
`target/v9-unit-cache-warm.log`, and `target/v9-unit-cache-relink.log`.
The initial warm invocation omitted `--incremental` and was correctly rejected by
the no-rebuild guard; the corrected command reused the existing cold cache.

The native-object budget does not encompass final images, Cargo incremental
storage, or legacy object namespaces. Checked implementations have the separate
bucket policy below. Loaded images need lifecycle-aware pins;
older compilers do not participate in the new retention lease. Neither category
is deleted by this change. Full preparation-wide cache ownership remains open.
The root lease alone cannot protect a surviving linker after its compiler is
killed. Each actual link therefore receives exclusively created, private hard
links outside the evictable unit namespace. These preserve immutable input bytes
without copying object payloads; canonical replacement or retirement cannot
invalidate the linker's paths. Normal scope cleanup removes those private names.
Hard-link failure is an explicit build error, not an unprotected fallback.

Three additional filesystem tests cover replacement/retirement, exclusive-name
collisions, and symlink rejection. All nine native-cache tests, strict library
Clippy, and compiler/VM/quality builds pass in `target/v9-link-pins-build.log`.
The Linux process-death rehearsal in `target/v9-link-pins-kill.log` paused the real
linker, sent SIGKILL to its compiler, and aged the two canonical inputs. A second
real build retired both inputs. The surviving linker then completed successfully
using its private names, and the second application's image executed successfully.
Rust size/documentation/format/whitespace checks also pass. The expected killed
compiler is not counted as a successful build or publication.

The final pin-enabled cold/warm/concurrent rehearsal also passes in
`target/v9-link-pins-warm.log`: four operations and one actual linker launch for
each cold cache, no work appended by unchanged warm reuse, and no private input
names left after successful linking. Both images of that fixture are 15,336
bytes and execute successfully. After the killed-linker fixture terminated, its
three identified temporary files (19,144 bytes) were removed; logs were retained.

Uncatchable termination deliberately leaves private input references rather than
risking deletion under a surviving descendant. Automatic orphan-reference cleanup
still needs process-tree ownership and belongs to the remaining V9-1 interruption
work. These references are outside the canonical object-cache byte budget; this
change does not claim a bound on total native build storage. The real kill test
was executed on Linux, not Windows or macOS.

### Checked implementation retention

Native objects and checked implementations now share `artifact_cache_retention`:
one budget/reservation/retirement algorithm with closed, family-specific payload
allowlists. Checked artifacts use `checked-v2/<first-hash-nibble>/entries/<sha256>`.
Older unleased `checked` storage is neither read nor deleted by this owner.

The sixteen buckets have independent leases, 256 MiB and 256 entries each, and
seven-day idle retention, enforced on access/publication. This bounds owned
canonical checked storage to 4 GiB and 4,096 entries without a whole-cache scan
for every module. A full bucket evicts its own oldest unpinned entries even when
another bucket has room: bounded scan scope trades some utilization for predictable
per-operation cost. The 64 MiB payload limit applies to publication as well as
loading; staging reservations are included in each bucket's byte limit.

The current reader is pinned while manifest verification copies its payload into
owned memory. The bucket lease is released before JSON reconstruction and imported
interface discovery, so those frontend operations do not serialize bucket peers.
A verified byte read refreshes access metadata; semantic dependency validation
still decides whether the checked implementation can be reused. Malformed or stale
payloads remain cache misses, but unknown files, invalid ownership, and retention
failures are explicit build errors rather than permission to silently recompile.

Focused coverage adds entry pressure with a pinned current reader, propagation of
retention errors without deleting unknown source, isolation from an old unleased
namespace, and interrupted checked-payload scratch recovery. Existing corruption
and dependency-invalidation tests continue to exercise the production frontend.
The shared refactor's 42 artifact tests and strict library Clippy pass in
`target/v9-checked-retention.log`. The final shorter lock scope passes all six
checked-cache frontend tests, strict library Clippy, and compiler/VM/quality
builds in `target/v9-checked-lock-scope.log`. Native publication also no longer
reacquires a per-entry lock and repeats miss verification under its already-held
exclusive object-set lease.

A 65-module directory fixture has before/after evidence in
`target/v9-checked-cache-before.log` and `target/v9-checked-cache-after.log`:

| Implementation | Cold wall / user CPU | Warm wall / user CPU |
| --- | --- | --- |
| Previous unbounded checked storage | 13.362 s / 75.084 s | 13.554 s / 74.699 s |
| Bounded buckets, short read lease | 11.046 s / 66.847 s | 11.293 s / 66.616 s |

These single observations were made on a shared host, with Rust compilation
overlapping the earlier sample; they are not a controlled speedup claim. All 65
checked payload modification times/sizes remain unchanged on warm reuse, and the
native execution log does not grow with cache misses forbidden. The inventory
validator accepts the cold log: two reachable module objects, one dispatch object,
and one link. The resulting image is 14,784 bytes before and after. Size,
documentation, formatting, and whitespace gates pass with the revised build.
The revised image executes and returns `true`; compiler SHA-256 identities and
Linux host details are retained in `target/v9-checked-cache-closeout.log`.

Warm frontend time remains substantial (8.891 seconds of its 11.293-second
invocation). Source inspection shows that directory interface loading reparses
the same `.typi`/`.terli` bytes for each module. Retaining serialized checked bodies
alone does not eliminate this repeated work; the shared parsing change below
addresses it. This evidence does not close V9-1 or establish the complete V9-2
cost report.

### Shared interface parsing

File-backed and embedded interface consumers now share one exact-content cache.
Every file is reread, so same-length edits with restored timestamps, removals, and
malformed replacements remain immediately visible. Identical bytes share parsed
results across paths; callers receive independent interface values. The parser
is pure and non-reentrant, and synchronized content stripes prevent simultaneous
identical cold requests from repeating a parse while the result remains resident.
Different stripes parse concurrently without holding the resident-map lock.

Retention is limited to 4,096 entries, 32 MiB of source keys, and fifteen minutes
of idle time. The byte figure bounds retained source representations, not all
heap allocations in parsed interfaces or caller-owned clones. Invalid text is
negative-cached; parser panic publishes no result and permits retry. Inputs larger
than the retention budget still parse but are not cached; cache capacity is not a
new language limit. Eviction can require reparsing, so this is not a promise that
all interface bytes are parsed only once across an entire preparation cycle.

`--timings` now reports cumulative interface requests and actual parse attempts,
including failed attempts. On the same 65-module fixture, the shared cache recorded
5,460 requests / 84 attempts cold and 5,525 requests / 84 attempts warm:

| Build | Previous wall / user CPU | Shared-parser wall / user CPU |
| --- | --- | --- |
| Cold | 11.046 s / 66.847 s | 3.643 s / 5.654 s |
| Warm | 11.293 s / 66.616 s | 2.808 s / 3.367 s |

Warm application-frontend time fell from 8.891 seconds to 0.436 seconds. These are
single shared-host observations, not CPU-quietness gates. The emitted image is
byte-for-byte identical (14,784 bytes), executes successfully, and unchanged warm
builds rewrite none of the 65 checked payloads and append no native work events.
Evidence: `target/v9-interface-cost.log` and its retained fixture inventories.

Nine cache tests cover concurrent first parsing, independent stripes, exact bytes,
negative results, returned-value isolation, entry/byte/age limits, panic recovery,
and same-size/same-time file edits. Those tests, three scoped-loading regressions,
strict library Clippy, and compiler/VM/quality builds pass in
`target/v9-interface-parsing.log`. A further 128 interface/editor/typechecker tests
and six checked-cache frontend tests pass in `target/v9-interface-consumers.log`;
the twelve already-run tests were excluded from that consumer sweep. Repository
size, documentation, formatting, and whitespace gates also pass.

### Linux Rust incremental-cache retirement

`terlan-build-cache` is a compiler-independent bootstrap executable, built by
`make rust-build-cache-bootstrap`. The read-only `make rust-incremental-cache-audit`
and explicit `make rust-incremental-cache-prune` consume it without invoking Cargo.
Its tests belong to the existing workspace-support tier, not a second release
test invocation. It is not added to published distributions or publication retry.

The tool recognizes the verified Rust 1.96.0 cache header and Linux session-lock
protocol only. It operates on this checkout's `target/debug/incremental`, keeping
each crate/flavor's newest finalized session and every recently created or busy
session. Older redundant finalized sessions and abandoned working sessions become
eligible after a five-minute grace. A missing/replaced lock never grants deletion
authority. Working sessions additionally require rustc's existing exclusive lease;
partial payloads are retired, never promoted to completed cache entries.
Exclusive leases on rustc's existing session files prevent retiring a directory
being copied or used by a reader. Rust's documented copy-on-write and lock model
is described in its [incremental filesystem module](https://doc.rust-lang.org/stable/nightly-rustc/rustc_incremental/persist/fs/index.html);
the tested finalized-cache header pins the supported version rather than assuming
every Rust release or Unix locking implementation is compatible. Interrupted
working payloads may lack a complete header; their retirement relies on the
verified lock protocol and recognized flat namespace, not completed-cache validity.

An independent maintenance lease serializes retirement and recovery. Rename into
`target/debug/.terlan-incremental-retired-v1` removes a session from rustc discovery
before flat, validated unlinks. SIGKILL between unlinks releases the leases; the
next explicit prune recovers that residue. Unknown names, nested directories,
symlinks (including dangling ones), and unsupported finalized-cache headers fail
closed before retirement. No recursive production deletion is used. Rust's lock files are
left in place for its GC, not replaced or removed by this tool.

The reported budget is 64 GiB of unique-inode allocated file bytes and 1,024
sessions. External hardlinks can retain storage after removal, so these counters
are not a claim about filesystem free space. Protected sessions over budget, or
active sessions whose bytes cannot be measured, produce a nonpassing budget
status rather than deletion. This is a point-in-time maintenance check, not yet
Cargo producer admission. Other profiles/targets, Cargo executables and harnesses,
registries, orphan lock-file retention, and age-based retirement of each crate's
newest finalized session remain outside this policy. It assumes a trusted local
build directory, not hostile ancestor replacement or power-loss durability.

The initial sixteen focused tests cover read-only inspection, protected generations,
shared/exclusive locks, hardlinks, grace, unsafe layouts, budget floors, recovery,
and real process death. An actual Rust compiler rehearsal confirms an existing
archive is untouched by pruning and current object inodes are reused on the next
build. Whole regenerated `.rlib` equality was deliberately not assumed: a control
run showed archive bytes changing even without pruning.

The real 2026-09-08 audit/prune removed 112 redundant sessions, retaining 364.
Unique-inode cache allocation fell from 55,569,342,464 to 35,737,284,608 bytes;
the observed filesystem free-space gain was 19,758,813,184 bytes. SHA-256 checks
preserved `terlc`, `terlan-vm`, and the sealed union-feature test harness. A
follow-up read-only audit found zero eligible sessions. Removed derived caches
can be regenerated by Rust; no source, executable, or evidence was removed.
Evidence: `target/v9-rust-cache-package.log` (all sixteen tests),
`target/v9-rust-cache-real-green.log`, `target/v9-rust-cache-kill.log`, and
`target/v9-rust-cache-prune.log`. Strict package all-target Clippy passes;
workspace policy passes in `target/v9-rust-cache-boundaries-final.log`.
`target/v9-rust-cache-closeout.log` records passing build-graph/canonical-type,
Rust size/documentation, formatter, and whitespace checks. Three same-name
classifications from preceding compiler-cache work now document their distinct
storage, representation, and accounting invariants. Canonical publication plans
retain zero build/test replay and four preparation Cargo invocations with no
duplicate builds (`target/v9-rust-cache-canonical-plan.log`). This does not certify
the previously failing full-workspace all-target Clippy scope.
This is one retention slice, not V9-1 closeout or automatic full-Cargo cleanup.

### Native inventory consumer

The consumer implementation is `release_promotion.NativeActivity`, exposed by
the prebuilt promotion image as `native-activity-check <log>` and covered by
`native-activity-self-test` (also included in the ordinary promotion self-test).
It uses maps keyed by attempt and operation/input identity, not repeated scans
of a growing event list. It checks schema/field shape, state transitions,
stable attempt identity and child PID, monotonic elapsed time, complete records,
and execution budgets. A failed attempt may be retried with a fresh attempt ID,
but every attempted work key must ultimately complete. Completed work cannot be
replayed, including two overlapping attempts that both complete. Missing logs
are errors; an explicitly owned empty log means no native work. The owner must
still ensure producers have stopped before validating a cycle.

Building this consumer exposed a portable-list type boundary defect:
`Json.keys()` returned `std.collections.List.List[String]`, while the list-length
intrinsic required the built-in list representation. Unification now recognizes
only the exact one-argument standard-library list alongside intrinsic lists;
user-defined `List` nominals, wrong arity, and incompatible elements remain
rejected. The special-type helper keeps the unifier source below 1,000 lines.
The final typechecker run passes 799 tests with one existing ignored test;
strict library Clippy and an actual JSON-keys/list-length AOT script pass.
See `target/v9-list-boundary-final.log`. The consumer uses `object_length()`
when it only needs a JSON field count, avoiding unnecessary key allocation.

The AOT consumer passes its adversarial self-test and accepts the real cold and
concurrent inventories: four completed operations, one actual linker launch,
nine events, and 4,040 emitted object bytes. Actual failed-link, failed-spawn,
and concatenated equivalent-cycle logs are rejected. Existing preparation-owner,
dependency-graph, and publication-cache self-tests pass in the rebuilt image;
TL0009/TL0010, Terlan/Rust formatting, Rust size/documentation, and whitespace
checks pass. See `target/v9-native-activity-validator-final.log` and
`target/v9-native-activity-closeout.log`. The promotion image is 9,109,296 bytes,
up 690,560 bytes from the prior 8,418,736-byte image. This cost remains part of
the V9-2 dispatcher-splitting work, not an application performance baseline.

The reproduced `Option[Bool]` exhaustiveness defect is fixed by pattern-directed
finite-region subtraction, without eagerly enumerating Boolean tuple products.
Missing alternatives, duplicate branches, and guarded case branches do not
establish coverage. Wide 40-Boolean tuples exercise lazy splitting; explicit
analysis budgets fail closed. This is not a general infinite-domain pattern
coverage rewrite or a change to the existing function-head warning policy.

The actual AOT smoke then exposed nested generic alias expansion: the cycle
guard incorrectly treated `Option[Option[Bool]]` as definition recursion.
Caller-supplied arguments now expand before the alias body enters the cycle
guard; genuinely recursive bodies still terminate. Local and qualified aliases
have regression coverage. `target/v9-finite-alias-final.log` records 806 passing
typechecker tests (one existing ignored test), strict library Clippy, successful
execution of all seven nested/non-nested Option assertions in a 72,072-byte AOT
image, rejection of an incomplete match, and passing Rust formatting, size,
documentation, and whitespace checks. The original minimized probe now passes.

### Prepared report owners

The Terlan `release_promotion.PreparationOwner` module owns one report-producing
command. Its declaration fixes the source candidate, target, profile, tool
identity, source files, dependency reports, argv, explicit environment, output
schema, successful-execution marker, and timeout. The production entry point
requires clean committed source and runs under an exclusive preparation flock.
Private test fixtures have no competing owners.

The ledger lives at
`target/quality/preparation/<candidate>/<owner>.json`. It records the input
fingerprint, outcome, output inventory, and completed subprocess. Only a passing
ledger with matching inputs and matching output bytes/schema is reusable.
Corrupt JSON fails closed; missing, failed, interrupted, or stale work is not
reused. Interrupted native scratch must first be accounted for before another
producer launch. Environment values contribute to a digest, not a plaintext log.

Owner checkpoint schema v2 additionally owns a private `native.jsonl` in its
scratch workspace. The owner reserves `TERLAN_BUILD_ACTIVITY_LOG`, starts the
log empty, and passes its absolute path through the isolated producer
environment. After the bounded producer returns, native records are validated
before accepting its staged report. Successful raw records are retained at
`<owner>.native.jsonl` and hashed alongside the primary output; the ledger also
contains their validated summary. Warm reuse verifies both retained files.
Old v1 checkpoints do not satisfy this evidence contract. Required and optional
consumption of another owner's native log requires an explicit graph edge;
parent evidence hashes include that log. Graph-local accounting below combines
owners' streams; Cargo/self-host launches, uninstrumented compilers, and escaped
descendants remain outside this coverage.

Failed owners retain their latest raw stream at `<owner>.failed-native.jsonl`
before scratch cleanup, with `native_outputs` hashes in their failed ledger.
Corrupt retained bytes reject a subsequent launch. `NativeActivity.inspect`
distinguishes passing, failed, and interrupted inventories; `validate` still
requires fully resolved work. Inspection can account for completed work and
actual linker spawns preceding a failure, but never authorizes successful owner
reuse. Malformed/truncated logs remain invalid evidence. If retention fails,
scratch is preserved instead of silently deleting the only raw log.

Native inspection and retention now share one bounded byte snapshot. Replacing
or deleting the producer file after inspection cannot change the retained bytes
or their summary; malformed originals remain invalid even if the source is later
replaced with a valid log. `target/v9-native-snapshot.log` records the rebuilt
11,938,352-byte promotion image, all 20 owner test functions (including these
three snapshot scenarios), all 15 graph scenarios, native-activity self-tests,
TL0009/TL0010, Rust size/documentation/formatting, and whitespace checks. These
tests establish byte binding, not completeness of logs written by escaped
descendants after their original producer exits.

Pending scratch recovery checks the original owner, candidate, and input
fingerprint before retaining its stream. The graph consumes this recovery
before another producer starts, including the failed-owner/graph commit gap.
The owner passes its parsed summary to the graph. Publication checks the sealed
raw bytes against their prepared hashes and summary; historical recovery repeats
that verification. It does not trust an unverified summary as an execution log.

### Recoverable owner publication

After a producer passes its exit/stdout, native-log, output-schema, and unchanged-
input checks, `OwnerCommit` records an atomic `prepared` ledger before replacing
either final file. The ledger seals the primary output and a separate copy of
the inspected native bytes. Recovery checks both files before moving either;
missing staged bytes are accepted only when the final bytes match exactly.
An old final output cannot satisfy a new prepared hash, and corrupt native
staging cannot cause the old primary output to be replaced.

Recovery completes missing renames, verifies the final hashes and native summary,
records `pass`, then cleans scratch. A crash after `pass` but before cleanup does
not convert completed work into a failed attempt. No producer executes during
these recovery steps. After historical publication is recovered, a direct caller
rechecks the current identity and rebuilds changed inputs in the same invocation.
Matching inputs reuse the verified publication without another producer. A failed
replacement preserves every recovered output. Graph recovery accounts for the
historical native work before normal current-input invalidation; it does not
discard earlier native claims when the current producer changes.

`target/v9-recovered-input-resume.log` records the rebuilt promotion validator and
passing owner/graph self-tests on 2026-09-10. All twelve transaction scenarios
pass, including a changed-input replacement followed by zero-producer warm reuse,
and a failing replacement that preserves all three recovered output generations.
All sixteen graph scenarios pass, including prepared-publication recovery and
duplicate native work rejection across interrupted attempts. These are scoped
transaction and graph fixtures, not full-candidate acceptance or publication.
Formatting of the changed Terlan sources, package-wide TL0009/TL0010 lint, and
`git diff --check` pass in `target/v9-recovered-input-closeout-final.log`.

`target/v9-owner-commit-final.log` records the 12,912,888-byte rebuilt promotion
image, all 21 owner test functions, all 16 graph scenarios, native-activity tests,
TL0009/TL0010, Rust size/documentation/formatting, and whitespace checks. The eight
transaction scenarios cover interruption before publication, after each rename,
after the success seal, corrupt staging/final/native bytes, and changed inputs.
The added graph case preserves duplicate-work rejection after a partially
published owner resumes. These fixtures inject explicit transaction residue;
they do not claim power-loss durability, full-VM signal cleanup, or a complete
clean-candidate release rehearsal.

The original sealed-before-cleanup case exposed a compiler defect: a synchronous
guard prefix following suspension lost its terminal managed result context,
returning scalar `None` where the continuation required a managed `Option`.
NativeIR now preserves that expected representation through lexical prefixes.
`target/v9-option-resume-execution-red.log` contains the small failing AOT script;
the final run above executes its corrected 43,072-byte image successfully.
`target/v9-option-compiler.log` records the focused source-to-object regressions,
strict library Clippy, and rebuilt binaries. The broader NativeIR suite passed
all 445 tests in `target/v9-option-native-suite.log`; the runtime managed-value
validation remains enabled.

On 2026-09-08, the rebuilt AOT promotion image passed all 19 owner test functions
and six graph scenarios. New fixtures reject incomplete, duplicate, missing,
and malformed native streams before replacing prior evidence; check retained
bytes and one-execution warm reuse; invalidate tampered/deleted logs; and reject
attempts to override the reserved inventory destination. Graph fixtures reject
hidden required/optional native-log dependencies before launching producers.
These ownership fixtures use synthetic native records; the execution-point
instrumentation's real compiler fixtures are documented separately above.
`target/v9-native-owner-final.log` records these passes, TL0009/TL0010 lint,
Rust formatting/size/documentation checks, and whitespace validation. The image
is 9,664,712 bytes (555,416 bytes above the prior 9,109,296-byte promotion image).
Changed Terlan sources were formatted before building. No full release suite,
publication, or clean-candidate rehearsal is implied by these focused gates.

Optional dependencies remain explicitly declared when absent. Their ledger rows
record absence or the present file's bytes; both reuse and post-execution sealing
recheck them. Creating or removing an optional input during execution rejects
the new output and preserves previous evidence. Graph validation also requires
explicit edges for optional consumption of another owner's output.

## Owned publication contract prerequisites

Publication preparation now checkpoints four distinct platform-validator
commands: `self-test`, `tsan-self-test`, `release-self-test`, and
`multicore-release-self-test`. These are contract self-tests, not execution of
the platform matrix or instrumented ThreadSanitizer suite. `PrepareContracts`
declares one owner per command, binding tracked source, the actual VM/image
bytes, explicit environment, and runtime-tool identity. A successful validator
writes a staged `terlan.validator-contract-receipt.v1` only after its assertions
pass. The ordinary owner protocol validates stdout, exit status, unchanged
inputs, and receipt schema before publishing it.

Receipts live under `target/quality/preparation/<candidate>/` beside their owner
ledgers, not in shared cross-candidate output names. Repeated Make requests
revalidate the owner and skip only a matching successful producer. A damaged
receipt reruns its owner; a prepared publication is recovered without running
the validator again. Different candidates cannot overwrite these receipts.

`publish-prepare` and `publish-evidence-refresh` enable this routing for their
recursive Make work. Attempts to disable it with a command-line override fail
before preparation. Ordinary development contract targets still execute
directly. The refresh-plan check rejects direct launches of these four contract
commands that bypass their owners. Other correctness/report/proof/distribution
producers still need their own ownership integration; these four receipts do
not authorize skipping their work.

The canonical `release-promotion-pipeline-check` now includes
`release-preparation-contract-check`. Its disposable fixture executes the real
prebuilt matrix validator with an independent VM-launch counter: four cold
launches, zero unchanged warm launches, one selective corrupt-receipt repair,
one completed producer whose obstructed publication resumes without another
launch, and one new-candidate launch with isolated old evidence. The fixture's
Git commits and counting launcher exist only inside its temporary project.
`target/v9-contract-negative.log` separately confirms a failing contract writes
no successful receipt.

`target/v9-contract-final.log` records this rehearsal, the full promotion
adversarial self-tests and repository contract, Rust size/documentation/format
checks, and whitespace checks. Both typed sources passed TL0009/TL0010 and were
formatted before compilation. The promotion image is 13,465,552 bytes and the
platform image is 9,499,496 bytes. Publication verification still schedules zero
builds/tests; the preparation plan reports four Cargo commands, one isolated
selector, and no duplicate Cargo build (within the existing budgets). Plan
counts are not a replacement for the remaining preparation-wide launch ledger
or clean-candidate interruption/retry rehearsal.

## Dependency graph

`PreparationGraph` accepts at most 256 owners in explicit predecessor-first
order. It rejects duplicate owner names, output paths, or direct producer
identities, mixed candidates, missing/forward/cyclic edges, and undeclared
consumption of another owner's output before launching the first producer.
Dependency edges fingerprint both the upstream output and its successful owner
ledger. A changed upstream input invalidates transitive consumers even when its
output bytes are unchanged; unrelated owners remain reusable.

The graph writes an atomic summary after each owner under
`target/quality/preparation/<candidate>/graphs/<graph>.json`. Failed owners stop
the graph. Resume revalidates individual owner ledgers, never treating a partial
or even a passing graph summary as permission to skip validation. Interrupted
summary writes do not discard successful owner evidence.

The graph-native accounting integration passes its focused acceptance scenarios.
`NativeCycle` keeps a checksum-protected journal beside the graph summary. It
retains successful native work identities across an interrupted invocation,
records the active owner before launch, and can recover the owner-commit / graph-
commit gap from the retained log and matching owner output hashes. A completed
graph starts a new native accounting cycle on its next invocation, so historical
warm-reuse logs are not counted as new launches. Duplicate-work rejection is
persistent and cannot be cleared by retrying with both owners now cached.
Failed-owner streams now contribute completed identities and linker launches;
the consumed receipts retain their inspection summaries. This still does not
span the entire Make/bootstrap/Cargo/self-host preparation cycle. Latest-owner
raw files are not a generation-retained history of every attempt. Remaining
hardening includes full descendant cleanup before inspecting logs and recovery
across preparation-wide producers beyond these report owners. Pending or truncated execution cannot prove
whether unreported work completed; cache receipts and global ownership still
need the end-to-end interruption rehearsal.

`target/v9-cycle-tag-acceptance.log` records the actual promotion image build
(10,854,032 bytes),
native-activity and preparation-owner self-tests, and all eleven graph scenarios:
cold/warm, transitive invalidation, failed/killed producer resume, interrupted
summary, invalid declarations, native warm reuse, persistent duplicate rejection,
owner/graph commit-gap recovery, retained claims after failure, and corrupt native
journal rejection. The same invocation passed TL0009/TL0010, Rust size and
documentation gates, Rust formatting, and whitespace checks. These are bounded
fixtures, not the clean-candidate end-to-end release rehearsal.

The expanded failure-accounting run is recorded in
`target/v9-failure-effects-final.log`: the actual rebuilt promotion image
(11,890,104 bytes),
native-activity and all 19 preparation-owner test functions, all 15 graph
scenarios, TL0009/TL0010, Rust size/documentation/formatting, and whitespace
checks passed. Four added scenarios cover work before owner failure, failed
owner/graph commit gaps, corrupt failed evidence, and interrupted native scratch.
These fixtures use synthetic native records and explicit interruption residue;
they are not evidence of full-process SIGKILL recovery or a clean release run.

The same work exposed two compiler defects beyond union identity: a nested
Result join crossed an outer Boolean gate, and yield-prefix liveness pruning
discarded effects whose values were unused. Joins now stay within their lexical
branch, prefix expressions execute in source order, and only continuation
captures are pruned. `target/v9-join-scope-green.log` and
`target/v9-yield-effects.log` record the source-to-object/composition regressions,
strict library Clippy, and rebuilt binaries.

Script assertion lowering also copied the entire remaining script into its
terminal failure branch, causing exponential growth (15 sequential assertions
were rejected at 65,535 estimated continuation records). Assertions now use a
Unit-valued check followed by one shared tail. Ten parser tests pass, including
linear-size coverage at 32 assertions. The final run executes all four nested
branch forms with side-effect counts in a 96,248-byte image and verifies that
a failing assertion exits with `native_failure:1` without performing later I/O.
The earlier linear-assertion probe was insufficient: the negative execution test
exposed the discarded-effect bug, which had to be corrected before accepting
the positive probe. Its erroneous output is preserved as before-fix evidence.

Building this integration reproduced missing expected collection types across
suspending struct constructor arguments. Constructor specialization now resolves
both local and qualified identities before lifting arguments into continuation
locals, while rejecting unrelated constructor names. The minimal real AOT
probe passes (`target/v9-local-constructor.log`, 21,512-byte image). The tracked
source-level regression also lowers and emits a struct whose later argument
calls `Process.yield_now()`; it and strict library Clippy passed in
`target/v9-cycle-source-test.log`.

The graph integration also exposed two connected union-lowering defects:
mixing a checked Result with a positional constructor invented duplicate
payload variants, and suspension splitting captured literal tuple tags in
locals that no longer identified a constructor. Type recovery now retains a
covering checked union, and eager suspension splitting preserves literal atoms
without moving evaluated expressions. Both branch orders have source-to-object
coverage. An independently reproduced inline-`case` assertion admission defect
is also corrected: its real script now executes successfully with a 21,864-byte
image (`target/v9-inline-case-assert-fixed.log`).
`target/v9-case-tag-validation.log` records 16 source-to-object, 19 call
composition, and 8 yield-region tests, strict library Clippy, and compiler/VM
binary builds. The earlier six control-type tests passed in
`target/v9-result-labels-corrected.log`. The graph acceptance evidence is separate
from these compiler regressions, as recorded above.

`prepare-local-reports` retains the focused two-owner graph. Its summaries count
completed and reused owners, not nested process launches. Duplicate declaration
rejection is not yet a complete Cargo/rustc/linker/AOT subprocess inventory.
The shared context verifies clean source and runtime-tool identity once; the
Rust owner adds its compiler/sysroot/native-tool identity without repeating the
runtime-tool probe. The preceding multicore gates still execute separately.

Publication staging now uses `prepare-release-reports` under one preparation
lock. This graph declares managed-list, multicore and AOT report owners together,
with an explicit managed-list predecessor for AOT. Source/runtime preflight,
VM version inspection and the Rust/build-tool identity are shared across these
declarations. Standalone entry points retain their own checked admission; this
does not eliminate all tool probes inside the report composers themselves.
Parent output/native-log/ledger paths already present in explicit dependencies
are subtracted before adding graph edges, avoiding duplicate input hashing.
The AOT owner therefore binds the upstream successful receipt as well as output
bytes, while unrelated multicore evidence can remain reusable.

The updated real Make/live-Cargo coverage fixture passes in 16.34 seconds
(`target/v9-report-make-fixture.log`), preserving its original Rust test-body
counts and rejecting report-stage failure before restore/matrix/consumers.
Report producers and archive services are instrumented in that Make fixture;
`release-preparation-release-reports-check` separately exercises the compiled
three-owner graph with real Cargo and real multicore/AOT composers. Its upstream
platform/sanitizer documents are admission fixtures, not certified hosted runs.
This graph does not yet own all preparation prerequisites or nested launches.

The actual three-owner rehearsal passed in 24.566 seconds
(`target/v9-report-graph-actual.log`). It verifies cold completion, zero-producer
warm reuse, exact upstream receipt/log binding, selective AOT failure/recovery,
and recovery after a real killed Cargo test. Original report bytes are preserved.
The rebuilt validator also passes all 16 generic graph scenarios and the focused
two-owner and standalone AOT rehearsals. That graph acceptance image measured
14,069,648 bytes; these are
scoped acceptance results, not an end-to-end release-cycle timing.

Closeout exposed an obsolete dry-run command matcher which silently counted zero
isolated owners after the graph command was renamed. The refresh-plan check now
requires exactly one `prepare-release-reports` entry. Its real Make recipe passes
seven synthetic-plan cases covering healthy accounting, missing/duplicate graph
owners, duplicate Cargo builds, exceeded invocation/selector budgets, and an
unowned contract self-test (`target/v9-report-plan-test.log`). Actual plans report
four explicit Cargo invocations, one isolated report owner and zero duplicate
builds; publication verification plans zero build/test replay
(`target/v9-report-plans-final.log`). These remain plan counts, not complete
nested-process observations. Strict orchestrator Clippy, fresh Rust structural
and headroom checks, repository build/release contracts, Rust quality/docs and
formatting pass (`target/v9-report-plan-clippy.log`,
`target/v9-report-final-closeout.log`). No count budget or size limit was raised.

The ordinary promotion self-test covers graph cold/warm runs, transitive
invalidation with byte-identical output, failed/killed producers, retained prior
evidence, interrupted summary recovery, and invalid declarations. Run the
production graph's real Cargo/composer fixture rehearsal with:

```sh
make release-preparation-local-reports-check
```

That fixture uses constructed hosted/sanitizer input documents to test ownership
and admission. It does not certify hosted workflows or sanitizer execution.

## AOT report composition

`publish-evidence-refresh` runs `tvm-aot-release-prerequisites`, then invokes
`prepare-aot-release-record` under the preparation flock. Only the composer is
checkpointed: a warm report does not skip prerequisite correctness checks.
Ordinary `tvm-aot-release-closeout-check` retains both prerequisites and recording;
read-only publication verification remains unchanged.

The composer declares the VM/image, candidate sources, hosted attempt document,
managed-list and host-platform reports, and optional AOT sanitizer report.
Its tool identity includes the selected Rust toolchain because the report records
Rust/Cargo identity. `TERLAN_AOT_RELEASE_OUTPUT` directs it into owner-local staging;
normal development recording keeps the established report path. Recovery tests
run the actual compiled composer against disposable admission documents:

```sh
make release-preparation-aot-check
```

This fixture does not execute or certify the platform/sanitizer suites described
by its constructed inputs. Full candidate recovery remains an independent
acceptance requirement.

The compiled composer rehearsal passed on 2026-09-07: cold execution, warm reuse
without a composer launch, optional sanitizer appearance/removal, changed managed
report bytes, rejected platform revision, and recovery to byte-identical output.
The ordinary promotion self-test now passes 16 owner recovery cases, including
optional-input creation/removal during execution, and six graph scenarios with
optional-output dependency validation. The AOT closeout contract, repository
build/release contract, six single-root tests, package grouped-binding/function-reference
lint, and formatting checks also pass. Make plans retain zero publication build/test replay and four
refresh Cargo invocations with no equivalent duplicate builds.

The promotion test/tool image is 8,418,736 bytes (previously 7,560,088), including
the additional owner, command routing, optional-input handling, and recovery
fixtures. This measurement is not a runtime application size baseline; splitting
validation/test families remains V9-2 work.

An extra `cargo clippy --lib --tests` diagnostic run failed with 609 errors,
including test helpers inheriting production-only panic/unwrap restrictions and
existing formatting/style debt. The canonical Make Clippy owner currently checks
workspace binaries, not test targets. Three diagnostic spans added by the process
test were corrected without allowances and that test passes; the broader test
target has not been recertified. Passing production/structural gates must not be
reported as passing test-target Clippy.

## Execution and cleanup

Execution uses `env -i` with the declared variables, closed stdin, bounded output
capture, and a timeout. The output variable points into `<owner>.work`, not at
the final report. The owner checks the staged schema and rehashes declared file
inputs before replacing the final report. A failed producer preserves prior
evidence. Scratch is cleaned before the successful ledger is committed. Atomic
same-filesystem replacement prevents partially written ledger publication;
interrupted pending files are never treated as completed evidence.

The shared linked-object harness now owns its scratch directories through a
scope guard. Exclusive creation never adopts or deletes an existing directory;
normal completion reports cleanup errors, and unwinding removes partial objects
and harness builds. Three filesystem regressions and the 436-test NativeIR suite
pass with this guard. Forced process termination still requires the outer
preparation owner; scope cleanup is not a claim of signal-wide recovery.

Current bounds are 30 minutes per producer, 8 MiB captured process output,
16 MiB per parsed owner JSON document, 16,384 explicitly inventoried files, and
128 bytes per candidate/owner path component. Hosted-download retention is
described below; other caches and cancellation cleanup across the entire
preparation graph remain V9-1 work.

### Isolated Lean replay

Runner configuration validation writes `lean-proof-runtime-policy.json`, never
`lean-proof-gate.json`. The former reports configured policy only: configured
memory limits are not measured peak RSS, and it does not invent I/O timings or
claim successful process cleanup. The latter remains proof-result evidence.

The Rust proof-track runner gives each toolchain probe and each independent
proof execution an exclusively created workspace beneath
`target/quality/proof-artifacts/workspaces`. It copies the local Lean sources,
Lake configuration, pinned toolchain file, and declared manifest/dependency
inputs; generated Lake state never shares the source checkout's `.lake`.
Replay no longer deletes shared Lake or legacy family build directories.
Snapshots reject symlinks and escaping paths and bound inputs to 4,096 files
and 64 MiB. Copied declared inputs are rehashed before execution.

The shared process owner closes stdin, limits combined output to 16 MiB, and
enforces a 30-second toolchain probe and a ten-minute proof deadline. On Linux
and macOS, timeout handling tears down the owned process group and bounds pipe
lifetime before workspace cleanup. Ordinary success, failure, and unwind clean
only the exclusively reserved workspace. Exact exit codes and output classes
must match before another replica runs; a signal is never an expected proof
rejection. Two matching, independently executed replicas remain required.

Proof processes clear the inherited environment. The runner captures an
absolute-entry PATH and installed Elan home once, resolves absolute tool paths
without cwd search, and explicitly pins `ELAN_TOOLCHAIN`. Home and temporary
directories are private to each workspace; locale and timezone are fixed.
Windows additionally declares its system root. Undeclared Lean/Lake flags,
loader overrides, and user-home configuration do not enter the child environment.
Elan proxy filenames are preserved for its argv-based tool dispatch. The Lean
version banner must match the complete version, not a prefix.

Every current family's metadata is validated before any toolchain probe, and
all distinct toolchain contracts are checked before the first proof replica.
Missing or stale later metadata therefore does not replay earlier proofs.
Workspace roots are absolute even when the quality command is invoked from
`.`, so absolute paths printed by tools normalize consistently across replicas.

Verification in `target/v9-proof-environment-final.log` passes 35 focused
runner tests, strict library Clippy, formatting, and Rust size/documentation
gates. All 13 proof families reproduce with deliberately incorrect ambient
`ELAN_TOOLCHAIN`, `LEAN_PATH`, and `LAKE_HOME`; the resulting report matches
the preceding validated report byte-for-byte. Source inventories remain
unchanged and private workspaces are cleaned.

The real Linux rehearsal in `target/v9-proof-isolation-real.log` passed all
13 current proof families, with matching replica signatures and unchanged
source-file inventories. Four stale replay metadata records and the parser
artifact digest were updated only after that isolated proof run passed.
Focused regressions cover copied-input drift, shared-state preservation,
symlinks, input limits, unwind, failed processes, closed stdin, timeout, exact
exit handling, and replica disagreement.

Final verification in `target/v9-proof-policy-close.log` reran the real proof
gate with refreshed metadata, then ran runtime-policy validation and verified
that the proof report's SHA-256 was unchanged. Source inventories matched and
private workspaces were empty after completion. The 99-test proof-focused suite
passed in `target/v9-proof-isolation-close.log`; the subsequent five-test runtime
policy suite, strict library Clippy, formatting, Rust size, and Rust documentation
gates passed. The disposable copied repository was removed after retaining its
proof reports in `target/v9-proof-isolation-{repro,gate}.json`.

### Linux proof-replica checkpoints

Linux replay now retains each independent successful replica under
`target/quality/proof-replay-cache/v1`. Its key binds copied source bytes,
replay metadata and expected exit, the running quality executable, the pinned
Lean installation, resolved GNU-loader libraries, loader configuration, and the
declared process environment. Native Lake is resolved through the explicit Elan
pin before execution. Tool inventory is bounded to 32,768 entries and 8 GiB.
The full tool byte digest is shared across families within an invocation;
file-identity checks guard mutation, with retained byte checks for recent files
that can change within a filesystem timestamp tick.

A stable lease serializes receipt consumption, publication, and retirement.
Replica one and two have distinct receipts. Failed executions are not committed;
fully written pending receipts recover across an interrupted rename. Partial
pending writes are discarded under the lease; corrupt completed receipts fail
with an attributed error rather than silently passing. Current input keys are
pinned before retention: unreferenced receipts have a seven-day age limit,
256-entry/64-MiB aggregate limits, and a 16-MiB per-receipt bound.

The 51 focused tests in `target/v9-proof-checkpoints-final.log` cover independent
reuse, first-success/second-failure recovery, pending publication, corruption,
foreign identities, symlinks, lease contention, age retention, changed tools,
timestamp races, and a real SIGKILL after the first receipt is committed.
The initial real Lean rehearsal produced 26 cold completions and then zero new
completions with 26 verified reuses; report bytes and source inventories matched.
Its 74-second cold and 52-second warm times exposed unoptimized debug SHA-2
compression. Developer/test profiles now optimize that dependency without
changing compiler optimization, debug line tables, or portable CPU selection.
The revised real runs in `target/v9-proof-fast-{cold,warm}.log` took 29.9 and
8.3 seconds respectively, preserving report bytes and the 26/0 execution counts.
These are local diagnostic timings, not a quiet-host requirement.

The actual Lean interruption rehearsal stopped its container with SIGKILL after
the first receipt committed. Resume completed 25 remaining replicas and reused
the committed one without changing its bytes; the final report matched the
uninterrupted run. Evidence is retained in `target/v9-proof-real-resume.log` and
`target/v9-proof-resume-{repro,gate}.json`. One abandoned private workspace
remained after the forced termination; it was removed only with the disposable
copied repository after the container was confirmed dead. This is not automatic
production orphan recovery. The final nine-test checkpoint suite also verifies
strict canonical receipt names; its Clippy, formatting and Rust quality/docs
checks pass in `target/v9-proof-checkpoint-close.log`.
The latest strict-name executable also passes the real cold/warm run in
`target/v9-proof-final-owners.log`: 26.1/8.2 seconds, 26/0 newly completed
replicas, identical proof reports, unchanged source inventories, and no remaining
normal-exit workspaces.

This does not complete preparation-wide ownership. Other hosts still use
uncached independent proof execution. Copied whole-project Lean inputs remain
conservative for invalidation; actual nested-process inventory and full-candidate
recovery remain open. SIGKILL workspace residue is not adopted or deleted without
a surviving-child ownership check. Generated Lake output has no separate disk
budget yet, and tool guards assume ordinary trusted local Linux filesystem clocks.

### Proof graph and current-attempt handoff (2026-09-10)

The proof-track, release-proof, feature-binding, and parser aggregates share
ordinary Make prerequisites. Runtime policy follows semantic kernels;
reproducibility follows runtime policy; smoke follows reproducibility and native
boundary binding; lanes follow smoke, PR policy and regression checks. Feature
binding follows completed lanes. Parser consumers share the grammar contract.
Each leaf declares its own bootstrap, so aggregate argument order does not
determine whether its executable exists.

`crates/terlan-test-orchestrator/tests/proof_make.rs` exercises the actual Make
dependency declarations with bounded fixture producers under `-j8`, in both
aggregate orders. All 15 producers execute once; injected reproducibility failure
prevents downstream completion. This validates scheduling, not execution of the
real proof bodies. The real bodies were then exercised separately: runtime
policy (5 Rust tests), proof track (100), native security (8), PR policy (5),
regression (4), native binding (5 Terlan tests), smoke (5), lanes (14), feature
binding (4), grammar contract (5), snapshot (4), and selected parser proof (1).
Logs are `target/v9-proof-{graph-real,replay-resume,consumers,consumers-closeout,
snapshot-parser}.log`. These are incremental verification runs, not a single
fully checkpointed preparation cycle.

A failed smoke attempt previously left an older passing report which a later
consumer could accept. Smoke now writes an attempt receipt before work, then
binds successful completion to the exact report SHA-256. Both Terlan lane
composition and Rust proof closeout reject missing, running, malformed or
mismatched receipts. An actual failed smoke attempt was followed by a rejected
lane invocation (`target/v9-proof-attempt-{failure,consumer}.log`); the focused
Rust closeout suite passes 10 tests. Successful sequential recovery passes all
13 proof families, 8 hard lanes and 9 baseline classes. A receipt does not yet
provide preparation-wide concurrency exclusion or cross-cycle freshness.

Semantic-kernel Rust reuse now calls the immutable orchestrator's authenticated
live coverage endpoint with the exact Cargo request. The legacy skip flag only
selects this route; it cannot certify success. Its script-root environment value
is classified as routing only when it equals the request checkout. The real
Make/Cargo coverage fixture accepts the matching root and rejects a different
root without replaying its 15 bodies. Actual compiled Terlan execution rejects
the skip flag without a live owner (`target/v9-semantic-coverage-rejection.log`).
Native-boundary and smoke consumers now use the same exact live coverage
protocol when an owner is present. They leave the inherited test environment
unchanged on coverage requests, require successful execution of the fixed
immutable orchestrator, and label the report's runtime oracle owner. Standalone
calls still run bounded Cargo tests and require one passing, non-ignored result;
coverage success is never presented as fabricated libtest output.
`TERLAN_LEAN_PROOF_ROOT` is classified as routing only for the exact checkout,
like the semantic-kernel root. Three environment unit tests and the real
Make/Cargo fixture pass, including matching/mismatched proof roots without
replaying its 15 bodies (`target/v9-proof-oracle-{environment,make}.log`).
An initial filename-based Rust filter selected zero tests; the recorded three
tests use the actual module filter, not that empty run.
Both compiled Terlan result-contract tests pass. Actual standalone native
binding passes 22 theorems, 24 manifests, 184 rows and 4 runtime oracles; a
separate deliberate skip-flag-only invocation fails. Actual smoke and all 14
lane tests pass after refreshing current reports. Replay reuses all 26 Lean
replicas with no new proof execution. See
`target/v9-proof-oracle-{terlan,native,native-rejection,replay,smoke,lanes}.log`.
The native script's reviewed input fingerprint changes only slice 14's trace in
the subsequent baseline proposal. The explicit Linux acceptance gate
`make release-preparation-aot-coverage-check` now compiles probes retaining the
three production request implementations (native boundary, smoke and semantic
kernels), then invokes their actual AOT images through the VM against a real
completed tiny Rust suite. All three accept exact covered selections, reject
missing selections and changed environments, and preserve the 15-body execution
count. The complete fixture passed in 23.66 seconds
(`target/v9-proof-aot-acceptance.log`). Its checkpoint models prior hosted
authentication; signature verification remains the downloader's separately
tested responsibility. Ordinary Rust-only fixture runs do not claim this AOT
acceptance tier; the named gate requires prebuilt tools via its bootstrap.
Standalone multi-script oracle ownership and the full clean-candidate rehearsal
remain distinct from this live-owner integration.

The AOT acceptance investigation reproduced rejected native lowering for a
structured `case` inside a list argument beneath a call-result field projection.
The eager-operand normalizer now introduces lexical owners in source order,
including scalar siblings whose effects must precede a later case. Nested source
bindings keep their scopes. Control-valued conditions get a join before branch
selection; later conditions remain in the fallback rather than becoming eager.
This uses existing native lexical/continuation lowering, not relaxed admission
or a source rewrite requiring users to add temporary bindings. Ordinary eager
expressions are inspected without allocating an operand list.
The maintained `crates/terlan/tests/fixtures/eager_case.terls` regression builds
through the actual compiler. Actual VM execution prints `first`, `ok`, `last`
in that order for both absent and supplied arguments; a wrong value prints
`first`, `wrong`, `last` and exits with the assertion failure. The focused Rust
selection passes seven tests (five new regressions and two existing matching
tests). The subsequent NativeIR run excludes that already-executed selection
and passes 454 additional tests (`target/v9-eager-case-{regression,native-ir}.log`).
Fresh structural, headroom, build/release contract, Rust-quality and Rustdoc
checks pass (`target/v9-eager-case-quality.log`).
Strict library Clippy, all-target orchestrator Clippy and workspace Rustfmt also
pass. After rebuilding the final compiler, the full three-caller acceptance
passes again (`target/v9-proof-aot-acceptance-final.log`). Publication plans still
require zero build/test replay on retry; refresh remains four Cargo builds,
one isolated performance owner and zero equivalent duplicate builds
(`target/v9-eager-case-plans.log`). These are focused acceptance and plan results,
not a measurement of the complete cold/warm/interrupted release cycle.

The release-proof baseline was reviewed against its generated proposal, not
silently recorded by preparation. Exactly four slice traces changed: slices 4
and 15 include the typed-lambda grammar fingerprint, slice 13 includes that
grammar and parser-output fingerprint, and slice 14 includes the extracted
dispatch-error source and native-binding consumer. No slice, theorem list,
coverage class, or lane was removed. Candidate-material changes also include
the process/cache crates, resolved tooling dependencies, isolated generated
output paths and live semantic coverage consumer. The rebuilt evidence
validator's adversarial self-test passes, and the accepted reviewed baseline
passes all 14 records (`target/v9-proof-evidence-reviewed.log`). Its accepted
baseline is a deliberate source edit; ordinary checking and proposal generation
continue writing only under `target/quality/proof-artifacts`.

### Shared proof entry points

Parser shape, feature cull, native boundary, semantic smoke, and semantic-kernel
consumers now call `terlan-quality lean-proof-replay` with exact current proof
paths. The command selects only admitted successful artifacts, validates current
inventory/source/metadata, and uses the same replicas as the complete track. It
rejects empty, duplicate, unknown, non-current, negative-exit, and noncanonical
requests before execution. A selected run never publishes a partial track report.
Smoke batches its two distinct proofs once even though three rows consume them;
semantic kernels batch all selected families under one tool preflight. Their
distinct runtime oracles remain required. Make declares the quality executable
prerequisite for these consumers.

The old inventory-only `proof-repro-report` test was removed: printing a passing
label for a `current` row was not independent execution evidence. The replay
owner continues to generate the actual verdict reports. The semantic-kernel
Postgres selector was also corrected to its actual Rust harness path. These
Rust oracle consumers now require exactly one passing, non-ignored test in
addition to process success; zero matching tests no longer pass.

`target/v9-proof-selection-build.log` records 100 focused Rust proof tests and
strict library Clippy. `target/v9-proof-selection-real.log` records the complete
26-replica run, six verified reuses for three selected proofs, parser/feature-cull
execution through the Terlan caller, smoke batching, and a templates/routes
kernel check. `target/v9-proof-oracle-validation.log` records typed zero-test
guards, the corrected Rust test, nine release-resume contracts, the kernel
self-test, and twelve verified reuses across six selected families. Complete
proof reports remain byte-identical after the selected consumers. These are
focused checks, not a rerun of every native-boundary/runtime oracle or completion
of global Cargo/test-tier ownership. Other platforms still replay uncached.

The final metadata-bound run reused 24 replicas and rebuilt only the two whose
native-boundary validator fingerprint changed. A following native-boundary
selection reused both. Rust size/docs and Rust/Terlan formatting checks passed.
Strict Terlan lint is **not clean**: comparison with the original four scripts
found 46 diagnostics before and 41 after, with no new diagnostic/function pairs.
The remaining complexity/readability diagnostics are retained in
`target/v9-proof-lint-{baseline,current}.log`; they are not allowances or a passing
lint gate. `target/v9-proof-final-readability.log` records the final typed caller
and semantic-kernel self-test after the local readability refactor.

### Rust producer process ownership

The compiler-independent `terlan-process-owner` crate now owns the child-lifetime
primitive shared by VM capabilities and the Rust test orchestrator. It replaces
the orchestrator's direct-child-only kill and unbounded stdout-reader join.
Linux/macOS retain the group leader until group termination/reaping, including
unwinding cleanup. Callers cannot replace or independently reap the owned child.

The orchestrator closes stdin, streams test output, and caps captured Cargo stdout
at 16 MiB. Process execution and stdout drainage use one deadline. Its captured
reader is nonblocking and interruptible on Linux/macOS, so an escaped session
holding the pipe cannot extend that deadline. Failures remain attributed as
launch, timeout, output-limit, wait, or exit failures instead of hanging capture.

Evidence: `target/v9-shared-process-tests-final.log` contains ten process-owner
tests and fifteen orchestrator tests plus strict package/all-target Clippy.
`target/v9-shared-process-vm.log` contains nineteen VM process tests, strict
library Clippy, and passing Rust size/docs checks. Its final graph check used stale
precomputed Cargo metadata and failed; fresh AST/Cargo inputs and the subsequent
graph/canonical-type check pass in `target/v9-process-graph-check.log`.
`target/v9-shared-process-consumers.log` contains eleven CLI/proof process tests.
The additional exact test in `target/v9-process-real-cargo-proof.log` compiles a
private Cargo fixture, selects its actual emitted harness, executes its test,
and removes the fixture only after both producers finish. The earlier unqualified
exact selector matched zero tests and is not counted as evidence.

This is not complete process-tree containment: descendants that establish their
own group/session can outlive the group owner. Other platforms still own only
the direct child, and a foreign pipe holder can leave a detached reader until it
exits; Windows job/handle ownership remains required. VM parent-process signal
handling and SIGKILL recovery also remain open. Full suite/evidence resume ownership and actual
nested launch accounting are not established by these focused process tests.

### Observed Rust launches and compiled test selections

The orchestrator's `terlan.rust-test-suite.v4` report records attempted phases,
confirmed child PIDs, running snapshots, and terminal outcomes. Its
`direct_cargo_launch_count` and `direct_process_launch_count` count successful OS
spawns, including children that subsequently fail. A failed spawn counts zero.
`launch_accounting_scope` is explicitly `orchestrator-direct-children`, not all
nested Cargo/rustc/native/test launches. The two-Cargo limit is enforced before a
third producer can start; duplicate phase owners fail before another launch.

One report writer holds a stable file lease. Snapshots use exclusive bounded
staging, file sync, replacement, and Unix directory sync. An interrupted pending
snapshot is retained in one bounded diagnostic slot, never promoted to passing
evidence. New runs have new IDs; these observations are **not** input-bound
successful-test receipts. Recording failure terminates the observed child and
poisons the run. The lease explicitly unlocks on drop, including unsuccessful
initialization, so a concurrent fork-before-exec descriptor cannot accidentally
retain ownership after the writer closes its descriptor.

Before executing any test phase, two distinct bounded `--list --format terse`
queries inspect the already compiled harness: all tests and ignored tests. They
are inventoried subprocesses, not additional Cargo builds or repeated tests.
Selections must be nonempty and nonoverlapping; every normal compiled test must
have one phase owner. Compiled ignored tests must have one declared tier owner,
and locally selected ignored tests must agree with that owner and tier. External
ignored tests are accounted for but not run by this preflight. The normal-library
coverage handoff still relies on the existing environment policy, not a verified
coverage receipt. Workspace-support Cargo harnesses and actual completion counts
still need equivalent evidence ownership.

Evidence: `target/v9-launch-ledger-tests.log` contains twelve shared process-owner
tests. `target/v9-compiled-test-inventory-close.log` contains the final thirty-six
orchestrator tests, strict process-owner/orchestrator all-target Clippy, and the
production orchestrator build. Tests include actual SIGKILL between a reaped
producer and its terminal snapshot, duplicate-descriptor lease release, failed
observation cleanup, and a real privately compiled libtest harness whose test
bodies panic if inventory discovery accidentally executes them. The initial
inventory test run exposed a wrong substring expectation and the lease lifetime
race; both were corrected before the passing run.
Fresh AST/Cargo inputs pass build-graph and canonical-type validation in
`target/v9-inventory-quality.log`. That command chain then stopped on an invalid
local command name (`rust-size`); the corrected remaining checks pass in
`target/v9-inventory-quality-close.log`: zero oversized/inline-test files, zero
undocumented items, workspace Rust formatting, and `git diff --check`.

Production entry-point probes in `target/v9-launch-production-probes.log` and
`target/v9-launch-probes-Ik7mZq/*.json` reject a missing Cargo executable (zero
launches), `/bin/false` (one failed launch), and `/bin/true` (one launch but no Cargo
artifact). They did not run or claim the full correctness suite. Nested-process
containment, immutable harness identity through execution, successful phase
receipts, and preparation-wide once-only ownership remain open.

### Direct-child escape and ordinary runner cancellation

Shared cleanup now terminates the retained direct PID as well as the original
process group. If the child moved into a foreign group, the original group can
disappear; previously cleanup could then wait for the surviving child to exit
naturally. The owner never follows the child by signalling its new group. A
real-process regression exercises explicit finish, unwinding, timeout, and failed
launch observation after a child joins a live peer group, and verifies that the
child is reaped while the peer survives.

`ProcessControl` adds an explicit caller-owned cancellation flag to the same
execution/drainage deadline. It rejects cancellation before spawning and cleans
up cancellation after spawning. The library installs no global signal handlers.
The standalone orchestrator owns its Unix INT/TERM/HUP registrations through
`signal-hook` 0.3.18, already present in the dependency lock. Signal handlers only
set a flag; ordinary code performs cleanup and persists the cancelled phase.
Cancellation observed at closeout also prevents sealing a passing run.

`target/v9-signal-owner-tests.log` contains fifteen process-owner and thirty-eight
orchestrator tests, strict all-target package Clippy, and the updated orchestrator
build. Its signal tests run isolated child fixtures; the test harness itself does
not install global handlers. `target/v9-signal-production.log` separately records
INT, TERM, and HUP delivered to the actual production executable during its Cargo
phase. All three exit with failure, retain a `cancelled` phase with one actual
spawn, and leave the producer reaped. Their reports remain under
`target/v9-signal-main-Vx0hPS/`.
The final `target/v9-signal-consumers.log` passes thirty affected VM/CLI/proof
process tests, strict union-feature library Clippy, fresh AST/Cargo build-graph
and canonical-type checks, Rust size/docs, formatting, and diff checks. Together
with the package suites, this is 83 focused tests, not the full release suite.
The temporary production-probe executable was removed after all three owners
terminated; its reports and diagnostic log were retained.

This does not catch SIGKILL, contain descendants that create separate groups, or
implement VM shutdown/Windows console and job policy. It also does not turn the
existing Make `ALREADY_RUN` flags into verified receipts: current source, tools,
test selections, and execution outcomes must be bound before those skip paths
can safely consume reusable test evidence.

### Rust working-source binding

The orchestrator records a `source_binding` alongside its direct-launch report.
Admission and closeout each invoke bounded `git ls-files --cached --others
--exclude-standard -z`, then hash actual working-tree bytes rather than index
blobs or an unchecked revision string. Tracked absence, nonignored additions,
file modes, and in-tree symlink text and contents affect the digest. Ignored
generated storage is excluded. Git's two observations are distinct boundary
checks, not additional builds or correctness-test executions.

Names must be canonical relative UTF-8 paths; hashing is limited to 50,000 names
and 8 GiB of streamed contents. Escaping/dangling symlinks and special files fail
closed. Unix opens use nonblocking/no-follow flags and compare identity, size,
mode, mtime, and ctime around each read. Cancellation and the phase deadline also
cover source hashing. Closeout retains both snapshots and fails when they differ;
missing closeout or attempted replacement of admission cannot seal success.

`target/v9-source-binding-close.log` passes all 45 orchestrator tests, strict
process-owner/orchestrator all-target Clippy, and the production build. The
initial compile exposed SHA-2 0.11's changed digest formatting API; it was fixed
before the passing run. A subsequent portability adjustment uses the existing
Unix `mkfifo` utility for the special-file fixture and an explicitly writable
file handle when restoring fixture timestamps. Its seven affected source tests
pass in `target/v9-source-quality.log`.
That same final log passes fresh AST/Cargo build-graph and canonical-type
validation, Rust size/docs, workspace formatting, and diff checks.

`target/v9-source-production-admission.log` runs the actual executable against
the working tree and an intentionally missing Cargo program. It captures 11,330
source names and 55,996,804 bytes in about 0.35 s, records one actual Git launch
and zero Cargo launches, and does not claim successful tests. Those measured
counts describe that observed source revision, not an enforced inventory total.
`target/v9-source-rehearsal.log` exercises both unchanged and changed closeout
through the production driver with private Git/Cargo/harness stand-ins. Both
traverse the fifteen declared direct launches; unchanged source passes and
changed source fails with different retained digests. The reports are
`target/v9-source-rehearsal-{matching,changed}.json`. These stand-ins prove driver
fault handling, not execution of Terlan's real correctness suite.
The two private rehearsal repositories were removed after terminal completion;
their copied reports and diagnostic log were retained.

Boundary equality cannot detect changes made and restored between observations;
it is not an immutable source workspace. Git metadata, ignored runtime/tool
artifacts, external SDK/library dependencies, and the execution environment are
not covered by this source-only digest. Source/tool-bound successful phase
receipts and the replacement of `ALREADY_RUN` flags therefore remain open.

### Selected executable binding

The Rust driver now admits six selected files before any child launch: its Git
and Cargo entry points, prebuilt `terlc`, `terlan-vm`, and `terlan-native-worker`,
and the driver itself. Missing, empty, nonregular, or nonexecutable inputs fail
admission before an expensive Cargo build. The produced library harness is added
only after Cargo's observed artifact selection. The selected set has a sixteen-
entry and 8 GiB budget. Source and executable identities share one bounded,
cancellation-aware file reader rather than separate hashing implementations.

Each direct Git/Cargo/harness launch verifies its admitted file and uses the
verified absolute invocation path. That path is not replaced by its canonical
symlink target: multicall shims and scripts may dispatch on their invocation
name. Both invocation and resolved paths are recorded. Native worker paths remain
absolute and use the platform executable suffix. Windows lookup follows the
explicit-path and `.exe` rules inspected in the pinned Rust standard library;
this change does not constitute Windows execution evidence.

The report's `executable_binding` has the explicit scope
`selected-executable-bytes-v1`. Each `identity_sha256` binds a domain separator,
size, permissions, and file bytes; it is not a raw `sha256sum` file checksum.
Missing or changed closeout cannot seal success. Failed verification poisons the
ledger, and changed closeout retains both observations. A changed harness is
rejected before the next inventory/test process starts.

`target/v9-executable-binding-close.log` passes all 54 orchestrator tests, strict
all-target package Clippy, and the production build. It covers same-size changes
with restored timestamps, alias retargeting, permission changes, pre-launch
rejection, failed/missing/repeated closeout, and a real symlink-invoked script that
fails if its invocation name is canonicalized. The previous seven source tests
continue exercising the shared reader after extraction.

`target/v9-executable-production-admission.log` records a real-worktree admission
of approximately 650 MB of selected executable files, followed by the deliberate
`/bin/false` Cargo probe: about 0.98 s total, one Git and one Cargo child, and no
claim of successful correctness tests. The final production-driver stand-in
rehearsals in `target/v9-executable-rehearsal.log` cover unchanged success, missing
runtime admission with zero launches, a changed ignored runtime with unchanged
source but failed executable closeout, and a changed harness rejected before the
next phase (seven launches, no launch for the rejected phase). Copied reports are
`target/v9-executable-rehearsal-{matching,runtime,harness,missing}.json`.
`target/v9-executable-quality.log` passes fresh AST/Cargo build-graph and canonical
type checks, zero oversized/inline-test and undocumented items, workspace Rust
formatting, and diff checks. The four private rehearsal repositories were removed
after their producers terminated; copied reports and logs remain.

This binds selected files, not the compiler/toolchain selected behind a Rustup
shim, transitive shared libraries, SDKs, or package tools. Direct-child environment
binding is described below.
Named-file boundary checks also do not eliminate every replacement race or
provide immutable execution snapshots. These reports are not yet reusable phase
receipts and do not authorize an `ALREADY_RUN` skip or close V9-1.

### Frozen Rust execution environment

The Rust driver now captures the inherited OS environment and working directory
once, before executable admission or any child. Admission is bounded to 4,096
entries and 1 MiB of native string bytes. Git receives that explicit snapshot;
Cargo and both inventory/test harness launches receive the same snapshot with
the declared prebuilt-tool PATH. Individual tests receive only their declared
additional overrides. Every production command clears ambient inheritance and
sets its captured working directory. Cargo/Git resolution uses the same captured
PATH values as execution; invalid test-PATH construction fails before launch.

`environment_binding` records the `frozen-direct-child-environment-v1` scope and
domain-separated hashes of platform, working directory, and effective OS key/value
pairs, including each selected phase's overrides. It does not serialize variable
names or values. These hashes are not encryption or a promise that low-entropy
inputs cannot be guessed. Native non-UTF-8 values are preserved without lossy
conversion. Standard-library `Command` owns environment-key ordering and
Windows case-insensitive replacement; no custom case-folding table is introduced.
Repeated or late admission fails rather than relabeling prior child launches.

`target/v9-environment-close.log` passes all 62 orchestrator tests, strict
all-target package Clippy, and the production build. New tests exercise bounded
admission, redaction, field boundaries, working-directory identity, effective
override identity, non-UTF-8 PATH values, and real children with no inherited HOME
or CARGO_HOME. The Windows-specific key test is present but was not executed on
this Linux host.

`target/v9-environment-quality.log` passes fresh AST/Cargo graph and canonical
type validation, zero oversized/inline-test files and undocumented items,
workspace Rust formatting, and diff checks.

`target/v9-environment-production.log` runs the actual driver against the real
worktree twice with different synthetic environment values and a deliberately
failing Cargo producer. Both reports record one Git and one Cargo launch, fail
rather than claim test success, and retain distinct environment digests without
plaintext variables. Reports are `target/v9-environment-production-{one,two}.json`.
These are admission probes, not correctness-suite executions or reusable receipts.

This freezes inherited inputs, not a hermetic filesystem, dispatched toolchain,
transitive dynamic libraries, or environments subsequently created by nested
producers. Hashing all inherited values is conservative and may invalidate future
receipts for irrelevant changes. Successful phase receipts and replacement of
the existing skip flags remain separate unfinished work.

### Cargo and Rustup configuration binding

The Rust driver now admits configuration before any producer and verifies it at
closeout independently of Git-listed source and selected executable bytes. The
inventory includes both Cargo configuration filenames at each working-directory
ancestor and Cargo home, both Rustup toolchain filenames at each ancestor,
Rustup-home settings, and Unix system fallback settings plus their explicit
override. Missing files are observations too: creating configuration after
admission invalidates the run. Lookup follows the documented
[Cargo hierarchy](https://doc.rust-lang.org/cargo/reference/config.html) and
[Rustup configuration](https://rust-lang.github.io/rustup/configuration.html);
the Unix fallback override was checked against the
[Rustup implementation](https://github.com/rust-lang/rustup/blob/1.28.2/src/config.rs).

The compiler-independent runner pins `home` 0.5.12 and feeds its frozen
environment into that crate's Cargo/Rustup home resolution. This is a new direct
dependency, resolved from the local cache without upgrading existing packages;
it reuses the already selected Windows platform dependency. Home-directory
fallback is captured once, and no process-global environment mutation is needed
for fixtures. Empty or relative tool-home settings use the shared resolver.

`tool_configuration_binding` has scope
`cargo-rustup-configuration-files-v1`, not complete toolchain provenance. It
retains before/after identities for file presence, native pathname bytes,
symlink destinations, resolved paths, modes, and contents. Contents are never
serialized. Non-UTF-8 paths have a separate pathname digest and a null display
path. Admission permits at most 64 ancestor levels, 512 paths, and 16 MiB of
file contents, using the same cancellation/deadline-aware byte reader as other
inputs. Nonregular or dangling inputs fail; failure, repeated closeout, or
missing closeout cannot be sealed as success.

Verification on 2026-09-08:

- `target/v9-tool-configuration-close.log`: 69 package tests passed; strict
  Clippy then found a needless borrow in the new FIFO fixture.
- `target/v9-tool-configuration-final.log`: that correction passes the six
  configuration tests, strict all-target Clippy, and production build.
- `target/v9-tool-configuration-fallback.log`: the additional Unix fallback
  regression and affected configuration/ledger tests pass (eight tests), along
  with strict all-target Clippy and the updated production build.
- `target/v9-tool-configuration-quality.log`: fresh AST/Cargo graph and
  canonical-type checks, file size/test placement, documentation, formatting,
  and diff checks pass. The current Cargo metadata produced during offline
  dependency resolution was reused rather than generated again.
- `target/v9-tool-configuration-audit.log`: `cargo audit --no-fetch --deny
  warnings` passes against the cached database (1,226 advisories, 511 locked
  dependencies); this is not a newly fetched release-time advisory audit.
- `target/v9-configuration-rehearsal.log`: the production driver with private
  stand-in producers passes unchanged configuration, then fails a changed
  ignored Cargo-home file despite matching source/executable closeout. Each
  rehearsal observes 15 direct launches, including two Cargo stand-ins. These
  are owner-protocol fixtures, not actual Terlan correctness tests.

Rehearsal reports remain at `target/v9-configuration-rehearsal-{matching,changed}.json`.
The two terminal private Git fixtures (420 KiB) were removed after copying their
reports; source, caches, and retained evidence were not removed.

This does not parse configuration to resolve every compiler/wrapper, follow
referenced external packages, hash the dispatched
sysroot/SDK, or eliminate transient mutate-and-restore races. Both legacy and
modern alternatives are captured conservatively even when one is inactive.
Actual toolchain selection and successful reusable phase receipts remain open;
no `ALREADY_RUN` flag is justified by this report alone.

### Installed Rustup toolchain identity

The standalone Rust runner now admits Rustup alongside its selected executables.
After source/configuration admission it records three distinct owned probes:
`rustup which cargo`, `rustup which rustc`, and the resolved native
`rustc --version --verbose`. Rustup resolution sets `RUSTUP_AUTO_INSTALL=0`; a
missing installation cannot trigger an automatic installation during admission.
The shared process control caps these probes and tree reads at 30 seconds without
extending a tighter caller timeout or dropping cancellation.

Native Cargo and rustc must resolve into one installation's `bin` directory.
Selected Cargo must be that native Cargo or the admitted Rustup proxy; hard-linked
or copied proxies are recognized by admitted bytes and permissions, not merely
canonical pathname equality. Unrecognized Cargo wrappers fail admission. The
native compiler release must match the repository's compiled-in toolchain pin,
parsed with the already locked `basic-toml` dependency. The current installation
was observed as Rustup 1.29.0 and rustc 1.96.0; the Rustup version inspection was
diagnostic, not a new hardcoded version requirement.

`rust_toolchain_binding` explicitly has scope `rustup-active-bin-lib-v1`. Its
before/after snapshots hash the installed `bin` and `lib` trees, including
runtime libraries, target rustlibs, Rust sources, and component manifests.
Documentation outside those trees is not part of this scope. Entry names, modes,
file bytes, and internal file symlink destinations participate in identity.
Native Cargo/rustc entry-point observations are also retained independently.
Changed membership or contents, missing closeout, late admission, and repeated
closeout cannot seal a successful run.

Traversal is bounded to 65,536 entries, 8 GiB of streamed bytes, and 64 nested
levels. Directory enumeration checks its remaining budget while collecting
entries, before allocating an unbounded listing. Escaping links, directory
symlinks/cycles, special files, and changes detected during hashing fail. The
implementation shares the existing bounded file-identity reader; it does not add
a second compiler library dependency or relax lint allowances. Input-observation
failure handling is shared by the executable, configuration, and toolchain owners.

`target/v9-rust-toolchain-close.log` passes 16 shared process-owner tests, all 77
orchestrator tests, strict all-target Clippy for both crates, and the production
driver build. Coverage includes the noninstalling resolver environment, real
probe launches, hard-linked proxies, wrong/missing/duplicate release lines,
timestamp-restored toolchain changes, bounded traversal, missing/late closeout,
and tighter timeout/cancellation preservation.

`target/v9-rust-toolchain-rehearsal.log` exercises the production runner with the
actual installed Cargo/rustc and a private, dependency-free workspace. The
fixture compiled a 14-test library inventory and a support crate; it executed
nine selected library fixture tests and one support fixture test. Unused prebuilt
Terlan-runtime inputs were stand-ins, so this is not Terlan release-suite evidence.
The run passed in 3.58 seconds with matching 1,339,513,312-byte, 3,190-entry
toolchain observations. It recorded 18 direct launches, including exactly two
Cargo launches and three toolchain probes. Rustup path queries took 25 and 24 ms;
the native compiler version probe took 11 ms. The fixture needed no separate
Cargo build or lockfile-generation invocation before the driver ran.

`target/v9-rust-toolchain-rehearsal.json` retains all four successful input
closeouts and the actual launch observations. The terminal private fixture and
its compiled artifacts (16 MiB) were removed after copying the report.
`target/v9-rust-toolchain-quality.log` passes fresh AST/Cargo graph and canonical
type checks, zero oversized/inline-test files and undocumented items, workspace
formatting, and diff checks.

This does not yet prove that arbitrary Cargo `build.rustc`, `RUSTC`, compiler
wrappers, or configuration-provided subprocess environments dispatch only into
the observed Rustup tree. Those selections require additional binding before
phase reuse is allowed. External dynamic-loader libraries, SDKs, build-script
tools, package sources, and immutable execution protection also remain outside
this scope. Nested subprocess accounting remains separate from the report's
explicit direct-child count. No existing skip flag is authorized by this new
observation, and V9-1 remains open.

### Cargo configuration includes

The configuration observation now uses scope
`cargo-rustup-configuration-files-v2`. Its bounded reader retains exactly the
bytes it hashed, and the include walker parses that snapshot rather than opening
the file again. Active hierarchy roots prefer extensionless `config` over
`config.toml`; inactive alternatives remain conservatively byte-bound but are
not parsed. Included paths are resolved from the declaring pathname, including
when that file is a symlink. The report records the low-to-high-precedence load
order without serializing configuration contents.

Required missing files, malformed configuration, repeated/cyclic includes,
special files, and invalid include entries fail admission. Missing optional
files remain explicit absent inputs, so their appearance cannot pass closeout.
Includes share the configuration inventory's 512-path and 16 MiB byte budgets,
with an additional 512-visit/64-level traversal limit and the same cancellation
deadline. Diagnostic parser excerpts are suppressed because configuration can
contain credentials. Files outside the repository are included in these bounds.

The runner uses exact `toml` 1.1.2 (TOML 1.1 support) with parse/serde features,
also replacing its separate basic-TOML parser for the Rust pin. Offline lock
resolution adds the parser and four support packages without upgrading existing
locked packages. This follows
[Cargo's configuration/include rules](https://doc.rust-lang.org/cargo/reference/config.html#include)
and the
[pinned Cargo 1.96 implementation](https://github.com/rust-lang/cargo/blob/rust-1.96.0/src/cargo/util/context/mod.rs).

Validation on 2026-09-08:

- `target/v9-cargo-includes-close.log`: 85 orchestrator tests, strict all-target
  Clippy, and the production build pass before consolidating the pin parser.
- The additional `load_order_agrees_with_real_cargo_compiler_selection` test
  passes five actual Cargo probes: Cargo-home fallback, both include orders,
  file-over-include precedence, and extensionless configuration preference.
  Selected fixture compilers record their identity and deliberately exit 89
  during `-vV`, before compilation. These expected failures are not build errors
  or successful-test receipts. The test and strict Clippy passed in a private
  host target directory after an unrelated system update temporarily prevented
  container startup.
- After parser consolidation, all three affected Rust-toolchain tests and
  strict all-target Clippy pass. The compiled inventory contains 86 tests.
  The already-passing unchanged tests were not replayed.
- File size/test placement and documentation checks pass with zero violations.
  Temporary compatible loader libraries allowed those prebuilt checks to run
  without modifying system libraries or mixing host/container Cargo caches.
- `target/v9-cargo-includes-quality.log`: fresh AST/graph and canonical-type
  checks pass, reusing current Cargo metadata instead of producing it again.
  The cached security audit passes against 1,226 advisories and 516 locked
  dependencies; this is not a fresh release-time advisory fetch. Workspace
  formatting and diff checks pass.

This records configuration inclusion; the following section covers explicit
compiler/wrapper selections separately. No phase reuse or skip-flag replacement is enabled.
Boundary observations still do not detect a transient mutation restored before
closeout. V9-1 remains open.

### Explicit Cargo compiler and wrapper entry points

The same include-parser pass now projects only Cargo's `build.rustc`,
`build.rustdoc`, `build.rustc-wrapper`, `build.rustc-workspace-wrapper`, and `[env]`
settings. It retains source locations and Cargo's table-merging behavior instead
of flattening settings prematurely. Cargo-home aliases of an ancestor `.cargo`
directory are not parsed again. The selected configuration bytes remain the
source of this projection; there is no second file read or parser invocation.

The runner binds explicit file selections and both environment override forms
(`RUSTC`/`RUSTDOC`/wrapper variables and their `CARGO_BUILD_*` equivalents) before
starting producers. Direct tool variables take precedence over Cargo build
variables, which take precedence over files. Empty wrappers disable their layer;
empty compilers/rustdoc fail admission. Program strings are not shell-split.
Config-relative paths use the declaring file's grandparent; direct environment
paths use the captured Cargo working directory.

Cargo's `[env]` settings affect compiler subprocesses, not tool-selection
configuration. The projection preserves `force`, `relative`, inherited values,
and the origin of merged option tables, and rejects Cargo's forbidden tool-home
and toolchain variables. It shares the existing native-string, platform-aware
environment lookup and digest implementation. Values and parser excerpts are
not serialized. The environment table is bounded to 4,096 entries and 1 MiB of
resolved key/value bytes.

The live v4 suite report adds `cargo_tool_binding`, explicitly scoped as
`explicit-cargo-tool-entrypoints-v1`. Closeout compares executable bytes and
re-resolves the frozen effective PATH: inserting a different executable earlier
in PATH cannot pass merely because the previous file is unchanged. No explicit
override is still an admitted state requiring closeout. Repeated/late admission,
missing closeout, and changed tools poison the ledger's decision. These
observations add no Cargo producer launches and never replace the user's
compiler or wrappers.

Validation on 2026-09-08:

- `target/v9-cargo-tools-close.log`: all 97 orchestrator tests pass, including
  seven actual Cargo probes that compare observed wrapper order, selected
  compiler, and effective environment against the projection. Cases cover
  file/build-environment/direct-environment precedence, empty wrappers, forced
  PATH, non-UTF-8 overrides, and an included options table's origin. Fixture
  compilers intentionally exit 89 at `-vV`; no crate is compiled by these probes.
  The subsequent Clippy check found one redundant borrow in a test, which was
  corrected without adding an allowance.
- `target/v9-cargo-tools-final.log`: the additional Cargo-home-alias regression
  and affected ledger test pass, strict all-target Clippy passes, and the
  production runner builds. The compiled inventory now contains 98 tests.
  The unchanged passing suite was not replayed for a test-only borrow fix.
- `target/v9-cargo-tools-quality.log`: fresh AST/graph and canonical-type checks,
  zero oversized/inline-test files and undocumented items, workspace formatting,
  and diff checks pass. Current Cargo metadata was reused; dependencies did not
  change in this slice.
- `target/v9-cargo-tools-rehearsal.log` and the matching/changed JSON reports:
  the actual production runner and installed Rustup/Cargo/rustc execute a small
  isolated workspace through a transparent configured wrapper. The unchanged
  run passes in 5.187 s; a fixture test then changes only the ignored wrapper
  file, producing a failed closeout in 1.795 s despite matching source,
  executable, and configuration observations. Wrapper path resolution still
  matches, but its recorded before/after byte identities differ. Both runs
  retain two direct Cargo launches and 18 direct child launches.

The production rehearsal compiles fixture tests, not Terlan's actual release
tests; its unused prebuilt runtime inputs are stand-ins. Its reports are retained
at `target/v9-cargo-tools-rehearsal-{matching,changed}.json`. The terminal private
workspace and its compiled artifacts (17 MiB) were removed after report
verification and retention.

This is not proof of the compiler that an arbitrary wrapper ultimately invokes.
Rustup/default PATH dispatch, actual compiler/sysroot version admission behind
custom wrappers, interpreter/dynamic-loader dependencies, SDKs, and build-script
tools still require closure before successful reusable phase receipts can be
issued. No existing skip flag is authorized by this observation; V9-1 remains
open.

### Rustup proxy search and toolchain aliases

Explicit tool binding now distinguishes native Cargo from the admitted Rustup
proxy by executable byte identity. Proxy binding follows Rustup's PATH policy
before applying Cargo's `[env]` table: insert Cargo home's `bin` only when absent,
preserve an existing entry's position, and apply the Windows toolchain-bin
append/prepend/omit policy. Cargo's forced PATH still wins. This corrects a gap
in the earlier differential fixtures, which invoked native Cargo rather than
its Rustup proxy. See the
[Rustup toolchain implementation](https://github.com/rust-lang/rustup/blob/1.29.0/src/toolchain.rs)
and its
[unique PATH insertion](https://github.com/rust-lang/rustup/blob/1.29.0/src/env_var.rs).

The runner now admits configured tools after successful Rustup installation
admission but before Cargo producers. It retains the paths returned by Rustup
instead of canonicalizing away custom-toolchain aliases. Canonical identities
are still hashed, while the invocation paths are re-resolved at closeout.
Retargeting a toolchain symlink therefore cannot pass using the old installation.

`cargo_tool_binding.rustup_proxy_path` records which PATH policy was selected.
Its environment observation is explicitly scoped to frozen inputs plus Cargo
env settings and the selected proxy PATH. It is not a capture of every variable
Rustup or Cargo later synthesizes, such as loader-path, recursion-count, or
toolchain-source variables. Actual compiler/default dispatch, wrapped version
and sysroot admission, and transitive loader/tool closure remain required;
this correction does not authorize successful reusable phase receipts.

Validation on 2026-09-08:

- `target/v9-cargo-proxy-check.log`: 101 orchestrator tests and strict all-target
  Clippy pass. Five actual Cargo/Rustup probes distinguish native lookup,
  absent versus already-present Cargo-home PATH entries, and forced/non-forced
  configuration PATH. Fixture compilers deliberately exit before compilation.
  Windows policy ordering is exercised as pure policy on Linux, not presented
  as a native Windows execution result.
- `target/v9-cargo-proxy-final.log`: all four affected toolchain tests, including
  custom-alias retargeting, pass; strict Clippy and the production build pass.
  The compiled test inventory contains 102 tests.
- `target/v9-cargo-proxy-rehearsal.log`: the production runner executes a small
  real Cargo workspace with competing transparent wrappers in prebuilt tools
  and a private Cargo home. The observed wrapper trace contains only
  `cargo-home`; the report binds that exact wrapper and passes all input
  closeouts. The run takes 4.150 s with two Cargo and 18 direct child launches.
  Its fixture tests are not the Terlan release correctness suite.
- `target/v9-cargo-proxy-closeout.log`: final report-scope regression, strict
  Clippy, production build, fresh AST/build-graph/canonical-type checks,
  size/documentation gates, formatting, and diff checks pass. The 16 MiB private
  rehearsal directory was removed after retaining its report and wrapper trace.

### Cargo default compiler and rustdoc selection

Production `cargo_tool_binding` now uses scope
`selected-cargo-tool-entrypoints-v2`. Explicit settings keep their precedence;
missing compiler/rustdoc overrides are resolved using Cargo 1.96's actual
[default-tool dispatch](https://github.com/rust-lang/cargo/blob/rust-1.96.0/src/cargo/util/context/mod.rs)
and [PATH heuristic](https://github.com/rust-lang/cargo/blob/rust-1.96.0/crates/cargo-util/src/paths.rs).
That heuristic uses Cargo's own PATH before `[env]` overrides, compares file
lengths rather than contents, and can see a nonexecutable regular PATH file.
The selected executable is still required to be executable and byte-bound.
Fallback compiler lookup uses the effective subprocess PATH. Both selection
and selected bytes are verified again at closeout, including new PATH shadows,
removed default tools, and changed proxy lengths. Explicit and default tools
share one admission hash pass, rather than recapturing the explicit binding.

Rustup-proxy admission records one additional bounded, noninstalling
`rustup show active-toolchain` child. Its output must identify exactly one name
mapping to the already admitted invocation root. This preserves custom aliases
and distinguishes a named toolchain from an absolute-path selection of the same
installation, even for paths with spaces/parentheses. Only named selections
qualify for Cargo's native-tool shortcut. Native Cargo uses its frozen incoming
`RUSTUP_TOOLCHAIN`; the runner does not replace the compiler or user wrappers.

Validation on 2026-09-08:

- `target/v9-cargo-default-check.log`: 105 tests pass. One ledger fixture failed
  because it provided a wrapper but no default compiler/rustdoc; that fixture
  was updated to supply the newly required inputs, without weakening admission.
  Eight real native-Cargo differential probes cover the size heuristic,
  nonexecutable candidates, missing native tools, explicit overrides, and
  absent/path-based toolchain selection. They deliberately stop at the compiler
  version request with exit 89 before compiling a crate.
- `target/v9-cargo-default-final.log`: the repaired ledger regression and new
  real Rustup custom-link/path-selection regression pass. The latter checks
  both Cargo's actual wrapper argument and the recorded active-name probe.
  All 107 inventory tests have passed across these runs; strict all-target
  Clippy and the production build pass.
- `target/v9-cargo-default-quality.log`: fresh AST, build-graph and canonical
  types, source-size, documentation, formatting, and diff gates pass. Existing
  Cargo metadata is reused; this slice introduces no dependency change.
- `target/v9-cargo-default-rehearsal.log` and `.json`: the production driver
  runs a small real Cargo workspace in 3.690 s, with two Cargo and 19 observed
  direct-child launches. Source, executable, configuration, toolchain, and
  selected-tool closeouts pass. Both default roles are recorded, and the
  retained `target/v9-cargo-default-wrapper-trace.txt` contains only the selected
  installed native compiler. Fixture tests do not constitute the Terlan release
  suite. The private 16 MiB build directory was removed after retaining and
  comparing both evidence files.

Remaining scope at that checkpoint: observing the actual compiler version/sysroot through the
selected wrapper chain, wrapper-internal executables and loader/SDK inputs,
and reusable phase receipts. The native Rustup compiler version probe does not
yet establish the version of a custom compiler selected by Cargo. These scoped
entry-point observations must not replace the existing skip flags with receipt
reuse, or be presented as complete transitive build-input provenance.

### Selected wrapper-chain compiler and default-sysroot admission

The native-only Rustup compiler version probe has been replaced, not retained
alongside the new checks. After binding Cargo's selected tools, the production
runner queries `-vV` and `--print sysroot` through the workspace wrapper chain.
When a workspace wrapper exists, it also checks the distinct dependency chain
without that wrapper. It checks the pinned compiler release and requires a
single well-formed host field before any Cargo producer. Each query has a
recorded child PID, closed stdin, a 30-second maximum, bounded stdout, and owned
timeout/cancellation cleanup.

`selected_compiler_binding` has explicit scope
`cargo-wrapper-compiler-default-sysroots-v1`. It preserves Cargo's original bare
compiler/wrapper arguments. Its environment includes the observed Rustup active
source, resolved homes, recursion count, and loader-path policy before Cargo's
`[env]` application; Cargo's native executable overrides `[env] CARGO` last.
Real differential probes compare this with Cargo's version-query environment,
including forced and non-forced loader settings. The implementation follows
[Cargo's compiler query](https://github.com/rust-lang/cargo/blob/rust-1.96.0/src/cargo/util/rustc.rs)
and [Rustup's toolchain environment](https://github.com/rust-lang/rustup/blob/1.29.0/src/toolchain.rs).

That comparison exposed Cargo's implicit certificate-path initialization.
The runner now resolves certificate locations once during initial environment
capture using the existing `openssl-probe` 0.2.1 dependency. Existing valid
choices are preserved; discovered defaults are frozen for all direct children.
It does not mutate global environment variables or disable certificate checks.
The resulting environment is revalidated against entry/byte budgets. Cargo's
and Rustup's later initialization therefore preserves those declared valid
locations on the tested Linux host. Certificate contents and platform-specific
trust-store behavior remain outside this compiler/default-sysroot scope.

Reported sysroot aliases are checked again at closeout. A default sysroot
matching the admitted Rustup installation references its existing bin/lib
identity and must wait for that owner's verified closeout; it is not rehashed
by the compiler owner. Independently selected sysroots bind their `lib` tree
without requiring a compiler `bin` directory. Distinct wrapper contexts sharing
one sysroot use one tree identity. These checks do not assert closure of arbitrary
wrapper-internal tools, target/rustflag-specific sysroots, external SDKs/loader
libraries, or all per-crate compiler environments. Cargo-internal queries and
nested build accounting still need preparation-wide ownership before reusable
Rust phase receipts can replace skip flags.

Validation on 2026-09-08:

- `target/v9-selected-compiler-check.log`: 111 tests pass; the new full
  environment comparison exposed the implicit certificate variables described
  above. This failure was not ignored or treated as a release pass.
- `target/v9-selected-compiler-final.log`: both environment tests and the new
  bounded certificate-policy test pass after correction; strict all-target
  Clippy and production build pass. Cargo metadata was refreshed for the added
  direct dependency; no new package or version was added to the lockfile.
- `target/v9-selected-compiler-closeout.log`: selected-compiler lifecycle,
  wrapper ordering, wrong-version, changed-sysroot, shared-owner, timeout,
  output-validation, and version-header tests pass; strict Clippy and production
  build pass. The fresh structural gate then caught an unsorted module inventory.
  `target/v9-selected-compiler-quality.log` records the corrected inventory,
  canonical-type, size, documentation, formatting, and diff gates passing, plus
  the cached RustSec advisory audit (1,226 advisories, 516 packages). This audit
  did not refresh the advisory database and is not a publication-time audit.
- `target/v9-selected-environment-consumers.log`: five affected existing
  environment/inventory consumers pass using the already compiled test harness,
  without rebuilding the orchestrator; the inventory test builds its own tiny
  fixture. The compiled inventory is 113 tests; all have
  passing observations across the scoped runs.
- `target/v9-selected-compiler-rehearsal.log`: the actual production driver
  checks two transparent wrapper layers, builds and executes a small real
  Cargo workspace, and verifies all input owners in 3.306 s. The matching report
  records two Cargo and 22 direct-child launches, two compiler contexts, and
  one sysroot identity owned by the installed toolchain. Separate wrong-workspace
  and wrong-dependency compiler runs fail before any Cargo launch (five and
  seven direct children respectively). These are wiring fixtures, not the
  Terlan release correctness suite. Reports and the wrapper trace were copied
  and compared before removing their private 16 MiB workspace.

### Exact direct-harness test completion

An exit-zero harness is no longer sufficient for a passing direct Terlan phase.
The already captured normal/ignored inventories produce exact runnable and
ignored name sets for each selection. The runner compares these with libtest's
separate result file, rejecting missing, duplicate, failed, unexpected, or
wrongly ignored records. Nested processes can print arbitrary summaries without
impersonating the parent harness's completed-test records. This adds no Cargo
build, inventory query, or test execution to the nine direct Terlan phases.

Rust 1.96's `--logfile` is deprecated but remains its stable separate per-test
output channel; JSON/JUnit output requires unstable options. This deliberately
narrow dependency on the pinned harness is covered by real `rustc --test`
fixtures and must be reviewed on toolchain upgrades. No nightly escape hatch or
warning allowance is enabled. Multiline ignore reasons are covered.

The live ledger writes `admitted-libtest-records-v1` selection/result identities
and counts with the phase's successful outcome, never as a later pass amendment.
The workspace Cargo phase separately records `cargo-summary-presence-v1`:
bounded stdout, a nonempty passing observation, and a valid final summary.
That weaker scope does not prove every Cargo child or doctest ran and cannot
authorize reusable test receipts. Exact workspace ownership remains required.

Each direct phase owns a fresh private directory under `target/quality` and its
single known result file. Reading/hashing is bounded to 16 MiB and rejects links
and nonregular files. Normal completion checks cleanup; failure and unwinding
also remove owned files without deleting unexpected siblings. The read bound
is not a live disk-write quota. SIGKILL orphan recovery and owner registration
remain open. Raw logs are disposable current-run observations, not retained
cross-run receipts or proof of an immutable source snapshot.

Validation:

- `target/v9-test-completion-check.log`: 17 shared process-owner and 119
  orchestrator tests passed. The retained-output API preserves diagnostics after
  child failure without treating that failure as success. Clippy then identified
  one Boolean simplification, fixed without an allowance.
- `target/v9-test-completion-closeout.log`: the affected summary test and new
  selection/ignore test passed; all 120 compiled orchestrator tests have passing
  observations across the scoped runs. Strict all-target Clippy for both crates,
  production build, fresh AST/build-graph, canonical-type, source-size,
  documentation, formatting, and diff checks passed.
- `target/v9-test-completion-rehearsal.log`: the production runner validated a
  tiny real workspace in 4.703 s with two Cargo and 20 direct child launches;
  all nine direct phases have exact completion evidence. A second run exiting
  early with a forged passing stdout summary failed before workspace execution
  (one Cargo, ten direct children). Reports are retained as
  `target/v9-test-completion-matching.json` and
  `target/v9-test-completion-early.json`. Copies were compared before removing
  the private 16 MiB build fixture; no result-log directories remained.

These are production wiring and regression fixtures, not another run of the
complete Terlan release suite. V9-1 and reusable phase receipts remain open.

### Enclosing process groups for nested test owners

Cargo's pinned test-runner hook can retain package-specific execution context
without another build, but a helper using a new isolated child group would let
its harness survive an outer Cargo timeout. The shared process owner now has
an explicit enclosing-group mode on Linux/macOS. Membership in the supplied
group is checked before every launch; a reused command builder cannot silently
restore a separate group. Inner owners kill/reap only their retained direct
child. The enclosing owner retains its group leader and performs group cleanup.
Ordinary callers keep the existing isolated-group behavior.

This is the process-lifetime prerequisite, not the completed Cargo runner hook.
Cargo test execution and summary-only workspace evidence are unchanged. Native
harnesses and rustdoc's standalone/merged doctests need distinct ownership:
attempting `--list` on an ordinary doctest executable can execute its body and
repeat work. The runner must not classify arbitrary programs as libtest solely
from their executable suffix. The existing two-Cargo launch budget is unchanged.

Validation in `target/v9-enclosing-group-final.log`: all 21 shared process-owner
tests and strict all-target Clippy for the process owner and orchestrator passed.
Real fixtures exercise successful run/capture, failed captured output, timeout,
cancellation, failed launch observation, and unwinding. Inner cleanup reaps its
own child while a sibling remains alive. A reused builder's group override is
checked. Invalid group identifiers cannot launch or invoke the observer.
Linux outer timeout/cancellation fixtures verify nested child/grandchild group
membership and termination, plus direct-child reaping. Grandchildren can remain
zombies until their adopter reaps them; that is not reported as direct reaping.
The initial check caught an incorrect Rustix accessor before tests executed;
the accessor and a fixture's failure-category expectation were corrected.

This does not contain deliberate `setsid`/`setpgid` escapes, establish Windows
job ownership, recover SIGKILL result directories, or prove native macOS cleanup.
Those remain explicit closeout requirements. No production caller has yet opted
into the new nested mode; the upcoming Cargo helper must do so explicitly.

`target/v9-enclosing-group-closeout.log` records all 120 orchestrator tests and
the production driver build passing, followed by fresh AST/build-graph checks
(which already include canonical-type ownership). The command chain then
rejected an unnecessary, nonexistent standalone canonical-type subcommand;
no additional canonical check or test replay was needed.
`target/v9-enclosing-group-quality.log` records the remaining source-size,
documentation, workspace formatting, and diff checks passing.

### Cargo runner boundary and doctest isolation

`cargo_runner_test.rs` runs a real pinned Cargo workspace with two native library
harnesses, one `harness = false` custom executable, ordinary 2021 doctests, and
merged 2024 doctests. Its temporary Rust runner records argv, package working
directory, and Cargo-configured environment while executing each requested
binary exactly once. This is a diagnostic fixture, not the production runner.

The observations rule out a generic libtest adapter:

- Cargo gives custom test programs the same test-shaped arguments as libtest
  and reports `profile.test = true` for both. Neither condition proves a program
  supports nonexecuting inventory queries. The declared target/harness contract
  must be admitted before probing it.
- Both standalone and merged doctest programs reach the runner without native
  libtest argv. The merged runner embeds its test arguments in generated code;
  appending `--list` to its process arguments would not select inventory mode.
- The generic runner changes merged-doctest isolation. In pinned Rustdoc's
  [execution path](https://github.com/rust-lang/rust/blob/1.96.0/src/librustdoc/doctest.rs),
  `RUSTDOC_DOCTEST_BIN_PATH` is supplied only without `test_runtool`. With the
  generic tool, the generated runner falls back to executing tests together.
  Two doctests calling the same library-local atomic counter observe `0, 1`,
  instead of the normal isolated `0, 0`. The test checks state, not PID uniqueness,
  and therefore does not depend on the operating system avoiding PID reuse.

Three independent fixture cycles compare the generic hook, ordinary Cargo, and
a native-only hook followed by unwrapped `cargo test --doc`. The latter two
preserve isolated doctest state and execute all six correctness bodies exactly
once in their respective cycle. Normal and split cold cycles emit the same five
nonfresh compiler-unit identities, with duplicates rejected within each cycle.
The split uses the same target cache for its native and documentation commands;
its second command builds the previously unneeded normal library, not another
copy of the already built unit. Cargo's compiler probes and every nested rustc
or linker launch are not fully inventoried by this comparison.

Evidence:

- `target/v9-cargo-runner-boundary-final.log` verifies the initial runner argv,
  context, artifact, and five-body probe. The first attempt failed to compile
  the small runner because its Rust edition was implicit; it now declares 2021.
- `target/v9-cargo-runner-comparison.log` records the first separate-owner
  comparison: 402 ms combined versus 476 ms split in the tiny fixture.
- `target/v9-cargo-runner-closeout.log` records the strengthened shared-state
  regression test and explicit duplicate-unit rejection: 403 ms versus 438 ms,
  six bodies exactly once and five equal compiler units. Strict all-target
  Clippy, fresh AST/build-graph (including canonical types), file size,
  documentation, formatting, and diff checks pass. The compiled orchestrator
  inventory is now 121 tests, with passing observations across the focused and
  preceding runs. Private fixture build directories are removed by their owner.

These timings are diagnostic, not a full-workspace performance baseline or a
quiet-host gate. Production commands, test execution, and the two-Cargo limit
remain unchanged. A native-only ownership hook plus a separate doctest owner is
the measured candidate, not a completed implementation or an approved budget
increase. It still needs declared custom-harness policy, exact artifact/result
reconciliation, nested-group integration, bounded atomic child observations,
failure/cancellation tests, and platform coverage before production closeout.

### Declared library-harness admission

The production Terlan union-feature build now returns a declared library-harness
identity, not just a path selected by `profile.test`. Admission requires one
matching Cargo library artifact, an absolute package manifest within the frozen
source root, the expected package/target name, a matching declared source path,
and the default or explicit `harness = true`. Custom library harnesses fail with
`harness-declaration-failed` before any inventory query. Missing, duplicate, or
inconsistent artifact observations fail closed. The old parallel path-only
selector and its synthetic tests were replaced; existing real Cargo selection
and nonexecuting-inventory tests now use the production admission function.

The manifest is read and hashed once per observation under the shared guarded
1 MiB file bound. Links, nonregular files, mutation during reads, and paths outside
the source root are rejected. The executable binding records
`declared-cargo-library-harness-v1`, including manifest identity and the declared
source alias. Before another harness query/run and at executable closeout,
manifest identity and source resolution must still agree. Executable bytes and
the full working-source admission/closeout remain separately checked. This is a
declaration check, not proof that an arbitrary compiler or hidden wrapper emitted
libtest, nor a reusable immutable-source receipt. Workspace binaries, integration
targets, and custom harness owners still need corresponding admission.

Evidence:

- `target/v9-harness-admission-check.log`: 123 tests passed; one negative fixture
  failed because it accidentally placed `lib = false` inside `[package]` rather
  than constructing an invalid library declaration. The fixture was corrected.
- `target/v9-harness-admission-final.log`: all seven focused admission tests,
  strict all-target Clippy, and the production driver build pass. The tests cover
  default/explicit contracts, ambiguous/missing artifacts, source mismatch,
  manifest mutation before another launch/closeout, size/root bounds, manifest
  links, and source-alias retargeting. A real Cargo `harness = false` library is
  compiled but rejected without executing its body. The current orchestrator
  inventory is 125 tests, all with passing observations across these scoped runs.
- `target/v9-harness-admission-quality.log`: fresh AST/build-graph including
  canonical types, size, documentation, formatting, and diff checks pass.
- `target/v9-harness-admission-rehearsal.log`: the production driver passes a
  tiny real workspace in 4.051 s, recording the declaration and verified input
  owners, with the same two Cargo and 20 direct-child launches as before.
  `target/v9-harness-admission-rejection.log` repeats the driver with an explicitly
  custom library and fails after one Cargo/seven direct launches, with zero
  inventory queries and no execution marker. Reports are retained as
  `target/v9-harness-admission-matching.json` and
  `target/v9-harness-admission-rejected.json`; copies were compared before the
  private 14 MiB build fixture was removed.

The rehearsal uses stub prebuilt runtime entry points and tiny real test
harnesses, not the complete Terlan correctness suite. It proves production
wiring and failure ordering only. No launch budget or roadmap checkbox changed.

### Shared library, binary, and integration-target declarations

The library admission now uses the same `DeclaredHarness` and target resolver
as binary and integration-test admission. The current report scope is
`declared-cargo-libtest-target-v1`, with package, target name, and kind added;
the earlier library-only rehearsal reports retain their historical scope.
Production Terlan library admission uses this shared path. Workspace execution
has not yet been switched to it and still needs exact per-harness observations.

The resolver follows the relevant
[pinned Cargo target conventions](https://github.com/rust-lang/cargo/blob/rust-1.96.0/src/cargo/util/toml/targets.rs)
for explicit declarations, normal library/binary/integration discovery,
renamed explicit sources, discovery switches, and the 2015 explicit-target
discovery rule. Inherited package editions use the normalized observed target
edition when needed. Matching targets retain their own harness setting even
when two explicit names share a source. Conventional-path ambiguity, hidden
names, unsupported kinds, malformed settings, missing names, duplicate target
names, and unregistered custom harnesses are errors. Discovery does not follow
a target-directory symlink; an explicit in-root source alias remains permitted
and is checked by the guarded declaration binding.

This does not re-run Cargo metadata or scan whole directories: only the admitted
manifest and the observed target's finite conventional source candidates are
examined. Cargo still owns feature selection and compilation. The existing
source-root, bounded manifest-read, executable-byte, and manifest/source-alias
closeout checks remain in place. Nonstandard legacy binary paths must be
explicit. Examples, benchmarks, and non-`lib` library kinds have no admitted
owner in this resolver and fail closed rather than being guessed as libtest.

Validation:

- `target/v9-target-declaration-check.log`: 129 tests passed. One fixture tried
  to insert a missing TOML key through index assignment and failed in setup.
  `target/v9-target-declaration-final.log` caught an unsupported fixture map
  conversion before execution. Both fixture construction issues were corrected.
- `target/v9-target-declaration-closeout.log`: all five resolver tests, strict
  all-target Clippy, and the production driver build pass. A real Cargo workspace
  produces nine test artifacts: seven default/explicit libtest declarations are
  admitted and two custom binary/integration harnesses are rejected. Its bodies
  panic if accidentally executed; admission performs no executable probes.
  Discovery, renamed/shared sources, inherited editions, malformed policy, and
  directory-alias cases also pass. The current 130-test orchestrator inventory
  has passing observations across these scoped runs.
- `target/v9-target-declaration-quality.log`: fresh AST/build-graph including
  canonical types, file-size, documentation, formatting, and diff gates pass.
  Owned temporary Cargo fixtures were cleaned after their terminal results.

No workspace execution, launch budget, or roadmap checkbox changed. The next
owner integration must deduplicate manifest admission across targets in one
package, reconcile observed artifacts with completion records, preserve Cargo's
execution context, and retain independent doctest isolation and failure handling.

### Live bounded Cargo output

Workspace execution now displays Cargo stdout as it arrives, without printing it
again at closeout. The shared process owner delivers ordered byte chunks on the
owner thread and retains those same bytes for the existing final verification.
It uses a bounded 64-chunk queue, drains batches without a polling delay per
chunk, and preserves the total capture limit and separate exit outcome.
Callbacks have no line-boundary guarantee and must remain short and nonblocking.
Callback error, panic, cancellation, timeout, or overflow cannot seal success;
stopping a full reader queue cannot deadlock its owner. Existing platform limits
on escaped descendants and Windows reader/job cleanup remain unchanged.

`target/v9-live-output-check.log` records 26 passing process-owner tests, 130
passing orchestrator tests, strict all-target Clippy for both crates, and a rebuilt
production driver. The new real-process tests prove delivery before child exit,
owner-thread callbacks, byte-exact 2 MiB binary output, retained nonzero status,
and direct-child reaping after observer error, panic, or cancellation. The
structural, size, documentation, formatting, and diff gates are recorded in
`target/v9-live-output-quality.log`.

This is production diagnostic delivery and a live observation API, not completed
workspace test ownership. Cargo evidence still has the explicitly weaker
`cargo-summary-presence-v1` scope; launch budgets and checklist boxes are unchanged.

### Streaming artifact admission and shared package snapshots

The production Terlan harness build now processes complete Cargo JSON records
through the live process-owner callback, instead of selecting a path from a
completed stdout buffer. The shared stream admits each selected target once,
rejects shared executable paths, and notifies consumers only after declaration
admission. Notifications remain provisional until the actual Cargo exit and
stream closeout succeed. Missing/failed build termination, incomplete framing,
unknown records, duplicate targets, callback errors or panics, and repeated
capture attempts cannot seal success. Typed declaration failures survive the
process-observer boundary. There is no additional Cargo invocation.

Framing is bounded to 1 MiB per record, 16 MiB per stream, and 1,024 harnesses.
One batch shares at most 256 parsed manifests and 16 MiB of manifest source;
the existing guarded 1 MiB per-manifest read remains. Multiple targets reuse the
same parsed snapshot, while per-harness manifest/executable/source checks still
reject changed inputs before execution. This is not permission to execute a
stale snapshot: a mutation fixture proves both returned declarations fail their
subsequent verification. The former buffer selector is now only a test adapter
to the production stream, not an independent selection algorithm.

Evidence:

- `target/v9-artifact-stream-check.log`: all 139 then-current orchestrator tests,
  strict all-target Clippy, and the production build pass. The existing real
  Cargo compilation/execution test now calls `prepare_terlan_harness` directly.
- `target/v9-artifact-stream-final.log` and
  `target/v9-artifact-stream-closeout.log`: the real preparation test and added
  failed-exit/repeated-launch test pass after the typed capture adapter was
  completed, with strict Clippy and the rebuilt driver. The current inventory
  is 140 tests, with passing observations across these scoped runs.
- `target/v9-artifact-stream-rehearsal.log`: full driver wiring on a tiny real
  workspace passes in 2.859 s, with two Cargo and 20 direct launches; source and
  executable closeouts pass. `target/v9-artifact-stream-rejection.log` rejects
  a custom library with `harness-declaration-failed` after one Cargo/seven direct
  launches, zero inventory queries, and no execution marker. The rehearsal uses
  unused stub prebuilt runtime entry points, not the complete release suite.
- Retained reports: `target/v9-artifact-stream-matching.json` and
  `target/v9-artifact-stream-rejected.json`. Copies were compared before cleaning
  the private 16 MiB Cargo fixture. Fresh structural, size, documentation,
  formatting, and diff gates are recorded in `target/v9-artifact-stream-quality.log`.

The artifact stream is not a workspace test runner or a reusable test receipt.
The remaining integration must publish native-runner authorization, preserve
Cargo's package environment, record actual nested launches, and reconcile exact
completion records without changing doctest isolation. No launch budget or
roadmap checkbox changed.

### Native workspace execution ownership and doctest isolation

The canonical Rust driver now uses an authorized Cargo runner for native
workspace library, binary, and integration-test targets. Cargo retains package
working directories, loader paths, and configured environments. Its JSON stream
admits each executable and package declaration before a private certificate is
published. The helper verifies that certificate and executable bytes, lists all
and ignored tests once each, and executes the admitted selection once. The
parent reconciles every emitted native target with its completion and actual
helper/inventory/test launch records. Missing, duplicated, or changed owners
cannot pass. Unregistered custom harnesses and workspace ignored tests without
an explicit separate tier owner are errors, not silent skips.

Native test stdout is captured and forwarded to stderr, so neither ordinary test
output nor forged Cargo JSON can enter artifact admission. Completion uses the
same private libtest record parser as direct Terlan phases. Empty declared native
harnesses are permitted; the overall workspace must execute nonzero tests, and
Terlan selectors remain nonempty. Successful targets release their raw result
log immediately. A bounded private registry owns certificates, claims, PID
records, and completion files; it removes only registered files. Ordinary
failures retain available launch observations in the failed phase report before
cleanup, explicitly marked incomplete rather than reusable test success.
Linux/macOS helper children join the enclosing Cargo process group. Windows
job-object containment, deliberate descendant group escapes, and SIGKILL orphan
recovery are still separate open requirements.

Budget review: native tests use `cargo test --tests` with the owned runner;
doctests use a separate normal `cargo test --doc`. This deliberately raises the
orchestrator Cargo allowance from two to three and the preparation-wide
allowance from six to seven. It avoids a generic runner changing Rust 2024
merged-doctest isolation or an invasive Rustdoc wrapper. The existing real-Cargo
differential (`target/v9-cargo-runner-closeout.log`) records 403 ms for the
combined baseline versus 438 ms for the split, five identical unique nonfresh
compiler-unit identities, and six bodies executed once in either cycle. This is
one extra coordination invocation, not evidence of duplicate equivalent
compilation. The real driver rehearsal below independently preserves package
context and merged-doctest isolation. Broader release-cycle timing and full
nested compiler launch accounting remain unproven; no broader cost win is claimed.

At this checkpoint, doctest evidence was `cargo-doctest-summary-presence-v1`;
the following completion-observation work replaces that weaker scope. Native
workspace evidence is `cargo-native-libtest-records-v1`. Failed owners retain
`retained-native-launch-records-v1`; these records cannot seal a successful phase.

Validation and rehearsals:

- `target/v9-workspace-native-final.log`: all 148 orchestrator tests, strict
  all-target Clippy, production build, and refreshed Cargo metadata pass. Earlier
  build/focused evidence is in `target/v9-workspace-native-build.log` and
  `target/v9-workspace-native-tests.log`. Serde is now a direct dependency for the
  private declaration certificate, using the already locked version; no new
  dependency versions were introduced.
- `target/v9-workspace-native-closeout.log` and `.json`: final driver rehearsal
  passes in 3.046 s with three Cargo and 21 direct launches, three native targets,
  three native tests, and 12 recorded nested launches. An empty binary target
  completes correctly. All three native bodies and both isolated Rust 2024
  doctests execute exactly once; records are retained in
  `target/v9-workspace-native-closeout-body-records.txt`. The initial cold fixture
  took 4.332 s; these are tiny-fixture observations, not release-suite timings.
- `target/v9-workspace-native-stdout.log` and `.json`: direct stdout containing
  a forged failing Cargo build record does not corrupt artifact admission.
- `target/v9-workspace-native-retained.log` and `.json`: exit zero plus a forged
  `999 passed` summary fails exact completion. The failed phase retains four
  actual nested launch records, no doctests start, and its scratch directory is
  cleaned. The earlier failure without retained records is preserved in
  `target/v9-workspace-native-early.log` and `.json`.
- `target/v9-workspace-native-bypass.log` and `.json`: an explicit target runner
  set to `/bin/true` cannot bypass ownership. Missing completions fail, with zero
  helper launches and no native test bodies executed.
- `target/v9-workspace-native-quality.log` records fresh structural/canonical
  type, source-size, documentation, formatting, and diff gates. Private fixture
  reports were copied and compared before its 39 MiB build tree was removed.

The rehearsals use tiny real Cargo targets and unused prebuilt-runtime stubs.
They establish this execution path, not complete release correctness. V9-1 still
requires preparation-wide ownership, complete nested and doctest accounting,
remaining cleanup/input boundaries, reusable receipts, and the clean-candidate
cold/warm/interruption-resume acceptance run.

### Rustdoc completion observations

The existing doctest Cargo invocation now requests stable pretty output and
reconciles every emitted harness start, unique test name/outcome, and terminal
summary. Missing, duplicate, excessive, failed, filtered, or inconsistent records
fail even if the last summary looks successful. Empty harnesses are legitimate.
Names retain spaces, source locations, and compilation/panic modes. Slow-test
warnings remain diagnostic only when the named test subsequently completes.
Record and harness counts are bounded; name/outcome identities and captured-byte
identities are recorded without treating timing text as test identity.

The scope is `rustdoc-emitted-harness-records-v1`, with
`independent_inventory: false`. This does not yet prove Cargo selected all required
packages, provide a private result channel, or authorize reuse as a completed
test receipt. No launch budget changes, additional Cargo/discovery pass, generic
doctest runner, or unstable compiler options are introduced.

Pinned Rust's [libtest console implementation](https://github.com/rust-lang/rust/blob/1.96.0/library/test/src/console.rs)
uses `File::create` for each result logfile. A real mixed-doctest fixture confirms
that one shared logfile loses earlier harnesses. Its single Cargo invocation
instead reconciles seven passing checks and one ignore, covering Rust 2021 and
2024, merged/standalone execution, compile-only, expected compile failure, and
expected panic. Five runtime bodies execute exactly once, shared-library state
remains isolated, and forged body stdout is captured rather than admitted.
A separate real fixture exercises the production phase function: three checks,
two harnesses, one Cargo launch, and an exactly-once append marker.

`target/v9-doctest-records-final.log` records all 154 orchestrator tests passing,
strict all-target Clippy, and a production build. Focused initial evidence is in
`target/v9-doctest-records-tests.log`. The final append-marker fixture, strict
Clippy, fresh structural/canonical-type analysis, source-size, documentation,
formatting, and diff checks pass in `target/v9-doctest-records-quality.log`.
These are scoped tests, not a full release candidate rehearsal. V9-1 remains open.

### Exact main-harness selection handoff

The Make coverage audit exposed an execution gap: the prior library-only build
and workspace phase with `--exclude terlan` did not own the product package's
standalone integration and binary tests. That exclusion is now removed. Build
and native execution share one `--workspace --tests` argument set with explicitly
qualified Terlan union features. The first Cargo invocation builds all selected
test units; the native execution invocation must see the main library as `fresh`.
A second main-library compilation is rejected, not accepted as cache reuse.

The Cargo-native runner delegates only the exact source-bound main library to
the suite's existing direct phases. It records a `delegated` completion and its
one helper PID, without another discovery or test execution. That record cannot
count as a test pass. All other selected targets, including Terlan's integration
and binary harnesses, execute through their Cargo package context. Missing,
changed, duplicate or aliased main-harness delegation is rejected. The existing
three-Cargo-invocation budget is unchanged.

Test CLI/runtime executables are build outputs and are now byte-bound immediately
after the workspace build, before tests execute. Git, Cargo, Rustup and the driver
remain pre-build tool inputs. This permits the owned build to produce the CLI
binaries required by integration tests without treating an intentional output
change as input corruption. Rebinding or changing an admitted test program still
fails. Fresh coverage verification follows the same ordering.

`target/v9-workspace-owner-final-tests.log` records 197 unit tests, three binary
integration tests and strict all-target Clippy passing. The real workspace's
expanded no-run build succeeds in 4m 04s and emits 44 test executables, including
13 Terlan integration targets (`target/v9-workspace-owner-build.{log,jsonl}`).
This build verifies compilation and target selection, not execution of all those
tests. Final orchestrator-only changes are additionally covered by its complete
test and Clippy run.

The real-driver fixture in `target/v9-workspace-owner-final-production.log`
includes product binary and standalone integration tests plus freshly built
`terlc`, VM and native-worker programs. Its 15 expected test bodies execute once;
nine native targets include exactly one delegated main library. Native execution
records 33 nested process launches and four passing test cases; direct library
phases and doctests own the remaining cases. An injected product integration
failure makes the suite fail in its native phase. Unchanged fresh coverage
verification succeeds without test replay. The earlier rehearsal before adding
the three built fixture programs is retained separately as
`target/v9-workspace-owner-production.log`.

This correction does not yet replace Make's unchecked skip flags. Batched request
admission must also account for these native targets and release-profile evidence
producers, instead of treating every request as a library selector.

The final current-source workspace no-run build also passes, reusing the product
units and rebuilding only the updated orchestrator in 4.25 seconds
(`target/v9-workspace-owner-final-build.{log,jsonl}`). It reveals an additional
bootstrap alignment requirement: the same final driver source produces different
binary bytes under focused-package and workspace-test dependency contexts. The
focused binary SHA-256 was
`aa51c536ae4ac9e74f007280193c3bd5db6b29e83347b48b763807f80519652d`;
the workspace build produced
`19137687e20ba12bdc81d9d636d3fde099488245a4a384b5b9c7cec3561b28ca`.
Previously a cold invocation from the other context could replace the admitted
driver and fail verification. Bootstrap now copies its already built driver to
`target/validation-tools/terlan-test-orchestrator` (with `.exe` on Windows).
Both metadata and canonical suite execution use that independent snapshot.
There is no added Cargo invocation, compilation profile, or shell script.
The prebuilt bootstrap requires the snapshot created by the owning bootstrap;
it must not silently replace it after the workspace tests change Cargo outputs.

Snapshot installation shares bounded atomic publication with report writers:
permissions are set before publication, unchanged bytes/permissions retain the
existing file, and interrupted staging cannot replace the previous executable.
Each payload is limited to 32 MiB; the final, pending and retained interrupted
payloads occupy at most 96 MiB across three fixed names. OS-backed shared reader
leases last through driver dispatch and its owned work. An installer requires
the exclusive lease and fails immediately while readers are active; a writer
also excludes startup. Cargo can rebuild its output independently. Missing,
non-executable, oversized and symlink sources are rejected. This snapshot does
not relax executable identity checks or create reusable test receipts.

`target/v9-driver-snapshot-tests.log` records 204 unit tests and the existing three
integration tests passing. The additional actual-binary cross-process test and
strict all-target Clippy pass in `target/v9-driver-snapshot-integration.log`.
That test exercises warm installation, reader/writer exclusion, and continued
snapshot execution after replacing the original build output.
The real workspace no-run build in
`target/v9-driver-snapshot-workspace.{log,jsonl}` takes 1.74 seconds, changes
Cargo's driver SHA-256 from
`0fccf838d99a93e0c39c6380d391c38171bf5c47aa7462d6f1fff78a5d257cd9` to
`e0b1d2af6f3caa9b342cd762948850315c56948ac6be093344af0538313f478a`,
and verifies the snapshot retains the former bytes. The fresh repository
validator self-test and build/release contract check also pass in that log.
This proves the observed self-replacement defect is corrected, not full-suite
execution or end-to-end preparation/resume acceptance.
Fresh structural AST, shared Make metadata/module/headroom/supply-chain checks,
Rust quality/documentation, Rust formatting and diff checks also pass in
`target/v9-driver-snapshot-quality.log`. These consumers use the retained
snapshot after Cargo has replaced its separate output. No file-size or build
count limits were raised.

Fresh structural AST, shared-metadata module/headroom/supply-chain, Rust quality,
documentation and formatting gates pass in `target/v9-workspace-owner-quality.log`.
Fixture sources, final reports, exact-name companion, fresh-verification result,
success/failure metadata and executed-body records are retained in
`target/v9-workspace-owner-evidence`. The 99 MiB disposable fixture build is
removed after byte comparison; the real workspace's completed build is retained.

The current companion schema is `terlan.rust-test-selections.v2`. It retains
the complete compiled-name inventory and original ignored classification from
the two existing discovery queries, including tests delegated to another owner.
Only locally executed phases appear in its completion selections. The same
bounded name/byte budgets cover both portions; the live-report ceiling is unchanged.

Historical coverage inspection uses the execution planner's selector function
for substring, exact, skip and ignored selection. The result reports selected,
passed and uncovered counts and never claims coverage for an empty match. A
broad selector cannot be satisfied merely because one locally owned test matched
while another matching test was delegated. Original ignored tests require actual
execution under an ignored-test owner; their presence in a normal phase's ignore
records is not passing coverage. Aggregate normal/ignored coverage is computed
once after closeout and retained in the live report. Current input admission and
the Make skip-flag consumer remain separate unfinished requirements.

`target/v9-coverage-inventory-verified.log` passes 184 unit tests, two Make
integration tests and strict all-target Clippy. It includes delegated-name,
ignored-owner, broad/exact/skip/empty-selector, classification, count and overflow
regressions. No additional Cargo or test-inventory calls are introduced.

`target/v9-coverage-inventory-production.log` runs the actual driver in full and
delegated-normal-coverage modes against one unchanged metadata generation. The
full mode executes the 13 expected bodies once and reports 2/2 normal tests
covered. Delegated mode executes its 12 expected bodies once and reports 1/2
normal tests covered, with one explicitly uncovered. Both retain all 14 compiled
names, identify 12 as originally ignored, and report seven executed ignored tests
versus five external tests. Each cycle makes three Cargo launches; the metadata
owner is not repeated between those compatible handoffs. The cycle policies
differ, so this is not a test-reuse claim.

`target/v9-coverage-inventory-quality.log` passes fresh structural AST,
shared-metadata module/headroom/supply-chain, Rust quality/documentation,
formatting and diff gates. Reports, both companions and fixture sources are
retained in `target/v9-coverage-inventory-evidence`; disposable fixture builds
are cleaned after byte comparison. The current-input coverage reader and actual
replacement of unchecked Make skip flags remain required before V9-1 closes.

The orchestrator now exposes the read-only command
`--inspect-coverage REPORT -- [LIBTEST SELECTORS]`. It reads the report and its
fixed sibling `<report>.selections.json`, checks the actual companion hash,
suite generation, harness binding, inventory budgets, unique ownership and
exact terminal completions, then applies the same selector implementation used
for execution. Archived pairs remain inspectable without the original build
directory. The reported path never redirects the reader to an arbitrary file.
Invalid or incomplete evidence fails; unknown orchestrator commands also fail
before source admission instead of accidentally starting the entire suite.

This is explicitly **historical inspection**, not a Make skip command: exit zero
means the query succeeded, including when `covered` is false. Every query returns
`reusable: false` and `current_inputs_verified: false`. It does not launch Cargo,
test discovery, tests or metadata, and must not replace current-input validation.
`target/v9-coverage-reader-tests.log` records 188 unit tests, three integration
tests and strict all-target Clippy passing, including real-binary invalid-command
dispatch and report/companion corruption, scope, count and ownership rejection.

The real command also reads both retained production report pairs: full normal
coverage is 2/2, delegated normal coverage is 1/2, and ignored coverage is 7/12.
The query outputs are retained in `target/v9-coverage-reader-{full,delegated,ignored}.json`.
Fresh structural AST, shared-metadata module/headroom/supply-chain, Rust quality,
documentation and formatting gates pass in `target/v9-coverage-reader-quality.log`.
The prior fixture's 38 MiB build directory was removed after comparing retained
sources, reports, companions, metadata and exact executed-body records. Current
input verification and the Make coverage consumer are still unfinished; these
historical queries do not close V9-1.

`--verify-coverage REPORT -- [LIBTEST SELECTORS]` now adds a fresh current-input
decision. Execution and verification call the same `suite_inputs` admission and
closeout sequence. The verifier re-admits the saved library declaration from the
current Cargo manifest, binds the original executable's current bytes, and
compares all eight closed input bindings plus test-thread and timeout policy.
These include source, executable, environment, configuration, selected Cargo
tools, Rust installation, compiler context and metadata/dependency source inputs.
It requires the existing matching metadata generation; it does not issue another
metadata query or silently normalize differing execution environments.

Its separate `<report>.verification.json` observation records `inputs-verified`,
not suite `pass`: it has no builds, discovery or test execution. The command's
JSON result uses `current-main-harness-coverage-v1`, requires every selected test
to be covered, and states `current_inputs_verified: true`. It is a decision for
that invocation, not a transferable cached skip token (`reusable` remains false).
An uncovered selector fails before expensive input admission. Declaration and
input rejection leave a terminal failed verification observation.

`target/v9-current-coverage-production.log` records 191 unit tests and three
integration tests passing with strict all-target Clippy, followed by the actual
driver building and executing a two-package fixture. Its 13 expected bodies run
once. Unchanged current verification takes 2,516 ms, records seven input-probe
processes and zero Cargo launches, and leaves the body record byte-identical.
Unexecuted ignored coverage and changed test-thread settings are rejected.
`target/v9-current-coverage-mutation.log` then changes an ignored dependency
source from 12 to 14 bytes: Git-listed sources remain unchanged, supplemental
package-source identity changes, and verification fails with zero Cargo launches
and no test replay. This is not a full-repository performance measurement.

Fresh structural AST, shared-metadata module/headroom/supply-chain, Rust quality,
documentation and formatting gates pass in `target/v9-current-coverage-quality.log`.
The original suite, exact-name companion, current and rejected verification
observations, metadata and fixture sources are retained in
`target/v9-current-coverage-evidence`. Its `original-ignored-input.txt` retains
the passing 12-byte dependency input; the copied support source retains the
14-byte rejection input. The 38 MiB disposable fixture build was removed after
byte-comparing the retained evidence; no repository build cache was deleted.

Make still needs integration with the batched current-input coverage consumer
and explicit handling of package/target selections and phase environments.
Calling this verifier separately for every gate would repeat the input scan and
is not the intended integration. Full preparation/retry acceptance remains open.

The driver now provides `--verify-cargo-coverage REPORT REQUESTS.json` for that
batch boundary. Requests are a bounded JSON array of unique `id` values and
`arguments` arrays containing the arguments after `cargo test`. It requires an
explicit package and library, binary or integration target, applies the reviewed
debug/union-feature policy, and shares libtest's exact/filter/skip matcher.
Missing selectors, unsupported profiles (including `--release`), target-wide
selection shortcuts and undeclared feature policies fail rather than substitute
different evidence. Duplicate gate IDs fail; different gates may legitimately
reference the same completed test without executing it again.

Native completion records now retain their already-discovered `passed_names`.
The parent reconciles them with the private result-log selection digest and
counts. This adds no discovery or test launches and does not raise the existing
1 MiB record/report ceilings. Batch inspection shares each native inventory
between requests, then re-admits each requested target only once against current
package declarations and executable bytes. A shared manifest cache and remaining
aggregate byte budget apply before reads. Library and native requests use one
common source/configuration/toolchain/metadata admission and closeout. The
result binds the request bytes and completed suite; it remains `reusable: false`
and is not permission to trust a future Make skip flag or a changed environment.

`target/v9-make-coverage-requests.plan` records 252 test requests from the actual
`check-gates` dry run: 242 library requests and ten integration requests across
five target names. All select Terlan; 22 request canonical optional features.
Two ignored capability-worker requests also supply a runtime executable
environment override, which the eventual Make consumer must preserve explicitly.
The first cutover must account for those phase environments, source/tool checks
across the enclosing Make invocation, and separately owned release-profile
producers. The unchecked `TERLAN_RUST_SUITE_ALREADY_RUN` branches are not yet
replaced; this batch command is the implemented verification boundary, not a
claim that Make is already safe to skip its work.

`target/v9-coverage-batch-production.log` records 210 unit tests, four integration
tests and strict all-target Clippy passing. Its initial fixture-launch typo
fails before the fixture starts; the production rehearsal itself is retained in
`target/v9-coverage-batch-fixture.log`. That real two-package suite executes its
15 bodies once (three Cargo calls, 21 direct launches, 33 native nested launches).
Seven library/integration/support requests, including repeated requests for the
same tests, verify in 3,689 ms with seven input probes, zero Cargo calls and an
unchanged executed-body record. Three unique native targets are verified.

Missing integration selectors and debug-for-release substitution are rejected
in `target/v9-coverage-batch-rejection.log`. That first cross-container byte
mutation attempt correctly fails on changed environment before reaching the
native byte check; it is not evidence of the latter check. The separate
same-environment mutation cycle in `target/v9-coverage-batch-mutation.log`
accepts the original native executable and rejects its replacement with the
specific `requested native harness differs from completed suite` diagnostic,
without Cargo or test replay during verification. Each cycle's body records
remain separate. Fresh structural AST, shared-metadata module/headroom/supply-chain,
Rust quality/documentation, formatting and diff gates pass in
`target/v9-coverage-batch-quality.log`. Full candidate acceptance remains open.
The 432 KiB `target/v9-coverage-batch-evidence` directory retains both cycles'
sources, requests, suite/selection documents, metadata, verification outcomes
and executed-body records. Its 98 MiB disposable build was removed after byte
comparison; the repository build cache and self-hosting checkout were untouched.

### Live Make coverage ownership

`--with-cargo-coverage REPORT -- make TARGETS...` now admits the completed suite's
current inputs once, owns one Make execution, and closes those inputs afterwards.
The main `check` path uses it. The Make routing must be installed before included
files expand generated recipes, so exact-test recipes cannot capture the old
launcher. Nested metadata consumers ask this same owner for the admitted metadata
instead of issuing a second Cargo query. The default-feature AOT library check
remains a distinct build policy; union-feature test success does not replace it.

Requests go to an authenticated loopback listener owned by the running driver,
not to a stored success flag. Messages, retained observations and elapsed reads
are bounded. The endpoint is closed on owner shutdown and is not recorded in
public evidence. Make/shell bookkeeping is classified separately from effective
test inputs; worker overrides and unclassified environment changes remain bound
to the selected test phases. Input admission ignores only pending closeout
observations, while final closeout still requires all eight complete bindings
to match. Make success cannot hide rejected or missing coverage requests.

The new `tests/make_coverage.rs` uses the production Make fragments and a real
two-package Cargo fixture. Its 15 bodies run once. Normal, native integration,
worker-specific and recursive gate requests reuse those results with zero Cargo
launches; metadata reuse, changed environments, missing selectors, swallowed
errors, old-flag-only calls and dry-run behavior are exercised. Disposable build
outputs are removed by the fixture owner. Cargo's outer integration-test binary
variables are excluded from this fixture environment because POSIX shells drop
their hyphenated keys; production request comparisons are not weakened for them.

`target/v9-make-cutover-integration.log` records this passing acceptance and strict
Clippy. `target/v9-make-cutover-contract.log` records the remaining 217 unit and
four integration tests, plus the typed validator self-test. Its initial release
contract rejection exposed the include-order defect; the corrected release
contract passes in `target/v9-make-cutover-quality.log`, alongside fresh AST,
shared-metadata module/headroom/supply-chain checks, Rust quality/documentation,
formatting and diff validation. No size or invocation budgets were raised.
The earlier manual fixture's positive coverage verification took 3.798 seconds,
with seven input probes, one observed Make launch and no Cargo/test replay.

This completes the main-check coverage cutover, not V9-1. Publication entry
points that formerly set the skip flag still require verified hosted coverage.
They now reject an unowned skip request instead of claiming success.
Global nested-process accounting, complete transitive build-input provenance,
remaining producer ownership and full cold/warm/interrupted preparation
acceptance remain open.

### Historical hosted coverage bundle inspection

The prebuilt Rust driver accepts `--check-hosted-coverage-records DIRECTORY
EXPECTED_CONTEXT_FILE`. The directory must contain exactly the three compiler
coverage artifact files: `rust-test-suite-report.json`, its `.selections.json`
companion, and its `.make-coverage.json` companion. Limits are respectively
1 MiB, 16 MiB, and 1 MiB; the context file is limited to 16 KiB. Unexpected
entries, missing records and final symlinks fail. Inspection does not launch
Cargo, Make, libtest, or current-input probes.

The expected context uses the downloader's `terlan.hosted-download-cache.v1`
shape. Its repository, source revision, compiler workflow ID/attempt and workflow
path must match the exported producer. The current accepted producer is the
`check` job of `.github/workflows/ci.yml` on `refs/heads/main`, executing on Linux
x86-64. OS/architecture are recorded from the driver itself, not runner labels.
The reader reuses the main-harness and native completion readers, validates the
exact suite and selection digests referenced by Make, requires the completed
canonical Make invocation and matching closed input bindings, and checks each
ordinary gate request against retained test names using the existing matcher.

Success is `records-consistent`, **not authenticated coverage**. The returned
file digests identify the checked subjects, with `authentication_required: true`,
`current_inputs_verified: false`, and `reusable: false`. Supplying matching JSON
coordinates is not proof of GitHub origin. The download/attestation consumer is
now wired as described below; separate publication coverage authority remains
unfinished, and a real hosted run of this new path is still required. Local
`--with-cargo-coverage` keeps its exact current-input policy unchanged.

The production Make integration fixture exercises this reader on records from
real test execution, then changes producer coordinates, source closure, Make
invocation, test names, outcomes, bytes, and directory membership. All requests
leave the fixture's 15 executed test-body records unchanged. This validates
historical record integrity, not an actual hosted CI run or full release cycle.

### Local release graph and hosted producer records

`release-evidence-refresh` now extends the canonical `check` invocation using a
frozen, exported release-evidence mode. It prepares the release image alongside
the other validators, then runs correctness and release composition in the
same live Make graph. Shared prerequisite nodes are not reentered through a
separate composition Make process. Simultaneously requesting multiple public
correctness entry points is rejected, avoiding target-specific environment
selection depending on Make's scheduling order. This does not remove every
recursive recipe inside individual gates; that remaining duplication must
still be audited.

The coverage owner now holds a shared lease on its source suite report and
records the exact report/selection digests, suite ID, and a boundary-preserving
hash of the Make executable path and arguments. Make argument values and the live
endpoint remain absent from public evidence. GitHub executions additionally
record bounded repository, revision, workflow, job, run ID and run-attempt
coordinates. These are explicitly unauthenticated workflow-environment context,
not permission to reuse tests. CI uploads the three completed documents as
`compiler-test-coverage-<run>-<attempt>`; no new build or test step produces them.

The production-entry-point fixture runs the real Make rules with lightweight
bootstrap and gate consumers. It observes one suite, three suite Cargo calls,
seven live coverage requests, one execution of a shared gate and one execution
of each bootstrap. All 15 fixture bodies run once. It rejects an ambiguous pair
of correctness roots before any body runs and validates the emitted GitHub
producer context and suite digest. `target/v9-release-producer-tests.log`
records 220 unit and six integration tests plus strict Clippy and the repository
contract checks. The added hosted-context execution assertion passes in
`target/v9-release-producer-quality.log`, followed by fresh structural AST,
shared metadata/module/headroom/supply-chain, documentation and format checks.
The fixture removes its disposable builds on exit.

Cold refresh-plan inspection now clears inherited Make command-line bookkeeping
as well as prebuilt flags, and counts prefixed Cargo build/check invocations.
`target/v9-cold-refresh-plan.log` records four explicit Cargo commands, one local
report-graph entry and no duplicate build command; publication verification still
plans zero build/test replay. Earlier prebuilt invocations incorrectly reported
one Cargo command because `MAKEFLAGS` restored the removed environment flags.
Existing budgets are unchanged. These are plan observations, not actual full
preparation launch counts.

The publication consumer now enters `--with-hosted-cargo-coverage` after bootstrap.
It admits the downloader's owned, previously authenticated checkpoint under its
shared publication-input lease, checking the verifier implementation, exact
producer, cached/installed candidate identity and three coverage-subject digests.
The admitted records stay immutable in memory; the lease ends before Make so
later distribution restoration can acquire its exclusive lock. This reuses prior
authentication, not a fresh signature verification or a caller's success marker.

The owner requires a clean matching Git revision and source bytes before and after
Make, verifies selected executable bytes, and applies the shared exact test-name
and requester-environment policy. Its `hosted-source-gates-covered` decision is
explicitly source-only: it does not certify local artifact/environment equivalence
or replace current Cargo metadata. Multicore/AOT source gates share ordinary Make
prerequisites, while the default-feature Cargo check and local report producers
remain local work. Publication now invokes release composition as an ordinary
dependency in that same live Make graph, eliminating its recursive Make replay.
`publish-evidence-staged-inputs` owns the ordered local-report, AOT-record,
authenticated restore and artifact-matrix sequence. Release composition and
the distribution-consumer entry points depend on it. Independent source/proof
checks can overlap, but a failed staging producer cannot admit distribution
consumers or successful composition, even with keep-going enabled.

`target/v9-publication-graph.log` records a production-rule rehearsal with real
Make `-j8 -k`, real Cargo and exact live coverage. Both entry-goal orders share
one source-gate execution; injected source, local-report, AOT-record, restore
and matrix failures prevent downstream consumers and the composition message.
The fixture's original 15 Rust test bodies are not replayed. Report/download
services are substituted here to verify graph ownership and ordering, not to
claim actual archive restoration or report-production acceptance. The complete
fixture passes in 16.16 seconds. Strict all-target orchestrator Clippy and
workspace Rustfmt pass. Remaining nested producer/subprocess ownership and the
complete cold/warm/interrupted candidate rehearsal are still required.

### Generated-artifact graph and inventory snapshots (2026-09-10)

The five `RELEASE_GENERATED_ARTIFACT_FRESHNESS_GATES` now share ordinary Make
prerequisites instead of five recursive Make invocations. A common order-only
snapshot edge applies even when a shared producer is first reached through
canonical `check-gates` or hosted publication source prerequisites. Focused
non-release checks do not snapshot unrelated generated artifacts. Custom aliases
must set `TERLAN_CHECK_RELEASE_EVIDENCE=1` before Make parses the graph or use the
public `release-generated-artifacts-check` entry point; unowned composition fails
instead of accepting a snapshot taken after already-completed producers.

The typed validator's internal snapshot v2 binds exact inventory bytes as well
as labeled artifact hashes and file counts. Finalization rejects changes to any
of them; the public report schema remains v1. Its actual AOT self-test exercises
unchanged finalization, output mutation and valid-but-changed inventory content.
The inventory also includes both generated JavaScript manifests, previously
checked by the JS drift gate but omitted from before/after artifact snapshots.

`target/v9-generated-tools-graph.log` records the real parallel Make fixture
passing, with one execution per freshness producer, both entry-goal orders,
keep-going failures, changed output, focused checks and unowned aliases.
Snapshot operations and leaf producers are instrumented in this fixture; the
typed validator and real generators are exercised separately. A second behavioral
test executes the production Node/npm preflight recipe with missing, failing and
working executables. Cached npm dependencies cannot hide missing host tools.
Strict all-target orchestrator Clippy passes in
`target/v9-generated-tools-clippy.log`.

The actual full gate first rejected stale standard-library summaries and JS
dependency metadata. Regeneration changed syntax-contract fingerprints following
the typed-lambda grammar update and refreshed existing Process API documentation;
public API hashes and generated JavaScript code did not change. The next attempt
passed all four stdlib gates but failed because the Ubuntu validation container
did not expose npm. The Node 24.5.0/npm 11.5.1 installation is now mounted
read-only for these local rehearsals. The preflight probes executable usability;
it is not a complete tool-version/input-closure policy.

With prebuilt compiler/validator images and existing caches, the full gate passed
in 62.328 seconds (`target/v9-generated-verified.log`), sealing 7,525 unchanged
files. That measurement predates adding the two generated manifests to the
inventory and is not a cold-preparation or end-to-end release-cycle benchmark.
The expanded inventory then passed in 59.594 seconds
(`target/v9-generated-manifests.log`), sealing 7,527 files with identical hashes.
Fresh Rust AST/graph checks, module structure, file headroom, repository
build/release contracts, Rust quality/docs and Rustfmt pass. The retry plan still
contains zero build/test replay; the refresh plan has four Cargo commands, one
isolated test owner and no duplicate builds (`target/v9-generated-closeout.log`).
Global cycle ownership, interrupted-attempt receipts and whole-candidate
cold/warm/recovery acceptance remain open.

### Proof composition checkpoint (2026-09-10)

`PrepareProofRelease` owns the evidence, baseline-diff and release-mode JSON
reports as one recoverable output set. Each has a private output variable,
declared final path and required schema. The preparation ledger seals all three
hashes plus the native log before any final file moves. Recovery preflights the
entire set, then finishes interrupted publication without rerunning the producer.
This is process-crash recovery under the preparation lock, not power-loss
durability or an atomic multi-file rename for unlocked readers.

The common owner protocol admits at most 16 reports, rejects duplicate or
overlapping destinations, path aliases and reserved/repeated variables, and binds
every output to graph dependency edges. Missing or wrong-schema additional
reports cannot replace previously published evidence. `ProofInputs` inventories
metadata-selected proof and compiler-manifest paths, including ignored files,
alongside tracked source, runtime reports and executable/tool identities.

Publication orders staged inputs before proof composition and candidate
readiness after it, avoiding both shared-lock contention and sealing reports
while the proof producer is writing them. Direct developer proof checks retain
their existing commands. No producer is added to read-only `make publish`.

`make release-preparation-proof-check` executes the real proof image in a
disposable Git fixture. Final evidence in `target/v9-proof-final-acceptance.log`
covers one cold launch, zero warm launches, repair of each corrupted report,
an actual killed producer and successful resume: six attempts total and identical
final evidence. It also includes the promotion/preflight self-tests, graph
admission and 11 prepared-transaction scenarios spanning partial publication,
corruption and changed inputs. All 14 proof slice traces remain unchanged; only
the reviewed code-bound baseline identity was refreshed.

Twelve parallel/keep-going Make scenarios and strict orchestrator Clippy pass in
`target/v9-proof-order-rust.log`. The full publication Make database has no
dependency-cycle diagnostics. `target/v9-proof-owner-staged-failure.log` records
an actual baseline failure writing only its private diff, preserving all three
final reports. Fixture directories and temporary probe files were removed.
Proof execution prerequisites and preparation-wide recovery remain open.

Final build/release contracts, Rust quality/docs, shard/resume contract and
formatting pass in `target/v9-proof-owner-final-quality.log`. Fresh AST/module
structure and 72-row headroom passed in `target/v9-proof-owner-quality.log`;
that earlier contract invocation correctly rejected the still-active partial
image and was rerun after compilation completed. Final proof/promotion images
are 11,295,656 and 19,952,960 bytes respectively. No file-size limit was relaxed.

### Focused proof image bootstrap (2026-09-10)

Proof composition now has one invocation per Make graph. The `closeout` command
constructs and canonically validates one evidence document, compares the accepted
baseline, exercises the existing seven adversarial/normalization checks against
independent clones, and derives the release-mode schedule from that same document.
`proof-readiness-release-mode-check` depends on this producer rather than
rebuilding the evidence. Standalone `check`, `self-test` and `release-mode`
commands retain their validation; there is no unchecked-document or skip flag.

The actual AOT closeout passes all 14 slices with evidence byte-identical to the
baseline-proposal path. Instrumented executable boundaries record exactly one
`rustc --version`, one `rustc -vV`, and one Git revision query, instead of three
sets from the previous three constructions. Logs are
`target/v9-proof-compose-accept.log` and `.probes`. The stale-baseline failure
probe rejects before producing a new release-mode schedule and preserves the
previous schedule bytes (`target/v9-proof-compose-reject.log` and `.probes`).
The accepted baseline identity was explicitly refreshed after reviewing that all
14 slice traces were unchanged; no theorem, lane or runtime requirement changed.

`tests/proof_composition_make.rs` executes the production recipes in seven
parallel/keep-going scenarios with an instrumented VM boundary. Both goal orders
share one composer; failed bootstrap, proof lane or composer prevents acceptance.
It passes alongside the existing proof dependency/bootstrap fixtures and strict
orchestrator Clippy (`target/v9-proof-compose-rust.log`). That first change
established in-process/Make composition sharing; the checkpoint above adds
cross-cycle recovery. Full-candidate acceptance is still required.

Fresh AST/module structure, 72-row file headroom, repository build/release
contracts, Rust quality/docs and formatting pass in
`target/v9-proof-compose-quality.log`. The rebuilt proof image is 11,277,200
bytes. Temporary probe wrappers were removed; their launch logs remain.

`terlan-proof-release-bootstrap` now owns the proof-evidence image, depending
only on the compiler and shared typed-validator input fingerprint. The full
`terlan-self-validation-bootstrap` aggregate includes that owner; proof artifact
closeout and focused baseline checking use it directly. A focused baseline check
no longer prepares the unrelated package, editor, release-promotion and other
validator images. No proof requirement or full-bootstrap member was removed.

The real parallel Make fixture in `tests/proof_bootstrap_make.rs` retains the
production dependencies and instruments image/leaf producers. Focused and full
entry points share a single proof producer in either goal order. All unrelated
bootstrap producers are absent from focused execution. Failed fingerprints or
image construction prevent baseline validation even with `-k`. The existing
proof aggregate/failure-order fixture also passes. Final focused fixture and
strict all-target Clippy logs are `target/v9-proof-bootstrap-graph-final.log`
and `target/v9-proof-bootstrap-clippy-final.log`.

The actual `make TERLAN_BUILD_ARTIFACTS_PREBUILT=1 release-proof-baseline-check`
rebuilt and sealed only the proof validator, then passed all 14 proof-evidence
records (`target/v9-proof-bootstrap-actual.log`, 86.806 seconds). The existing
compiler was reused; this is not a cold compiler build or a full proof replay.
No accepted proof baseline was changed. This narrows bootstrap ownership, not
completion of preparation-wide receipts or the remaining V9-1 acceptance paths.
An unchanged bootstrap then verified and reused that image in 0.369 seconds
(`target/v9-proof-bootstrap-warm.log`) without entering the compiler. The
structural, headroom, repository contract, Rust-quality/docs and release-plan
checks passed again. An additional shard/resume contract check exposed outdated
expectations for the removed skip-flag and recursive-composition design; that
failure is retained in `target/v9-proof-bootstrap-closeout.log`.

The shard/resume checker now requires live coverage admission, the exact owned
suite report, exported release scope and ordinary composer prerequisites. Its
target reader distinguishes target-specific variable assignments from dependency
declarations and combines split rules. The report remains static contract
validation, not evidence of actual cache reuse or interruption recovery; the
contract document now makes this limitation explicit. The rebuilt checker passes
the real repository in `target/v9-release-contract-actual.log`. Its first test
invocation selected the binary target, which contains no tests; that zero-test
run is not acceptance evidence. The corrected library target, run through the
existing zero-test guard, passes all 11 regressions in 0.06 seconds after harness
compilation (`target/v9-release-contract-library-tests.log`). These include
independent owner/report/scope/composer mutations and the real repository graph.
Strict library/quality-binary Clippy passes with warnings denied
(`target/v9-release-contract-clippy.log`). Final fresh-AST structural/headroom,
repository build/release, Rust-quality/docs, shard/resume-contract and Rustfmt
checks pass (`target/v9-release-contract-closeout.log`). This replaces the
previous static contract failure without reinstating unchecked skip flags.

During this rebuild, available filesystem space fell to about 500 MB. The owned
test container was paused while the prebuilt cache tool audited and pruned 63
obsolete incremental sessions, preserving all active/newest generations, source,
executables and evidence. Unique-inode allocation dropped by 6,917,439,488 bytes;
observed free space after cleanup was 9.1 GB and the container resumed. The audit
and prune reports retain a nonpassing budget status because one protected active
session could not be measured, not because it was deleted. These are explicit
maintenance records (`target/v9-release-contract-cache-{audit,prune}.json`), not
automatic build admission or proof that all Cargo storage is bounded.
After compilation ended, the read-only audit passed with zero unmeasured or
redundant sessions: 528 sessions and 57,579,528,192 allocated bytes within the
existing 64 GiB/1,024-session budget
(`target/v9-release-contract-cache-final.json`).

`target/v9-hosted-owner-wiring.log` records the actual production-rule fixture:
15 test bodies run once, overlapping source gates execute once, and altered source,
request environments, evidence, missing tests, swallowed failures and an active
checkpoint writer fail. Signature authenticity is mocked at the prior-download
boundary here and exercised separately by downloader tests. The local-input owner
retains its stricter eight-input comparison. No hosted workflow or full clean
candidate rehearsal has yet validated the uncommitted integration.

The production suite publishes `<report>.selections.json` from its already
admitted all/ignored inventories, before running test bodies. The companion is
bounded to 16 MiB, 100,000 retained name occurrences and 256 phases; it does not
raise the 1 MiB live-report ceiling or launch another inventory query. It binds
the suite run ID, compiled harness identity, union features and exact passed,
ignored and filtered selections per owned phase. Externally owned coverage
phases are not certified by this suite.

Closeout reconciles every main-harness phase with the shared selection identity
algorithm used by private libtest completion checks. Missing, extra, failed,
duplicate or forged completions fail. A changed companion document also fails.
Content verification uses the shared cancellation-aware regular-file reader,
with a single streaming hash and no retained second copy. The report carries
the companion path, content SHA-256, counts and verified state. Both documents
remain observations marked `reusable: false`: they are not yet current-input
coverage admission or permission to honor an `ALREADY_RUN` flag.

`target/v9-test-selections-verified.log` records 176 unit tests, two Make
integration tests and strict all-target Clippy. The real production rehearsal
in `target/v9-test-selections-production.log` verifies nine owned main-harness
selections, executes all 13 expected fixture bodies once, and preserves three
Cargo launches within 21 total direct launches. Its companion is 4,428 bytes;
an independent SHA-256 agrees with the live report. This fixture size is not a
measurement of the full repository suite.

In `target/v9-test-selections-tamper.log`, a real test changes the companion
after admission. Every body still passes once, but the suite exits unsuccessfully
with an unverified selection binding. `target/v9-test-selections-quality.log`
passes shared-metadata module/headroom/supply-chain checks, Rust size/test
placement, documentation, formatting and diff checks. Reports and sources are
retained in `target/v9-test-selections-evidence`; disposable builds are cleaned
after byte comparisons. No limits are raised and V9-1 remains unchecked.

### Rustdoc observation-hook boundary

The real Cargo differential in `cargo_rustdoc_test.rs` establishes a viable
configuration-level hook without installing a doctest `--test-runtool`. The
production doctest phase now uses this adapter and independently admits enabled
workspace library targets from the same metadata generation as native tests.

- Cargo selects the observation executable through `--config build.rustdoc`.
  An existing direct `RUSTDOC` would take precedence. Remove it from the Cargo
  command only, then restore the original effective child value through Cargo's
  `env.RUSTDOC` configuration. The child environment must retain both the value
  and whether it was absent; do not replace absence with an empty string.
- Preserve the existing scalar/table configuration shape. Cargo rejects an
  inline table passed as a single `--config key=value` argument. Table settings
  instead use separate dotted `value`, `force`, and `relative` keys. A resolved
  relative value must not be made relative to the hook's directory again.
- The observer must delegate to the originally admitted Rustdoc invocation,
  not rediscover it from its own environment. Its subprocess inherits Cargo's
  actual package context; the hook must not replace the generated doctest runner.

Six baseline/observed pairs cover default selection, `CARGO_BUILD_RUSTDOC`, direct
`RUSTDOC`, forced child settings, scalar child settings, and relative child
settings. Every pair preserves two identical unique nonfresh Cargo compiler-unit
identities. Each cycle runs the build script once and its two isolated doctest
bodies once. Compile-time and runtime environment assertions agree. The initial
passing observations are in `target/v9-rustdoc-hook-tests.log`; full-suite and
quality closeout are recorded separately in `target/v9-rustdoc-hook-closeout.log`.
These counts do not yet inventory Rustdoc's nested compiler/linker processes.

Package admission must use Cargo's resolved workspace/target inventory, not a
handwritten approximation of workspace members and implicit path dependencies.
That metadata needs one preparation-wide owner shared with Rust-quality and SBOM
consumers; loading an old unbound JSON file or adding another equivalent metadata
query does not satisfy the requirement. The production integration now records
one observer and one original Rustdoc process per admitted target, reconciles
their exact completion records, and rejects missing targets even after a zero
Cargo exit. An independently admitted empty target set needs no doctest Cargo
launch. No Cargo budget or checklist completion is changed.

`target/v9-rustdoc-owner-verified.log` records strict all-target Clippy, 173 unit
tests and two Make integration tests. The six-configuration differential now
uses the production configuration adapter. Target tests reject missing policy,
duplicate targets, outside sources, wrong editions, and changed runtools.
Completion tests reject mismatched contexts, targets, processes, and harness
counts. Cross-filesystem targets use a bounded, exclusive observer copy when a
hardlink is unavailable. Ordinary failures retain available process records.
`target/v9-rustdoc-owner-final-rehearsal.log` records the real driver executing 13 unique
fixture bodies exactly once, including two isolated Rust 2024 doctests, without
adding Cargo launches. This admits packages, not an independently enumerated
doctest-case inventory or Rustdoc's internal compiler/link subprocesses.
`target/v9-rustdoc-owner-empty.log` verifies the independently admitted empty
target case: two Cargo launches, no doctest phase, and the exact remaining 11
bodies once each. `target/v9-rustdoc-owner-quality.log` passes shared-metadata
module/headroom/supply-chain checks, Rust size/test placement, documentation,
formatting and diff checks. No limits or allowances changed. Reports and fixture
sources are retained under `target/v9-rustdoc-owner-evidence`; disposable fixture
builds are removed after comparing retained evidence.

### Shared Rust-quality metadata and file-growth closeout

`CargoMetadataReceipt` now admits the completed native producer handoff before
workspace projection. It checks the output SHA-256, matching nonempty attempt
identity, successful attempt state, exact workspace/version, matching launch
records, and completed source/configuration/executable/resolver-cache observations. Rustup
dispatch additionally requires the selected native Cargo binding. Failed,
unfinished, corrupted, and mismatched handoffs fail without launching Cargo or
modifying either record. Reading the attempt before and after the document
rejects an observed concurrent refresh. File-size prechecks do not provide an
atomic snapshot or replace enclosing process memory limits.

This is deliberately not current-source admission: the consumer does not
re-observe transitive resolver inputs or authorize cross-cycle reuse. It does
not authenticate a deliberately forged matching document/attempt pair. Those
boundaries remain with the preparation-wide owner.

The fresh AOT build, metadata self-test (including real file rejection cases),
new-module lint, and formatting checks passed in
`target/v9-metadata-handoff-verified.log`. The terminal command was quiet on
success. `target/v9-handoff-probe-accepted.log` records a real native producer's
output accepted by the typed reader. `target/v9-handoff-probe-rejected.log`
records a deliberate manifest/lock mismatch: Cargo fails with `--locked`, the
prior metadata remains byte-identical, and the typed reader rejects it because
the new attempt failed. No successful Cargo query is repeated for this failure
case. Existing `CargoMetadata` lint diagnostics remain; the new reader is clean.
The final module lint has 16 existing diagnostics versus 18 in the HEAD-source
baseline; no new diagnostic category was introduced.

`target/v9-metadata-handoff-quality.log` records the actual Make lint-allowance,
module-structure, and file-headroom targets passing together, including their
self-tests and final diff check. The run reused the already verified native
tools and freshly built typed validator, without another compilation. Its one
metadata attempt recorded four direct PIDs (Rustup resolution, source-before,
Cargo metadata, source-after), and an independent SHA-256 check matched the
published document. The headroom check retained all 73 no-growth schedules.
This scoped consumer rehearsal does not validate the bootstrap receipt or the
full release preparation cycle. The 22 MiB probe build tree was removed after
copying and comparing its sources and success/failure records; the retained
64 KiB evidence is under `target/v9-handoff-probe-evidence`.

The original probe's script-entry inference failure is now fixed. Application
normalization closes a generated dynamic result using the existing call-aware
type recovery. It changes only the normalized clone's result signature, not
the checked source/interface contract or the control-flow body. Unknown,
conflicting, and ungrounded cyclic results remain unresolved. The shared
recovery also forgets a shadowed variable's old type when its replacement is
unknown. An earlier outer-cast prototype was rejected because it hid a
suspending structured case; that prototype is not the final implementation.

The investigation also exposed gaps in closed-image call admission. Admission
now shares the exhaustive expression walker with call analysis, including casts,
collections, case guards, and nested callable references. An unimported provider
cannot evade admission by appearing inside these expressions.

`target/v9-dynamic-admission-tests.log` passes all 454 NativeIR tests, reconciled
against the exact independent harness inventory. This includes the direct-call,
managed-result, grouped-guard, ambiguity, cycle, shadowing, and nested-admission
regressions. Strict library Clippy, the production compiler build, and formatting
pass in `target/v9-dynamic-admission-production.log`. Fresh structural analysis,
documentation, build-graph, and unchanged 73-file headroom schedules pass in
`target/v9-dynamic-admission-quality.log`, with one fresh metadata query.

The byte-identical original script now builds and executes without the assertion
workaround: `target/v9-dynamic-admission-probe.log` records `true` for the valid
handoff and `false` for a missing handoff. The no-argument run records `false`
from the grouped guard's fallback in
`target/v9-dynamic-admission-probe-no-args.log`. Its native image is 319,568 bytes.
These are scoped compiler/consumer regressions, not a full candidate rehearsal.

The lint-allowance, module-structure, and file-headroom Make targets now share
`rust-cargo-metadata-report` with the existing dependency/build-graph consumers.
The initial shell producer wrote a unique temporary file, preserved the preceding
report on failure or empty output, and atomically installed successful nonempty
output. It explicitly requested locked Cargo resolution and stripped inherited jobserver
descriptors from this metadata-only command. The root Makefile already selected
locked Cargo; this is not a claim that the preceding root build was unlocked.

Source inventory reads the shared document instead of launching its own Cargo
query. Workspace packages are selected by exact `workspace_members` IDs, with
version, count, duplicate, and missing-member checks. Registry packages and
excluded local packages are not mistaken for members. This is one producer per
Make invocation, now also shared with the SBOM and test orchestrator. Suite
admission additionally compares its own observations as described below; this
is not yet a complete input-bound preparation-wide reusable receipt.

The production producer is now the existing Rust test orchestrator's early
`--cargo-metadata <output> -- <cargo>` mode, before test setup or execution.
Compiler bootstrap builds that binary in its existing Cargo invocation; prebuilt
bootstrap checks it is executable. The mode accepts the Cargo executable and
an optional `--locked` prefix, not arbitrary Cargo overrides. Its bounded child
capture admits Cargo's version, workspace root, and unique package/member IDs,
then verifies source, configuration, and entry-point byte observations before
atomic publication. The 16 MiB Cargo-output and 32 MiB document budgets are
explicit; ordinary suite reports retain their existing 1 MiB budget.

`terlan_preparation` records the query, run identity, effective environment
digest, and actual direct Git/Cargo launch PIDs. The sibling
`.attempt.json` is atomically updated before work, after every observed launch,
and at completion. Failed refreshes retain the previous successful metadata
document but leave a distinct failed attempt, including available launch records.
An unfinished attempt remains `running`, never successful. These are latest
observations, not a historical attempt archive or proof of SIGKILL orphan cleanup.
Both record types explicitly set `reusable: false`: transitive resolver inputs,
wrapper-internal tools, and preparation-wide consumer admission still need
binding. The supply-chain consumer now uses a fresh Make prerequisite, described
below; these observations still authorize no test-suite skip or cross-cycle reuse.

The Cargo-home resolver observation covers registry index and Git database bytes,
registry/Git checkout filenames, and manifest/configuration/checksum contents.
Ordinary Rust source bodies are not hashed by this metadata-only observation;
adding or removing auto-discovered target paths changes its digest. The owner
checks before and after the query, bounds entries to 262,144, pathname bytes to
32 MiB, content bytes to 512 MiB, and depth to 64, and observes cancellation.
Nested symlinks and special files are rejected; the selected Cargo-home location
is bound. First-use cache population can invalidate the observation: this is
not yet an isolated resolver cache or the complete workspace/vendor input closure.

`target/v9-resolver-cache-tests.log` passes 165 unit and two Make integration
tests plus strict all-target Clippy. Cache mutations, missing caches, canonical
location changes, limits, and cancellation are covered. The typed reader's fresh
build/self-test passed in `target/v9-resolver-cache-typed.log`. The repository
query took 2.862 seconds in `target/v9-resolver-cache-repository.log`, observing
102,400 entries and 92,722,715 content bytes per snapshot. This measures metadata
production, not overall preparation speed.

### Resolved package source observations

The one shared metadata query now uses `--all-features`, covering optional
dependencies needed by the union-feature test build without compiling them.
Suite admission supplements its Git and Cargo-resolver observations with each
resolved package root and any declared target source outside that root. Nested
package roots are collapsed so their files are traversed only once. Files already
hashed by the Git/resolver owners are referenced, not rehashed. Those original
owners must still pass their own closeout checks.

The existing resolver tree walker supplies bounded traversal and regular-file
hashing: 262,144 entries, 32 MiB of names, depth 64, and an explicit 8 GiB package
content budget, without raising the resolver's separate 512 MiB budget. File
links bind their destination and bytes; directory links fail rather than expand
an undeclared tree. Cargo target storage, root `.terlan` storage and Git internals
are excluded. Explicit Rust target sources inside excluded output storage fail.
Source additions, deletions, byte changes and declaration retargeting invalidate
suite closeout. Cancellation is checked during declaration discovery and reads.

`target/v9-package-sources-verified.log` passes 182 unit tests, two Make integration
tests and strict all-target Clippy. A real metadata-owner fixture selects an
optional ignored path dependency without building it and rejects its mutation.
`target/v9-package-sources-production.log` runs the actual suite with an ignored
file consumed by `include_str!`: 13 expected bodies once, three Cargo launches,
and one supplemental 12-byte file hashed while ten Git-owned inputs are referenced.
This is not yet a full repository package-source rehearsal or a reusable receipt.
Arbitrary build-script inputs, workspace discovery outside resolved packages,
external SDKs, and wrapper-internal tools still require ownership.

The live mutation rehearsal in `target/v9-package-sources-mutation.log` changes
that ignored input after every relevant build. All 13 test bodies pass once,
and both Git and resolver closeout remain verified, but the supplemental source
identity changes and the suite rejects success. Reports and source fixtures are
retained under `target/v9-package-sources-evidence`; disposable builds are removed
only after comparing the retained bytes.

`target/v9-package-sources-quality.log` passes shared-metadata module/headroom,
supply-chain, size/test-placement and documentation checks. The real all-features
query resolves 511 packages, compared with 490 in the earlier default-feature
document. The final immutable-declaration digest is cached once, not recomputed
on each live report update. `target/v9-package-sources-closeout.log` passes all
184 orchestrator tests, strict Clippy, a fresh structural AST observation,
Rust quality/documentation, formatting and diff checks after that optimization.
No existing limits or allowances changed.

### Rust-suite metadata admission and shared bootstrap

The canonical Rust suite now requires the fresh Make metadata prerequisite.
It reads one bounded document bracketed by its attempt, validates its hash,
generation, locked query, and completed observations, and compares source,
Cargo/Rustup configuration, selected Git/Cargo/Rustup/native-Cargo bytes, effective
metadata environment, and the current resolver cache with suite admission.
Missing or changed inputs fail before the harness build. Both the main harness
and streamed native workspace harnesses must identify a name/manifest pair from
the independently selected Cargo workspace membership. No license projection,
JSON serialization, or metadata subprocess is replayed by this native reader.
The suite retains a small package index and generation digest in its report,
then verifies its resolver-cache observation at closeout alongside source/tools.

Metadata commands remove Make jobserver variables, the suite's report path,
coverage-owner/thread/timeout settings, and shell bookkeeping `SHLVL`/`_` from
their actual environment before hashing or execution. The consumer constructs
the same environment; a different registry selection is not ignored. This is
conservative same-input handoff admission, not complete transitive provenance:
ignored workspace discovery, external vendored inputs, wrapper internals,
immutable snapshots, and cross-cycle successful-test reuse remain open.
Doctest cases remain emitted-harness evidence. The production Rustdoc observation
hook now independently reconciles the metadata-admitted library targets.

`target/v9-suite-metadata-tests-final.log` passes 167 unit tests and two production
Make integration tests, plus strict all-target Clippy. The integration fixture
now includes both actual suite/SBOM prerequisite declarations under `make -j4`:
four consumers share one query; failed or empty refreshes invoke neither consumer
on retained output. Native reader tests exercise changed source/environment/cache,
generation/hash/attempt mismatches, package/path rejection, cache closeout failure,
and rejection of repeated closeout without another Cargo metadata launch.

`target/v9-suite-metadata-rehearsal.log` executes the production driver on a tiny
real two-package workspace. All nine selected main-harness marker bodies, two
support-crate bodies, and two isolated Rust 2024 doctests execute exactly once
(13 names reconciled independently with the fixture and tier declarations).
The suite takes 6.303 seconds, observes three Cargo and 21 direct launches, and
records three native targets with 12 nested launches. The preceding shared
metadata producer has its own four launch records; it is not hidden inside the
suite's three-Cargo count. Its exact output digest is
`c210ed57baac117cd8461936bbf3562c7bebc1e5a0418e1bc919ad110a69335a`.
Unused prebuilt runtime stubs make this a driver/ownership rehearsal, not evidence
that the named production capability, C++, or EPMD tests ran.

`target/v9-suite-metadata-changed.log` then adds a source file without refreshing
metadata. The real driver rejects it with zero Cargo launches and unchanged
test-body records. No successful metadata query or correctness body is replayed
for that rejection case. The complete success and failure reports are retained
with fixture sources under `target/v9-suite-metadata-evidence`.

Compiler bootstrap now builds the native worker in the same invocation and is
the normal native-worker target's prerequisite. The Rust suite consumes that
bootstrap rather than owning a second equivalent build. The fresh repository
validator distinguishes the declared suite command from metadata mode, binary
availability checks, and compilation text. Its fixture cases preserve duplicate
suite rejection. `target/v9-suite-metadata-formatted-contract.log` passes the
fresh AOT build, self-test, formatting, and complete build/release contract gate.
The plan reports five literal Cargo commands against its unchanged limit of six,
one suite command, and zero duplicate equivalent builds. These remain plan
observations, not complete accounting of nested Cargo/tool invocations.

`target/v9-suite-metadata-quality.log` passes the real SBOM, module structure,
headroom (73 unchanged schedules), structural/documentation baselines, canonical
build graph, Rust formatting, and diff checks. The main source handoff uses one
fresh metadata producer; preparation-wide accounting must still include implicit
queries made by Cargo subcommands such as formatting. No end-to-end candidate
rehearsal or V9-1 closeout is claimed here.

For an admitted Rustup proxy, the metadata owner now uses the same bounded
`rustup which` path admission as the Rust suite, preserving each caller's own
command environment and recording the additional resolution PID. The probe
disables auto-installation. The native Cargo binary is byte-bound before the
metadata query and rechecked afterward, including alias retargeting through the
shared executable binding. A supplied nonproxy entry point is labeled as such;
its internal dispatch is not inferred or claimed to be bound. Successful attempt
records also contain the SHA-256 of the exact atomically published metadata;
failed or unfinished attempts cannot supply that successful-output binding.

`target/v9-metadata-native-final.log` passes 161 unit and two production Make
integration tests with strict all-target Clippy. The added proxy fixture changes
the native Cargo file outside the source inventory and verifies rejection while
preserving the preceding document; malformed Rustup resolution fails before the
metadata query. Real Make tests verify the successful output hash and its absence
on failed refreshes. An initial test-only digest-formatting compile error is
retained in `target/v9-metadata-native-closeout.log`; the corrected final run is
the passing evidence, not that earlier failed command.
The real repository run in `target/v9-metadata-native-repository.log` then passed
build-graph, headroom, structural, documentation, formatting, and diff checks
using one metadata query. Its attempt records one Rustup resolution PID, the
Cargo metadata PID, and two source-inventory Git PIDs. The selected Cargo was
the installed `1.96.0-x86_64-unknown-linux-gnu/bin/cargo`, with matching pre/post
byte identities; an independent `sha256sum` matched the attempt's metadata
digest. This is direct-launch and output evidence, not nested subprocess
coverage, transitive dependency admission, or a reusable full-suite receipt.

The actual production CLI now has Cargo-built integration tests driving parallel
Make consumers with real and adversarial Cargo commands. The latest run in
`target/v9-metadata-attempt-closeout.log` passes 160 unit and two integration
tests plus strict all-target Clippy. It verifies failed/empty query accounting,
source mutation rejection, unfinished observations, and unchanged successful
metadata after a failed locked refresh. Before attempt journaling was added,
`target/v9-native-metadata-consumers.log` exercised one real repository query,
all 11 workspace members, and the build-graph, file-headroom, structural,
documentation, formatting, and diff gates successfully. This establishes actual
consumer compatibility, not full preparation acceptance or warm-cache reuse.
After the attempt-recording change, `target/v9-metadata-attempt-quality.log`
passes fresh AST/build-graph and headroom checks, zero lint allowances, module
structure, zero oversized/inline-test files, zero undocumented items, formatting,
and diff checks. These consumers shared the already produced package/target
inventory; no second equivalent Cargo metadata query was launched for them.

Two real Make tests cover parallel consumers, failed/empty refresh preservation,
temporary-file cleanup, and Cargo rejection of a manifest requiring a lockfile
update. All 157 orchestrator tests, strict Clippy, and the production build pass
in `target/v9-metadata-refactor-rust.log`. The real repository consumer rehearsal
in `target/v9-metadata-consumers.log` launched Cargo once; a wrapper rejected any
second query. Lint and module checks passed, but the headroom check correctly
failed. That initial failure is retained, not rewritten as a passing rehearsal.

The follow-up corrects nested `_test`/`_tests` directory classification without
classifying production `commands/test` code as tests. It separates declarative
orchestrator phases, process responses, dispatch errors, checked-field lowering,
lexical renaming, union-pattern plans, call formatting, constructor checking,
opaque-alias inspection, and CoreIR contract rendering into focused modules.
Existing public entry points and behavior are retained. Obsolete headroom rows
are removed only for files below the warning threshold; remaining ceilings are
lowered to actual size, never raised. Strict union-feature library Clippy passes
in `target/v9-headroom-refactor-clippy.log`. The headroom, fresh AST/build-graph,
Rust size, documentation, and diff gates pass in
`target/v9-headroom-refactor-quality.log`.

The library test harness was rebuilt once with the union feature set, then run
directly over NativeIR, typechecking, expression formatting, and native dispatch:
1,427 passed, zero failed, one existing ignore, and 5,926 outside-scope tests
filtered out in 11.12 seconds. `target/v9-headroom-test-build.jsonl` identifies
the built artifact; `target/v9-headroom-test-inventory.txt`,
`target/v9-headroom-test-results.txt`, and `target/v9-headroom-tests.log` retain
inventory, per-test results, and terminal outcome. This is focused refactor
validation, not a full release run or a reusable preparation receipt. The
completed metadata rehearsal's 4.3 MiB disposable tree was removed only after
its single-call record and wrapper source were copied and compared.

### Supply-chain admission and same-Make metadata sharing

`release-supply-chain-provenance-check` now explicitly depends on the same fresh
`rust-cargo-metadata-report` as the Rust-quality gates. The typed supply-chain
loader admits its completed attempt, projects licenses from the parsed document,
and serializes that document for the SBOM; it neither launches a private Cargo
query nor reparses the document to check licenses. Invalid handoffs fail before
candidate extraction. A direct invocation still needs a fresh producer run;
this dependency is not yet a preparation-wide reusable receipt.

`target/v9-sbom-sharing-rust.log` passes strict all-target orchestrator Clippy
and both production Make integration tests. The tests now include the actual
supply-chain prerequisite declaration, request three consumers under `make -j3`,
observe one query, and verify a failed refresh cannot invoke the SBOM stand-in.
`target/v9-sbom-sharing-typed.log` records the fresh release-closeout AOT build,
metadata self-test, lint-clean metadata module, and both changed modules' format
checks. String and JSON error payloads are handled separately rather than
propagated through a heterogeneous grouped fallback.

The real repository rehearsal in `target/v9-sbom-sharing-repository.log` passes
the complete supply-chain provenance Make gate alongside module structure and
file headroom, using prebuilt tools and freshly rebuilt typed images. Its one
metadata query has PID 23; the attempt also records Rustup resolution and two Git
inventory PIDs. Provenance reports 11 licensed workspace packages and 30 reviewed
unsafe files. The SBOM and producer output are byte-identical in this rehearsal,
with SHA-256 `32162ea97eba40ba145d23befdc047f0d803a93ba797ec7ac5c3d0f16517896b`.
Structural/documentation baselines, Rust formatting, and diff checks pass.
This is not a clean-candidate release or cross-cycle recovery rehearsal.

`target/v9-sbom-sharing-probe.log` executes the actual compiled supply-chain
loader on a one-package native producer result and a missing handoff.
`target/v9-sbom-sharing-failure.log` changes that fixture's manifest without its
lockfile: the locked refresh fails, the previous document stays byte-identical,
and the compiled consumer rejects its failed attempt. No fixture-success query
is replayed for this rejection case.

The supply-chain license check previously searched JSON text for non-null
`license` fields while claiming every workspace package was Apache-2.0. It now
uses the same exact Cargo member projection as Rust-quality, validates the
workspace root, and checks each member's typed name and Apache-2.0 license.
Dependency licenses cannot supply or invalidate a workspace member's license.
Malformed, empty, duplicate, missing, and cross-workspace membership fails.
The common projector also rejects empty package IDs. Release-closeout's typed
validator fingerprint includes its newly explicit Rust-quality path dependency.

The supply-chain owner hashes each checksum input once and uses those same
values for both validation and its report, including the release archive.
It no longer advertises a stale-input test based only on a missing-file check.
Its early failure paths clean the installed candidate; preparation itself now
cleans its allocated directory when extraction or workspace setup fails.

`target/v9-sbom-final-quality.log` records the rebuilt release-closeout image,
typed adversarial self-test, lint-clean new metadata module, and diff check.
`target/v9-sbom-probe.log` validates the existing real metadata's 11 workspace
licenses and compares temporary-root contents before and after failed extraction.
An initial empty-root assertion was incorrect because the running VM keeps its
sealed image there; the corrected probe preserves that live artifact. The
diagnostic run is retained separately. These are scoped checks, not a complete
supply-chain/candidate run. Package-wide lint remains nonzero; its existing
findings are retained in `target/v9-sbom-lint.log`.
The changed preexisting files retain the same 36 diagnostics after normalizing
line locations; the complete baseline comparison is retained in
`target/v9-sbom-lint-before.log` and `target/v9-sbom-lint-before-complete.log`.
`target/v9-sbom-shared-metadata.log` records a fresh Rust-quality AOT build, its
metadata self-test including empty-ID rejection, formatting, and diff checks.

The supply-chain consumer now shares the fresh Make-owned metadata producer with
Rust quality and the suite; it rejects retained output after a failed refresh.
Rustdoc library targets also consume that admitted generation. Preparation-wide
reusable receipts remain incomplete, and V9-1 remains unchecked.

## Hosted-download retention

Preparation now downloads compiler coverage separately from release reports and
distribution payloads. It queries the exact successful compiler job to establish
its run, source SHA, status and producing attempt. A signing-only workflow retry
may have a later overall attempt without requiring successful compiler tests to
run again. The CI signing job downloads the producer's immutable artifact ID,
not an artifact name reconstructed from the signing job's attempt.

On cold preparation, each of the three coverage records must pass
`gh attestation verify` for the repository, `.github/workflows/ci.yml`, source
commit and `refs/heads/main`. The prebuilt historical reader then validates the
bundle against the authenticated producer coordinates. Both cold and warm paths
run this bounded reader; a warm path reuses previously authenticated unchanged
bytes under the complete cache manifest, while live producer API checks remain
mandatory. Changed verifier implementation is not replaced by a cached JSON
success. Release report downloads cannot overwrite the private expected-context
file or compiler coverage directory.

The cache retains coverage in its own `coverage/` directory. The candidate record
binds its cache key, producer job/attempt and checked file digests; the original
local Rust suite report is never overwritten. This downloaded historical
evidence does not authorize local rebuilt-binary test reuse. Publication's live
coverage authority still needs integration before its old skip-flag call sites
can pass. Local fixtures exercise downloader failures and retry ordering with
explicit GitHub/reader stubs; they do not certify real GitHub signatures or a
hosted run of the uncommitted workflow changes.

On 2026-09-09 the full rebuilt promotion validator self-test passed, including
bad producer attempts, failed signatures, rejected coverage records, cold
failure after cache publication, warm reuse, signing-only retry, locks, failed
CI and corrupt cached bytes. Warm and signing-only retries perform live producer
checks and record inspection but no artifact downloads or repeated attestations.
The signing workflow passes pinned actionlint; the Rust change described below
passes strict Clippy, repository build/release contract, module/file-growth,
supply-chain, Rust quality and Rust documentation gates.

Rebuilding the complete validator exposed a canonical receiver-call admission
defect: typed receiver lowering generated `std.data.Json.get`, but closed-image
admission incorrectly demanded an additional source import while the application
resolver already accepted that fully qualified public identity. Admission now
uses the same public-qualified versus imported-unqualified distinction. All 455
NativeIR tests pass: 29 admission tests followed by the other 426 through the
same compiled harness, explicitly excluding the already-run admission tests.
They include rejection of private, unqualified and wrong-module calls. The full
promotion image now compiles and executes its self-tests. Neither a
synthetic import nor a skipped admission check was added to the Terlan fixture.

`publish-prepare` runs `publish-cache-prune` after obtaining verified inputs and
bootstrapping the promotion validator. `publish` and its retries do not prune,
download, or rebuild. The downloader records the exact cache generation in the
candidate's checksummed `download-cache-key`; pruning rejects missing keys and
generation metadata belonging to a different revision.

Cleanup holds the downloader's exclusive `target/publication-inputs.lock`.
Its default budget is 8 GiB and three generations, with a seven-day maximum age
and five-minute creation grace. The selected generation, recent generations,
and future-dated generations cannot be evicted. If those alone exceed the
budget, planning fails without evicting them. Inventory is limited to 1,024
generations; malformed identities, duplicate entries, corrupt context hashes,
and symlinked cache roots fail closed.

Only `target/publication-downloads` is eligible. Source, `dist`, candidate input
copies, and owner ledgers are never cleanup targets. Each retirement revalidates
its measured generation and atomically renames it to `.retired-<sha256>` before
deletion. A retry drains these durable retirement names even if an earlier
deletion removed their metadata. It also recognizes the downloader's exact
`.partial.<six-alphanumeric-characters>` staging names while holding the lock;
unknown residue is rejected, not removed through a broad pattern.

The ordinary promotion self-test covers byte/count/age decisions and disposable
filesystem fixtures for cold/warm cleanup, interrupted staging and retirement,
candidate binding, corrupt identity, unknown residue, and symlink escapes.
These passed on 2026-09-07. No existing candidate's real cache was pruned during
this validation. This is a hosted-download policy, not completion of retention
for compiler caches or old candidate input copies.

## AOT continuation correctness

Executing the retention validator exposed three independent lowering defects:
non-tail recursive suspension flattened unbounded completion frames; mixed
grouped fallbacks tried to project impossible closed-union arms; and eager yield
captures used a variable-map count instead of the next physical native slot.
The fixes preserve VM-owned completion frames, reject only provably disjoint
patterns, and reuse the existing native-slot allocator. Recursive scheduler
entries propagate through direct and indirect callers without relaxing native
transition-buffer admission. Ordinary optimized tail loops remain unchanged.

Linked tests exercise scalar and mutual recursion, zero-capture completions,
managed captures with collection at every yield, and indirect callbacks. Closed
union tests keep unknown and nominal alternatives conservative. The sparse-slot
regression fails before its fix and passes afterwards. All 435 NativeIR tests,
strict library Clippy, and the rebuilt promotion self-tests passed with these
changes. This does not replace full candidate validation or certify every
higher-order recursion shape.

Explicit lambda parameter types now survive parsing, formatting, syntax output,
CoreIR, specialization, and AOT lowering. Incompatible callback annotations are
rejected; conditionally selected local closures execute through the managed ABI,
while statically known lambdas keep their allocation-free path. Generated
functions retain explicit declaration provenance for packaged debugger records.
Packaged and linked images share sorted callable-admission metadata. The focused
closeout passes 436 NativeIR tests, four packaged debug tests, 796 typechecker
tests, syntax/JS regressions, strict Clippy, and the pinned parser-shape Lean proof.
The rebuilt promotion self-tests pass, with its image unchanged at 7,560,088 bytes.
This remains compiler/preparation-slice evidence, not full-candidate closeout.

The process helper now creates a request-owned process group on Linux/macOS.
It terminates remaining group members before reaping the direct child, including
normal child exit, so background descendants cannot retain its output pipes.
Keeping the leader waitable until group termination prevents PID reuse from
targeting a different request. Scope exit also owns child cleanup during unwind.
Framed protocol writes run separately from the timeout controller: a child that
does not read stdin cannot block that controller.

Linux/macOS pipe ends are nonblocking and use OS readiness waits. Their scope
carries the original request deadline and cancellation signal through input
writing and final output draining, even after the direct child exits. Overflow
during draining stops idle pipe workers, and scope unwind interrupts them.
Already-cancelled requests do not launch a child. A separate-group peer keeping
a pipe open therefore cannot make these joins outlive the request deadline;
this bounds the operation but does not by itself terminate that peer.

Git source-identity subprocesses now use the same byte-exact capture machinery,
with closed stdin, disabled terminal prompts, a 30-second deadline per command,
and a combined 64 MiB output ceiling. Raw NUL and non-UTF-8 bytes are retained;
source fingerprints do not pass through the text-oriented public process API.

Compiler tool capture now uses that same process owner instead of maintaining
a second polling loop. The former CLI runner waited for exit before draining
stdout/stderr, which could stall a verbose child on a full pipe. Native AOT
linking now uses bounded capture too: a five-minute execution/drain deadline,
closed stdin, and a combined 16 MiB diagnostic ceiling. Failures identify the
linker and target output; failed temporary images remain covered by the existing
cache-file scope. Command arguments, environment, working directory, and output
remain OS-native/byte-exact. The WebAssembly command runner shares this path.

On 2026-09-08, eight new CLI tool tests passed for pipe-buffer-sized output,
closed stdin, native arguments/environment/directory, exited descendants,
timeouts, combined output limits, invalid bounds, and attributed spawn failure.
The two existing CLI timeout tests, ten WebAssembly runtime tests, and all
nineteen shared process tests also passed; strict library Clippy passed. Logs:
`target/v9-bounded-tools-build.log`, `target/v9-bounded-tools-tests.log`, and
`target/v9-bounded-tools-closeout.log`. One initial process-test selector matched
zero tests and was corrected to the actual runtime namespace; it is not counted
as evidence. These tests cover execution behavior, not actual-launch inventory
or whole-preparation deduplication.

The rebuilt compiler also compiled and executed the 14,952-byte standalone
script fixture. Its unchanged `--incremental` invocation succeeds with
`TERLAN_NATIVE_CACHE_MISS_POLICY=error`; a `/bin/false` linker fails without
publishing an image, and the original linker still reuses and executes its
verified image afterward. `target/v9-bounded-link-recovery-final.log` records
this rehearsal. The initial warm command omitted `--incremental` and correctly
hit the explicit cache-miss guard; it was corrected, not treated as a passing
reuse check. Rust size/documentation gates, formatter, and whitespace checks pass.

This is not a security sandbox or a complete signal-cleanup guarantee. Separate
sessions/groups (including process groups created by a nested VM), forced
termination of the VM itself, and Windows process-tree ownership still require
explicit treatment.
Preparation-wide interruption recovery and cache budgets remain open. The
Linux process suite passes 19 cases covering framed sequencing/validation,
normal exit with inherited pipes, blocked-input timeout, cancellation, output
overflow, byte-exact capture, actual descendant termination, and panic cleanup
without affecting another owner. Trailing frames remain captured after the last
input phase. The Terlan Process/Git tests and both preparation recovery fixtures
also pass with the new process-group runtime. macOS execution has not been
certified here. An isolated compile probe of the production process modules
passes for macOS ARM64 and Windows GNU with warnings denied; it is not a full
cross-platform compiler build or runtime certification.

The synchronous `process-wrap` 10 Job Object adapter was inspected and not
adopted: it creates the job with kill-on-close disabled, so that adapter alone
does not meet forced-VM-termination cleanup requirements. A Windows solution
must establish ownership before child execution and retain a kernel-backed
kill-on-close lifetime; normal Rust `Drop` cleanup is insufficient for forced
termination. See the [upstream synchronous adapter](https://github.com/watchexec/process-wrap/blob/v10.0.0/src/std/job_object.rs)
and [job creation policy](https://github.com/watchexec/process-wrap/blob/v10.0.0/src/windows.rs).

The managed-list owner additionally fingerprints the Rust sysroot, the selected
`rustc`, `cargo`, and native-tool executable bytes, Ubuntu/Debian package
identity, and Cargo configuration from the home and ancestor directories.
Absent configuration files participate in the key,
so subsequently creating one invalidates reuse. This owner targets the existing
Ubuntu-compatible Linux x86-64 publication environment.

Both owners fingerprint the Git, environment, move, and shell executables used
by their control path. The report-only owner does not hash a Rust sysroot or
launch Cargo: its producer is the prebuilt VM and platform-matrix image, whose
bytes are explicit dependencies.

## Multicore report composition

`vm-multicore-publish-evidence-refresh` still runs its complete preceding local
gate graph. Publication invokes the same `vm-multicore-publish-prerequisites`
before the shared local-report graph; standalone multicore refresh retains its
single report owner. Publication's four contract self-tests now have separate
owners as described above; the other local gates still execute through Make.
A successful report checkpoint must never be used to skip those gates without
separately checkpointing their producers.

The owner fingerprints all tracked source files, the hosted candidate record,
memory-model and ThreadSanitizer reports, and the prebuilt VM/image. It rejects
dirty source and unsupported tool paths before composition. A changed hosted
attempt or changed evidence invalidates reuse, even if the resulting report is
byte-identical. The composer writes into private owner scratch through
`TERLAN_MULTICORE_RELEASE_OUTPUT`; normal non-preparation invocations keep the
existing default output path. Read-only publication verification is unchanged.

Run its explicit, bounded Linux fixture rehearsal with:

```sh
make release-preparation-multicore-check
```

This executes the real prebuilt composer against deliberately constructed test
evidence. It covers cold/warm reuse, dirty source, changed hosted attempts,
changed and failed evidence, corrupted images, and successful recovery while
preserving the previous report. It does not rerun or certify a sanitizer or
memory-model suite, and never modifies the original VM/image or repository
history.

## Verification

The promotion validator's ordinary `self-test` includes deterministic owner
tests for warm reuse, source/dependency/producer changes, interrupted state,
corrupt reports/ledgers, failure, timeout, killed producers, invalid schemas,
zero-test execution, Cargo configuration changes, and interrupted pending writes
beside an already completed ledger.

The separate `preparation-candidate-self-test` command exercises the actual
Cargo-backed owner against a disposable, dependency-free Git project. It checks
cold execution, unchanged warm reuse with no Cargo replay, and dependency-driven
reexecution with equivalent output. It also kills the fixture's actual Rust test
process, checks that the previous report survives, and resumes to an equivalent
report. The test fixture owns these intentional producer launches; they are not
duplicate repository validation.

Re-running this gate on 2026-09-10 exposed an obsolete final assertion: it
expected exactly one checkpoint-directory file, predating retained native logs.
The real cold/warm/invalidation/kill/resume sequence had succeeded, but that
assertion failed (`target/v9-candidate-owner-audit.log`). The gate now validates
the exact candidate's v2 ledger, expected outcome, output/native-log hashes,
inspected native inventory, and successful Cargo exit. It checks failed-attempt
evidence before resume as well as the final successful receipt. Deliberately
corrupted report and native-log bytes are rejected; restoring them validates
without another producer launch. The rebuilt AOT validator passes the real Make
recovery target (`target/v9-candidate-owner-fixed.log`), retaining the original
four intentional Rust test-body executions. This corrects an acceptance test;
it does not claim to add nested Cargo/rustc launch observation or complete
preparation-wide recovery.

Run this explicit Linux acceptance tier with:

```sh
make release-preparation-recovery-check
```

The complete local owner graph can be exercised with:

```sh
make release-preparation-owner-graph-check
```

This entry point shares bootstrap prerequisites and runs the contract, AOT,
report, multicore, readiness, staged-distribution, proof, and candidate
recovery rehearsals as one closeout tier.

The target uses the shared validator bootstrap and a 15-minute outer timeout.
It does not publish, alter repository history, run from publication verification,
or substitute for the required full-candidate preparation rehearsal. Phase
messages identify cold, warm, invalidation, interruption, and recovery work;
tool byte verification still runs before deciding whether a checkpoint matches.

The first owner passed all 14 deterministic recovery cases and the real Cargo
fixture rehearsal on 2026-09-07. The rehearsal observed the intentional SIGKILL,
preserved the prior report, resumed successfully, and verified equivalent bytes.
The multicore composer fixture rehearsal also passed on 2026-09-07. These results
validate the two report owners, not the remaining preparation graph.

## Proof-kernel owner integration (2026-09-11)

Proof replay and semantic-kernel consumption now run as one ordered preparation
graph. The replay owner admits the quality tool and its selected proof inputs;
the semantic owner consumes that receipt and the immutable VM/image, and runs
the cached-only semantic consumer. Both owners stage their output privately and
publish only through the existing owner commit protocol. A changed dependency,
tool seed, semantic image, failed replacement, or blocked final parent reruns
only the affected owner; unchanged owners launch no producer.

The focused sealed-image self-test passes selective reuse and blocked-parent
recovery. The canonical promotion image was resealed after fixing an explicit
`File.size` error conversion that previously surfaced as a native-lowering
`StringRef` mismatch. `target/v9-proof-kernels-build2.log` records the sealed
build and `target/v9-proof-kernels-selftest.log` records both scenarios.

The complete proof-preparation rehearsal now passes with exit code zero in
`target/v9-proof-prep-final.log`: input, track, policy, proof-kernel, cold
composer, zero-launch warm composer, per-output repair, killed-producer resume,
and unchanged recovery. The accepted proof baseline was updated from its
proposal after verification: all 14 slice traces are unchanged; only the
candidate identity and derived normalized digest changed. The repository build
and release contract, fresh Rust boundary AST, Rust structure/headroom,
Rust-quality/docs, focused TL0009/TL0010 lint, formatting, and whitespace gates
also pass (`target/v9-contract-final.log`, `target/v9-quality-final.log`).

This closes the proof-kernel integration slice only. All upstream
smoke/lane/native-boundary owners and cold/warm/interrupted full-candidate
acceptance remain open. Nested process containment is now exercised by the
combined candidate fixture, while automatic cache cleanup is integrated into
both preparation branches under the publication lock but still needs to be
exercised in the full-candidate rehearsal. V9-1 is not closed.

### Platform-contract and readiness owner closeout (2026-09-11)

The platform-contract owner now publishes a revision-scoped receipt under
`target/quality/`, outside the private preparation subtree. Its validator
accepts the declarative `VM_MULTICORE_PUBLISH_LOCAL_GATES` prerequisite graph,
and the refreshed matrix image passes all four contract commands with cold,
warm, repair, interruption, and candidate-isolation coverage. The readiness
fixture uses an immutable VM path (no large binary copy) and the production
two-argument Make wiring; its cold/warm/interrupted/invalidation rehearsal
passes. The complete Make preparation target set passes in one run in
`target/v9-preparation-targets-green.log`. These are owner and wiring fixes;
they do not claim preparation-wide acceptance or close V9-1 by themselves.

The legacy `TERLAN_MULTICORE_CLOSEOUT_ALREADY_RUN` bypass was removed from the
AOT closeout graph. Every invocation now admits the complete local correctness
gate set; Make target identity provides only in-invocation deduplication. The
updated matrix contract and publication-preparation graph tests reject the old
unchecked skip path while preserving the single-owner execution boundary.
The focused legacy-routing contract run is retained in
`target/v9-legacy-contract-final.log`; the three orchestrator environment tests
and eleven release-graph tests pass in `target/v9-legacy-make-environment.log`
and `target/v9-legacy-release-gate.log`. Canonical Terlan formatting and diff
checks pass in `target/v9-legacy-format.log`.

### Candidate warm and publication-retry acceptance (2026-09-11)

The shared Make fixture now executes a cold preparation followed by unchanged
warm preparation in one candidate root and records the real producer markers;
the warm invocation launches no producer again. Its resume scenario interrupts
the final owner and relaunches only that incomplete owner. A second fixture
interrupts the asset-upload step, retries `make publish`, and records only
source verification,
tag/upload operations, and promotion on the retry. It rejects any preparation,
Cargo, download, or evidence-refresh work on that path. The focused tests pass
(`target/v9-release-acceptance-final.log`). This is executable retry-path
evidence; full production-candidate preparation-wide acceptance remains open.

The focused acceptance log contains five passing publication-preflight tests
and twelve passing preparation-graph tests. It also statically rejects any retry
recipe that could select preparation, Cargo, or evidence-refresh work, while
the unchanged-warm case compares every successful owner output byte-for-byte.

The preparation graph additionally asserts one shared, locked resource-admission
owner for both cold and warm publication branches. This serializes admission
decisions before branches proceed; it does not reserve the budget for the whole
graph or claim that every upstream producer is yet one end-to-end candidate owner.
Admission waits for transient lock contention under a 120-second outer
deadline, rather than converting a short overlap into an immediate failure.

The process-owner suite passes all 44 tests, including nested-owner membership,
grandchild termination on timeout/cancellation, direct reaping, and inherited
inventory scopes. Those are component containment guarantees; the full production
candidate rehearsal still must exercise the same boundary around every owner.
The recovery preparation target now runs this suite before its candidate fixture,
making containment a prerequisite of recovery acceptance.
The same target runs the combined cold/warm/resume/upload-retry candidate
fixture, so the recovery gate exercises the complete retry sequence rather than
only isolated owner scenarios.

The real Cargo-backed candidate owner rehearsal also passes after the updated
ledger assertion and interruption sequence
(`target/v9-candidate-owner-final.log`).
The named Make recovery target also passes against the refreshed promotion image
(`target/v9-candidate-owner-make-final.log`), running process containment and
the combined candidate acceptance fixture before the Terlan rehearsal.
The combined candidate fixture now includes a deliberately hanging preparation
descendant and verifies that the enclosing owner times out, reaps it, and leaves
no live process residue. This keeps containment on the candidate cold path;
the refreshed gate log is
`target/v9-owner-final-candidate-rehearsal-20260912.log`. It remains local
fixture evidence, not hosted clean-candidate acceptance.
The publication entry point also rejects a missing or mismatched pinned Rust
channel, and non-executable compiler paths, before hosted-input download or
build/test owners begin; the fixture exercises the pinned `1.96.0` contract.
The download and compiler-version probes are bounded by explicit outer
deadlines; the preparation fixture asserts those timeout wrappers remain in
the live Make recipe. The publication asset-upload target is likewise bounded
to 900 seconds, so a stalled upload cannot hold a retry indefinitely. The same
fixture verifies that both cold and warm preparation branches invoke locked,
bounded publication-cache retirement before completing. The staged release-
reports owner and artifact-matrix verification are also bounded to 900 seconds,
so the shared preparation lock has a finite failure window.
The hosted coverage refresh itself is bounded to 1,800 seconds, giving the
canonical validation owner a finite failure window before recovery can resume.
The native-boundary proof input split (including the dedicated dispatch-value
source) was re-proved and its accepted Slice 14 baseline was updated only after
the Lean proof, four exact Rust oracles, and manifest replay passed. The
promotion/readiness self-tests bind `TERLAN_RELEASE_ROOT` explicitly; Docker
worktree rehearsals additionally mount the worktree Git metadata and register
the checkout as a safe directory, so Git inventory observes the candidate root
rather than an external worktree path.
The full `terlan-test-orchestrator` crate exits successfully in
`target/v9-orchestrator-full-final.log`; its embedded `FAILED` lines come only
from expected fault-injection child processes, not from the outer Cargo gate.
The live publication-plan contracts also pass in
`target/v9-publish-plan-final.log`, proving zero verification replays and five
refresh Cargo invocations with no duplicate builds.

The current owner graph was rerun after the release-tooling lockfile change.
The proof composer initially rejected the old baseline, so a proposal was
reviewed: all fourteen Slice 4–20 trace IDs were unchanged; only the derived
candidate identity and normalized digest changed. The accepted baseline now
matches that proposal, and `target/v9-proof-self-test-current.log` records an
exit-zero cold/warm/repair/kill/resume proof rehearsal for candidate
`sha256:8f38bc9748b43671990fc1cfe1bd272c4cc8a1eb62a0ac0309b6c55619a9aa86`.
The full-candidate, contract, AOT, release-report, local-report, multicore,
readiness, and staged-distribution rehearsals also pass against the current
owner graph. This closes the previously stale proof baseline, while the
preparation-wide hosted-candidate boundary remains the final V9-1 acceptance
requirement.

The hosted proof-smoke producer is now wired as a typed `proof-smoke` owner
with private smoke, blocker, and attempt outputs. Its declaration self-test is
included in `release-preparation-owner-graph-check` and passes in the refreshed
promotion image (`target/v9-proof-smoke-owner-self-test-20260912.log`). The
self-test originally validated declaration and dispatch wiring. The result recorded in
`target/v9-proof-smoke-clean-candidate-20260912.log` is withdrawn as acceptance
evidence because subprocess observation was disabled. Observation is restored;
the cold/warm/resume boundary must pass with full native and process accounting.
The strengthened smoke-owner rehearsal now executes a bounded fixture compiler,
requires both native and process observation variables in the actual child,
and verifies zero-producer unchanged warm reuse. It and the owner/table suite
pass in `target/v9-preparation-proof-smoke-self-test-observation.log` and
`target/v9-preparation-owner-self-test-observation.log`. Empty blocker tables
have an explicit optional-row policy; ordinary proof-baseline tables still
require data, and the output policy is bound into the owner fingerprint.

The isolated candidate `0c382f2091f00c6ad07e02b024f9a60497592342` passes
observed cold and warm smoke execution in Ubuntu 24 as the non-root host user.
See `target/v9-proof-smoke-observed-cold-user-scratch.log` and
`target/v9-proof-smoke-observed-warm.log`. Cold evidence contains 11 completed
native work units, two native links, and 17 reaped subprocesses (including one
Cargo launch). The deliberately rejected unsupported-target case accounts for
the one nonzero subprocess exit. Three semantic families, eight lanes, and
seven script tests pass. Warm execution reports zero completed producers and
one reused owner; `target/v9-proof-smoke-observed-before-warm.sha256` verifies
that reports, execution log and owner receipt remain identical. This does not
yet prove full release preparation.

The incremental follow-up candidate
`06cd4617c086cd6a5042be6845a294024b6ab669` is interrupted with SIGTERM after
its native image manifest is sealed, in a named disposable container. The
trigger is recorded in `target/v9-proof-smoke-interruption-trigger.log`; the
terminated command exits 143 (`target/v9-proof-smoke-interrupted.log`). Resume
uses the same source, tools and cache, without discarding the owner or native
cycle journals. It passes all eight script tests and the three-family/eight-lane
semantic checks (`target/v9-proof-smoke-resumed.log`). The native cycle retains
the nine completed work units from interruption. The resumed producer records
zero native work and zero native links, with 11 reaped subprocess launches and
no pending processes. Subsequent warm execution reports `completed=0 reused=1`
in `target/v9-proof-smoke-resumed-warm.log`; verification against
`target/v9-proof-smoke-resumed-before-warm.sha256` passes for all three reports,
the native/process log and owner receipt. This is evidence for the post-seal interruption boundary;
it does not prove interruption at every compiler/cache publication boundary.
The updated owner rehearsal and type/format checks pass. Direct smoke-script
lint reports readability/complexity warnings; it is not passing closeout evidence.

The consolidated owner-graph run, including the original smoke-owner self-test, passes
in `target/v9-owner-graph-smoke-owner-20260912.log`.

The earlier consolidated run is retained as
`target/v9-owner-graph-current.log` with `owner_graph_status=0`; it covers the
contract, AOT, release/local report, multicore, readiness, and staged
distribution rehearsals in one toolchain invocation. The expected rejection
diagnostics in that log are fault-injection cases, and every enclosing owner
reports success after recovery.

### Lane producer ownership (2026-09-12)

Hosted preparation routes `lean-proof-lanes-check` through `prepare-proof-lanes`.
The owner seals both lane and gate JSON reports from private staging paths.
Candidate-local `OwnerHistory` freezes the prior lane output before production;
newly generated output cannot change that comparison input or cause perpetual
warm misses. History is content-bound and rejects corrupt or mismatched records.
The owner binds upstream report hashes, tracked/metadata-selected proof sources,
compiler bytes, and the compiler-reported native toolchain identity. Both the
standalone and owned script command request verified incremental AOT reuse.

The canonical promotion image builds and seals successfully in
`target/v9-proof-lanes-owner-build-repaired.log`. The recovery fixture passes in
`target/v9-proof-lanes-owner-self-test.log`: actual subprocess execution, private
outputs and observation, unchanged warm reuse, partial-output failure with both
published reports preserved, changed inputs, corrupt-output regeneration,
candidate-specific history, and rejection of corrupt history and unsafe paths.
The initial build exposed a mixed JSON-error/string-error branch in the new
preflight; JSON identity validation is now separate, retaining typed errors.

The isolated candidate `6ecec52a32bd0ddedfe7b4e155b2b5ba3bb05758` passes all
15 script tests and the eight existing lane-policy checks in Ubuntu 24 with
user-owned scratch (`target/v9-proof-lanes-cold.log`). Existing accepted proof
gaps remain explicit; this does not claim additional executable proof coverage.
Cold execution records eight completed native work units, one native link,
and two reaped subprocess launches, with no pending work. Unchanged warm
execution reports `completed=0 reused=1` (`target/v9-proof-lanes-warm.log`).
All five report/history/log/receipt hashes pass verification against
`target/v9-proof-lanes-before-warm.sha256`.

Two focused Make contract tests and strict Clippy pass in
`target/v9-proof-lanes-make-contract.log` and `target/v9-proof-lanes-clippy.log`.
The full promotion source-directory check and affected formatting checks pass.
The release owner-graph target now includes the lane-owner rehearsal. The
earlier consolidated owner-graph evidence predates that addition and is not a
claim that the expanded whole graph was rerun. Native-boundary directory
publication and preparation-wide candidate acceptance remain unfinished.

### Directory-generation publication (2026-09-12)

Preparation owners can now declare a flat directory as their primary or
additional output. Its exact JSON member names and schemas participate in the
input identity; the prepared checkpoint binds every member's name, size and
hash. The directory remains one generation, so replacing it cannot retain an
obsolete manifest from the previous generation. File-only output contracts and
the existing 16-output limit remain intact. Each directory permits at most 256
regular files and 16 MiB of content; empty, nested, linked and special-file
generations are rejected.

Publication requires the enclosing preparation lock. The old directory moves
to a private backup before the staged generation takes its final name. Recovery
accepts the sealed staged generation or its exact already-published bytes; it
never hides corrupt staging behind an older final directory. The backup remains
until the enclosing owner commits and cleans its private workspace. Moves reject
symlinked path components and cross-filesystem copy/delete fallback. This is
process-interruption recovery, not atomic directory exchange or power-loss
durability.

Downstream owners bind complete directory inventories. Reading a directory
member also requires an explicit dependency edge, preventing unordered reads of
another producer's generation. Fixtures cover cold/warm execution, failed
replacement preservation, obsolete-member removal, output repair, interrupted
publication without producer replay, schema/path rejection and dependency edges.
These fixtures join the existing `preparation-owner-self-test`; no additional
publication gate or shell script is introduced.

The first isolated filesystem run passed 14 scenarios in
`target/v9-directory-publication-test.log`. That predates the full integration
and final path guards and is not acceptance evidence for the final code.
The full source check and lint for the five new modules pass in
`target/v9-directory-source-final-graph.log` and
`target/v9-directory-lint-final-graph.log`. A subsequent AOT build exposed a
lowering failure when an irrefutable JSON binding shared a Result-specific
grouped-let error arm (`target/v9-directory-owner-build-final.log`). The helper
now separates the fallible inventory read from JSON construction; this avoids
that lowering failure but is not a general compiler fix. The corrected image
builds and seals successfully in `target/v9-directory-owner-build-row-fix.log`.
The complete owner suite passes in `target/v9-directory-owner-self-test.log`,
including all 15 final directory-publication scenarios, exact member/schema
and dependency-edge admission, cold/warm dependent reuse, failed replacement
preservation, obsolete-member removal, output repair and prepared-directory
recovery without producer replay. Existing JSON/TSV, multi-file commit,
process-observation and interruption scenarios pass in the same invocation.
The graph suite passes in `target/v9-directory-graph-self-test.log`; the candidate
rehearsal passes cold/warm, zero Cargo replay, changed-dependency, killed-producer
and equivalent-output recovery checks in `target/v9-directory-candidate-self-test.log`.
These are component/fixture checks, not a new release-candidate acceptance run.
The native-boundary producer integration follows below; V9-1 and whole-candidate
acceptance remain open.

### Native-boundary generation owner (2026-09-12)

Hosted preparation now owns the native-boundary directory as one generation.
The exact manifest selection (including ignored source manifests), member
schemas, source hashes, compiler/linker identity, quality and coverage tools,
fresh Lean admission, and coverage endpoint all participate in reuse. The
producer inherits native/process observation and the existing Rust coverage
context. It requires completed Lean replicas instead of becoming another proof
producer on a cache miss. Missing hosted coverage fails before preparation.

The script writes only to the owner's private output directory. Binding report
paths remain canonical, not staging-specific. Lean and runtime validation finish
before output writes begin, including in standalone mode; standalone writes do
not claim the owner's transactional publication guarantees. Parallel Make now
orders native proof consumption after replay and all proof consumers before
distribution staging, avoiding competition for the preparation lease.

The sealed image passes the new native-boundary owner rehearsal in
`target/v9-native-owner-self-test.log`: cold/warm execution, failed partial
replacement preserving the previous generation, changed manifest content and
selection, coverage-context invalidation, extra-output repair, and changed-tool
invalidation. The new modules pass full package source checking and focused lint
(`target/v9-native-owner-source3.log`, `target/v9-native-owner-lint-final.log`).
The image builds in `target/v9-native-owner-build.log`; its size changes from
30,727,472 to 31,642,704 bytes (3.0%), including the added fixture code.
The existing owner regression suite also passes against that image in
`target/v9-native-owner-regression.log`, including all 15 directory interruption,
corruption and path-safety scenarios and prepared-directory recovery.

The script's three focused AOT rejection tests pass in
`target/v9-native-script-aot-tests.log`; all four exact runtime oracles pass in
one Cargo invocation in `target/v9-native-runtime-oracles.log`. Its source
fingerprint in the proof metadata is updated. Real Lean execution passes two
replicas, then freshly admitted cached consumption reuses both with zero new
executions (`target/v9-native-proof-replay-container.log`,
`target/v9-native-proof-admitted-cached-container.log`). This uses the existing
Ubuntu 24 container with read-only source and isolated scratch because the
retained quality binary requires glibc 2.39, unavailable on the host. The initial
unadmitted cached invocation correctly fails and is not passing evidence.

The parallel Make failure fixture and strict Clippy pass in
`target/v9-native-owner-make-final.log` and `target/v9-native-owner-clippy.log`.
Publication plans still report zero retry builds/tests, five refresh Cargo
invocations and zero duplicate builds (`target/v9-native-publication-plans.log`).
This is component evidence, not execution of the complete native producer under
a clean candidate's live hosted coverage, nor full-candidate acceptance. No
roadmap checkbox, release baseline acceptance, commit, tag or publication is
implied by these results.

### Nested preparation leases and current build-plan admission (2026-09-12)

The full preparation entry point held `preparation.lock` on descriptor 9 while
nested producers reopened that same file with nonblocking `flock`. A real
two-open reproduction confirms this rejects the nested producer: the enclosing
invocation conflicts with itself. The earlier isolated owner rehearsals did
not cover that enclosing lease.

All preparation producers and resource admission now share one Make wrapper.
It validates the inherited descriptor against the named regular lock file and
reuses that kernel file description. Standalone calls acquire their own outer
lease. A separate `preparation-owner.lock` serializes sibling producers inside
the invocation; inheriting the outer lease does not permit concurrent graph
mutation. Both waits are bounded. Invalid scope flags, missing/wrong descriptors
and symlinked lock paths fail before producer execution. Neither nested success
nor failure unlocks the enclosing invocation. The outer entry point also checks
its lock path before opening it and verifies descriptor identity after locking.

`target/v9-preparation-lease-tests.log` passes the real inherited/standalone,
parallel, foreign-owner timeout, invalid-descriptor, symlink-preservation and
failed-child recovery scenarios. The 22 publication Make tests and proof Make
tests pass (`target/v9-preparation-lease-make-final.log`,
`target/v9-preparation-lease-make-tests.log`); the three actual resource-admission
tests pass in `target/v9-preparation-lease-admission.log`. Strict orchestrator
Clippy and scoped formatting pass. No new shell script or dependency is added.

The repository build/release contract initially failed because its old text
matcher counted commands printed into receipt hashes as additional compiler
builds and still expected the pre-owner quality bootstrap form. It now checks
the current compiler/quality declarations and their order, excludes only the
explicit command-word `printf` from the Cargo count, and retains the roadmap's
already-reviewed eight-invocation budget. Duplicate declarations and actual
repeated command lines remain rejected/counted by its self-tests. Conditional
branches still contribute to a conservative dry-run bound; the report labels
this scope explicitly and does not claim observed subprocess counts.

The rebuilt validator self-test and actual repository build/release contract
pass in `target/v9-release-plan-self-test.log` and
`target/v9-release-plan-contract-final.log`. The sealed report records eight
planned Cargo references, sixteen incremental AOT builds, thirty-two Terlan
test commands, one Rust orchestrator and both lifecycle checks. The image grows
from 1,211,384 to 1,229,152 bytes. Publication plans still show zero retry
build/test replay and five refresh Cargo invocations with zero duplicate builds
(`target/v9-preparation-lease-plans-final.log`). Broad legacy lint warnings in
`RepositoryValidation.terls` are not claimed as resolved by this correction.

These fixes remove reproducible preparation blockers, not the need for a clean,
committed candidate and full production acceptance. No existing branch, release
baseline, tag or public release is changed by these checks.

### Isolated candidate and cold-plan recovery (2026-09-12)

With explicit permission, local detached candidate `5c796890` was created in
`/tmp/terlan-v9-candidate.6XPGid` from the existing clean rehearsal snapshot plus
21 reviewed preparation files. Existing branches and the source worktree's
index were left untouched. This is a development validation candidate: active
versions remain 0.0.8 until V9-3, and no tag, push or publication is authorized.

The empty checkout exposed a real bootstrap cycle: GNU Make executes recursive
recipes even under `-n`, so the refresh-plan check tried to launch an unbuilt
orchestrator. Planning now traverses Make's covered graph directly under an
explicit dry-run-only context. Actual execution still enters the bounded live
coverage owner. A cold fixture without any generated tools passes both plan
checks and creates no target directory; a normal invocation using long Make
options still propagates coverage-owner failure without entering covered gates.
`publish-prepare` also resolves its default version from workspace metadata.

The parallel live-coverage fixture now imports the production native, smoke,
and lane dependencies and uses the real preparation lease wrapper. It exercises
failure at each producer and verifies that staging waits for all proofs; passed
Rust test bodies are not replayed. This fixes a stale fixture, not a bypass of
the failed ordering assertion. The three compiled production coverage callers
passed in the initial attempt before that fixture failure; only the changed
Rust/Make graph was rerun afterward.

Validation logs beside the isolated checkout:

- `/tmp/terlan-v9-candidate.6XPGid-validation-final.log`: 38 outer tests pass
  across live Make coverage, preparation leases, proof Make, publication
  preflight and preparation/retry behavior. Child-process fault messages are
  expected; the enclosing Cargo command exits zero.
- `/tmp/terlan-v9-candidate.6XPGid-plan-tests.log`: all three cold-plan,
  live-owner and plan-budget tests pass.
- `/tmp/terlan-v9-candidate.6XPGid-native-owner.log`: the sealed native-boundary
  generation-owner rehearsal passes against the isolated source checkout.
- `/tmp/terlan-v9-candidate.6XPGid-clippy.log`: strict all-target orchestrator
  Clippy passes; scoped Rustfmt and whitespace checks also pass.

The actual plans retain zero publication-retry builds/tests and five refresh
Cargo references with zero duplicates. No hosted evidence was fabricated or
copied across revisions. Production hosted acceptance still requires verified
compiler/release artifacts for the exact commit; local fixture success cannot
satisfy it. V9-1 acceptance, V9-2 and V9-3 remain open.
