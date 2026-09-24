# Terlan 0.0.10 Roadmap

Updated: 2026-09-24.

## Scope And Order

These requirements were explicitly postponed from 0.0.9 on 2026-09-10.
0.0.9 is the focused build/release optimization release; finish and verify that
release before starting this feature work. No postponed item is marked complete.
On 2026-09-24, the user delegated a narrower release-scope decision: unfinished
replicated storage also moves here, retaining quorum writes and automatic
failover as requirements rather than replacing them with all-peer replication.

Preserve the separate self-hosting checkout and integrate it deliberately.
The [self-hosting plan](ROADMAP_SELF_HOSTING.md) owns compiler migration phases;
the [accelerator plan](ROADMAP_0_0_10_CUDA.md) retains its existing evidence and
package-owned follow-ups. Reuse the release execution and recovery machinery
established by [0.0.9](ROADMAP_0_0_9.md); do not duplicate validation.

## Active Checklist

- [ ] V10-1: Integrate and close the first self-hosted frontend vertical slice.
  - Bring the separate self-hosting work into the normal build deliberately;
    preserve unrelated development changes and do not infer readiness from a
    successful standalone package build.
  - Complete Phases 0 and 1 of the [self-hosting plan](ROADMAP_SELF_HOSTING.md):
    versioned boundaries, bootstrap provenance, spans/tokens, module/import and
    public-interface extraction, and a maintained positive/negative corpus.
  - Connect canonical Rust frontend evidence and run the named Phase 0/1 gates.
    Compare tokens, interface identity, diagnostics, and byte spans; minimize
    disagreements and retain actionable reproducers.
  - Register each gate with the established release execution owner. Proposed targets must be
    implemented and wired into the normal tier, not merely listed in a document.
  - Acceptance: the complete maintained corpus agrees across Rust and Terlan,
    including malformed inputs, and clean bootstrap/recovery is demonstrated.
    Rust remains authoritative at this milestone. Full parsing, typechecking,
    lowering, driver migration, and `terlc1`/`terlc2` convergence remain explicit
    later phases; scheduling them requires the preceding gates to pass. This
    item must not be presented as completion of full compiler self-hosting.

- [ ] V10-2: Define and verify the native compatibility policy.
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

- [ ] V10-3: Deliver the runtime inspector TUI.
  - Use the existing debugger/runtime inspection boundary to show actors,
    shard ownership, mailbox pressure, timers, supervision, and pending native
    operations. Do not create a parallel debugger or scheduler state model.
  - Provide bounded snapshots/streams, disconnect handling, and access control.
    Start with inspection; any state-changing controls need explicit semantics.
  - Acceptance: exercise a running multicore application, actor exit/restart,
    stale identities, overload, and client disconnect. Inspection must not block
    shard owners or expose unsafe memory. Include CLI help and user examples.

- [ ] V10-R: Deliver replicated storage with a default Raft-backed provider.
  - Provide an implementation-neutral storage service/package above the VM.
    Raft is the default consensus implementation, not part of language or VM
    core semantics. The VM supplies scheduling, timers, bounded asynchronous
    I/O, supervision and capabilities; isolated workers perform persistence.
    Use maintained consensus and protocol libraries, not handwritten protocols.
  - Keep public operations independent of Raft terms and log indexes. Alternative
    providers must satisfy the advertised consistency, durability and recovery
    contract and its conformance suite. Weaker guarantees require explicit
    selection; no silent substitution. Choose a provider when creating a group;
    replacing it for existing data requires a verified migration, not hot reload.
  - Implement majority-quorum commits and automatic leader election/failover.
    Persist the algorithm's required term, vote, log and membership state before
    the corresponding acknowledgements. Apply only committed operations; couple
    durable application progress and retry deduplication to state changes.
    A lost reply may leave an indeterminate commit, never an assumed rollback.
  - Fence stale leaders and former members. Minority partitions cannot complete
    successful writes; reads claiming linearizability require a verified leader
    and applied commit barrier. Bind authenticated peers to authorized group and
    member identities. Discovery cannot grant votes or shrink the quorum.
    Membership changes must use the library's safe protocol or be explicitly
    unsupported; do not rewrite the voter set during restart or configuration reload.
  - Use independent peer processes and independent durable stores. Bound message,
    log, snapshot and pending-request resources; validate snapshot installation,
    log compaction and restart. Blocking persistence must stay off shard owners.
    A local append, shared file, logical UUID, or TLS handshake is not replication.
  - Acceptance: production Terlan/AOT tests cover quorum commits, leader crashes
    before/after commit, reply loss and duplicate retries, majority/minority
    partitions, stale leaders after healing, follower catch-up, snapshot recovery,
    unauthorized peers, corruption, and full-cluster restart. Include three- and
    five-voter configurations; failover requires a communicating majority and
    makes no Byzantine-fault or arbitrary exactly-once-effect claim. Test-provider
    models and passing rejection tests are not positive replication evidence.
    Admit the pinned dependency graph through the normal security audit and run
    the shared release gates without duplicate validation or weaker budgets.

- [ ] V10-4: Build supervised actor recovery on verified logical checkpoints.
  - Reuse the local persistence and logical-checkpoint contracts verified for
    0.0.9. Distributed recovery additionally depends on V10-R; 0.0.9 does not
    claim replication. This item owns actor/mailbox/timer recovery semantics.
  - Extend the verified 0.0.9 logical-checkpoint contract with explicit actor
    recovery semantics: mailbox/timer treatment, message delivery, snapshot consistency,
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

- [ ] V10-5: Add HTTP/3 through maintained protocol libraries.
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

- [ ] V10-6: Close and publish 0.0.10 with accurate user-facing support claims.
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
    0.0.9 publication authorization does not authorize a 0.0.10 tag or release.

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
