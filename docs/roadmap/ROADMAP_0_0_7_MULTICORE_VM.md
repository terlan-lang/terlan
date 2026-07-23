# Terlan 0.0.7 Mini Multicore VM Roadmap

This is the focused execution plan for multicore Terlan actor execution. It
decomposes the scheduler, preemption, actor-isolation, and native-execution
requirements already owned by Slice 40 in
[`ROADMAP_0_0_7.md`](ROADMAP_0_0_7.md). It does not replace the main roadmap,
make benchmark machinery count as VM completion evidence, or authorize
multicore implementation before the direct-AOT pivot closes.

This roadmap is **inactive until the complete AOT roadmap is finished**.
Scheduler-thread implementation, BEAM/ERTS multicore inventory work, ownership
refactoring, new MC gates, and performance claims all begin after the AOT
Completion Boundary passes. AOT itself must nevertheless preserve the
multicore-readiness constraints below so it does not freeze single-thread-only
interfaces that this roadmap would immediately have to replace.

The normative actor, heap, continuation, and scheduler-thread contract remains
[`TVM_NATIVE_DATA_ABI_SPEC.md`](../runtime/TVM_NATIVE_DATA_ABI_SPEC.md).
If this file conflicts with that specification or the main roadmap, the
specification and main roadmap win.

## Execution Rules

1. Before selecting MC-1, verify Slices 100 and 101A through 101F in
   [`ROADMAP_0_0_7.md`](ROADMAP_0_0_7.md) remain checked and rerun
   `make tvm-aot-roadmap-reconciliation-check`. If AOT reconciliation fails,
   stop and repair the AOT boundary; do not begin multicore implementation.
2. After activation, work only on the first unchecked top-level multicore item,
   scanning from the start of this file. Do not skip or reprioritize it.
3. A top-level item includes every nested requirement, positive and adversarial
   test, named gate, report, inventory update, and closeout gate.
4. Change `[ ]` to `[x]` only after the implementation and every named gate
   pass. Partial evidence stays beneath an unchecked item.
5. Preserve the one-scheduler configuration as a supported deterministic mode
   throughout the migration. Adding threads without parallel actor execution
   is not progress evidence.
6. Each actor must have exactly one mutator owner while executing. No design may
   permit two scheduler threads to execute or mutate one actor concurrently.
7. BEAM/ERTS material is design and failure-feedback input. Mine invariants and
   port relevant tests; do not transplant ERTS data layouts, lock structure,
   tagged terms, emulator loops, or C ownership into the Terlan runtime.
8. Do not restore deleted BEAM/ERTS source trees to product history merely to
   inspect them. Use a pinned upstream checkout outside the product runtime and
   record its revision in the invariant inventory.
9. Ordinary Rust source files must remain at or below 1,000 lines and test files
   at or below 2,000 lines. Run `make rust-quality-check` at closeout.
10. A blocked item stops multicore work. Do not select a later item to remain
   busy.

## Activation Boundary

Multicore work becomes active only after the completed AOT Slices 100 and 101A
through 101F are revalidated. In particular, the VM
must already have complete reachable-function lowering, managed values,
same-shard actor fast paths, migrated consumers, zero transitional execution
inventory, and no evaluator or worker-transport fallback for ordinary local
actors.

Documentation of this future roadmap is not implementation progress. BEAM/ERTS
mining, mutator-token refactoring, new multicore gates, and scheduler-thread
prototypes are deliberately deferred so they target the final AOT execution
model rather than preserve a transitional runtime.

### AOT multicore-readiness constraints

These are AOT design constraints, not permission to start an MC item early:

- parked or yielded continuations must contain owned captures, stable image and
  continuation identities, and precise roots without a native stack identity,
  scheduler-thread address, thread-local borrow, or worker-connection identity;
- same-shard runtime calls must receive explicit actor and execution context
  rather than depend on a process-global mutable evaluator or registry lock;
- actor-local heaps must remain independently owned and movable at safepoints;
  no ordinary managed reference may depend on the address of a scheduler-local
  cache or thread stack;
- mailbox transfer must preserve the normative multi-producer,
  single-consumer publication contract even while the pre-multicore
  implementation has only one producer thread;
- AOT-8 must delete ordinary pure-worker sidecars, evaluator state, and global
  pure-native execution serialization, leaving external NativeBoundary workers
  as explicit isolated services rather than the local actor execution path; and
- AOT closeout evidence must state that these interfaces are compatible with a
  future exclusive mutator owner, without claiming that compatibility is
  multicore implementation or proof.

## Completion Boundary

Multicore VM support is complete only when all of the following are
simultaneously true:

- one execution shard owns a configurable pool of scheduler threads derived
  from explicit configuration and available host parallelism;
- at least two independent AOT actors execute simultaneously on distinct
  scheduler threads when the host exposes at least two logical CPUs;
- actor heaps, continuations, mailboxes, links, monitors, timers, resources,
  and failure state remain actor-isolated under scheduler migration;
- every actor has one exclusive mutator token, with acquisition, release,
  safepoint transfer, stale-owner rejection, and actor-exit cleanup proven;
- the actor directory, lifecycle word, mailbox publication path, scheduler
  queues, and reclamation policy have a documented happens-before contract and
  executable memory-model tests;
- local run queues, wakeups, bounded stealing, parked actors, and shutdown make
  progress without a global actor-execution, process-table, registry, timer, or
  tracing lock on the common actor path;
- compiler-generated AOT safepoints can yield, preempt, migrate, and resume a
  continuation without preserving a native stack identity;
- ordinary same-shard AOT actors own independently resumable execution contexts
  and never serialize through an evaluator, pure-worker connection, or
  shard-global native-execution mutex;
- mailbox, signal, timer, I/O, NativeBoundary completion, cancellation,
  supervision, hot-reload, and shutdown races have executable adversarial
  coverage;
- EPMD advertises one ready logical node endpoint rather than individual cores,
  and transport admission routes incoming work to scheduler-owned actors
  without coupling discovery registration to one scheduler thread;
- actor failure remains actor-local, external NativeBoundary worker failure
  remains worker-local, and scheduler-thread panic or ownership corruption
  fails the execution shard closed for supervisor restart;
- production scheduling is observable, while record/replay mode can reproduce
  a captured scheduler decision stream without claiming that normal host
  thread interleavings are deterministic;
- one-scheduler semantic results remain identical, multicore semantic results
  remain valid under repeated randomized schedules, and no race is accepted as
  a performance tradeoff; and
- every multicore gate and required closeout gate passes.

## Pre-Activation Scorecard — 2026-07-22

This table was refreshed after AOT closeout and before MC-1 implementation.
The remaining entries describe the single-scheduler foundation, not multicore
completion evidence.

| Boundary | State | Current evidence |
| --- | --- | --- |
| Cooperative scheduler semantics | Partial foundation | Weighted queues, reductions, yielding, blocking, cancellation, and telemetry exist |
| Parallel actor execution | Open | `VmScheduler::run_next` executes one mutable process slice at a time |
| Actor mutator ownership | Open | The ABI requires it; no production scheduler-thread token transfer exists |
| Scheduler thread pool | Open | Host threads exist in benchmarks, not as the actor scheduler |
| AOT migration safepoints | Partial foundation | Stack maps and continuations exist; cross-thread actor migration is not proven |
| Concurrent mailboxes and signals | Open | Current actor runtime is composed through one mutable owner |
| Native execution parallelism | AOT foundation | Ordinary local actors use the direct AOT execution path and external native boundaries remain isolated services; scheduler-owned parallel actor execution is still open |
| Record/replay | Partial foundation | Logical scheduler telemetry exists; captured multicore decision replay does not |
| EPMD/node lifecycle | Complete | Managed fixed-scheduler node listener, bounded current-owner routing, exact discovery registration, migration-stable lookup, and supervisor-owned withdrawal are proven |
| Multicore performance proof | Open | No actor-runtime scaling gate exists |

## How BEAM/ERTS Helps

BEAM/ERTS is the primary reference for concurrency invariants and accumulated
failure cases, not the target implementation architecture.

| Reference area | Invariant to extract | Terlan-owned replacement |
| --- | --- | --- |
| Per-scheduler run queues | Runnable ownership, wake/sleep transitions, bounded stealing, empty-poll behavior | Rust scheduler workers with explicit queue ownership and typed transitions |
| Process scheduling state | Exactly one executing owner, safe runnable/blocked/exiting transitions | Actor mutator token plus generation-checked state machine |
| Message and signal delivery | FIFO guarantees, signal ordering, exit/monitor races, wakeup correctness | Typed mailbox and signal queues with actor-owned receive state |
| Reduction scheduling | Bounded execution slices, preemption, starvation control | Compiler safepoints and VM-owned reduction accounting over AOT continuations |
| Timers, ports, and I/O | Park/wake races, cancellation, late completion, shutdown | Reactor/native completion events admitted through scheduler-owned wakeups |
| Scheduler stress suites | Rare interleavings and lifecycle failures | Focused Rust model tests and executable Terlan/AOT adversarial fixtures |

The invariant inventory introduced below must record the pinned upstream
revision, source or test path, extracted invariant, Terlan semantic decision,
owning test, and disposition. Direct line reuse is not the default; any proposed
reuse requires explicit provenance and license review.

The following ERTS details are non-goals unless a later Terlan-owned requirement
demonstrates otherwise:

- BEAM instruction dispatch, opcode accounting, tagged-term representation,
  and emulator stack layout;
- ERTS process locks, allocator shards, reader groups, pollsets, dirty-scheduler
  labels, scheduler affinity flags, and online scheduler mutation APIs;
- exact ERTS run-queue counts, migration paths, thread names, or diagnostic
  tuple shapes;
- Erlang distribution/ETF mechanics, node-name modes, cookies, or remote
  scheduler behavior; and
- unrestricted NUMA placement, real-time scheduling classes, or executing one
  actor concurrently on multiple cores.

## Required Concurrency Model

The first implementation must preserve the following architecture. A different
design may replace it only with equivalent executable ownership, memory-order,
and progress evidence.

