# Std VM Internals

This directory owns Terlan VM process-oriented standard-library modules. These
modules are not portable core APIs; they expose reliability, supervision,
process, and native bridge concepts for targets that provide a VM process
runtime.

Terlan VM owns this surface. `std.vm.*` is the public Terlan namespace for
process-oriented runtime APIs and must not be described as a Beam-named
standard-library surface.

## Responsibilities

- Define typed wrappers for VM process-oriented abstractions.
- Keep actor, supervision, backpressure, and bridge APIs under `std.vm.*`.
- Avoid leaking VM-only behavior into portable `std.core` modules.
- Preserve room for default trait-style behavior where runtime abstractions share
  process contracts.

## Public Surface

- `std.vm.Agent`: stateful process helper.
- `std.vm.Process`: process spawning and messaging contract.
- `std.vm.Task`: task-style asynchronous process contract.
- `std.vm.GenServer` and `std.vm.Supervisor`: supervision-shaped process
  modules.
- `std.vm.NativeBridge`: bridge boundary for supervised native resources.
- `std.vm.Bytes`: runtime-owned binary protocol frames.
- `std.vm.Timeout`: typed timeout values for receive-style operations.
- `std.vm.Tcp`: connected TCP socket operations for integration tests.
- `std.vm.Port`: external OS process and runtime port lifecycle operations.
- `std.vm.Cluster`: VM-owned cluster profile, membership, and distributed
  transport session descriptors.
- `std.vm.DistributedState`: VM-owned state scope, version, conflict-policy,
  write-outcome, and checkpoint descriptors.
- `std.vm.DistributedStorage`: VM-owned checkpoint storage mode, policy,
  adapter, snapshot, lifecycle-outcome, and capability descriptors.
- `std.vm.ModelSync`: VM-owned model key, optimistic version, write/delete,
  conflict, and adapter capability descriptors.
- `std.vm.PersistentActor`: VM-owned persistent actor id, typed snapshot schema
  id, retention policy, redaction policy, snapshot plan, replay plan, mailbox
  checkpoint, mailbox restore plan, timer checkpoint, timer restore plan, and
  package store binding descriptors.
- `std.vm.Fault`: VM-owned distributed fault policy, recovery-window, failure,
  and migration-rollback descriptors.
- `std.vm.Scheduler`: VM-owned distributed placement, shard-affinity, migration,
  and scheduler-event descriptors.

## Core Model

VM modules model runtime behaviors that are meaningful only when the target
runtime can provide processes, supervision, and message passing. Terlan source
must import these modules explicitly, and target validation owns rejection for
non-VM-capable targets.

`std.vm` must keep a strict boundary between runtime mechanics and runtime
policy.

The Rust VM owns mechanics that require scheduler, memory, resource, or failure
control: process identity, spawn, mailbox send/receive, selective receive,
timers, cancellation, links, monitors, supervision mechanics, resource
ownership, parked-process wakeups, and NativeBoundary lifecycle.

Terlan stdlib owns policy that can be expressed as source: GenServer-style
call/cast/state loops, Agent-style state cells, Task orchestration, supervisor
convenience APIs, persistent actor protocols, and typed service frameworks.
These modules should compile to VMIR like any other Terlan code and use
lower-level VM primitives instead of introducing framework-specific VMIR opcodes
or `native` implementations for behavior that can be written in Terlan.

Magic lowering is acceptable only for thin wrappers around real VM primitives.
For example, `Process.receive`, timer creation, link/monitor operations, or
resource cleanup may be primitive. GenServer timeout policy, stale reply
handling, callback dispatch, and state-transition ordering should be readable
and testable as Terlan stdlib code.

The main flow is:

1. Source imports a `std.vm.*` module explicitly.
2. Type checking validates the selected target can provide the VM contract.
3. The VM lowers the typed surface to runtime process operations.

Important invariants:

- VM-only APIs stay under `std.vm`.
- Process protocols are typed at the Terlan boundary.
- Native bridge APIs are reserved for supervised or long-lived native work.
- Daemon and socket protocol tests should use `Bytes`, `Tcp`, `Port`, and
  `Timeout` instead of embedding backend-specific helper code in tests.

## Runtime Mechanics Versus Runtime Policy

The VM owns hard runtime primitives: process spawn, process identity, mailbox
send/receive, selective receive, timers, cancellation, links, monitors,
supervision mechanics, scheduler accounting, resource cleanup, and native
parking/resume.

