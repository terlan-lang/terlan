# Std DB Internals

This directory owns portable database capability contracts. 0.0.5 starts with
Postgres only; there is no generic database abstraction and no ORM.

## Responsibilities

- Define source-visible database capability modules.
- Keep target-specific socket, TLS, pooling, and protocol details outside
  Terlan source.
- Preserve typed errors and rows at module boundaries.
- Document which database capabilities are release contracts and which runtime
  adapters remain pending.

## Public Surface

- `std.db.Postgres.Config`: source-visible Postgres connection settings.
- `std.db.Postgres.Pool`: opaque pool handle.
- `std.db.Postgres.Connection`: opaque transaction connection handle.
- `std.db.Postgres.Row`: opaque query row handle.
- `std.db.Postgres.connect`, `query`, `query_one`, `execute`, and
  `transaction`: first small Postgres capability surface.

## Core Model

Postgres is a standard capability, not a core primitive. Terlan source owns
business logic and transaction boundaries. A runtime adapter owns socket I/O,
TLS, pooling, protocol/client library calls, row decoding, and backpressure.

The default BEAM path must use a SafeNative supervised worker bridge rather
than NIFs.

## Pending Runtime Work

- SafeNative worker metadata.
- Rust/Tokio Postgres adapter.
- BEAM bridge lifecycle and stale-handle handling.
- Target-profile rejection for unsupported targets.
- Live database validation and integration tests.
