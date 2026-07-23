# TVM Multicore Concurrency Contract

Status: frozen MC-1 implementation contract.

This document owns Terlan's actor concurrency semantics. Erlang/OTP is mined
for invariants and failure cases, but its process structures, locks, queues,
and scheduler implementation are not part of the Terlan runtime contract.

## MC-C01 Actor Lifecycle

An actor lifecycle is `created -> runnable -> queued -> executing`, followed by
exactly one of `yielding`, `parked`, `migrating`, or `exiting`. Yielding returns
to runnable, a valid wake changes parked to runnable, migration publishes a new
owner before returning to runnable, and exiting advances through `retired` to
`reclaimed`. No transition may leave two executable queue entries for one actor.

## MC-C02 Owner Generation

The lifecycle word identifies the actor generation, lifecycle state, scheduler
owner, and owner generation. An execution token must match all four fields.
Acquisition is exclusive; release, migration, retirement, and completion
delivery reject stale actor or owner generations. Generation values never wrap
into a live identity.

## MC-C03 Publication

Producers finish and validate message, signal, completion, and migration data
before a release publication. Queue insertion or wake publication is the final
release operation. A scheduler performs the matching acquire before reading
published state. Actor-local heap references are integrated only by the actor's
mutator at a safepoint.

## MC-C04 Lock Order

The order is directory lookup guard, actor publication state, scheduler queue,
registry metadata, timer metadata, tracing metadata, then code metadata. Actor
execution, heap collection, socket work, worker waits, and condition waits run
without any of these shared locks. A critical section is bounded and may not
call a lower-order owner after releasing and reacquiring state implicitly.

## MC-C05 Park And Wake

Parking first publishes receive or completion interest and then atomically
changes executing to parked. A producer publishes data before attempting the
parked-to-runnable transition. Exactly one successful transition owns enqueue;
observing queued or executing suppresses duplicate enqueue without suppressing
the published data. The scheduler acquires state after dequeue.

## MC-C06 Reclamation

An actor is reclaimable only when it is retired, has no queue or wake ticket,
has no pending completion for its generation, and no directory reader can
resolve its cell. Reuse allocates a new generation. Timing, empty polling, and
thread quiescence without an explicit epoch, hazard, or equivalent proof are
not reclamation evidence.

## MC-C07 Scheduler Failure

An actor failure remains actor-local and emits declared link, monitor, and
supervision effects. An external NativeBoundary failure remains worker-local.
Scheduler panic, token corruption, or impossible lifecycle state fails the
execution shard closed; no peer scheduler may guess ownership or continue an
actor with uncertain mutable state.

## MC-C08 Replay Events

Stable replay identities cover scheduler choice, actor and owner generation,
queue transition, message and signal publication, logical timer delivery,
I/O and native completion, migration or steal result, image generation,
cancellation, failure, and supplied nondeterminism. Production interleavings
need not repeat; replay follows a recorded decision stream in controlled mode.

## MC-C09 Reductions And Fairness

Compiler safepoints charge reductions to the executing actor. Exhaustion
publishes an owned continuation before yielding. Priority weighting, maximum
runnable wait, steal batch size, failed-steal backoff, and idle sleep are
bounded shard policies. One-scheduler mode uses the same accounting and keeps
its deterministic semantic order.

## MC-C10 Mailboxes And Signals

The mailbox is multi-producer and single-consumer. Each producer preserves its
publication order. System signals use an explicit priority order without
reordering ordinary messages from one sender. Selective receive and its save
queue are actor-owned. Failed or stale-generation publication performs no
partial actor mutation.

## MC-C11 Timers And I/O

Timers, reactor events, and NativeBoundary completions carry actor and owner
generation plus a stable completion identity. Publication follows MC-C03 and
wakeup follows MC-C05. Cancellation and actor retirement make late completion
observable as stale without reviving or mutating the actor.

## MC-C12 Shutdown

Shutdown stops admission, publishes cancellation, drains or rejects completions
by generation, retires actors, waits for proven reclamation, and only then
releases shard resources. EPMD and node readiness describe one logical node,
not scheduler threads. Shutdown cannot depend on queue polling eventually
finding no work while publication remains possible.

## Source Policy

The machine-readable inventory is
[`TVM_MULTICORE_INVARIANT_INVENTORY.json`](TVM_MULTICORE_INVARIANT_INVENTORY.json).
Its pinned Erlang/OTP paths are reference citations only. Product source,
Cargo metadata, release artifacts, and runtime loading must not depend on that
checkout.