High-level service semantics belong in Terlan stdlib. `Agent`, `Task`,
`GenServer`, `Supervisor` policy helpers, persistent actors, and typed service
loops should be implemented as Terlan modules over lower-level `std.vm`
primitives whenever the behavior can be expressed safely in source.

Magic lowering is reserved for thin primitive wrappers. It may expose VM-owned
operations to Terlan code, but it must not become the hidden implementation of a
framework abstraction whose behavior should be readable, testable, and
replaceable as stdlib code.

This split keeps `std.vm` serious: application-level reliability policy remains
source-visible, while the VM stays focused on the mechanics that require runtime
ownership.

## Integration Points

- Terlan VM execution owns runtime behavior.
- NativeBoundary adapters may be called behind `NativeBridge` for native work.

## Edge Cases

- Process cleanup and failure semantics belong to the selected VM runtime
  contract.
- Non-VM-capable targets must reject these modules before artifact emission.
- Native bridge operations must not be used for pure helper calls that can
  lower directly to native functions.

## Types And Interfaces

`Agent[T]`
: Runtime-backed state process abstraction.

`Task[T]`
: Runtime-backed asynchronous result abstraction.

`NativeBridge`
: Runtime-supervised native resource bridge boundary.

`Bytes`
: Runtime-owned binary protocol frame.

`TcpSocket`
: Runtime-owned TCP socket handle.

`Port`
: Runtime-owned external process or port handle.

`Profile`, `Membership`, `Session`, `Frame`
: Runtime-owned distributed coordination and transport descriptors.

`DistributedState.Scope`, `DistributedState.Version`, `DistributedState.Policy`,
`DistributedState.Store`, `DistributedState.Entry`, `DistributedState.Outcome`,
`DistributedState.Conflict`, `DistributedState.Snapshot`
: Runtime-owned distributed state descriptors for explicit write outcomes,
  deterministic version handling, conflict capture, and checkpoint restore.

`DistributedStorage.Mode`, `DistributedStorage.Policy`,
`DistributedStorage.Adapter`, `DistributedStorage.Snapshot`,
`DistributedStorage.Outcome`
: Runtime-owned distributed storage descriptors for force-local, durable, and
  cluster checkpoint adapter lifecycle outcomes, including append, flush,
  cluster replication, compaction, checksum validation, unavailable backend,
  and unsupported capability paths.

`ModelSync.Key`, `ModelSync.Version`, `ModelSync.Write`, `ModelSync.Delete`,
`ModelSync.Conflict`, `ModelSync.Capability`, `ModelSync.AdapterContract`,
`ModelSync.PersistentActorAdapter`, `ModelSync.PackageStoreAdapter`
: Runtime-owned model synchronization descriptors for source-visible
  optimistic concurrency, typed write/delete plans, stale-write conflict
  metadata, portable adapter capability declarations, and persistent actor
  or package store adapter bindings.

`PersistentActor.ActorId`, `PersistentActor.SchemaId`,
`PersistentActor.SchemaDeclaration`, `PersistentActor.RetentionPolicy`,
`PersistentActor.RedactionPolicy`, `PersistentActor.SnapshotPlan`,
`PersistentActor.ReplayPlan`,
`PersistentActor.ResourceCheckpoint`, `PersistentActor.ResourceRestorePlan`,
`PersistentActor.MailboxCheckpoint`, `PersistentActor.MailboxRestorePlan`,
`PersistentActor.TimerCheckpoint`, `PersistentActor.TimerRestorePlan`,
`PersistentActor.PackageStoreBinding`
: Runtime-owned persistent actor descriptors for source-visible actor
  identity, typed snapshot schema identity, schema declarations, snapshot write
  plans, retention policies, redaction policies, replay schema expectations,
  durable resource checkpoints, restart resource restore plans, mailbox
  checkpoints, restart mailbox restore plans, actor-owned timer checkpoints,
  restart timer restore
  plans, and package-owned storage bindings.

`Policy`, `State`, `Transition`, `Recovery`, `Failure`, `Rollback`
: Runtime-owned distributed fault and recovery descriptors for heartbeat
  suspicion, partition isolation, bounded recovery, and migration rollback.

`Node`, `Scheduler`, `Policy`, `Placement`, `Migration`, `Event`
: Runtime-owned distributed scheduler descriptors for process placement, shard
  affinity, controlled migration, and scheduler event observation.

## Testing Notes

- Positive tests should live beside the owning module when source-level
  behavior is testable.
- Backend process behavior is validated through Terlan-owned runtime tests.
- Target-profile tests should reject VM modules for incompatible targets.