### Actor storage and ownership

- A shard-global generational actor directory resolves stable actor identities
  to independently addressable actor cells. It must not require a mutable
  shard-global `BTreeMap` borrow to execute, send to, inspect, or retire one
  actor.
- Each actor cell separates an atomic lifecycle/owner word and MPSC mailbox
  publication state from heap, continuation, receive, and mutable process state
  accessible only while holding the actor's exclusive mutator token.
- Actor ids and owner tokens carry generations. Queue entries, wake tickets,
  timer completions, native completions, monitor/link signals, and steal
  tickets validate those generations to prevent ABA reuse.
- Reclamation waits until the actor is terminal, absent from every run queue,
  no wake or completion can publish against its generation, and no directory
  reader can still resolve the retired actor. The implementation must use a
  proven generational, epoch, hazard, or equivalent reclamation scheme rather
  than timing assumptions.

### Publication and progress

- Publishing runnable actor state, mailbox fragments, completions, or migration
  state happens-before the release enqueue or wake transition; a scheduler
  performs the matching acquire before inspecting that state.
- The parked/runnable/queued handshake has one authoritative state transition
  that prevents both lost wakeups and duplicate simultaneous execution.
- Mailbox producers publish validated, fully reserved fragments without
  acquiring the receiver's mutator token. Only the receiver integrates actor
  heap references at a safepoint.
- No actor executes while holding a shard-global directory, registry, timer,
  tracing, code-server, or run-queue lock. Blocking I/O and external native
  work always park the actor before leaving scheduler ownership.
- Every shared lock has a documented order and bounded critical section. No
  condition wait, worker reply, socket operation, actor execution, or heap
  collection occurs while such a lock is held.

### Scheduler policy

- Scheduler count uses an explicit override when supplied; otherwise it uses
  effective host parallelism after container or cgroup CPU limits, clamped to
  at least one and a documented shard maximum.
- Actors begin on a deterministic home scheduler. Local enqueue is preferred;
  remote wakeups target the current owner; migration changes ownership only at
  a published safepoint.
- Each scheduler owns priority, normal, and background queues. Weighted local
  service, idle detection, victim selection, steal class, steal batch size,
  failed-steal backoff, and sleep/wake transitions receive explicit bounded
  policies.
- Fairness is a shard-wide reduction and maximum-wait contract. Per-scheduler
  weighting is insufficient if a runnable actor can starve on another queue.
- One-scheduler mode uses the same actor directory and state machine but
  preserves the existing deterministic semantic order.

### Failure policy

- Ordinary actor failure remains isolated to that actor and its declared
  link/monitor/supervision effects.
- Unsafe or external NativeBoundary worker failure terminates that worker and
  fails only its attributed requests unless declared supervision escalates it.
- Scheduler-thread panic, poisoned scheduler ownership, impossible lifecycle
  state, or mutator-token corruption terminates the execution shard. The shard
  supervisor restarts it; the first multicore implementation does not attempt
  in-process recovery from partially mutated scheduler state.
- EPMD registration belongs to the logical node/shard supervisor, not a
  scheduler thread. The node registers only after its listener, transport
  router, and scheduler pool are ready, and unregisters after admission stops
  during fail-stop or orderly shutdown.

### Record and replay authority

- Stable event identities are defined with the actor ownership state machine,
  before multiple scheduler threads are enabled.
- Record mode captures scheduler choice, owner generation, queue transition,
  message/signal publication order, logical timer delivery, I/O/native
  completion order, migration/steal result, image generation, cancellation,
  failure, and externally supplied nondeterminism needed by replay.
- Replay follows the captured decision stream under a controlled scheduler. It
  does not claim that ordinary production host-thread interleavings repeat.
- Per-scheduler bounded trace buffers publish independently; diagnostics may
  merge them by stable sequence metadata without adding a global hot-path
  tracing lock.

## Ordered Work

- [x] MC-1: inventory BEAM/ERTS invariants and freeze the Terlan concurrency
  contract.
  - Create a machine-readable inventory covering run queues, lifecycle and
    mutator ownership, memory publication, work stealing, reductions,
    migration, mailboxes, signals, links, monitors, timers, ports/I/O,
    cancellation, failure, reclamation, and shutdown.
  - Pin the upstream Erlang/OTP revision used for mining without adding it as a
    product runtime dependency or restoring deleted implementation history.
  - Classify every case as `port-semantic-invariant`,
    `port-adversarial-test`, `terlan-different-api`, or
    `remove-erts-implementation-detail`.
  - Map every retained invariant to a Terlan-owned contract paragraph, test
    identity, and planned gate. An uncited statement that BEAM handles a race
    is not evidence.
  - Freeze the actor lifecycle state machine, owner-generation layout,
    happens-before edges, lock-order policy, wakeup handshake, reclamation
    rule, scheduler failure policy, and replay event identities described by
    this roadmap before implementation begins.
  - Add adversarial checker tests for duplicate rows, missing revisions,
    unowned retained invariants, invalid classifications, deleted paths treated
    as product dependencies, and ERTS mechanics mislabeled as Terlan semantics.
  - Gate: add `make vm-multicore-invariant-inventory-check` and run
    `make terlan-vm-erl-suite-audit-check` and `make rust-quality-check`.
  - Exit: the runtime can be implemented from explicit Terlan concurrency
    contracts without retaining ERTS architecture or source dependencies.

  Evidence (2026-07-22):
  [`TVM_MULTICORE_INVARIANT_INVENTORY.json`](../runtime/TVM_MULTICORE_INVARIANT_INVENTORY.json)
  pins official Erlang/OTP `OTP-29.0.1` revision
  `f26c7e590c5d1b3afa0dee38093442df117822e3` as reference-only material and
  classifies 18 rows across all 16 required domains. The inventory covers all
  four dispositions and maps every retained row to one of the 12 frozen
  clauses in
  [`TVM_MULTICORE_CONCURRENCY_CONTRACT.md`](../runtime/TVM_MULTICORE_CONCURRENCY_CONTRACT.md),
  a stable test identity, and a planned gate. Eleven adversarial checker tests
  cover duplicate rows, revision integrity, ownership, classifications,
  dependency leakage, ERTS-mechanic misclassification, domain coverage, path
  escape, and contract drift. `make vm-multicore-invariant-inventory-check`,
  `make terlan-vm-erl-suite-audit-check`, and `make rust-quality-check` pass.

- [x] MC-2: replace monolithic process ownership with a generational actor
  directory and exclusive mutator state machine under one scheduler.
  - Resolve stable actor ids through independently addressable actor cells
    rather than requiring one mutable process-table borrow for execution,
    messaging, inspection, or retirement.
  - Separate the atomic lifecycle/owner word and mailbox publication boundary
    from heap, continuation, receive, and mutable process state protected by
    the exclusive mutator token.
  - Implement queued, executing, yielding, parked, migrating, exiting, retired,
    and reclaimed transitions with actor and owner generations.
  - Implement safe directory lookup and reclamation with a proven generational,
    epoch, hazard, or equivalent scheme.
  - Emit stable ownership and transition event identities now so later
    scheduler threads, tracing, and replay use the same state machine.
  - Reject double acquisition, stale generation release, execution without
    ownership, migration with borrowed state, retirement while queued, and
    reclamation while any lookup or completion can still resolve the actor.
  - Add bounded model tests for every transition, ABA reuse, cancellation at a
    safepoint, exit during transfer, stale directory handles, and fail-stop
    ownership corruption.
  - Gate: add `make vm-actor-mutator-ownership-check`; run
    `make vm-scheduler-fairness-check`, `make tvm-managed-memory-check`, and
    `make rust-quality-check`.
  - Exit: one scheduler uses the final actor storage and ownership model without
    changing one-scheduler semantic ordering.

  MC-2 execution evidence (2026-07-22): implementation proceeded under the
  explicit activation-gate exception recorded for this execution run. The
  process table now stores actors in generation-qualified reusable directory
  slots. Each cell owns a packed atomic lifecycle, actor generation, owner
  generation, and scheduler owner word; lookup pins block reclamation, mailbox
  publications receive receiver-local generation-qualified sequence ids, and
  scheduler execution can mutate actor state only through a checked mutator
  token. One-scheduler yield, park, cancellation, exit, retirement, and requeue
  paths emit stable transition identities without changing observed run order.
  Eighteen focused tests cover every lifecycle, scheduler and control-plane
  ownership generations, double acquisition, stale owner release, missing
  ownership, migration while owned, exit during migration, direct queued
  retirement, lookup-pin reclamation, ABA slot reuse, stale handles and
  publications, invalid owners, corrupt state, mailbox publication,
  cancellation at a safepoint, control-mutation unwinding, and one-scheduler
  ordering. Production actor
  state no longer exposes an unowned mutable process getter: scheduler,
  memory, receive, code-frame, timer, HTTP, resource, and ACME mutations use
  scoped generation-checked ownership, while mailbox insertion uses its
  separate generation-validated publication boundary. The ownership gate also
  compiles the production `terlan-vm` binary with the test-only compatibility
  getter absent. `make
  vm-actor-mutator-ownership-check`, `make vm-process-model-check`, `make
  vm-scheduler-contract-check`, `make tvm-managed-memory-check`, and `make
  rust-quality-check` pass.

  The completed non-AOT gate policy again leaves focused VM checks independent
  from the canonical workspace suite during the AOT cutover, matching the
  Makefile's documented ownership rule. `make vm-scheduler-fairness-check`,
  `make tvm-managed-memory-check`, and `make rust-quality-check` pass together.
  All MC-2 production source passes the quality scanner and remains below
  1,000 lines. Real cross-thread mailbox fragment publication remains assigned
  to MC-3.

