# Actor Directory

The actor directory is the VM-owned storage and ownership boundary for local
actors. A stable process identity resolves to a reusable slot qualified by an
actor generation. Reusing a slot advances that generation, so stale lookup,
completion, and ownership handles cannot resolve a replacement actor.

Each cell keeps lifecycle, actor generation, owner generation, and owner in one
atomic word. Scheduler execution moves a queued actor to executing and accesses
mutable actor state only through a validated `VmActorMutatorToken`. VM control
operations acquire a fresh owner generation while preserving the actor's exact
lifecycle, perform one scoped mutation, and release ownership before invoking
another subsystem. There is no production API returning an unowned mutable
actor reference. Scoped control ownership also releases during Rust unwinding,
so a caller that deliberately contains an operation panic cannot strand the
actor in an owned state.

Each actor cell owns a bounded `concurrent-queue` MPSC mailbox. Producers
publish fully initialized fragments under the current actor generation without
acquiring the receiver's mutator token. The single consumer integrates those
fragments into selective-receive and priority ordering only under a checked
mutator token. Capacity is reserved before publication sequence allocation, so
backpressure is typed and a rejected send changes neither queue contents nor
the visible sequence.

The mailbox wake state is the authority for the park race. A receiver first
enters `parking`, rechecks the queue, and then becomes `parked`. A producer
publishes before changing the state to `notified`; publication against a parked
actor promotes its lifecycle to `queued` at most once. Actor release rechecks
the notification after publishing the parked lifecycle, covering publication
on either side of that transition without losing a wakeup.

## Lifecycle

The canonical states are `Queued`, `Executing`, `Yielding`, `Parked`,
`Migrating`, `Exiting`, `Retired`, and `Reclaimed`. Migration starts only after
the current mutator releases ownership. Retirement requires terminal cleanup,
and reclamation requires a retired actor with no lookup pins.

## Failure Contract

Double acquisition, stale owner generations, stale slot generations, invalid
transitions, lookup-pin underflow, and corrupt packed lifecycle values are
typed failures. The directory does not attempt to repair ownership corruption.
Transition events retain stable actor, lifecycle, owner, and generation
identities for later multicore tracing and replay.
