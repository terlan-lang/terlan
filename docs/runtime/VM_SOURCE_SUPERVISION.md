# Supervised processes from Terlan

`Process.spawn` starts an ordinary unsupervised process. Use
`std.vm.Supervisor.start_child[T](supervisor, spec)` to register an executable
`ChildSpec[Process.Entry[T]]` in the VM graph and start a supervised process.
The result is `Result[Process[T], SupervisorError]`.

`Supervisor.root` and `child_spec` build immutable declarations. Admission owns
the mutable graph. Supervisor names belong to the calling actor; reusing a name
with a different policy fails. The owner’s exit terminates its supervised
children and revokes pending restarts.

The existing image-entry convention applies: `Process.entry[T](705)` names a
public zero-argument `spawn_705` function in the image. Admission resolves the
entry before mutating the child graph. Mailbox operations retain the declared
payload type `T`.

The VM applies permanent, transient, and temporary restart classes and
one-for-one, one-for-all, and rest-for-one strategies. Child `order` controls
sibling selection, independently of admission order. Restart intensity counts
failure incidents across the supervisor within its rolling window. Reaching
the limit marks the supervisor failed and terminates its remaining children.
Restart delays use capped exponential backoff. Windows, backoff, and shutdown
deadlines use monotonic VM clock ticks.

`current_child[T](supervisor, spec)` resolves the current replacement using the
original typed declaration; it fails while restart is pending or after a stop.
`snapshot(supervisor)` returns child identities, restart counts, pending state,
and actual restart events with old and replacement identities. The identity
returned by `start_child` belongs to that particular process instance.

`stop_child[T](supervisor, spec)` suppresses restart and cancels pending backoff.
A zero shutdown timeout terminates immediately. A positive timeout requests
cancellation at a native safepoint and enforces termination at the deadline.
Stopping a blocked process releases its native heap and parked continuation.

## Durable recovery

Use `recovery[T](path, schema, version, restore_entry)` and
`start_persistent_child[T](supervisor, spec, recovery)` to bind a logical child
to a file-backed snapshot/event store and a typed recovery entry. The initial
entry runs first; subsequent replacements run the recovery entry.

Inside that child, `checkpoint(sequence, bytes)` commits a snapshot and
`append_event(sequence, bytes)` commits ordered events. `restore()` reopens the
store, validates its schema, and returns the snapshot and ordered event suffix.
The recovery entry decodes the application’s bytes and applies those events.
The logical store identity survives replacement process identities.

The file adapter commits through a synchronized temporary file and rename,
with directory synchronization on Unix. Snapshot and event payloads are bounded
to 16 MiB and recovery rejects stores larger than 64 MiB. This is a local file
adapter with synchronous writes; it does not provide a distributed storage
lease or a schema migration implementation. Supervision-owned actors remain
on their owning shard; transferring a live supervision graph is rejected.

## Executable contract

Previously, public source functions built policy values while admission and
restart tests exercised a separate Rust runtime. No operation admitted a typed
ordinary-process entry into the running actor graph. The source API now crosses
an explicit VM-owned transition and shares the real process table, scheduler,
exit cleanup, and backoff queue. Rust-only policy tests cannot substitute for
the compiled program below.

Lifecycle validation also covers discarded effects across suspension,
consecutive synchronous supervision calls that reuse a continuation identity,
and retirement of stopped native heaps, continuations, and idle timer owners.
Those cases prevent successful policy decisions from masking parked or leaked
actors.

Stopped declarations leave the active sibling order. Their historical process
records can be reaped without breaking a later group restart, while owner exit
still terminates children already waiting for a graceful-shutdown deadline.

`tests/language/VmSupervisorRuntimeTest.terl` is a compiled Terlan program. Its
counter child commits state and an event, crashes through `Process.fail`, and
recovers under a different process identity. The replacement loads the durable
state and adds the replayed event. Other cases exercise actual restart classes,
intensity exhaustion, sibling strategies, backoff cancellation, and shutdown.

Run `terlc test tests/language/VmSupervisorRuntimeTest.terl` or
`make vm-supervision-restart-check`. The Rust project-test suite also compiles
and executes this source fixture, so ordinary commit validation covers it.