- [x] MC-3: implement MPSC mailbox, signal publication, and race-free wakeups
  under one scheduler.
  - Publish validated, fully reserved mailbox fragments without acquiring the
    receiver's mutator token; integrate actor-local managed references only
    when the receiver owns the actor at a safepoint.
  - Preserve promised per-sender ordering, priority/system-signal ordering,
    selective receive, save-queue behavior, and mutation-free failed sends.
  - Implement one authoritative parked/runnable/queued handshake with release
    publication and acquire consumption so a concurrent producer can neither
    lose a wakeup nor enqueue simultaneous executions.
  - Define and test message versus exit, signal versus unregister, monitor/link
    delivery versus actor retirement, and late publication versus generation
    reuse.
  - Use bounded memory-model exploration for compact queues and state machines,
    plus seeded stress for mailbox floods, priority storms, exit races, and
    repeated park/wake cycles.
  - Gate: add `make vm-multicore-mailbox-publication-check`; run
    `make vm-process-model-check`, `make vm-failure-primitives-check`, and
    `make rust-quality-check`.
  - Exit: the final cross-thread publication protocol is proven before a second
    scheduler thread can reach it.

  MC-3 execution evidence (2026-07-22): every actor cell now owns an established
  bounded `concurrent-queue` MPSC queue. Producers reserve capacity and publish a complete
  generation-qualified fragment without receiver mutation; the receiver drains
  it only under its checked mutator token and then applies existing priority
  and selective-receive ordering. The queue owns one active/parking/parked/
  notified handshake, while actor release and producer publication converge on
  an at-most-once parked-to-queued lifecycle transition. Seven focused queue
  tests cover four-producer ordering, an eight-producer seeded flood at exact
  capacity, unique publication sequences, mutation-free backpressure,
  pending-message park rejection, exact parked wake action, 1,000 repeated
  park/publish/drain cycles, and all 56 bounded interleavings of the compact
  park/publish/release model. Actor-directory coverage publishes through the
  actual shared directory from four host threads, proves a publication during
  owned execution prevents a stale park, integrates only after receiver
  ownership is reacquired, rejects exiting and retired targets, and starts a
  reused generation with an empty queue. Sends to an executing actor leave the
  fragment pending for integration by that actor at its next safepoint rather
  than seizing its mutator. The process suite remains green with 49 tests.
  `make vm-multicore-mailbox-publication-check` passes and
  owns focused priority/signal ordering, link/monitor, registry-retirement,
  process, failure, and reference regressions; `make rust-quality-check` and
  `git diff --check` also pass.

- [x] MC-4: add multiple scheduler threads with fixed actor placement.
  - Split shard control from per-scheduler queues, metrics, wake handles,
    allocator/runtime context, and bounded trace buffers without introducing a
    shard-global actor-execution lock.
  - Derive default scheduler width from effective container-aware host
    parallelism, support an explicit bounded override, and preserve one as the
    minimum and deterministic compatibility width.
  - Assign actors to deterministic home schedulers and route message, signal,
    and completion wakeups to the current owner without migration or stealing.
  - Preserve priority/normal/background service, reduction charging,
    starvation bounds, parking, cancellation, and orderly shutdown on every
    scheduler.
  - Treat scheduler panic, poisoned ownership, impossible lifecycle state, and
    partial scheduler startup as fail-stop shard failures attributed to the
    supervisor; do not attempt in-process repair.
  - Prove one-scheduler results and event order remain stable, while two
    schedulers record overlapping AOT actor execution on distinct identities.
  - Add adversarial tests for simultaneous spawn, remote enqueue, wake storms,
    partial startup, scheduler panic, shutdown with runnable actors, and queue
    ownership corruption.
  - Gate: add `make vm-multicore-fixed-placement-check`; run
    `make vm-multicore-mailbox-publication-check`,
    `make vm-scheduler-fairness-check`, and `make rust-quality-check`.
  - Exit: communicating AOT actors execute safely in parallel but remain pinned
    to their home schedulers.

  MC-4 execution evidence (2026-07-22, first slice): the VM now owns one
  bounded scheduler topology derived from effective host parallelism, Linux
  cgroup v2 quota and cpuset limits, or the explicit
  `TERLAN_VM_SCHEDULERS` override. Scheduler identities are nonzero in actor
  ownership words, width one retains the primary owner, and deterministic
  shard-global actor routes distribute by identity without migration. The AOT
  handler runtime consumes this topology, starts one bounded owner queue per
  scheduler, pins entry/resume/cancel to the same route, and rejects a route
  delivered under another local process or scheduler identity. Independent
  scheduler runtimes now acquire actor mutators under distinct scheduler
  owners. Six topology tests cover bounds, placement, owner identity, cpuset,
  quota, and effective-limit selection; focused owner and generated-handler
  lifecycle tests remain under `make vm-multicore-fixed-placement-check`.
  That first slice left the shard-global directory, MC-3 remote publication,
  parallel AOT execution, and fail-stop lifecycle as the next boundaries.

  MC-4 execution evidence (2026-07-22, second slice): all fixed schedulers now
  share one VM-owned generational actor control directory. Structural
  registration and reclamation use short `RwLock` write sections; ownership,
  publication, drain, park, and release use bounded read sections, and an
  acquired execution lease outlives no directory lock. The actual native HTTP
  path registers each shard-global route there, parks it after generated code
  suspends, publishes typed resume/cancel events through the MC-3 MPSC mailbox,
  wakes only its deterministic home scheduler, reacquires the generation-
  checked lease, and reclaims the route after completion. Two scheduler
  threads now overlap real generated AOT export execution on distinct named
  owners. A scheduler panic is caught at the owner boundary, reported through
  `VmExecutionShardSupervisor`, latched across the whole handler generation,
  and closes peer admission while still permitting owner joins and orderly
  peer shutdown. Four shared-control tests cover remote ordered publication,
  concurrent leases, wrong ownership, duplicate registration, and stable
  one-scheduler events; five compiled-handler tests cover sticky suspension,
  cross-scheduler routing, exact typed wake, overlapping AOT execution, and
  generation-wide panic closure. The remaining boundaries were per-scheduler
  metrics and bounded traces, explicit partial-startup and runnable-shutdown
  fault injection, and production cross-scheduler message/signal traffic.

  MC-4 completion evidence (2026-07-22, third slice): every fixed owner now
  records scheduler-local counters and a 1,024-event bounded trace for command,
  generated entry, message publication/dispatch, signal publication/dispatch,
  park, completion, failure, and shutdown boundaries. Typed I/O wake values
  travel as ordinary actor messages; cancellation travels as a distinct system
  signal, and both are fully published through the shared MC-3 mailbox before
  the home scheduler is notified. Generation startup now has deterministic
  partial-failure rollback that closes admission and joins every owner already
  started. Orderly shutdown joins scheduler owners and then retires and
  reclaims all unowned queued or parked routes; an executing route rejects the
  whole control-plane shutdown during a side-effect-free preflight instead of
  guessing ownership. Eight shared-control tests cover concurrent execution,
  32 simultaneous registrations, a 64-publication wake storm with one enqueue,
  ordered remote delivery, wrong ownership, duplicate registration, stable
  one-scheduler events, runnable shutdown, and executing-route fail-closed
  behavior. Three telemetry tests cover bounded eviction, concurrent producer
  accounting, and event classification. Seven compiled-handler tests cover
  fixed placement, two-scheduler generated AOT overlap, message and signal
  dispatch, partial startup, scheduler panic, sticky suspension, and exact
  typed wake ownership. `make vm-multicore-fixed-placement-check` passes with
  its inherited mailbox, scheduler-fairness, memory, process/failure, and Rust
  quality gates.

