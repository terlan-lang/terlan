# Std Sync Internals

This directory owns portable synchronization contracts for source-visible model
change streams. The implementation is centered on typed resources and typed
change events. Its most important boundary is that std sync describes data-flow
shape, while target runtimes own transport, delivery, backpressure, and
reconnection.

## Responsibilities

- Define portable change-event types that can be reused by Postgres streams,
  actor-backed UI, and future cloud runtime features.
- Keep WebSocket, SSE, BEAM process, Postgres replication, and JavaScript event
  details outside the core source contract.
- Preserve explicit typed payloads so syncable models remain typechecked.
- Avoid promising durable delivery until a concrete runtime contract exists.

## Public Surface

- `Resource[T]`: opaque typed source of values or model changes.
- `Inserted`, `Updated`, and `Deleted`: portable change-kind singleton values.
- `ChangeKind`: language-neutral atom carrier for supported model-change kinds.
- `Change[T]`: typed change event carrying one `ChangeKind` and one value.
- `inserted`, `updated`, and `deleted`: constructors for typed change events.
- `kind` and `value`: accessors for change-event fields.

## Core Model

`Resource[T]` represents a source-visible handle for a syncable value stream.
It does not specify whether the backing runtime uses Postgres logical
replication, BEAM processes, WebSockets, SSE, browser events, or a cloud
runtime channel.

The main flow is:

1. A target capability produces or owns a `Resource[T]`.
2. Runtime code emits `Change[T]` values for inserted, updated, or deleted
   model values.
3. Template, handler, or cloud-runtime layers consume the typed changes without
   depending on the transport.

Important invariants:

- Change kinds are language-neutral atom aliases, not backend atoms.
- `Change[T]` always carries a typed payload.
- The std contract does not claim guaranteed delivery or replay semantics.

## Integration Points

- `std.db.Postgres`: future Postgres change resources can expose
  `Resource[T]`.
- `std.beam.Agent`: BEAM process state can later publish typed changes through
  the same contract.
- Typed templates: future reactive rendering can bind to `Resource[T]` without
  changing static template semantics.

## Edge Cases

- Delivery guarantees are intentionally absent from this module.
- Deletion events still carry `T`; runtime adapters decide whether that is a
  full prior value, a keyed tombstone model, or a domain-specific deletion
  shape.
- Transport-specific presence and broadcast behavior should be modeled in
  separate modules after runtime requirements are proven.

## Types And Interfaces

`Resource[T]`
: Opaque typed synchronization source.

`Change[T]`
: One typed model-change event.

`ChangeKind`
: Atom-backed carrier for `Inserted`, `Updated`, and `Deleted`.

## Testing Notes

- `std/sync/ResourceTest.terl` proves the public surface is typecheckable.
- Runtime delivery tests should be added only when a target implementation
  exists.
