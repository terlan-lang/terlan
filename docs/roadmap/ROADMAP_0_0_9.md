# Terlan 0.0.9 Release Optimization Roadmap

Updated: 2026-09-24. Baseline: 0.0.8 is published.

## Scope

By explicit user decision, 0.0.9 is limited to build and release optimization,
reliable validation, and verified publication. The self-hosted frontend slice,
new native compatibility policy, runtime inspector,
HTTP/3, and accelerator follow-ups move to the
[0.0.10 roadmap](ROADMAP_0_0_10.md). They are postponed, not completed.

On 2026-09-24, after the user delegated the release-scope decision, unfinished
replication, quorum writes, automatic failover, and interchangeable replication
providers were moved to [0.0.10](ROADMAP_0_0_10.md#active-checklist). This supersedes
the earlier requirement to implement replicated storage before publishing 0.0.9.
Keep and verify the implemented local durability and logical checkpoint/restore
work. Cluster capability queries must return false, and replication requests must
return explicit unsupported outcomes without local-write fallback. Rejection
tests are diagnostic evidence, never positive replication coverage. Other
advertised capabilities and their existing verification obligations remain.
Live actor heap, continuation, mailbox, and timer migration remain out of scope.

Preserve existing compiler/runtime correctness and supported installed artifacts.
Do not use this scope change to drop correctness tests, weaken evidence, or
claim new features. CPU quietness is not a publication prerequisite.

The user also requires immediate repair of discovered gaps in advertised
capabilities, including baseline Effect execution. Such gaps remain release
blockers; placeholder tests and unverified API declarations cannot prove support.

Every advertised public API requires Terlan-source execution coverage on each
target for which support is claimed. The existing `std-package-coverage-100`
command checks manifest consistency only; its success is not 100% API coverage.
Completing the declaration inventory and binding it to real target execution
remain release requirements, not accomplished capabilities.

Prepare the focused 0.0.9 release, but obtain explicit user authorization before
tagging or publicly publishing it. The exact candidate and its artifacts must
pass verification first. This scope does not cover publishing 0.0.10.

## Current Status

0.0.9 is not release-ready. Finish the supported local storage API audit and the
existing build/release acceptance gates; do not start consensus or provider
architecture work in this release. The former test-only cluster success claims
now return explicit unsupported outcomes through production AOT. All 19 storage
source tests and 20 focused Rust storage tests pass. The separate-process durable
storage fixture also passes, including rejection of replication without changing
the persisted sequence or creating a local snapshot. Evidence is recorded in
`target/quality/release-diagnostics/storage-scope-{source-tests,unit-tests,source-restart}.log`.
These rejection checks establish the unsupported boundary, not replication support.
No release checkbox is closed by this scope decision, and no publication is
authorized. The implementation history below records earlier requirements and
evidence; its replication blockers are superseded by this scope and the active
checklist, not silently counted as completed features.

The current V9-1 rehearsal passes all 24 preparation/retry contract tests and
49 process-containment tests. Rebuilding the production promotion validator
exposed reachability pruning of imported free-function builder calls. The fix
passes 75 focused compiler tests, both strict production Clippy configurations,
and structural gates. The production validator now builds and seals; its real
Cargo-backed cold/warm/invalidation/interruption/resume rehearsal passes, as do
the owner and graph fault-injection suites. The affected download checkpoint
fixture also executes successfully through the sealed image. These remain
component/fixture results, not a complete current-candidate publication rehearsal;
V9-1 stays open. See `target/quality/release-diagnostics/v9-builder-*.log` and
`v9-current-{preparation-contract,process-containment,publication-plans,candidate-recovery,owner-recovery,graph-recovery}.log`.
The readiness-owner rehearsal and all seven proof input/policy/kernel/track/
native-boundary/smoke/lane recovery rehearsals also pass against that image
(`v9-current-readiness-recovery.log` and `v9-current-preparation-proof-*-self-test.log`).
The current checkout has neither staged distribution metadata nor hosted-candidate
or proof-release evidence. A fresh isolated committed candidate and its verified
artifact producers are still needed; fixture reports cannot replace them.

## Implementation History

The release API audit now passes through the core Task suite. All six real
`std/core/TaskTest.terl` tests pass with the rebuilt compiler and VM. Only
already-successful tasks (`done` and `result`) are supported; failed, deferred,
spawned, mapped, chained and recovered Tasks remain explicitly unsupported by
the VM target. Module documentation now states this exact scope. The next
manifest suite, `std/vm/ClusterTest.terl`, now passes all nineteen tests through
the rebuilt compiler and VM, including the connected session/frame operations
and additional atom/aggregate payload cases. This proves in-memory transport
execution, not peer discovery or socket connectivity. Map, Set, and Binary
bitstring payloads now pass real source tests: VM-owned operations preserve
typed values, and conversion to external-worker terms occurs only at an actual
worker boundary. Unsupported external values still fail explicitly without
echoing their contents. Evidence: `storage-typed-arguments-cluster-tests.log`
and its result manifest. A generic source helper exposed a qualified-local-call
lookup omission in admission and native lowering; both stages now resolve exact
same-module identities without admitting another module's private functions.
All 31 admission and 17 generic-specialization tests pass in
`storage-qualified-calls-*-tests.log`.
The remaining API audit and release closeout are still open.

The `std/vm/DistributedStateTest.terl` AOT gap is repaired locally. All seven
Terlan tests execute successfully, replacing the checkpoint placeholder with
snapshot-content replay and isolation assertions. Native operations use the
VM-owned conflict logic and actor-checked mutable stores; immutable entry,
conflict, and snapshot views remain owner-scoped. `write[Value]` preserves the
checked payload type rather than accepting an unlowerable `Dynamic` parameter.
The added source test checks checkpoint/restore with Map, Set, and unaligned
bitstring payloads, including exact replay and conflict rejection. Evidence:
`storage-typed-arguments-state-tests.log` and its result manifest under the
release diagnostics directory.
This is in-memory state support, not persistence or replication.

The next manifest suite, `std/vm/DistributedStorageTest.terl`, remains open.
Public durable lifecycle, batch/CAS append, schema transitions, and compaction
now execute through a real sandboxed SQLite worker and survive a complete VM
restart; they do not use the old in-memory test model. Typed failures preserve
transaction-observed conflict metadata. Receiver/free-function qualification
and fully qualified same-module overload admission defects are fixed.
The full suite now reaches the still-unbound `expected_entries/1` declaration and
fails before execution (`storage-metadata-full-source-tests.log`).
Remaining public contracts and real peer replication still block release.

JavaScript compile-only test reports now have `not_executed` entries, zero
passes, and a nonzero command exit. Eighteen test-command regressions pass,
including a false JavaScript test body that cannot become a reported pass,
and actual VM/Wasm execution cases. Evidence: `test-execution-honesty-tests.log`.
This corrects false evidence; it does not supply a JavaScript test executor or
complete the declaration-derived, exact-candidate API coverage gate.

Scoped closeout for these changes: eleven state/runtime/interface tests,
four shared-registry tests, eighteen test-command tests, and six real state
source tests pass. Both strict workspace-bin Clippy profiles pass; API-boundary
(3,082 existing string-error sites, no budget increase), file-headroom (56
near-limit files), module-structure, Rust documentation, dormant-code inventory,
and documentation checks pass. Evidence is in `distributed-state-*`,
`execution-honesty-*`, and `state-honesty-*` release diagnostics. The standalone
JS CLI probe confirms a nonzero exit and explicit non-execution JSON; it is a
diagnostic test, not a supported-JS execution pass. Runtime tests have not been
replayed for subsequent comment-only documentation corrections.

Storage scope is resolved by explicit user approval: implement real storage in
0.0.9 and keep publication blocked until verified. No coverage obligation has
been removed, no release item has been checked off, and no tag or publication
has been created.

The first real persistence slice is `crates/terlan-storage`: an independent
SQLite WAL engine with full synchronization, explicit database paths, bounded
checkpoint/batch sizes, atomic append/CAS, exact replay, SHA-256 integrity,
and compaction that preserves sequence history. The backend has eleven behavioral
tests plus a subprocess fixture; all twelve harness cases pass, including
abrupt child exit before/after commit, transaction rollback, competing writers,
corruption, engine-level oversized-row rejection, and symlink rejection.
Strict all-target Clippy and the dependency
security audit pass. Evidence: `durable-storage-tests.log`,
`durable-storage-clippy.log`, and `durable-storage-security-audit.log`.
The next worker slice passes 21 protocol/storage cases, 29 VM transport/sandbox
cases, and an explicitly executed real sandboxed-worker restart test. The VM
binds a separate private durable directory; worker scratch cleanup cannot delete
it. Requests cannot choose database paths or SQL. Capability grants, blocking
admission, frame bounds, cancellation, and request/epoch fencing remain enforced.
The restart test is now owned by the canonical Rust orchestrator, with no extra
Cargo producer. Evidence: `storage-worker-resumed-protocol-tests.log`,
`storage-worker-resumed-vm-tests.log`, and `storage-worker-restart-test.log`.

Database format 2 now persists application schema CAS transitions and their
checkpoint boundaries. Old payloads retain their schema, stale-schema writes
fail, and failed batches/migrations roll back. All 18 backend harness cases
(including two child fixtures) and backend all-target Clippy pass:
`durable-storage-schema-tests.log` and `durable-storage-schema-clippy.log`.
The expanded worker schema RPC/restart test now passes with a rebuilt worker
and harness, including persisted schema CAS, stale-schema rejection, and replay
of older-schema checkpoints after restart. Linux worker startup now admits only
writable local ext-family, XFS, and Btrfs filesystems; memory, network, overlay,
unknown, and read-only filesystems fail before launch. The check does not prove
hardware persistence or power-loss safety. The 21 protocol cases, 32 VM cases,
and explicitly executed worker restart pass in `storage-schema-*-tests.log` and
`storage-schema-restart-test.log`.

Explicit flush now uses SQLite FULL WAL checkpoint completion instead of a
test-model counter. Reader/writer contention fails without claiming a flush
proof or rolling back already committed appends. All 21 backend cases and strict
backend all-target Clippy pass in `storage-flush-backend-tests.log` and
`storage-flush-backend-clippy.log`. The rebuilt worker/harness also pass 22
protocol cases, 32 VM cases, and the explicit schema/flush/restart test in
`storage-flush-protocol-tests.log`, `storage-flush-vm-tests.log`, and
`storage-flush-restart-test.log`. Formatting, API-boundary, file-headroom, module
structure, and lightweight documentation checks pass without raised budgets.
The dependency-impact inventory was refreshed for the changed compilation inputs;
its check passes in `storage-flush-dependency-impact.log`. Both additional real
worker lifecycle and inherited-descriptor isolation tests also pass.
Both production workspace-bin Clippy profiles pass against the flush changes
in `storage-flush-production-clippy.log` and
`storage-flush-all-features-clippy.log`. The two specifically approved September
14 incremental caches were removed, allowing validation to resume; no other
caches were deleted.

The shared TETF encoder now enforces byte limits before growing output and
canonical-sort buffers, rather than rejecting an oversized payload only after
allocation. Canonical wire bytes and exact-limit behavior are preserved,
including duplicate Set entries. Fifteen codec tests, 38 coordination tests,
15 Cluster adapter tests, and both production Clippy profiles pass in
`storage-codec-*` evidence. API-boundary inventory is now 3,081 string-error
sites, with no budget increase; headroom, module structure, documentation,
and dependency-impact checks pass for that slice.

The typed-argument bridge and qualified-call fixes also pass both production
Clippy profiles, API-boundary, file-headroom, module-structure, documentation,
dependency-impact, and Rust/Terlan formatting checks. Three argument-boundary
tests, 46 package-helper tests, three protocol-owner tests, one capability
completion-order test, and both explicitly executed real-worker dispatch tests
pass. Evidence: `storage-typed-arguments-*` diagnostics. The worker binary is
the existing storage-flush build; these are scoped dispatch regressions, not
exact-candidate publication evidence. No zero-test invocation is counted as
coverage. The next implementation remains the authorized, asynchronous public
storage binding, followed by authenticated independent-peer replication.

Logical checkpoint encoding and restore now execute through public AOT calls.
`DistributedStorage.checkpoint` captures immutable canonical TETF bytes;
`Snapshot.restore` decodes them into a new actor-owned state store. The added
restore operation closes the missing source-level path from a loaded checkpoint
back to usable state. Both new selected Terlan tests pass, including nested
Map/Set/bitstring values, exact replay, conflicting replay, and independence from
later mutations. Evidence: `storage-logical-checkpoint-source-tests.log` and
`storage-logical-checkpoint-source-results.json`. These two tests are not a pass
for the entire storage suite or its still-unbound adapter lifecycle APIs.

The codec has explicit byte, entry, nesting, and total-value limits. Collection
allocation reserves all children, including pending siblings, before allocating;
compact wire tags cannot cause unbounded decoded allocations. It rejects
undeclared atoms, native-handle metadata, invalid versions/policies, duplicate
or unordered scopes, truncation, and trailing bytes. All 22 shared codec tests
and 48 package-helper tests pass. The real worker restart test now stores this
encoded logical state rather than a placeholder byte string, verifies restoration
after schema transition/flush/restart, and rejects atoms not admitted by the
restoring image. Evidence is in `storage-logical-checkpoint-*` diagnostics.
This remains separate source-codec and worker integration evidence, not a claim
that public durable append/flush or replication is already wired end to end.
Both production workspace-bin Clippy profiles, all scoped Rust quality checks,
dormant-runtime inventory, and Rust/Terlan formatting pass for this slice without
raising budgets. Regenerated Cluster/DistributedStorage interface summaries and
the release API test map include the implemented source contracts. The complete
storage suite was retried and still stops at unresolved `policy_name/1` before
execution (`storage-logical-checkpoint-full-source-tests.log`); unbound adapter
operations have not been hidden, counted as passes, or replaced by the old model.

A further source-level atom test exposed missing atom-manifest propagation to
the two checkpoint operations. Its recorded failing run rejected the declared
`checkpoint_ready` atom; the fixed path takes the vocabulary from the admitted
image, never from checkpoint contents. The rebuilt compiler/VM pass all three
selected checkpoint source tests in `storage-owner-lifecycle-source-tests.log`
and its result manifest. Root helper resources now have a scope guard so every
returned error and unwind revokes actor-owned checkpoint/state descriptors,
including errors after native resume or result projection. All 50 package-helper
tests pass in `storage-owner-lifecycle-tests.log`, including success, failure,
unwind, and foreign-owner preservation. These latest changes have targeted
execution and formatting evidence; broad closeout checks will be refreshed with
the public durable-binding slice rather than counted as already rerun.
The subsequent public durable-binding slice now passes four selected Terlan
tests and a real AOT integration test that compiles once, denies an unbound
program, writes/flushes/closes in one VM process, restores in another, and repeats
the write/read to verify exact replay. Typed Map/Set/bitstring/atom contents survive.
`--storage NAME=/absolute/private/directory` grants supervisor-selected authority;
source policy flags cannot grant it. Bounded worker requests retain owner/epoch
continuations, wake the VM owner, and fail conservatively on timeout or worker
loss. The source test is owned once by the canonical Rust orchestrator, reusing
its compiler/VM/worker build rather than adding a Cargo producer.
Evidence: `storage-public-lifecycle-*` diagnostics, including 54 helper tests,
35 application-lowering tests, 32 admission tests, five CLI argument tests,
32 worker transport tests, both production Clippy configurations, orchestrator
tests and strict Clippy, and the scoped Rust quality/formatting checks. No
quality budgets were raised. Local-only mode, typed lifecycle failures,
batch/CAS/schema/compaction/proof facades, additional VM entry points, complete
API execution coverage, and authenticated independent-peer replication remain
unfinished. No release item is closed.
Process-exit tests are not power-failure testing, and local WAL recovery is not
distributed replication.

The transactional public slice now passes the expanded single-image AOT restart
test: two adapters compete through durable CAS, a rejected batch leaves no
partial writes, schema CAS survives restart, old checkpoint schema tags remain
readable, and compaction preserves both the high-water mark and retained data.
`checkpoint_with_schema` explicitly selects a positive application schema;
`checkpoint_schema` exposes it without changing or implicitly migrating the
logical codec. Five selected Terlan tests pass with execution manifests.
Domain errors use a closed typed worker record, not parsed database messages;
database errors fence the adapter pending reconciliation. A failed observation
cannot return an Outcome where the source signature promises a proof.

Evidence under `storage-transactions-*`: 21 backend harness cases, 55 helper
tests, 31 admission tests, six storage protocol tests, fourteen overload tests,
two failure-codec tests, 32 worker tests, and three explicitly executed real
worker tests pass, alongside the public AOT restart test. Backend all-target
Clippy and both production workspace-bin Clippy profiles pass. The regenerated
embedded interface includes the new schema APIs. These are scoped local
results, not an exact-candidate publication seal. Typed transport failures and
invalid batch resource bounds, local-only mode, remaining proof/metadata APIs,
other VM entry points, declaration-derived coverage, and authenticated
independent-peer replication remain open. No release item is closed.

The broad source-quality check also exposed six test-placement violations.
Two inline modules were extracted and four test files adopted the established
adjacent `_test.rs` naming; assertions and module identities are unchanged.
The rebuilt library harness has the exact same 6,643-test inventory, and the
backend still inventories 21 cases. No unchanged runtime test was replayed
solely for these relocations. Rust source quality now reports zero oversized
or inline-test files, and Rust documentation reports zero undocumented items.
Both production Clippy profiles and backend all-target Clippy pass after the
relocations. Refreshed AST-backed API-boundary, file-headroom, module-structure,
documentation, and dependency-impact checks pass without increased budgets;
Rust/Terlan formatting and whitespace checks pass. These scoped checks do not
close the still-failing full storage suite or the release coverage obligations.

The metadata/isolation slice now exposes checkpoint identity and checksum
diagnostics from real operations, plus immutable snapshot-isolation observations
read through the worker. Full SHA-256 comparison remains mandatory; displayed
integer checksums are only diagnostic prefixes. The expanded AOT restart fixture
corrupts the tail of a stored digest while leaving its prefix unchanged: the
source program observes typed corruption, and missing/corrupt checkpoint proof
requests fail. Previously issued observations survive compaction and closure.
Evidence: `storage-metadata-*` includes 21 backend cases, 95 targeted runtime
cases, three explicit real-worker tests, and this single-image AOT integration
test. Backend and both production Clippy profiles pass. The complete storage
source suite still fails before execution; no full-suite coverage is claimed.
The subsequent metadata review also corrected successful schema migration's
`actual_schema` projection: it now returns the acknowledged new version, not
zero. The AOT fixture asserts this result, and all five storage projection/owner
tests pass. The worker/backend and unrelated runtime tests were not replayed
for this VM-only projection correction.
Both strict production Clippy profiles pass after that correction. Fresh AST
inputs confirm the API-boundary input is byte-identical; affected headroom,
module-structure, and dependency-impact checks pass again in
`storage-metadata-schema-quality.log`. Rust/Terlan formatting, source quality,
Rust documentation, and dormant-module checks pass without budget changes.

The next local-only slice adds a bounded pure-Rust volatile engine with explicit
byte/count limits and no filesystem authority. Append/CAS/replay/schema decisions
are shared with the durable SQLite engine; it does not reuse the old test model
or advertise memory as persistence. All 27 backend cases pass, including failed
quota admissions without partial writes, replay after schema changes, compaction,
and isolation between independent stores; strict backend all-target Clippy passes
(`storage-local-backend-*`). That backend-only evidence did not cover public
local-only routing, aggregate owner budgets, or source execution.
The shared-rule refactor also passes the rebuilt durable AOT restart/corruption
fixture, three real-worker tests, and eleven storage protocol/projection tests.
Both production Clippy profiles, source/documentation quality, AST-backed
structural/dependency checks, and formatting pass without raised budgets
(`storage-local-shared-*`). Existing workspace-support ownership covers the new
backend cases; no additional release test producer was added.

The follow-up local wiring now connects `force_local` and requested `local_only`
policies directly to independent actor-owned stores, without worker RPC or host
filesystem authority. Adapters reserve bounded capacity; all storage resources,
loaded outcomes, and extracted snapshots share owner/runtime byte and count
limits. Owner exit reclaims descriptors and reservations. Local flush remains a
volatile barrier; durable-flush requirements are denied and durable proofs fail
explicitly. Close/reopen retains the same adapter's memory, not process durability.
All eighteen targeted storage/ownership/reply tests pass. The existing compiled
Terlan restart/corruption fixture also passes with local lifecycle, CAS, batch
rollback/replay, schema changes, compaction, retained views, restore, independent
stores, and durability rejection added to the same image build
(`storage-local-wiring-*`). Both strict production Clippy profiles, formatting,
Rust source-size/documentation and dormant-code checks, plus AST-backed API,
headroom, module-structure, and dependency-impact gates pass without raised budgets.
This source evidence is Linux-only. Authenticated independent-peer replication,
remaining API bindings, cross-target execution coverage, and release closeout
remain required; no release checklist item is closed by this slice.

The recovery-metadata follow-up binds `local_sequence`, `incoming_sequence`,
`expected_entries`, and `persisted_entries`. Ordinary obsolete writes now report
the backend-observed high-water mark and rejected checkpoint sequence separately
from explicit CAS-token mismatches. Exact retained replay still succeeds;
compacted replay is rejected without advancing state. Atomic local/SQLite
outcomes have no partial-write counts, and unknown commits cannot be presented
as known rollback or measured partial progress. Unsupported partial-write
receipts fail explicitly rather than inventing counts.
All thirteen storage-runtime tests and fifteen selected real Terlan source tests
pass. The worker-backed restart/corruption fixture passes after correcting its
obsolete CAS assertions for compacted replay; both failed diagnostic runs remain
recorded alongside the passing final result (`storage-recovery-metadata-*`).
The source tests no longer claim durability for local memory, and now distinguish
invalid schema zero from a valid-but-stale schema version. Real durable proof
coverage remains in the sandboxed worker fixture. Both strict Clippy profiles pass.
Rust/Terlan formatting, source/documentation and dormant-code checks, and fresh
AST-backed API/headroom/module/dependency gates also pass without raised budgets.
The full eighteen-test standard suite still fails compilation at
`missing_resource_handle/1`. Ten declarations remain unbound: eight resource
custody/validation APIs and two replication APIs. The other three source tests
are not waived or counted as passing. Registering an arbitrary string must not
be presented as proof that a live VM-owned resource is available after restore.

The custody/replication prerequisite now adds a persistent, non-authorizing store
identity. Format 3 records OS-random identity bytes, while valid format-2 databases
upgrade transactionally without losing checkpoint data or schema. Initialization
is synchronized before committing metadata. Corrupt/missing/zero identities are
rejected, never regenerated. A new internal open observation reads identity and
the committed boundary together; the VM pins the identity and fences an existing
adapter if reopening observes another store. Database copies retain logical
identity, so this is neither authentication nor independent-replication evidence.
Thirty backend tests passed initially; the one failed future-format assertion
used the newly supported version 3 as its sentinel. It was changed to the maximum
signed SQLite version and passed on its own, without replaying the thirty passing
cases. Backend strict all-target Clippy and twenty-three storage worker/projection
tests pass (`storage-identity-*`). The new harness is
`terlan-98602fad98a5228f`. Read-only follow-up checks pass for formatting, Rust
source/documentation quality, dormant-code inventory, and fresh AST-backed
API/headroom/module/dependency gates (`storage-identity-quality.log`), without
raised budgets. After explicit approval, only the two shared incremental caches
`terlan-2sq45fapry09n` and `terlan-06x0wg9uhfijg` were deleted, reclaiming about
3.4 GB without removing source, binaries, or test evidence. The compiler, VM, and
native worker rebuild passed (`storage-identity-executable-build.log`). The real
sandboxed-worker restart test passed with persistent identity and committed state
(`storage-identity-worker-restart.log`); the compiled Terlan fixture also passed
separate-VM restart, transactions, and corruption/proof rejection against those
rebuilt executables (`storage-identity-source-restart.log`). Both strict production
workspace-bin Clippy profiles passed (`storage-identity-production-clippy.log` and
`storage-identity-all-features-clippy.log`). These checks validate the identity
addition, not resource custody or replication, and do not close the release.
The user approved supervisor-authorized resource-name bindings. Registration must
resolve existing supervisor authority and reject unknown or unbound names; it
must never create authority. Restore and restart must revalidate resource identity
and explicitly reject stale or replaced resources. A persistent logical database
identity alone is not proof of resource custody. Implementation and real-source
validation remain pending; no string-only custody registry has been installed and
all ten public declarations remain unbound.

Supervisor identity continuity now works across complete VM restarts, not just
reopening an existing adapter. The optional binding form
`--storage NAME@IDENTITY=/absolute/private/directory` accepts exactly 64 lowercase
hexadecimal digits for a nonzero identity supplied by trusted supervisor
configuration. Every new durable adapter inherits that pin. A mismatching store
is fenced before source mutations or proofs; retries and new adapters cannot
silently learn a replacement identity. The unpinned provisioning form retains
its explicitly documented adapter-lifetime-only guarantee. Copies sharing the
same logical identity are still not independently authenticated replicas.
Twenty-one targeted tests pass, including malformed/duplicate binding rejection
and fresh-runtime identity checks. The rebuilt compiler/VM/worker pass the real
Terlan AOT fixture with both a wrong supervisor pin and an independently created
replacement database, unchanged replacement checkpoint state after denial, and
successful recovery using the original pinned database. Both strict production
Clippy profiles, Rust/Terlan formatting, source/documentation checks, dormant-code
inventory, and fresh AST-backed API/headroom/module/dependency gates pass without
raised budgets (`storage-supervisor-pin-*`). This closes the cross-VM identity-pin
prerequisite only; the eight resource-validation and two replication declarations
remain unbound, and no release checklist item is closed.

The eight resource-validation declarations now execute through production AOT on
Linux. The first concrete provider resolves `db.NAME` only to supervisor-configured
storage binding `NAME` with an explicit identity pin. Registration and validation
query the real worker asynchronously; unknown, unpinned, replaced, revoked, or
unregistered resources cannot manufacture authority. At most sixteen unique names
are checked, with fixed owner accounting and one deadline shared across all probe
continuations. No registration or validation proof is advanced before every probe
and receipt-capacity check succeeds. Proofs are immutable historical observations,
not leases or cross-worker atomic snapshots. Empty validation records zero; actor
exit releases registrations, and restart requires fresh registration against the
supervisor's retained pin. Restored strings never restore resource authority.
The old source test's assumption that an unconfigured local adapter can register
an arbitrary string has been replaced by explicit unsupported behavior. Positive
source assertions are in the real two-worker AOT fixture, including fresh-VM
re-registration, replacement denial and recovery. Twenty-five targeted Rust tests,
the rebuilt real AOT lifecycle fixture, both strict production Clippy profiles,
formatting, source/documentation and dormant-code checks, and fresh AST-backed
API/headroom/module/dependency gates pass (`storage-resource-*`). The first harness
build exposed a missing typed-error conversion in the continuation failure path;
it was corrected before the passing build and tests. Exactly two storage
declarations remain unbound: `replicate_snapshot` and `require_cluster_replication`.
Authenticated independent-peer replication, cross-target source execution, the
declaration-derived release coverage gate, and overall release closeout remain open.

Replication transport groundwork now reuses the production VM-owned Hyper/rustls
adapter rather than depending on the `serve` command. Mutual TLS tests reject
missing and untrusted client certificates before HTTP dispatch. The shared
adapter now reports truncated TLS connections instead of treating a raw socket
close as authenticated EOF, drains buffered plaintext before reporting closure,
handles write backpressure without a successful zero-byte write, and bounds
authentication with a 30-second VM-owner timer. Hyper still owns HTTP; no second
scheduler or hand-written TLS/HTTP protocol was introduced. Ten focused tests and
both strict production Clippy profiles pass (`storage-tls-*` diagnostics).
The production check caught and corrected an initial extraction into a test-only
HTTP module; the adapter is now `runtime::vm::hyper_tls`, included in production.
These tests are transport evidence, not peer authorization, durable replication,
or public `replicate_snapshot` execution evidence. Both replication declarations
and release readiness remain open.

The baseline Effect execution gap is repaired locally. All seven real
`std.core.EffectTest` tests pass through the rebuilt compiler and VM. Their old
unconditional `true` bodies did not prove execution; those placeholder passes
remain withdrawn. The release manifest names the actual `Effect.run` API.

The compiler now generates shared, concrete native runners for plans built with
the typed combinators and checked direct `Mapped`/`FlatMap` constructors.
Callback admission uses canonical descriptor storage and concrete signatures,
not helper names. Checked actor-local envelopes preserve intermediate
types and callback signatures. Ordinary VM-owned call/continuation lowering
executes callbacks, including captured and suspending callbacks; no interpreter
or worker-RPC execution fallback was introduced. Runtime plan depth does not
generate additional runners and shares the existing specialization budget.
Linked tests cover parameter-carried plans, lexical result inference, different
callback input types sharing one output type, mixed-type flat-map, and a
512-node runtime plan using scheduler-owned recursive continuations. Direct
constructor tests cover named callbacks, captured suspending lambdas, and tuple
and case bindings. Wrong callback arity, non-callable values, and flat-map
callbacks without Effect results fail native admission before execution.

Execution of supported failures preserves the typed value through actor exit,
links, monitors and resource cleanup. Cancellation terminates the owning actor;
later callbacks do not run. Compiled-image tests exercise String and Int
failures and cancellation, including heap release. Type-query tests distinguish
a legitimate type mismatch from invalid, foreign or stale envelopes. Forged
and replayed failure authority is rejected before actor state is changed.

These tests also exposed shared compiler defects: lexical run results needed
refreshing after producer specialization; constructor-pattern bindings were
missing from closure lifting; conditional callers lacked lifted callback-owned
continuation metadata. The fixes reuse the existing type solver and pattern
binder and retain shape-only external continuation records, not copied bodies.
The direct-descriptor slice also repairs checked lambda inference and
tuple-bound callable types. Stored callbacks become owned closures; immediate
calls and their compiler-generated aliases retain static lowering. Existing
static-call regressions remain enforced rather than being waived.

Effectful comprehensions now construct deferred plans; mapped guards execute
only at the VM boundary, retain generator order, and short-circuit later guards.
Failure and cancellation are verified by executing the compiled source fixture
and asserting the owning actor's terminal reason, replacing checks that merely
expected compiler rejection. The two purity placeholders now execute real
guard and deferred-plan assertions. A short-circuit test now contains an actual
division-by-zero trap in its skipped branch.

Nested suspending callbacks enter through a precise-root VM continuation rather
than accumulating native frames behind the fixed indirect-call transition
buffer. Named callbacks and lambdas share that scheduler-owned entry; pure
callbacks retain their direct path. The buffer capacity was not increased.
Generated Effect types are expanded before generic specialization. Checked
range filters retain their normalization, and GuardResult folding no longer
rewrites unrelated user functions based on a matching short name.

Support is not advertised as unrestricted: arbitrary Dynamic execution is not
supported, and executable functions or opaque resource handles are not
supported failure payloads. The module and individual API documentation
disclose these limits. Broader payload support and the remaining capability
audit stay open; passing baseline tests are not proof that every expressible
Effect plan is supported.

Effect checkpoint evidence: 610 NativeIR tests (including all 15 Effect-runner
tests), 35 source tests through the rebuilt CLI (21 comprehension, two purity,
seven Effect, five GuardResult), and two compiled-image failure/cancellation
tests pass. The previous checkpoint additionally passed
150 managed-memory, 193 actor, 75 native-execution-boundary, 16 native-image,
48 process and eight termination tests. Focused selections overlap the broader
groups and are not additional unique-test totals. Both strict workspace-binary Clippy profiles, Rust
documentation, formatting, API boundaries, module structure and file headroom
pass. Refreshed dependency-impact verification retains 27 production domains,
13 production coupling edges, 13 test coupling edges and 13 integration-test
targets. Documentation checks cover 81 Markdown files.
At that checkpoint string errors remained 3,082; their inventory only relocated the extracted
dispatch function. Four newly affected large files were split without raising
limits, leaving 59 near-limit files with no-growth schedules.

Effect evidence is under `target/quality/release-diagnostics/effect-comprehension-*`;
the preceding checkpoints use `effect-descriptor-*` and `effect-execution-*`;
earlier runner-development evidence uses `effect-runner-*` and
`effect-terminal-*`. Existing quality validators consume current Rust AST
inputs and source files; their use is not a claim that the complete validation
toolchain has been rebuilt and accepted with this compiler.

The first source acceptance attempt failed on range-filter compilation; only
the other 14 tests passed at that attempt. Its command correctly returned a
nonzero exit status. The current 35-test result comes from the subsequent
successful run, not from that failed attempt.

The preceding source-manifest sweep passed 203 tests through Bool, Binary's
typed empty-list correction, and GuardResult's five tests. Instant now imports
Duration explicitly. Those corrections are preserved. The remaining manifest
traversal, later HTTP placeholder-coverage audit, full compiler/script validation,
and committed-candidate closeout remain required.
All three release checklist items stay open. Changes remain local and
uncommitted; no push, merge, tag or publication has occurred.

### Cluster session execution checkpoint

All twelve session/frame operations now dispatch on the VM execution thread,
using the existing coordination state machine and TETF codec, without spawning
native workers. Immutable snapshots retain exact frame identity for outbound
acknowledgements, preventing independent sessions or sibling snapshots from
acknowledging each other's same-numbered messages. Resource ownership, stored
kind, generation checks, and actor cleanup share the existing typed registry.
All five disconnect reasons remain distinct. Reconnect also checks application
identity, non-regressing peer epochs, and monotonic reconnect ticks.

The receiver's admitted AOT image supplies the atom vocabulary; incoming payloads
cannot expand it. Portable transport rejects reserved native-handle fields and
uses the same nesting limit for encoding and decoding. This is an in-memory
frame API, not an implementation of cross-process networking.

Executing the suite exposed and repaired shared ABI gaps: closed unions with
nominal record variants now receive discriminated layouts and checked pattern
projections; foreign nominal identities remain errors. Source record construction
uses the same layout. Nullary public union values reuse the existing image-checked
variant validator, and tuple capability arguments preserve their tuple shape.

Local evidence in `target/quality/release-diagnostics/`:

- `cluster-session-split-source-tests.log` and its result manifest: all **16**
  real Terlan tests pass through the rebuilt compiler/VM; no validation-only rows.
- NativeIR: **627** tests pass across the seven focused nominal-identity tests
  and 620 remaining tests, including existing identity-rejection cases.
- Native backend: **25** tests pass, including nullary atom admission, rejecting
  unknown/payload-bearing variants, and allocation rollback.
- Cluster adapters: **15** tests pass; TETF: **9** tests pass.

Strict workspace binary Clippy passes with both default and all features,
without new lint allowances. The refreshed API inventory passes with **3082**
internal string-error sites and unchanged budgets. Rust documentation has zero
undocumented items, and the dormant-runtime audit still reports four explicitly
classified modules. These scoped results do not replace full candidate gates.

The no-growth size gate also caught the managed-value bridge entering its
headroom band. Shape selection/materialization now lives in a cohesive 159-line
child module, leaving `managed_values.rs` at 773 lines. The 25 backend tests and
all 16 Terlan tests pass after the extraction. File headroom, module structure,
and dependency-impact gates pass with no raised limits or coupling budgets.

This checkpoint does not close V9-3. Full declaration-derived API coverage,
candidate-bound results on every advertised target, the remaining release API
audit, and exact-candidate release verification are still required. The current
manifest-consistency check is not that enforcement gate. No release checkbox,
commit, tag, or publication is created by this checkpoint.

### Earlier Cluster membership execution checkpoint

All fourteen Membership operations now call the existing VM membership state
machine directly, alongside the four Profile operations. The state machine is
production code shared with its Rust tests, not a separate adapter algorithm.
Source updates clone the view and retain the prior view; profiles and views
share one typed, actor-owned resource registry. Relabelling a handle cannot
alias another resource kind, and actor cleanup preserves other owners.

The first nine-test source acceptance attempt exposed an unqualified nested
record pattern in AOT lowering. A diagnostic seven-test selection then passed
only the two profile tests: five membership tests failed because the adapter
returned records where the boundary required atoms. Neither attempt counts as
acceptance. Canonical identity qualification now traverses patterns at binding
sites, and lifecycle/health values use their declared atom representation.
After rebuilding, all nine source tests pass, with per-test runtime results in
`target/quality/release-diagnostics/cluster-membership-final-source-results.json`.
The full-suite retry still rejects the session `accept` placeholder.

The final scoped Rust evidence is 625 NativeIR tests and 38 native-helper tests;
the unchanged coordination and distributed-scheduler selections passed 38 and
56 tests. Both strict workspace-bin Clippy profiles pass, as do API-boundary,
module-structure, file-headroom and dependency-impact checks. String-error debt
remains 3,081; extraction redistributed existing entries without raising the
budget. Evidence is under `target/quality/release-diagnostics/cluster-membership-*`.
These results do not establish session support or full release API coverage.

The historical `std-package-coverage-100` check now identifies itself as
manifest consistency only: its rows are not unique executed tests and it does
not measure declaration completeness. Whole-release source execution coverage
is required by V9-3, including existing APIs and shipped packages. The stronger
inventory/execution gate remains to be implemented. JS validation-only reports
must not satisfy it; the subsequent execution-honesty fix reports them as not
executed and fails the test command. Nothing in this checkpoint closes a release item.

### Earlier Cluster profile implementation checkpoint

The four profile operations (`profile`, `node_id`, `epoch`, `next_epoch`) now
use a direct VM-owned adapter and the existing coordination profile validation.
Resource ownership uses a shared typed registry extracted from the native
adapter store, retaining generation and actual-owner checks and actor cleanup.
Unknown Cluster operations fail explicitly without starting an external worker.
Profiles alone do not implement membership, transport sessions or networking;
the module documentation states this limitation.

Two source profile tests passed with the initial rebuilt CLI. Six focused Rust
tests pass, including immutable epochs, overflow, forged handles, owner cleanup
and rejection without worker fallback. Separate regression selections passed
193 native-boundary, 38 coordination, 28 non-Cluster native-helper and 623
NativeIR tests. These runtime results predate the subsequent helper-process
extraction. File-headroom validation caught `package_native_helper.rs` growing
from its 982-line baseline to 990 lines. Extracting the existing subprocess
lifecycle and bounded request transport reduces it to 890 lines, with a
114-line `helper_process.rs`; no allowance was raised or protocol changed.
The final CLI rebuild, post-extraction runtime tests and strict Clippy profiles
remain outstanding; the initial source test
result predates the final epoch-overflow guard. Low disk space prevents safely
starting another large build. Evidence is under
`target/quality/release-diagnostics/cluster-*`. This is partial implementation,
not acceptance of the full Cluster suite or release closeout.

Post-extraction AST-based API-boundary and module-structure checks pass; Rust
documentation reports zero undocumented items. The initial quality-consumer
attempt failed with `error[tvm.image.seal_write]: No space left on device`.
The subsequent scoped checks use `TMPDIR=/run/user/1000` for the same sealed
validator image, preserving private image admission and avoiding shared-cache
deletion. This temporary-directory change is not evidence that Cargo builds
or full release validation can finish with the remaining disk space.

### Completed Task execution checkpoint

Completed tasks use compiler-owned, canonical managed storage on the owning
actor, with no interpreter or worker-RPC fallback. Payloads survive aliases,
repeated observations, suspension, nested tasks and typed empty collections.
`result` preserves the standard Error identity even when a caller declares an
unrelated Error type. General asynchronous Task scheduling is not implemented
or advertised by this repair.

The acceptance tests exposed and repaired four shared compiler issues:
normalized case-valued Boolean conditions were rejected despite having native
lowering; expected collection types were not propagated into completed tasks;
fresh Result aliases lost their pattern-bound payload types; generic calls could
narrow a declared union based on argument order. Build and test source closures
also omitted type-only providers, giving declared and inferred Error payloads
different managed identities. Both loaders now retain those schema dependencies;
runtime semantic-type checks remain enforced.

Scoped validation passes 623 NativeIR tests (including eight new regressions),
three production source-closure tests, 827 typechecker tests, 38 formal-pipeline
tests, six checked-cache tests and 136 target-profile tests. The one ignored
typechecker test retains its separate closeout owner and is not counted as passed.
The final source sweep passes 112 tests across Unit, Option, Result, Object,
Error, Equal, Int, Float, Task and Effect. Both strict workspace-bin Clippy
profiles, Rust documentation, API-boundary, module-structure and file-headroom
checks pass without raising quality budgets. Evidence is under
`target/quality/release-diagnostics/task-*`. This is local repair evidence, not
committed-candidate, installed-artifact or publication acceptance.

### Selected-function import checkpoint

The next source sweep passed 99 tests across Unit (7), Option (17), Result (14),
Object (4), Error (3), Equal (11), Int (19), and Float (24). Equal first failed
native admission because checked selected imports were treated as ambiguous
whole-module imports. CoreIR now retains sorted provider/function/alias
provenance in serialization and contract fingerprints. Reachability preserves
the selected candidates until typed resolution chooses the unique provider.
Unrelated providers and private bodies are not admitted, and genuinely
ambiguous imports retain loud errors. Intrinsic-only providers use their
retained checked interface signatures and the shared intrinsic lowering.

Scoped validation passes 615 NativeIR tests, 827 typechecker tests, 38 formal
pipeline tests, six checked-cache tests and 136 target-profile tests. The one
ignored typechecker test remains owned by `stdlib-release-contracts-check` and
is not counted as passed. New regressions cover concrete Int/Float dispatch,
import aliases, ambiguity rejection, serialization, and mixed intrinsic/source
providers. String-error debt falls to 3,081 without a budget increase. The
near-limit typechecker module remains at 950 lines.

Evidence is under `target/quality/release-diagnostics/selected-import-*`, with
the successful Equal source run in `selected-import-intrinsic-Equal.log`.
The earlier failed attempts remain diagnostic evidence, not accepted runs.
The original Task failure is recorded in `selected-import-core-Task.log` and is
superseded by the completed Task checkpoint above. The completed
import repair does not close V9-1, V9-2, V9-3, or the broader capability audit.

### Random adapter checkpoint

All nine Random operations now use the existing Rust RNG through the resource-owned
standard dispatcher. Generator state stays immutable and process-owned; draws
return a new generation-checked handle. Generic collection values retain their
shape, tuple transport preserves ownership and term budgets, and resource-owner
validation now descends into records as well as lists and tuples. Package helpers
are not required for these safe standard operations.

The current compiler passes all 16 Random API tests and both Random property tests.
Scoped Rust validation passes 795 distinct tests, including the complete NativeIR
selection and affected native-boundary/VM regressions. Both strict Clippy profiles,
Rust documentation and formatting pass. The rebuilt quality validator passes
documentation, API boundaries, module structure, file headroom and refreshed
dependency-impact verification. Its size remains 12,003,336 bytes; near-limit source
files decrease from 65 to 64 without raised limits. Evidence uses `random-bridge-` under
`target/quality/release-diagnostics/`.

At this checkpoint the next release-manifest check, BinaryTest, failed: 61 tests
passed and 11 failed with
a managed semantic-type mismatch in byte construction. A minimal probe passes
`Bytes.from_list([42])` but rejects `Bytes.from_list([])` and its lexical alias.
The required byte operand schema is not applied to the empty-list bottom layout.
This was not waived or skipped. The earlier recorded Binary checkpoint passed all
72 tests; these regressions are corrected in the current status above. The failed
reports remain available as diagnostic evidence.

### Captured-callback and empty-map checkpoint

Captured callbacks now keep their checked function signatures through escaping
closure conversion, lexical aliases and branches. Lambda parameters still shadow
outer bindings. Linked execution covers callbacks that suspend and resume, and
the full Property suite passes all 18 tests, including shrinking and replay.

Standard List, Map, Set and Iterator declarations keep their compiler-owned
collection storage rather than becoming native-package resource handles.
Package-defined namesakes retain their resource boundary. Unconstrained empty Map
slots receive an uninhabited Never schema; concrete consumer and mutation types
remain intact. The full Map suite now passes all 13 tests.

The final combined compiler passes all 572 NativeIR tests and 158 source tests
across fourteen standard-library suites. RandomProperty was rechecked and still
fails both tests: the standard random adapter is routed to a missing package helper.
It is not waived. Both strict Clippy profiles, Rust documentation, formatting,
API boundaries, module structure, file headroom and refreshed dependency-impact
verification pass. The rebuilt quality validator remains 12,003,336 bytes.
Evidence uses `callback-map-` under
`target/quality/release-diagnostics/`, with focused repros under `captured-contract-`
and `empty-map-`. These are local, uncommitted corrections, not exact-candidate
release acceptance. All three roadmap items remain open; no push, merge, tag or
publication has occurred.

### Empty-list correction checkpoint

The current local correction gives checked `List[Never]` values per-use layouts
instead of permanently pinning a binding to its first consumer. One empty value
can reach both Int and String consumers. A producer still executes exactly once;
only its uninhabited result is adapted. Concrete and user-defined collections are
not retyped. Generic `List.new()` retains its checked consumer constraints rather
than acquiring new polymorphic semantics.

Mutation refinement now happens at the write, not at the initial bottom binding:
reading an empty list as String before pushing an Int no longer changes the earlier
read's type. Mutation receivers survive generic callback instantiation, and unknown
rebindings invalidate earlier witnesses. Generic inference now handles scoped let
expressions, including the sequencing needed to retain producer effects. Runtime
semantic checks stay enabled and Never is not replaced with Unit.

All 567 NativeIR tests pass through non-overlapping final selections. The actual
compiler and VM pass 127 standard-library source tests, including Iterator and
PropertyDistribution, plus four source probes for reuse, effects, read-before-write
and runtime filters. The effect probe records exactly two producer invocations:
one bound producer reused twice and one direct call. Both strict workspace-binary
Clippy profiles, Rust documentation, language coverage, formatting and whitespace
checks pass. The final quality-validator rebuild and its documentation, API
boundary, module structure, file headroom and dependency-impact gates pass. The
rebuilt validator is 12,003,336 bytes. Evidence uses
`empty-reuse-` under `target/quality/release-diagnostics/`.

The preceding empty-literal and runtime-argument corrections remain: [] retains
its checked bottom element; concrete nested siblings provide a shared layout;
runtime string-list contracts refine bottom literal annotations without changing
concrete casts or discarding effects. Their earlier validator passed API/module/
headroom, dependency-impact and docs gates at 12,003,336 bytes. Completed work is
rerun only for changed compiler inputs, not as an unchanged-input retry.

Three required full source suites still fail on this compiler: Map (unconstrained
empty-map inference), Property (a captured callback loses its closure type), and
RandomProperty (the standard random adapter is routed to a missing package helper).
All three were rechecked. Property's shrinking test reproduces its failure alone,
independently of empty-generator input. None is waived. These are local, uncommitted
corrections, not exact-candidate release acceptance; all three roadmap items remain
open, with no push, merge, tag or publication.

The previously failing `empty-bottom-reuse-probe.log` case now passes; the original
failure and its passing `empty-reuse-final-` run are both retained. This does not
close the unrelated empty-map or captured-callback failures above.

### Previous correction checkpoints

The current local correction preserves a constructor's already resolved provider
through CoreIR identity annotation. A selected import must not be rebound by its
bare name when another provider exports the same spelling. Module-style constructor
facades still acquire their final type component. Calls, constructor chains and
patterns share the same idempotent rule. Linked execution covers two distinct user
records named List alongside the standard collection constructor, fixing the
previously recorded imported-record alias failure.

The final compiler selection passes 622 tests, including all 553 NativeIR tests.
Another 875 frontend/backend tests pass; the separately owned release-scale stdlib
contract test also passes: 1,498 distinct scoped Rust tests. The actual compiler
and VM pass 110 List/Functional/Option/Result/Gen/Shrink and collection-property
source tests. Both strict workspace-binary Clippy profiles, Rust documentation and
language coverage pass. The validator rebuild succeeded on one bounded incremental
retry after an unexplained SIGTERM; its artifact remains 12,003,336 bytes. Completed
test owners were not replayed, and the terminated build is not counted as passing.
Evidence uses `record-alias-` under
`target/quality/release-diagnostics/`. These are scoped correction checks, not the
canonical exact-candidate release campaign.

The preceding collection correction is retained: call-result types use argument
witnesses and the shared generic unifier without erasing symbolic constructor
context; qualified standard List types share builtin storage. The preceding
receiver-declaration, reachability and generic struct initializer fixes also remain.

Five required full source suites still fail on the rebuilt compiler: Map and
Iterator on unconstrained empty collections, Property and PropertyDistribution on
empty-generator inference, and RandomProperty on native adapter routing. All five
were rerun and none was waived. Changes remain local and uncommitted; there has
been no push, merge, tag or publication. All three roadmap items remain open.

The preceding correction retains explicit trait-instance arguments in serialized
CoreIR and reachability, and selects generic implementation bodies before argument
monomorphization. Linked regressions verify return-only trait selection, imported
aliases, pruning and rejection of incompatible generic arguments. Named callbacks
inside Option/List containers use the existing owned-closure ABI. Private-helper
specialization respects lexical shadowing, preserves static invocation-only callbacks, and retains
owned values when forwarding callbacks. Concrete and generic calls share contextual
lambda typing. Generic arguments now use their instantiated parameter layout;
Result literals no longer arrive as plain tuples at union-typed callees.

The combined compiler run passes 706 tests, including all 543 NativeIR tests.
Another 1,046 frontend/backend checks pass; the separately owned release-scale
stdlib contract gate executes its one required test and passes. The actual compiler
and VM pass all 12 Functional tests, three selected List trait tests, and 83
Option/Result/List/Map/Gen/Shrink source tests. Both strict workspace-binary Clippy
profiles, Rust documentation and language-feature coverage pass. A headroom failure
prompted an unchanged extraction of structural type scoring: overloads.rs is now
845 lines, and all eight affected tests pass after the split. API/module/headroom
and refreshed dependency checks pass without increased budgets. The validator
rebuilt by the final compiler remains 12,003,336 bytes. These runs do not replace
the required exact-candidate release validation.
The preceding qualified-import and managed-buffer correction is retained.
Six required source suites still fail: List (each callback resolution), Map and
Iterator (unconstrained empty collections), Property and PropertyDistribution
(empty-generator inference), and RandomProperty (native adapter routing). All six
were rerun on the current compiler; no assertion or suite was waived. Logs use
`trait-callback-` under `target/quality/release-diagnostics/`. These are scoped local
correction results, not full-candidate acceptance. All three roadmap items stay open.

The preceding correction composes suspending arguments before an escaping callback's
tail call, using the same fast-path predicate as ordinary call lowering. Linked
execution covers nested calls and a managed list surviving multiple suspensions.
Bulk Map/Set construction now derives its schema from a generic source callable's
checked list result before receiver resolution; both specialization passes share
that derivation. All 537 NativeIR tests pass. RangeProperty now passes both tests,
and 12 Map tests, ten Set API/property tests and 23 Gen tests pass (47 source tests).
Both strict workspace-binary Clippy profiles, API/module/headroom, refreshed
dependency-impact and documentation checks pass with unchanged budgets. The rebuilt
validator remains 12,003,336 bytes.
The full Map suite still rejects its unconstrained-empty-map test. The six remaining
required failures are RandomProperty (native adapter routing), PropertyDistribution
and Property (empty-generator inference), MapTest (unconstrained-empty-map inference),
IteratorTest (untyped-empty-list inference), and ListTest (ambiguous trait resolution).
Logs use `range-tail-`, `map-generic-entries-` and `range-map-` under
`target/quality/release-diagnostics/`. These are scoped local correction results,
not candidate-wide acceptance; all three checklist items remain open.

The preceding constructor correction retains explicit type arguments through CoreIR,
adapters, defaults, chains and specialization. Transparent tuple payloads keep
their checked element types; concrete generic structs retain distinct registered
layouts within the existing specialization budget. Local private type bodies and
parameters are retained without exporting them through module interfaces.
The final combined compiler/frontend/backend/artifact run passes 1,683 tests,
including all 537 NativeIR tests; the existing release-scale std-contract test
keeps its separate execution owner. Object, Shrink, Gen and Set source suites
pass 62 tests. Both strict workspace-binary Clippy profiles, the rebuilt quality
validator, API/module/headroom, fresh dependency-impact and documentation gates
pass without increased budgets. Near-limit files decrease from 68 to 65, and the
validator remains 12,003,336 bytes. Evidence uses `constructor-type-` and
`constructor-private-` in `target/quality/release-diagnostics/`. All seven required
source-suite failures listed below were rechecked at that checkpoint; none was waived.
The Range failure had a minimal escaping-callback repro in
`range-tail-before.log`: a suspending tail call retains a suspending ordinary call
inside its arguments. These are local correction results, not candidate-wide
acceptance; all three checklist items remain open.

The preceding correction normalizes Unit's expanded singleton type inside
native callback signatures and generic arguments. Collection inference reuses
the existing closed-variant merge, and closed atom domains have a canonical
order across alias expansion and inferred lists. All 530 NativeIR tests pass,
including linked Unit callbacks and mixed/reversed atom lists with a named
callback. OrderingProperty's three source tests and a separate reversed-domain
named-callback probe pass. Iterator's selected Unit callback test and all seven
Unit tests pass; the full Iterator suite now reaches an untyped-empty-list
inference failure. Another 81 affected collection/generator/table/atom/shrink
source tests pass on the final atom-domain correction. Both strict workspace
binary Clippy profiles, the rebuilt quality validator, API/module/headroom,
refreshed dependency impact and documentation gates pass with unchanged budgets.
The rebuilt validator remains 12,003,336 bytes. Logs use
`unit-callback-`, `atom-list-` and `atom-domain-`; these are scoped local results,
not candidate-wide acceptance.

The preceding source-constructor correction retains checked bodies and constant
defaults as ordinary typed callables, preserves their reachability, and reuses
the existing generic and suspension pipeline. Generated bodies, default helpers,
arity adapters and specializations retain original constructor-clause provenance.
The artifact regression verifies exact debug spans for defaults, overloaded
clauses and generic varargs. The latest combined NativeIR/Core-lowering/debug
artifact run passes 572 tests; both strict workspace-binary Clippy profiles and
the actual Rust-quality validator rebuild pass. The rebuilt image is 12,003,336
bytes versus 11,977,424 previously. This is local correction evidence, not a
candidate-wide pass. API boundaries (3,082), module structure, file headroom
(68 near-limit files), refreshed dependency impact and documentation checks pass
without increased budgets. Logs use `source-constructor-` under
`target/quality/release-diagnostics/`.

Object's API/property suites and Shrink now pass (29 tests combined); GenTest
passes 23 and Set's API/property suites pass another ten. Remaining required
source-suite failures are RangeProperty (suspension composition), RandomProperty (native adapter routing),
PropertyDistribution and Property (empty-generator inference), MapTest (empty-map
receiver inference), IteratorTest (untyped-empty-list inference), and ListTest
(ambiguous trait resolution). None is waived. All roadmap items remain open.
The earlier explicit-constructor gap is now corrected: the original
`Items[Int]().length()` source-file probe passes, and linked regressions verify
distinct Int/String schemas, defaults, imported constructors and private types.

The preceding local collection correction passes all 515 NativeIR tests and all 23
GenTest tests. Intrinsic-only providers retain their type declarations; collection
mutation inference follows lexical scope; returned entry lists, structural callback
results and explicitly typed empty generators retain their checked schemas.
Map/Set/iterator operations use the same canonical managed identities as registered
layouts, including Option[String]. Option/Result/List/Map/Set property suites and
SetTest pass another 23 source tests. Both strict workspace-binary Clippy profiles,
the rebuilt quality validator, API boundary (3,082), module structure and file
headroom (68 near-limit files, unchanged budgets) pass locally. The linked string
lookup regression also exercises an explicitly expanded Option alias.
At that checkpoint, seven property modules plus MapTest and IteratorTest still
failed; the updated outstanding inventory is above. Evidence uses `collection-witness-` under
`target/quality/release-diagnostics/`. These are scoped local results, not full
candidate acceptance; all roadmap items remain open.

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
now passes all 12 tests after lexical tail-call profiles require explicit pure
callee evidence. Escaping callbacks use the ordinary yield/continuation lowerer;
continuation interning rewrites lifted roots as well as named-function roots.
Linked execution covers captured values, non-tail calls, direct yields and both
branches of a callback factory. All 497 NativeIR tests pass. Base64's closed
error atoms are admitted with its provider, and the existing pure Rust MD5
adapter uses the direct-safe std dispatcher. Base64 passes 11 tests, MD5 two,
the other affected std suites 127, and the adapter/helper Rust suites 43.
Both strict Clippy profiles, API/module/headroom and dependency gates pass for
this correction without increased budgets; function_lowering.rs shrinks from
937 to 911 lines and its no-growth ceiling decreases. Logs use `closure-codecs-` under
`target/quality/release-diagnostics/`. These are local correction results, not
candidate-wide acceptance. The subsequent required property-suite batch exposes
collection receiver resolution, Atom conversion, contextual generic/union-layout
and suspension-composition failures, plus URI error atoms and Random helper
routing. That batch records 46 passing tests, seven runtime failures and 11 modules
that fail before execution. A follow-up retains contextual callback parameter and
map-constructor types, reuses checked collection receiver contracts, and infers
empty-list types from push operands. Late alias expansion and a shared Option
payload matcher keep iterator layouts consistent. Empty-list inference excludes
custom receiver initializers. All 499 NativeIR tests pass;
the List and Map property suites now pass three and four tests respectively, and
Table's 12 tests still pass. The other property failures remain required baseline
fixes, including Object and generator receiver resolution. Both strict Clippy
profiles and the rebuilt API/module/headroom/dependency gates pass with unchanged
budgets; evidence uses `collection-receivers-`. No candidate-wide success is claimed.
The next correction lowers `Atom.to_string` and `String(atom)` through the
managed-string ABI using the immutable image-local atom table. A linked test
executes runtime-selected ASCII and Unicode atoms, and malformed-index/operation
tests reject invalid inputs. Atom's three property and three API tests pass.
URI's fixed `uri.parse` error code is now admitted with its provider, so all three
URI properties and four API tests pass, including malformed input. The correction
passes 541 additional NativeIR/runtime tests and eight atom-inventory tests;
logs use `atom-text-`. Both strict Clippy profiles and the rebuilt
API/module/headroom/dependency checks pass with unchanged budgets. The other
required property failures and all three roadmap items remain open.
No source assertions are removed and no failed suite is waived.
The next local correction retains explicit call type arguments through CoreIR
and generic specialization, including return-only parameters. Contextual
return-only arguments retain their enclosing generic substitution; argument-owned
inference is not replaced by unresolved callee type variables. Specialization
keys include resolved type arguments. Inferred lists retain their checked runtime
schemas, and repeated contextual passes no longer accumulate identical casts.
All 506 NativeIR tests pass, including linked regressions for explicit and
contextual generic returns and lists of function results. Option's two and
Result's four property tests now pass. The affected frontend, accelerator,
formal-pipeline, JS/Rust backend and target-profile checks pass 1,132 tests;
the pre-existing release-scale std-contract test retains its separate execution
owner. Both strict Clippy profiles and the rebuilt API/module/headroom/dependency
gates pass without increased budgets. The string-error inventory only relocates
one existing helper; its total stays 3,082. Near-limit source files decrease from
69 to 68. Option/Result API and List/Map property regressions pass 38 more tests.
The remaining eight property modules still fail and are not waived.
Evidence uses `generic-context-final-`, `generic-schema-frontend-backends` and
`generic-closeout-` under `target/quality/release-diagnostics/`. These results are
scoped local evidence, not candidate-wide acceptance. All roadmap items stay open.
Both strict Clippy profiles and the rebuilt
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
    Once the support owner exists, clean-candidate Cargo owners additionally
    bind actual Git-listed source bytes and the selected revision before
    execution/reuse and before sealing;
    source changes invalidate reuse even when Git status hides the edit.
    These boundary observations require an exclusively owned checkout, not
    adversarial transient-mutation protection. First-ever support compilation
    still needs pre-build admission of source bytes and external Cargo
    configuration/tool inputs; its post-build snapshot and shell-level
    revision/manifest checks do not close that requirement.
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
  - Complete verification of supported local `DistributedStorage` execution;
    replication is explicitly unsupported in this release by the scope decision
    above, not simulated or counted as implemented:
    - Use a maintained storage engine for transactional local persistence;
      never treat in-memory `flush` counters as durable acknowledgement.
      Keep database identity, payload schema, atomic batch/CAS boundaries,
      compaction, corruption checks, and retry semantics explicit.
    - Authorize configured local storage paths through VM-owned
      capabilities. Blocking storage operations must run off shard owners;
      preserve bounded admission, owner/epoch fencing, cancellation, and
      indeterminate-commit handling when a worker dies or a reply is lost.
    - Implement resource-name registration against supervisor-authorized live
      bindings. Unknown names must fail without creating authority or advancing
      validation proof metadata. Persist the expected resource identity where
      required for recovery, and revalidate it against the authorized provider
      after restart or restore. Reject missing, revoked, stale, or replaced
      resources explicitly; a string registry or logical database UUID alone
      is not custody evidence. Exercise these cases through production AOT,
      including successful recovery with the original authorized resource.
    - Document cluster replication, quorum writes, and automatic failover as
      unsupported in 0.0.9. Verify false capability queries and typed rejection
      of replication requests, including caller-supplied availability flags and
      configured local backends. Rejection must not append, advance sequence or
      proof metadata, or contact a peer. Retain the declarations and separately
      report their negative execution evidence; do not imply positive coverage.
    - Execute every supported storage operation from Terlan through production
      AOT. Verify restart/restore, failed atomic batches, stale writers,
      corrupted/incompatible checkpoints, and worker failure.
      Capability booleans must reflect actual configured backends, not a caller
      supplied `available=true` assertion. Keep all coverage obligations open
      until the claimed local capabilities' execution and backend evidence pass.
  - Require 100% Terlan-source execution coverage of advertised public APIs:
    - Apply this to the entire current release candidate, including existing
      APIs and shipped packages, not just changed code or newly introduced
      APIs. Keep package-owned tests with their packages and consume their
      candidate-bound results in the release report without duplicate runs.
      Enforce the same completeness rule for subsequent release candidates.
    - Derive the inventory from public declarations, including methods,
      overloads, constructors and re-exports by canonical identity. Compare it
      against supported-target claims; do not use only the existing handwritten
      manifest as the denominator or silently omit uncovered APIs.
    - Bind every supported API/target obligation to a Terlan test that compiles
      through the production backend and executes observable assertions against
      the API on that target. Cover documented success, error, boundary and
      lifecycle contracts as applicable; Rust unit tests and interface checks
      are complementary, not substitutes.
    - Reconcile mapped tests with the exact candidate's execution results.
      Missing, skipped, failed, declaration-only, placeholder or stale evidence
      must fail acceptance. A manifest row or a passing unrelated test is not
      execution proof. Reuse the canonical test run rather than rerunning tests
      to produce coverage evidence; one test may satisfy several proven
      obligations, with unique execution counts preserved.
    - Report explicitly unsupported API/target pairs separately, backed by
      diagnostic tests; never count rejection as positive capability coverage.
      An existing advertised capability gap remains a defect to repair, not a
      reason to shrink the coverage denominator or change the support claim
      without review. Generated JS declarations likewise need real JS-target
      execution evidence for any runtime support claim.
    - Reject incomplete declaration inventories and uncovered added APIs in the
      release gate. Report API execution coverage separately from Rust line or
      branch coverage; 100% API coverage is not a claim that every behavior has
      been proven.
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