- [x] MC-5: implement explicit AOT actor migration at published safepoints.
  - Require every preemptible AOT path to expose precise roots, owned captures,
    stable image/function/continuation identities, and a transfer-safe actor
    context without a native stack or thread-local borrow.
  - Publish the complete heap, continuation, resource, and scheduler state with
    release ordering before transferring the owner generation; the destination
    acquires and validates it before resume.
  - Start with an explicit deterministic migration request between two selected
    schedulers. Work stealing is forbidden in this item so transfer failures
    remain attributable to migration rather than scheduling policy.
  - Preserve actor-local collection, managed relocation, stack-map validation,
    mailbox state, resource ownership, and continuation authority across
    repeated migrations.
  - Reject missing maps, raw native stacks, scheduler-local pointers, stale
    migration tickets, ABA owner generations, cross-actor roots, and resume on
    two schedulers.
  - Gate: add `make tvm-aot-multicore-migration-check`; run
    `make tvm-aot-runtime-transition-check`, `make tvm-managed-memory-check`,
    `make vm-actor-mutator-ownership-check`, and `make rust-quality-check`.
  - Exit: a long-running AOT actor migrates repeatedly between explicitly
    selected schedulers with identical results and no duplicated execution.

  MC-5 execution evidence (2026-07-22, first slice): actor routes now preserve
  an immutable home scheduler separately from the scheduler currently
  authorized to execute them. The shard-global control directory owns a linear
  migration ticket containing exact source and destination routes plus actor
  and scheduler-owner generations captured only after mutation is released and
  lifecycle reaches `Migrating`. Completion validates the ticket, publishes
  the destination route under the directory write boundary, and queues the
  actor there; abort restores the source queue. Publications accepted during
  migration remain in the existing generation-qualified MC-3 mailbox and are
  drained only after the destination acquires mutation. A consumed ticket,
  changed route, changed actor generation, changed owner generation, duplicate
  destination, or executing actor is rejected without guessing ownership.
  Focused coverage migrates one actor 100 times between two explicit
  schedulers, preserves its home identity and every in-transfer publication,
  rejects duplicated tickets after each handoff, restores an aborted transfer,
  and rejects migration during execution. Actor-directory, topology, normal
  `terlc` compilation, Rust quality, and
  `make vm-multicore-fixed-placement-check` are green. MC-5 remains open for
  extracting and importing the complete generated actor state: precise managed
  heap and mailbox roots, continuation captures and stack-map authority,
  process/resource/timer state, generation identity, and destination resume.
  The current HTTP execution shard aggregates multiple actors, so moving only
  `PureNativeSuspension` is explicitly forbidden; the next slice must introduce
  an actor-owned transfer envelope and source/destination extraction APIs.

  MC-5 execution evidence (2026-07-22, second slice): the pure-native execution
  runtime now detaches one parked actor into a linear transfer containing its
  exact request and generated-continuation identities, optional precise
  continuation captures, managed heap, and receiver-owned managed mailbox
  fragments. Destination admission validates owner identity, cross-actor
  roots, heap and fragment collisions, and stable fragment identities before
  mutation. A failed continuation or managed-memory import returns the complete
  transfer for exact source rollback; managed reference words and fragment
  identities are never serialized or remapped. The envelope is `Send` and owns
  no native stack or scheduler-local borrow. Five focused transfer tests cover
  successful movement and single claim, continuation collision rollback,
  managed heap and mailbox-root movement, managed destination collision
  rollback, and thread-transfer eligibility; the existing six managed
  execution tests remain green. MC-5 remains open: the actor-owned process,
  scheduler-class, resource, timer, and suspension state must join this
  envelope, the fixed-scheduler migration ticket must drive source detach and
  destination import, and a generated long-running actor must complete repeated
  cross-thread migrations before the gate can be added.

  MC-5 execution evidence (2026-07-22, third slice): actor-directory transfer
  now extracts only an unowned generation with no lookup pins or undrained
  publications and imports it through a linear value-preserving API. Process
  transfer moves the actual process record, integrated mailbox, execution
  stack, registered names, and process/message identity watermarks. Scheduler
  transfer moves class, runnable membership, and process-local accounting while
  scheduler-wide historical totals remain with the scheduler that observed
  them. The actor-runtime envelope composes those components with the exact
  native continuation indexes and requires a suspended native safepoint. The
  pure-native shard envelope then composes actor runtime state with the managed
  heap, precise roots, and generated continuation transfer from the preceding
  slice. Destination rejection at any layer returns every component for source
  rollback. A generated actor was detached and imported 100 times between two
  execution shards, then resumed and completed exactly once; a destination
  identity collision restored both runtime layers, and the complete transfer is
  `Send`. Three process, three scheduler, three actor-runtime, five managed
  execution-transfer, and three generated-actor transfer tests are green.
  `make vm-multicore-fixed-placement-check` passed its runtime suites and then
  stopped in the unrelated quality inventory because the concurrent untracked
  `protocol_task_executor.rs` contains an uninventoried `HashMap`. MC-5 remains
  open for owner-scoped resources, timers/delayed messages, aliases, memory and
  relationship state, and for driving this envelope through the fixed-owner
  thread command protocol and its migration ticket.

  MC-5 execution evidence (2026-07-22, fourth slice): exact owner-scoped alias,
  resource, and timer records now have linear detach/import APIs with stable
  identity watermarks and complete rollback failures. Alias migration preserves
  priority and one-shot reply capabilities. Resource migration preserves both
  owner-only and actor-transferable policy because the process owner does not
  change. Timer migration preserves timer id, kind, interval, absolute
  deadline, and source clock position; an initialized destination clock must
  match, while an uninitialized destination adopts the source position and an
  actor with no timers is clock-independent. The actor envelope now moves these
  tables with process resource handles and moves delayed-send payloads under
  their exact timer ids. An imported delayed message fires once at its original
  deadline and reaches the migrated mailbox. Relationships and memory
  accounting remain fail-closed rather than being silently stranded. Three
  alias, three resource, three timer, five actor-runtime, five managed
  execution-transfer, and three generated-actor transfer tests are green; all
  25 transfer-filtered tests pass together and the generated actor still
  completes after 100 cross-shard transfers. MC-5 remains open for memory and
  relationship transfer plus fixed-owner command-channel integration with the
  generation-qualified migration ticket.

  MC-5 execution evidence (2026-07-22, fifth slice): actor migration now moves
  the exact per-process memory metrics, pressure-decision history, native
  resource charges, exclusively owned shared allocations, configured memory
  limits, and shared-allocation identity watermark. Detach validates that the
  process heap bytes and resource table match the accountant before mutation.
  Destination admission requires identical limits, exact heap and resource
  graphs, unchanged actor ownership, and collision-free stable identities;
  every rejection returns the complete memory transfer for source rollback.
  Shared allocations retained by another actor are rejected before detach as
  the required cross-actor-root failure case. Memory import is composed after
  process, scheduler, alias, and resource admission and is re-detached if a
  later timer import fails. Four focused memory-transfer tests, five complete
  actor-runtime tests, all 29 transfer-filtered tests, and all three generated
  actor tests are green; the generated actor still completes exactly once
  after 100 cross-shard transfers. `make vm-multicore-fixed-placement-check`
  is green with its process, failure, memory, scheduler, topology, fixed-owner,
  compiled-handler, deterministic-container, and Rust quality gates. MC-5
  remained open for link/monitor relationship policy and for driving the
  complete envelope through the fixed-owner command protocol under its
  generation-qualified migration ticket.

  MC-5 completion evidence (2026-07-22, sixth slice): explicit migration now
  runs exclusively through source and destination fixed-owner command queues.
  The generation coordinator first captures a linear migration ticket, asks
  the source owner thread to detach the complete actor envelope, asks the
  destination owner thread to import it, and only then publishes the new route
  through the shared control directory. Import rejection retains the complete
  transfer for source-owner rollback and aborts the ticket. The ticket records
  the exact pre-migration lifecycle: yielding actors return runnable, while
  parked actors remain parked unless an in-transfer mailbox publication made
  them runnable. Typed I/O waits are rebound to the destination execution-shard
  identity without changing request, continuation, or actor identities.
  Relationship-bearing actors are rejected before detach as cross-actor-root
  migrations; no one-sided link or monitor record is moved. A real generated
  HTTP handler parks, migrates 100 times between two owner threads, resumes
  from the final scheduler with the exact typed wake, returns the expected HTTP
  response once, and releases every shard actor reservation. The dedicated
  `make tvm-aot-multicore-migration-check` gate covers all transfer layers,
  fixed-scheduler migration authority, the full-cycle generated handler, and
  the inherited transition, managed-memory, actor-ownership, and Rust quality
  contracts. MC-5 is complete; MC-6 may steal only actors satisfying this
  explicit migration eligibility policy.

