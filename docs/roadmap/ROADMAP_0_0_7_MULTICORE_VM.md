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

1. Before selecting MC-1, verify every top-level item in
   [`ROADMAP_0_0_7_AOT.md`](ROADMAP_0_0_7_AOT.md) is checked and rerun its AOT
   release closeout. If AOT is incomplete or a closeout gate fails, stop and
   continue AOT work; do not begin multicore preparation or implementation.
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

Multicore work becomes active only after all AOT-1 through AOT-9 items are
checked and the AOT Completion Boundary is revalidated. In particular, the VM
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

## Pre-Activation Scorecard — 2026-07-21

Refresh this table after AOT closeout before MC-1 begins. Transitional
implementation details listed here are observations, not post-AOT work items.

| Boundary | State | Current evidence |
| --- | --- | --- |
| Cooperative scheduler semantics | Partial foundation | Weighted queues, reductions, yielding, blocking, cancellation, and telemetry exist |
| Parallel actor execution | Open | `VmScheduler::run_next` executes one mutable process slice at a time |
| Actor mutator ownership | Open | The ABI requires it; no production scheduler-thread token transfer exists |
| Scheduler thread pool | Open | Host threads exist in benchmarks, not as the actor scheduler |
| AOT migration safepoints | Partial foundation | Stack maps and continuations exist; cross-thread actor migration is not proven |
| Concurrent mailboxes and signals | Open | Current actor runtime is composed through one mutable owner |
| Native execution parallelism | Transitional | Pure-native calls currently share a registry mutex and one persistent worker per module; AOT closeout must delete this ordinary local path before activation |
| Record/replay | Partial foundation | Logical scheduler telemetry exists; captured multicore decision replay does not |
| EPMD/node lifecycle | Open | EPMD is retained but its golden-owned port and scheduler-pool lifecycle integration are not proven |
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

- [ ] MC-1: inventory BEAM/ERTS invariants and freeze the Terlan concurrency
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

- [ ] MC-2: replace monolithic process ownership with a generational actor
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

- [ ] MC-3: implement MPSC mailbox, signal publication, and race-free wakeups
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

- [ ] MC-4: add multiple scheduler threads with fixed actor placement.
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

- [ ] MC-5: implement explicit AOT actor migration at published safepoints.
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

- [ ] MC-6: add bounded work stealing and shard-wide fairness.
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

- [ ] MC-7: integrate timers, I/O, external NativeBoundary capacity, and
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

- [ ] MC-8: complete multicore record/replay, observability, debugging,
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
    `make terlc-debugger-check`, `make vm-supervision-restart-check`, and
    `make rust-quality-check`.
  - Exit: support evidence explains every actor handoff and a captured execution
    replays without depending on wall-clock thread timing.

- [ ] MC-9: prove race freedom, semantic stability, scaling, and bounded tail
  latency.
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
  - Gate: add `make vm-multicore-performance-check` and
    `make vm-multicore-thread-sanitizer-check`; run
    `make vm-multicore-runtime-integration-check`,
    `make vm-multicore-replay-observability-check`,
    `make vm-http-vs-axum-check`, `make vm-semantics-vs-otp-check`, and
    `make rust-quality-check`.
  - Exit: multicore improves real actor throughput without hiding fairness,
    latency, memory, replay, or correctness regressions.

- [ ] MC-10: perform multicore release closeout.
  - Revalidate the already completed AOT roadmap and reconcile Slice 40 only
    after this mini-roadmap's full Completion Boundary passes.
  - Remove temporary single-thread assumptions, benchmark-only scheduler-width
    proxies, obsolete monolithic process-table ownership, stale mutex-backed
    runtime paths, and superseded transition gates.
  - Promote every multicore gate into the main roadmap's planned-gate inventory
    and make the integrity checker reject drift between the main and mini
    roadmaps before any parent item is checked.
  - Add `make vm-multicore-release-check` as the canonical composition of every
    multicore gate below, then run it from a clean reproducible environment.
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
make vm-multicore-runtime-integration-check
make vm-epmd-discovery-check
make vm-multicore-replay-observability-check
make vm-multicore-performance-check
make vm-multicore-thread-sanitizer-check
make vm-scheduler-fairness-check
make tvm-aot-runtime-transition-check
make tvm-managed-memory-check
make rust-quality-check
make roadmap-gate-integrity-check
make check
make vm-multicore-release-check
```