- [x] MC-6: add bounded work stealing and shard-wide fairness.
  - Define local-service budgets, idle detection, victim selection, eligible
    scheduling classes, steal batch size, failed-steal backoff, locality bias,
    and scheduler sleep/wake transitions.
  - Steal only completely published runnable actors. Parked, exiting, borrowed,
    pinned, or partially migrated actors are ineligible.
  - Enforce priority/normal/background weighting and maximum runnable wait
    across the shard, not merely inside each local queue.
  - Preserve actor home preference without allowing persistent imbalance or a
    remote priority flood to starve normal/background actors indefinitely.
  - Add deterministic policy-model tests and randomized skew, burst, fanout,
    idle-race, steal-race, pinned-actor, and shutdown workloads.
  - Gate: add `make vm-multicore-work-stealing-check`; run
    `make tvm-aot-multicore-migration-check`,
    `make vm-scheduler-fairness-check`, and `make rust-quality-check`.
  - Exit: the scheduler remains work-conserving and meets the declared
    shard-wide starvation bounds under adversarial imbalance.

  MC-6 execution evidence (2026-07-22, first slice): the VM now owns one
  deterministic bounded work-stealing policy over complete per-scheduler
  snapshots. The policy fixes a finite local-service budget, 3:2:1
  priority/normal/background service cycle, explicit per-class maximum-wait
  bounds, locality threshold, rotating victim selection, four-actor default
  batch limit, exponential failed-steal backoff capped at 64 polls, and an
  at-most-once sleep-to-wake publication handshake. Shard-wide overdue work
  overrides local service and load locality, so an old background actor is
  selected even while priority work remains elsewhere. A typed steal plan
  accepts only a fully published, unowned `Queued` actor on the exact victim
  and class; parked, executing, borrowed, lookup-pinned, affinity-pinned,
  unpublished, exiting, and migrating actors are ineligible. Eleven focused
  tests cover the retained `bounded_steal_owner_transfer_model` invariant,
  local-budget exhaustion, background starvation under a priority flood,
  equal-victim rotation, one-scheduler weighted order, bounded exponential
  backoff, single wake publication, every ineligible actor state, shutdown,
  malformed snapshots, and 2,000 seeded skew/burst/fanout decisions.
  `make vm-multicore-work-stealing-policy-check` and
  `make rust-quality-check` pass. At this slice boundary MC-6 remained open for
  a linear queued-to-migrating claim, bounded owner-command transfer through
  the MC-5 envelope, and the final production scheduling gate.

  MC-6 execution evidence (2026-07-22, second slice): the actor directory now
  exposes one generation-qualified `Queued -> Migrating` steal claim with only
  two terminal operations: destination publication or exact source rollback.
  Lookup pins, pending mailbox publication, active mutator ownership, and every
  non-queued lifecycle reject the claim before execution eligibility changes;
  lookup acquisition also rechecks migration after pinning to close the claim
  race. The canonical scheduler removes the victim queue tail, scheduling
  class, and enqueue age under that linear claim. A destination scheduler can
  publish the same process exactly once, retaining class and accumulated wait,
  while collision or state rejection returns the complete claim for lossless
  victim rollback. `transfer_steal_batch` validates policy and mutator-owner
  identities and executes no more than the plan's explicit actor bound. Three
  actor-directory tests and seven scheduler-claim/batch tests cover lifecycle
  exclusion, publication and lookup pins, duplicate authority, exact class and
  tail selection, bounded transfer, fairness-age preservation, collision
  rollback, and owner/plan mismatch. The dedicated policy gate now runs these
  tests in addition to the eleven deterministic policy-model tests. The stale
  dormant-code row is removed because the canonical scheduler now consumes the
  policy plan through its runtime transfer API. At this slice boundary MC-6
  remained open for live owner snapshots, bounded owner commands, backoff and
  sleep/wake integration, and runtime starvation-bound evidence.

  MC-6 execution evidence (2026-07-22, third slice): canonical schedulers now
  publish owner-qualified live snapshots containing exact per-class queue load
  and oldest accumulated wait. Victim owners detach linear bounded claim
  batches; destination owners publish them through bounded command channels;
  rejected and unconsumed claims return to the victim in reverse claim order,
  reconstructing the original queue tail exactly. `VmWorkStealingRuntime`
  owns one command thread per scheduler, collects ordered live snapshots,
  executes policy-selected transfers only on the named victim and thief, feeds
  observed transfer counts back into policy backoff, performs at-most-once wake
  commands for sleeping owners, fail-stops the shard when an import reply makes
  claim ownership unknowable, and joins owners during idempotent shutdown.
  Five owner-thread tests cover skewed bounded transfer, destination-collision
  rollback, local-service budget enforcement, sleep/wake publication, shutdown,
  malformed owner sets, and bounded command capacity. Two additional scheduler
  tests cover live oldest-wait snapshots and exact multi-claim rollback order.
  `make vm-multicore-work-stealing-owner-check` is the staged gate. MC-6 remains
  open: generated AOT yield/preemption boundaries must enqueue runnable actors
  into these owners, local service must execute real slices, concurrent queue
  changes must exercise failed-steal backoff, and the final adversarial fairness
  gate must replace this staged owner-protocol gate.

  MC-6 execution evidence (2026-07-22, fourth slice): generated AOT `Yield`
  transitions no longer auto-resume inside one owner command. The fixed
  scheduler now enforces the complete `Executing -> Yielding -> Queued ->
  Executing` lifecycle before the continuation can run again. Each handler
  owner owns a bounded 1,024-entry runnable queue and alternates command ingress
  with one real generated continuation slice, retaining the original blocked
  invocation reply until code completes or reaches an external receive wait.
  Queue exhaustion cancels and reclaims the actor rather than growing memory.
  Shutdown also cancels queued continuations before joining the owner. Two
  compiled Terlan handler tests prove repeated direct yields and the harder
  receive-wake-then-yield path, including scheduler telemetry order and exact
  directory transition evidence. The dedicated
  `make tvm-aot-multicore-yield-queue-check` gate composes the work-stealing
  owner protocol, fixed lifecycle control, bounded owner queue checks, and both
  generated AOT execution paths. MC-6 remains open for transferring eligible
  generated runnable actor envelopes between owners, live concurrent queue
  mutation/backoff evidence, and the final shard-wide starvation-bound gate.

  MC-6 execution evidence (2026-07-22, fifth slice): one queued generated AOT
  continuation can now move between real handler owner threads as a complete
  linear envelope. Invocation outcomes carry the actor's current route, so a
  migrated completion or later receive wait releases and retains the current
  scheduler reservation rather than its stale entry route. Fixed migration
  authority now admits `Queued -> Migrating` and restores the actor to
  `Queued` before destination acquisition. The source owner removes exactly
  one bounded queue entry, publishes its destination route, and detaches the
  canonical `PureNativeActorTransfer` together with its suspension and blocked
  invocation reply. The destination imports the complete actor before queueing
  it; rejection returns every component, moves the route and reservation back,
  and restores the source queue without duplicating continuation authority.
  Compiled Terlan tests deterministically pause local service, transfer a
  yielded handler from scheduler 0 to scheduler 1, and verify destination-only
  resume and completion. A second test injects destination rejection and proves
  source restoration, eventual completion, and zero leaked reservations. The
  staged `make tvm-aot-multicore-runnable-steal-check` gate composes all prior
  work-stealing and yield gates with queued migration, successful transfer, and
  rollback evidence. MC-6 remains open for policy-driven automatic generated
  queue coordination under concurrent mutations, observed failed-steal
  backoff, and the final adversarial shard-wide starvation-bound gate.

  MC-6 execution evidence (2026-07-22, sixth slice): generated handler owners
  now publish live normal-queue load and oldest enqueue age to the canonical
  shard-wide work-stealing policy. While an invocation awaits its owner reply,
  bounded coordination polls every owner snapshot, applies local-service and
  victim-selection policy, transfers complete runnable envelopes, and records
  the observed result for exponential backoff. Enqueue age survives transfer
  and rollback. Compiled Terlan handler tests now exercise automatic source-to-
  destination transfer, destination-owned receive parking and wakeup, and a
  concurrent destination rejection that restores the source actor, observes a
  backoff directive, retries, and completes on the destination without leaking
  actor reservations. The staged
  `make tvm-aot-multicore-policy-coordination-check` gate composes every prior
  MC-6 gate with all thirteen generated invocation lifecycle tests. MC-6 remains
  open only for the final adversarial shard-wide priority/normal/background
  starvation, randomized queue-mutation, sleep/wake, and shutdown gate.

  MC-6 completion evidence (2026-07-22, seventh slice): production generated
  runnable queues now preserve priority, normal, and background class through
  enqueue, repeated yield, exact-class steal, destination import, rejection
  rollback, snapshot publication, and shutdown. Local owner service follows the
  canonical 3:2:1 cycle, and live snapshots publish exact per-class load and
  oldest wait. A compiled six-actor workload proves the weighted service order.
  A 48-actor, four-scheduler adversarial workload combines paused owners and
  class skew, transfers work in all three classes, completes every invocation,
  and releases every actor reservation. A separate queued-background shutdown
  workload proves deterministic cancellation and reclamation. The final
  `make vm-multicore-work-stealing-check` gate composes policy coordination,
  migration, scheduler fairness, Rust quality, all sixteen generated invocation
  lifecycle tests, and the three completion workloads. The complete gate is
  green. MC-6 is complete; MC-7 is the next executable slice.

- [x] MC-7: integrate timers, I/O, external NativeBoundary capacity, and
  EPMD/node lifecycle.
  - Route timer, socket, HTTP, database, filesystem, and external
    NativeBoundary completions through generation-checked scheduler events;
    reactor and worker threads never execute or mutate actor state.
  - Define message versus timeout, cancellation versus completion, shutdown
    versus late completion, and worker crash versus continuation-resume order.
  - Keep external or unsafe native execution in an explicitly bounded worker
    pool with per-request ownership, backpressure, cancellation, continuation
    affinity, crash attribution, and generation-safe replacement.
  - Reject concurrent use of one non-reentrant worker connection, duplicate
    resume, stale worker generations, pool oversubscription, and capacity
    bypass after a worker crash.
  - Register one EPMD entry per ready logical node endpoint, never per
    scheduler. Start scheduler pool, listener, and transport router before
    registration; stop admission and unregister through the node/shard
    supervisor during fail-stop or orderly shutdown.
  - Route incoming node transport work to the current actor owner without
    changing OTP-compatible EPMD framing or coupling node registration to one
    scheduler's lifetime.
  - Gate: add `make vm-multicore-runtime-integration-check` and
    `make vm-epmd-discovery-check`; run `make vm-timer-deadline-check`,
    `make native-boundary-runtime-adversarial-check`,
    `make vm-http-concurrency-investigation-check`, and
    `make rust-quality-check`.
  - Exit: blocking work parks actors, completions resume only the matching live
    generation, and node discovery remains stable across scheduler activity.

  MC-7 execution evidence (2026-07-22, first slice): typed I/O waits now carry
  the canonical execution-shard epoch in addition to shard, actor, request,
  continuation, and boundary-type identity. The fixed scheduler owner validates
  that complete authority before generated continuation execution, so a late
  reactor or worker completion cannot enter a recovered image generation even
  when the process, request, continuation, and type identities are reused.
  Explicit actor migration rebinds shard identity and destination epoch
  together before the parked invocation can resume. A dedicated typed-I/O
  backend crashes with a parked wait, recovers under epoch 2, recreates the
  epoch-1 local identities, and proves the stale completion fails closed before
  generated code runs; the replacement actor is cleaned through the unified
  exit path. The existing 100-migration generated-handler test now asserts the
  wait epoch after every handoff, and the exact typed-wake test still rejects
  foreign requests and wrong value types. The staged
  `make tvm-aot-multicore-io-epoch-check` gate composes the complete MC-6 gate
  with all three lifecycle cases and is green. MC-7 remains open for timer,
  protocol reactor, bounded capability-worker, and EPMD/node lifecycle
  integration under the same generation-checked scheduler-event contract.

  MC-7 execution evidence (2026-07-22, second slice): an execution shard can
  now issue an immutable timer clock event carrying its canonical shard
  identity, image epoch, stable operation identity, and observed monotonic
  tick. A reactor can retain that value without borrowing actor state; only
  the owning shard consumes it. Consumption validates shard identity and epoch
  and admits the event through the existing `TimerDelivery` operation ledger
  before the actor timer table or scheduler is touched. The committed operation
  remains available for exact duplicate suppression. Focused tests prove one
  current delayed message is delivered once, an exact duplicate cannot deliver
  twice, a foreign-shard event cannot advance local timers, and an epoch-1
  event cannot fire an identity-reused epoch-2 timer after crash recovery. The
  staged `make vm-multicore-timer-epoch-check` gate composes the prior I/O epoch
  gate with these cases and the existing exact-deadline actor migration case.
  MC-7 remains open for generated `Timer` transition parking and publication
  through the fixed scheduler, protocol-reactor completions, bounded external
  capability workers, and EPMD/node lifecycle integration.

  MC-7 execution evidence (2026-07-22, third slice): generated `Timer`
  transitions now create an absolute scheduler-clock deadline, retain their
  continuation and epoch-qualified timer event in a bounded scheduler-local
  queue, release actor mutation as `Parked`, and resume only after the fixed
  actor directory publishes and dispatches a typed timer event. The deadline
  queue blocks only an idle scheduler owner; command ingress and runnable peer
  actors remain serviceable while timers wait. The deadline wait never retains
  an actor mutator lease, and timer delivery revalidates shard, epoch, actor,
  request, continuation, timer id, kind, and deadline under the reacquired
  owner lease. Timer event identity is now separate from monotonic supervisor
  progress, allowing newer peer operations to commit before an older timer
  expires without regressing the progress signal. Scheduler shutdown publishes
  cancellation through the same parked-actor path and settles the retained
  caller before generation shutdown. Focused generated AOT tests prove a real
  deadline does not fast-forward, a peer completes while the timer is parked,
  and shutdown cancels the timer without retaining actor state. The staged
  `make vm-multicore-timer-scheduler-check` gate composes all prior multicore
  timer and I/O epoch evidence and is green. MC-7 remains open for
  protocol-reactor completions, bounded external capability workers, and
  EPMD/node lifecycle integration.

  MC-7 execution evidence (2026-07-22, fourth slice): every protocol future is
  now polled with an exact connection-task route bound to its reactor-local
  context. A generated request entered from that context is admitted on the
  matching fixed scheduler and retains the protocol route while parked. Its
  typed wake is represented as `IoCompletion`, fully published through the
  fixed actor directory, and observed as `IoCompletionPublished` before the
  actor owner records `IoCompletionDispatched`; the reactor never receives an
  actor mutator lease. Resume validates both the protocol scheduler and exact
  connection process, so an ambient thread, a foreign scheduler, or another
  connection on the same scheduler fails before publication. Rejection drops
  and cancels the parked invocation without retaining an active actor route.
  The former reactor-local immediate shard and its protocol-local resource
  registry were removed; even terminal callbacks now enter through the fixed
  actor-owner inbox and produce owner-thread entry and completion telemetry.
  The staged `make vm-multicore-protocol-reactor-check` gate composes the timer
  scheduler gate with exact origin rejection, full generated continuation
  completion, publication order, and same-scheduler foreign-connection cases.
  MC-7 remains open for bounded external capability workers and EPMD/node
  lifecycle integration.

  MC-7 execution evidence (2026-07-22, fifth slice): VM capability-worker
  clients can now be grouped into a non-empty pool of uniquely identified,
  explicitly bounded logical slots. Admission selects only a live worker that
  grants the requested capability and has an unused local concurrency credit;
  saturation and missing capability authority fail before another actor is
  parked or another transport frame is published. A serial slot remains
  non-reentrant even when its worker protocol advertises additional remote
  credits. Every accepted request retains its exact worker id and process
  generation for cancellation and crash attribution. EOF, protocol failure,
  and orderly shutdown remove that process from live capacity without changing
  configured capacity. A vacant slot accepts only the same logical worker at
  generation `N+1`; stale assignments cannot cancel through the replacement,
  and a failed process contributes zero credits until replacement succeeds.
  Exact tests also prove duplicate replies remain stale after the first
  scheduler wake, cancellation releases one request credit and suppresses a
  late reply, duplicate logical slots fail closed, and capacity cannot be
  bypassed after a crash. The staged
  `make vm-multicore-capability-worker-check` gate composes the protocol-reactor
  gate, the existing isolated worker process/sandbox gate, and all pool
  lifecycle cases. MC-7 remains open for publishing generated AOT capability
  completions through the fixed actor owner and for EPMD/node lifecycle
  integration.

  MC-7 execution evidence (2026-07-22, sixth slice): generated `Capability`
  transitions no longer fail at the HTTP scheduler boundary. The owning
  execution shard decodes the closed capability operation, retains an
  at-most-once `CapabilityCompletion` operation under its exact image epoch,
  and returns a linear parked wait carrying shard, epoch, actor, request, and
  continuation identity. Worker clients and pools now have a separate
  already-parked request path: it consumes the same bounded local and remote
  credits but never creates a proxy process, touches scheduler tables, or
  double-parks the generated actor. Completion enters the fixed actor
  directory as a typed `CapabilityCompletion` publication; the owner
  reacquires the actor lease, validates the full wait identity, converts the
  worker term against the generated result type, resumes the continuation,
  and commits the epoch operation only after generated code accepts the
  value. Dedicated telemetry distinguishes capability publication from
  dispatch. Focused tests prove bounded already-parked completion,
  cancellation plus late-reply suppression, and a compiled Terlan
  `File.exists` call whose publication precedes owner dispatch and returns the
  injected Boolean. The staged
  `make vm-multicore-capability-completion-check` gate composes all previous
  MC-7 worker and scheduler evidence. MC-7 remains open for binding the
  capability event pump to these pool assignments and for EPMD/node lifecycle
  integration.

  MC-7 execution evidence (2026-07-23, seventh slice): a VM-owned capability
  event pump now binds each already-parked worker assignment to an opaque
  scheduler payload under one deterministic generation-qualified key. The
  payload can carry the fixed actor route, generated suspension, epoch wait,
  and retained caller without exposing any of those types to worker transport.
  Submission consumes pool credit before retaining the payload and returns the
  untouched payload when authority or capacity rejects the call. Polling
  returns a payload only with its exact correlated context and reply; stale
  protocol events cannot claim live continuation ownership. Explicit
  cancellation returns the payload exactly once. Worker EOF, protocol failure,
  and orderly shutdown drain every payload attributed to that exact process
  generation, allowing the scheduler owner to cancel those actors rather than
  leaking parked state; the failed slot still contributes zero capacity.
  Focused tests prove successful correlation, lossless backpressure and
  cancellation, and two-payload generation drain on worker loss. The staged
  `make vm-multicore-capability-event-pump-check` gate composes all prior MC-7
  capability and scheduler evidence. MC-7 remains open for installing this
  pump in generated HTTP scheduler startup/shutdown and for EPMD/node lifecycle
  integration.

  MC-7 execution evidence (2026-07-23, eighth slice): every generated HTTP
  scheduler owner now installs one lazy, bounded capability event pump. A
  generated `Capability` suspension submits its complete route, continuation,
  epoch wait, actor owner, and caller reply only after external worker
  admission succeeds; the scheduler then releases the actor as parked. While
  assignments remain pending, the owner interleaves nonblocking worker polls
  with command, timer, and runnable service. Correlated replies and exact
  generation worker-loss failures are published through the fixed actor
  directory before the same scheduler reacquires and resumes the actor. Route
  cancellation recovers the retained caller even when cancellation transport
  fails, and scheduler shutdown cancels every retained capability actor before
  worker termination. Worker admission now reuses the shared Rust-backed
  operation inventory for closed filesystem and stdio families instead of
  hardcoding all process calls to the Postgres manifest. Focused tests prove
  shutdown payload recovery, in-process filesystem admission, preservation of
  the explicit manual completion seam in tests, and a full generated Terlan
  `File.exists` call through the real sandboxed worker to `Bool(true)`. The
  staged `make vm-multicore-capability-scheduler-check` gate composes all prior
  MC-7 evidence. MC-7 remains open only for EPMD and node lifecycle
  integration.

  MC-7 execution evidence (2026-07-23, ninth slice): the retained
  dependency-free EPMD protocol codec, deterministic registration registry,
  client frame planning, and runtime configuration model now live in the
  compiler-owned VM tree. EPMD socket handling is represented as a nonblocking
  future on the existing `mio`-backed fixed protocol scheduler pool rather than
  restoring the quarantined standalone Tokio daemon. A new logical-node
  supervisor state machine admits exactly one endpoint only after the complete
  scheduler pool, listener, and transport router report readiness. Incoming
  node work resolves the actor's current fixed-scheduler route at delivery
  time, so an actor migration changes transport ownership without changing the
  logical node registration. Orderly and fail-stop shutdown close transport
  admission before unregistering the exact ALIVE2 connection. Focused tests
  cover malformed framing, invalid names, duplicate registration, exact
  connection lifetime, startup ordering, migration, shutdown ordering, and
  fixed-scheduler request handling. The new
  `make vm-epmd-discovery-check` gate owns that evidence, while
  `make vm-multicore-runtime-integration-check` composes it with the timer,
  protocol-reactor, capability-worker, HTTP concurrency, NativeBoundary, and
  Rust quality gates. Both gates are green.

  MC-7 execution evidence (2026-07-23, tenth slice): the protocol executor now
  exposes a supervisor-owned server handle only after every fixed scheduler
  reports readiness. Shutdown publishes one control event to each scheduler,
  wakes blocked readiness polls, drops connection futures, and joins all owner
  threads. The production logical-node bootstrap binds that managed listener
  to a bounded actor-addressed transport router, opens admission only after
  scheduler, listener, and router readiness, and then registers the exact
  listener port once in the shared EPMD registry. Complete messages resolve
  the actor's current route and enter its fixed-owner mailbox; protocol threads
  never receive actor mutation authority. Shutdown closes router admission,
  joins listener owners, withdraws the exact registration, and reaches
  `Stopped` under one supervisor. Socket-free tests prove bounded framing and
  publication across actor migration. A loopback full-cycle test starts real
  EPMD and node listeners, discovers the advertised port through PORT2,
  publishes before and after actor migration, drains both messages under the
  new owner, and proves discovery withdrawal after shutdown. The test is
  compiled on restricted hosts and runs through
  `make vm-epmd-discovery-check` where loopback sockets are available. MC-7 is
  complete; the next executable section is MC-8.

- [x] MC-8: complete multicore record/replay, observability, debugging,
  reload, and supervision.
  - Record scheduler choice, owner generation, queue transition, message and
    signal publication sequence, timer/I/O/native completion order, safepoint,
    migration, steal outcome, wake source, image generation, cancellation,
    failure, and execution interval through per-scheduler bounded buffers.
  - Define record mode and controlled replay mode over the stable event
    identities introduced in MC-2. Normal production scheduling does not
    promise repeatable host-thread interleavings.
  - Make debugger pause/step, inspector snapshots, hot reload, support bundles,
    actor supervision, shard restart, and orderly shutdown respect ownership
    and scheduler generations.
  - On scheduler panic or ownership corruption, emit bounded pre-failure
    evidence and terminate the shard; recovery begins from supervisor-owned
    durable state rather than partially mutated in-process scheduler state.
  - Add adversarial tests for trace pressure, dropped diagnostic events, replay
    corruption, debugger pause during migration, reload during steal, crash
    while owning an actor, and observation of partially published state.
  - Gate: add `make vm-multicore-replay-observability-check`; run
    `make tvm-aot-debugger-consumer-check`,
    `make vm-supervision-restart-check`, and `make rust-quality-check`.
  - Exit: support evidence explains every actor handoff and a captured execution
    replays without depending on wall-clock thread timing.
  - First slice complete: fixed-scheduler telemetry now writes metrics and a
    canonical, versioned per-scheduler replay stream through one bounded
    recorder. Captures retain scheduler-local sequence, actor and owner
    generations, shard epoch, publication sequence, peer scheduler, execution
    interval, and explicit event kinds without wall-clock or host-thread
    identity. Controlled replay rejects lossy, foreign-scheduler, corrupt, and
    divergent captures. `make vm-multicore-replay-observability-check` protects
    this foundation. MC-8 remains open for generation-qualified production
    call sites, capture aggregation and support bundles, debugger/reload and
    supervision integration, and bounded scheduler-panic evidence.
  - Second slice complete: the fixed scheduler now preserves authoritative
    `VmActorPublication` identities through publish and drain instead of
    discarding them. Generated timer, I/O, capability, and cancellation
    publication events record actor generation, mailbox sequence, and shard
    epoch; owner dispatch joins the same identity with the acquired mutator
    generation. Foreign actor/publication pairs fail before metrics or replay
    mutation. The existing generated timer full cycle proves park, identified
    publication, generation-qualified owner dispatch, and completion through
    the production AOT scheduler. The MC-8 gate now includes this full-cycle
    evidence.
  - Third slice complete: every generated actor slice now records a paired,
    scheduler-local execution interval from one acquired mutator lease. Entry,
    resume, yield, park, completion, and failure evidence retains actor and
    owner generations at the authoritative lifecycle mutation. Runnable steals
    carry one canonical migration context from source migration start and route
    publication through detachment and destination import, including explicit
    source and destination steal outcomes. Interval identities fail closed on
    exhaustion instead of wrapping. Focused full-cycle tests prove three
    execution intervals across two cooperative yields, preserve migration
    identity across two scheduler owners, and retain existing timer behavior.
    The MC-8 gate owns these tests. Remaining work starts with bounded capture
    aggregation, support-bundle and debugger/reload consumers, supervision and
    shutdown generations, and scheduler-panic evidence.
  - Fourth slice complete: live handler generations now aggregate exactly one
    bounded capture per scheduler without inventing a global host-thread event
    order. Aggregation sorts scheduler streams deterministically, checks the
    complete topology, validates retained sequence metadata even after bounded
    prefix loss, uses checked retained and dropped-event accounting, enforces a
    caller-owned total event bound, and marks lossy evidence diagnostic-only.
    The existing native support-bundle schema can carry this validated evidence,
    while generation diagnostics expose only retained, dropped, and replayable
    summaries rather than dumping event payloads. Adversarial tests reject
    missing and duplicate schedulers, forged sequence metadata, zero bounds,
    and aggregate overflow. Remaining work starts with debugger and hot-reload
    consumers, supervision and shutdown generations, and bounded scheduler-panic
    evidence.
  - Fifth slice complete: read-only debugger admission and compiler-owned native
    hot reload now consume the canonical bounded replay schema. Debugger reports
    bind descriptor metadata to the admitted runtime generation, scheduler
    topology, retained and dropped counts, and replayability, with one explicit
    `ImageGeneration` event exposed in text and structured JSON output. Hot
    reload records each admitted image generation before publishing code-server
    metadata and returns the complete generation history in its report. A
    quarantined replacement does not append a publication event. Full-cycle
    tests prove debugger admission and rendering, two executable reload
    generations, and failed pinned-generation replacement. Remaining work
    starts with live debugger pause/step ownership, reload during actor
    migration or steal, supervision and shutdown generations, and bounded
    scheduler-panic evidence.
  - Sixth slice complete: live debugger execution control now enters each fixed
    scheduler through its bounded owner command channel. One canonical VM state
    machine owns running, paused, and bounded stepping modes; pause leaves
    command, timer, completion, migration, and shutdown ingress active, while a
    step permit authorizes exactly one queued actor slice before returning to
    pause. Pause, continue, and actual stepped slices enter scheduler-local
    replay evidence, and stepped slices retain the actor, actor generation,
    mutator-owner generation, and image-shard epoch observed under the acquired
    lease. A compiled Terlan actor pauses on scheduler 0, migrates while both
    owners are paused, executes its two remaining continuation slices through
    two destination-owner steps, and completes exactly once. Invalid running,
    zero, and oversized step requests fail before state mutation. Remaining
    work starts with reload during actor migration or steal, supervision and
    shutdown generations, and bounded scheduler-panic evidence.
  - Seventh slice complete: detached actor envelopes are now explicit native
    generation references rather than an invisible gap between source detach
    and destination import. Each linear transfer retains a checked atomic
    source lease, the exact sealed image identity and descriptor digest, the
    source shard epoch, and a fork of the executable mapping. Source reload,
    shutdown, and recovery cannot unload that generation while the envelope is
    in flight. Destination admission validates its currently routable image
    before mutating actor or execution tables; a destination reloaded to a
    different generation rejects the complete envelope for exact source
    rollback. Successful import releases the temporary source lease only after
    destination-owned actor state is established. Focused evidence opens the
    former quiescence race, proves source replacement is deferred with an
    `actor_transfers` reference, reloads the destination, rejects the stale
    transfer intact, restores the source continuation, and then resumes it.
    Existing 100-handoff, rollback, and compiled Terlan pause/migrate/step tests
    remain green. Remaining work starts with supervision and shutdown
    generations, followed by bounded scheduler-panic evidence.
  - Eighth slice complete: native image admission, replacement, supervised
    failure, restart scheduling, recovered readiness, shutdown start, and
    shutdown completion now enter one bounded execution-shard lifecycle
    recorder. Every event carries the exact admitted shard epoch; supervision
    events also carry the nonzero restart-attempt sequence. Source reload now
    consumes this shard-owned history instead of maintaining a second
    synthetic generation recorder, preserving the existing one-event first
    generation and two-event replacement evidence. Focused lifecycle tests
    prove an epoch-1 crash schedules restart attempt 1, recovery publishes
    epoch 2 before its image event, and orderly shutdown starts and completes
    under epoch 1. The production fixed-scheduler shutdown test also proves
    queued actor cancellation ends in a scheduler shutdown event qualified by
    the owning image epoch. The MC-8 gate owns these exact tests and source
    checks. Remaining work is bounded scheduler-panic evidence.
  - Ninth slice complete: fixed scheduler telemetry now retains the exact
    actor, actor generation, mutator-owner generation, shard epoch, and
    execution interval active during a panic. Fail-stop containment consumes
    that interval into an explicit `SchedulerPanicked` event, terminates the
    owner thread, closes generation-wide admission, and reports the crash
    through the shard supervisor. Each owner retains one immutable panic
    artifact joining its bounded scheduler replay with the supervisor-owned
    shard lifecycle replay; production generation diagnostics can inspect
    these artifacts after thread termination. Panic text is bounded at a valid
    UTF-8 boundary. The full-cycle test fills the scheduler buffer beyond
    capacity, proves exact dropped-prefix accounting, panics while holding a
    real actor lease, observes the unmatched pre-failure execution start and
    actor-qualified terminal event, verifies epoch-1 restart attempt 1, and
    rejects peer-owner admission. The MC-8 gate owns the telemetry, bounded
    payload, and full-cycle containment tests. MC-8 is complete; the next
    executable section is MC-9.

- [ ] MC-9: prove race freedom, semantic stability, scaling, and bounded tail
  latency.
  - First executable slice completed:
    - [x] Run one generated AOT actor workload through actual fixed scheduler
      owners at widths one, two, and four.
    - [x] Record logical and physical CPU topology, process affinity, effective
      cgroup cpuset and quota, scheduler override, Rust toolchain, optimization
      profile, load averages, sample count, dispersion, workload hash, and
      native-image hash in `terlan.vm-multicore-performance.v1`.
    - [x] Prove overlapping generated execution from scheduler-owner telemetry,
      including distinct owner-thread identities and maximum simultaneously
      active scheduler count. Client threads do not contribute to that count.
    - [x] Add the incremental `make vm-multicore-performance-check` report
      gate. This gate does not yet satisfy the complete MC-9 exit.
  - Second executable slice completed:
    - [x] Run identical actor spawn/exit, mailbox round-trip, logical timer
      delivery, generated HTTP response, supervision restart, and EPMD
      register/lookup/unregister workloads at widths one, two, and four.
    - [x] Record operation count, throughput, minimum, median, p95, p99,
      maximum, and median absolute deviation for all 18 workload/width rows.
    - [x] Label generated fixed-scheduler-owner execution separately from
      independent VM state-machine lanes so host threads cannot be counted as
      runtime scheduler overlap.
    - [x] Include the versioned workload contract in benchmark hash evidence
      and make `vm-multicore-performance-check` reject missing workloads.
  - Third executable slice completed:
    - [x] Run a generated CPU-bound actor with runtime seed arguments and ten
      sequential 20,000-operation phases through the actual fixed scheduler
      owners at widths one, two, and four. Retain 31 raw duration samples per
      width and independently validate every result without relying on an
      unbounded generated-recursion depth.
    - [x] Record the median width-one to width-two throughput speedup and a
      deterministic 95% independent-median bootstrap confidence interval with
      4,096 resamples.
    - [x] Add the versioned
      `benchmarks/baselines/vm-multicore-performance-limits.json` policy for
      the dedicated Linux x86-64 runner, including one-scheduler throughput and
      p99 regression budgets.
    - [x] Keep ordinary hosts record-only. Once
      `TERLAN_VM_MULTICORE_DEDICATED_RUNNER` requests policy enforcement, fail
      closed on runner identity, target, release profile, controlled-load
      declaration, CPU eligibility, workload drift, scaling, confidence, or
      one-scheduler regression.
    - [x] Enforce at least 1.5x median width-one to width-two speedup and a
      1.25x lower confidence bound on the eligible dedicated runner.
  - Fourth executable slice completed:
    - [x] Run two generated one-million-operation CPU actors on actual fixed
      scheduler owners while independently sampling scheduler command wait,
      mailbox delivery, timer delivery, generated HTTP response latency,
      failed-steal backoff, actor-local bump allocation, and precise
      collection.
    - [x] Require direct two-owner overlap before every retained mixed-load
      sample and record 31-sample minimum, median, p95, p99, maximum, and
      median absolute deviation distributions for all seven paths. Batch and
      normalize sub-microsecond paths so clock and preemption noise cannot
      masquerade as failed-steal or allocation regressions.
    - [x] Add versioned dedicated-runner p95 and p99 references and fail closed
      when any observed/reference ratio exceeds its 2.0 limit.
    - [x] Make `vm-multicore-performance-check` reject missing mixed-load
      evidence, overlap proof, or metric identities.
  - Fifth executable slice completed:
    - [x] Add one portable fixed-scheduler stress contract covering concurrent
      mailbox publication, exclusive mutator ownership, wakeup, drain,
      migration, immutable home placement, terminal release, and reclamation.
    - [x] Execute eight recorded deterministic seeds in isolated test
      processes. A 15-second parent watchdog kills and reports a stuck child,
      so a deadlock cannot hang the platform or sanitizer gate indefinitely.
      A forced-hang case proves the watchdog kills and reaps the child.
    - [x] Compose the seeded stress with the existing exhaustive continuation
      schedule model, mailbox publication flood, and bounded work-stealing
      model in `make vm-multicore-memory-model-check`.
    - [x] Run the bounded seeded stress from every Linux, macOS, and Windows
      x86-64 and AArch64 native matrix row.
    - [x] Add the mandatory Linux x86-64
      `make vm-multicore-thread-sanitizer-check` lane on the exact Rust 1.96.0
      `x86_64-unknown-linux-gnutsan` target. Its attestation rejects a skipped
      decision, unpinned compiler, uninstrumented target, missing or repeated
      seed, stale revision, unofficial repository, or incomplete CI identity.
  - Sixth executable slice completed:
    - [x] Qualify every performance report with the full checked-out source
      revision, clean-tree attestation, and local or official GitHub Actions
      provenance. A hosted dedicated-policy run requires complete run, attempt,
      commit, workflow, repository, runner-name, and self-hosted runner
      evidence.
    - [x] Add the release-only
      `terlan-linux-x86_64-multicore-v1` self-hosted runner lane. It uses Rust
      1.96.0, declares controlled background load, requests dedicated policy
      enforcement, and retains the resulting performance artifact.
    - [x] Add `make vm-multicore-mc9-evidence-check`. It rejects record-only
      performance, hosted-runner substitution, policy or workload drift,
      missing overlap, unpinned sanitizer evidence, malformed provenance,
      cross-run artifacts, and different source revisions.
    - [x] Require performance and sanitizer artifacts to share the same
      official workflow reference, run, attempt, repository, and commit before
      writing `terlan.vm-multicore-mc9-evidence.v1`.
    - [x] Reject promoted performance or ThreadSanitizer evidence unless each
      report proves that tracked and untracked source state exactly matched its
      checked-out commit.
  - Next executable slice: run release validation on the labeled controlled
    runner, collect a passing pinned ThreadSanitizer report in the same
    workflow run, execute `make vm-multicore-mc9-evidence-check`, and check off
    MC-9 only after the joined report exists for that release candidate.
  - Run identical actor, mailbox, timer, HTTP, supervision, EPMD lifecycle, and
    AOT workloads with one, two, and up to four schedulers on recorded hardware.
  - Record effective cgroup/container CPU quota, affinity, logical and physical
    topology when available, scheduler override, toolchain, optimization
    profile, background load, sample count, dispersion, and benchmark hashes.
  - Require direct runtime evidence of overlapping AOT actor execution and a
    maximum simultaneously active scheduler count of at least two on eligible
    hosts. Client threads, compiler workers, reactor threads, and benchmark-only
    handler pools do not count.
  - On the dedicated release runner with at least two effective CPUs, require
    the independent CPU-bound actor fixture's median throughput to improve by
    at least 1.5x from one to two schedulers, with a recorded confidence bound,
    while the one-scheduler result and performance remain inside their
    versioned regression budgets.
  - Record higher widths without promising portable linear scaling. Bound p95
    and p99 scheduler wait, mailbox delivery, timer delay, HTTP latency,
    migration/steal failure, allocation, and collection pauses under mixed
    CPU/I/O load.
  - Run bounded memory-model tests on every supported platform, reproducible
    seeded stress with a deadlock watchdog, and a mandatory pinned-toolchain
    ThreadSanitizer release-candidate lane on the supported Linux x86-64 runner.
  - Gate: add `make vm-multicore-performance-check`,
    `make vm-multicore-thread-sanitizer-check`, and
    `make vm-multicore-mc9-evidence-check`; run
    `make vm-multicore-runtime-integration-check`,
    `make vm-multicore-replay-observability-check`,
    `make vm-http-vs-axum-check`, `make vm-semantics-vs-otp-check`, and
    `make rust-quality-check`.
  - Exit: multicore improves real actor throughput without hiding fairness,
    latency, memory, replay, or correctness regressions.

- [ ] MC-10: perform multicore release closeout.
  - First executable slice completed:
    - [x] Add `make vm-multicore-release-check` as the canonical distributed
      closeout. It validates same-run MC-9 evidence before running the
      remaining multicore semantic, runtime, quality, integrity, and repository
      gates.
    - [x] Keep performance and ThreadSanitizer execution in their owning
      release jobs so hosted final validation cannot overwrite the controlled
      runner or instrumented artifacts.
    - [x] Add an adversarial contract gate that rejects missing local gates,
      reordered evidence validation, missing artifact producers, or release
      workflows that bypass the canonical closeout target.
  - Second executable slice completed:
    - [x] Add the recorder for
      `target/quality/vm-multicore-release-closeout.json` with the versioned
      `terlan.vm-multicore-release-closeout.v1` schema only after the complete
      local gate graph passes.
    - [x] Bind joined MC-9 evidence, the six-target platform matrix, the ordered
      local gate graph, the checked-out source revision, and a
      domain-separated revision over the invariant inventory and concurrency
      contract.
    - [x] Reject cross-run, cross-revision, incomplete platform, stale
      invariant, malformed MC-9, and contract-drift evidence before writing
      closeout.
    - [x] Retain the multicore closeout report in release artifacts and require
      it through the platform release-evidence contract.
  - Third executable slice completed:
    - [x] Delete the test-only `VmWorkStealingRuntime`, scheduler steal-claim,
      and actor-directory steal-claim predecessor implementations. Production
      generated AOT owner threads and `PureNativeActorTransfer` now form the
      only runnable actor transfer path.
    - [x] Remove the obsolete process-table steal adapters and synthetic
      candidate eligibility model. The actor transfer transaction owns
      lifecycle, publication, mutator, pin, and generation admission.
    - [x] Remove policy-owned scheduler shutdown and wakeup proxies. Bounded
      scheduler command channels own wakeup and shutdown; policy snapshots
      contain only runnable load and queue-age evidence.
    - [x] Collapse four MC-6 transition targets into the canonical
      `make vm-multicore-work-stealing-check` without dropping their retained
      production tests.
    - [x] Add `make vm-multicore-runtime-cleanup-check` to reject restored
      predecessor files, symbols, staging targets, and stale activation
      annotations while compiling both VM binaries with warnings denied.
    - [x] Retain the explicit one-scheduler mode for deterministic execution,
      REPL, debugging, and constrained targets; it is a supported topology,
      not a temporary multicore assumption.
  - Fourth executable slice completed:
    - [x] Promote the complete ordered 21-gate multicore closeout sequence into
      the main 0.0.7 planned-gate inventory without duplicate shared gates.
    - [x] Parse the main and multicore fenced gate inventories in declaration
      order and require the mini-roadmap sequence to appear as one exact
      contiguous block in the main roadmap.
    - [x] Reject missing, reordered, interleaved, empty, or duplicate
      cross-roadmap gate inventories through the existing
      `make roadmap-gate-integrity-check`.
    - [x] Update the validated main-roadmap inventory to 196 planned gates,
      65 unchecked slices, and 516 Make targets.
  - Next executable slice: obtain same-run official MC-9 performance and
    ThreadSanitizer evidence, then run `make vm-multicore-release-check` from a
    clean reproducible release environment.
  - Revalidate the already completed AOT roadmap and reconcile Slice 40 only
    after this mini-roadmap's full Completion Boundary passes.
  - Run `make vm-multicore-release-check` from a clean reproducible
    environment.
  - Run `make check`, `make rust-quality-check`,
    `make roadmap-gate-integrity-check`, and the required 0.0.7 release
    preflight after the focused composition passes.
  - Record host topology, CPU quota, scheduler configuration, toolchain, AOT
    image, invariant revision, race/stress seeds, sanitizer evidence, scaling
    results, tail latency, fail-stop results, EPMD lifecycle, and replay
    artifacts.
  - Exit: the Completion Boundary is true without qualifications such as
    benchmark-only, interpreter-only, fixed-placement-only, worker-parallel,
    or experimental.

## Complete Multicore Gate Set

```bash
make vm-multicore-invariant-inventory-check
make vm-actor-mutator-ownership-check
make vm-multicore-mailbox-publication-check
make vm-multicore-fixed-placement-check
make tvm-aot-multicore-migration-check
make vm-multicore-work-stealing-check
make vm-multicore-runtime-cleanup-check
make vm-multicore-runtime-integration-check
make vm-epmd-discovery-check
make vm-multicore-replay-observability-check
make vm-multicore-performance-check
make vm-multicore-memory-model-check
make vm-multicore-thread-sanitizer-check
make vm-multicore-mc9-evidence-check
make vm-scheduler-fairness-check
make tvm-aot-runtime-transition-check
make tvm-managed-memory-check
make rust-quality-check
make roadmap-gate-integrity-check
make check
make vm-multicore-release-check
```
