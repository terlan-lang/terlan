# Std BEAM Internals

This directory owns BEAM-specific standard-library modules. These modules are
not portable core APIs; they expose reliability, supervision, process, and
native bridge concepts for targets that use Erlang/BEAM as the orchestration
runtime.

## Responsibilities

- Define typed wrappers for BEAM process-oriented abstractions.
- Keep actor, supervision, backpressure, and bridge APIs under `std.beam.*`.
- Avoid leaking BEAM-only behavior into portable `std.core` modules.
- Preserve room for default trait-style behavior where BEAM abstractions share
  process contracts.

## Public Surface

- `std.beam.Agent`: stateful process helper.
- `std.beam.Process`: process spawning and messaging contract.
- `std.beam.Task`: task-style asynchronous process contract.
- `std.beam.GenServer` and `std.beam.Supervisor`: OTP-shaped process modules.
- `std.beam.NativeBridge`: bridge boundary for supervised native resources.
- `std.beam.Bytes`: BEAM-owned binary protocol frames.
- `std.beam.Timeout`: typed timeout values for BEAM receive-style operations.
- `std.beam.Tcp`: connected TCP socket operations for BEAM integration tests.
- `std.beam.Port`: external OS process and BEAM port lifecycle operations.

## Core Model

BEAM modules model runtime behaviors that are meaningful only when the target
runtime can provide processes, supervision, and message passing. Terlan source
must import these modules explicitly, and target validation owns rejection for
non-BEAM targets.

The main flow is:

1. Source imports a BEAM module explicitly.
2. Type checking validates the selected target can provide the BEAM contract.
3. The BEAM backend lowers the typed surface to runtime process operations.

Important invariants:

- BEAM-only APIs stay under `std.beam`.
- Process protocols are typed at the Terlan boundary.
- Native bridge APIs are reserved for supervised or long-lived native work.
- Daemon and socket protocol tests should use `Bytes`, `Tcp`, `Port`, and
  `Timeout` instead of embedding backend-specific Erlang helper code in tests.

## Integration Points

- `terlan_erlang`: owns Erlang/BEAM source emission.
- `std.http`: may dispatch BEAM-backed handlers through an internal bridge.
- `terlan_safenative`: may be called behind `NativeBridge` for native work.

## Edge Cases

- Process cleanup and failure semantics belong to the BEAM runtime contract.
- Non-BEAM targets must reject these modules before artifact emission.
- Native bridge operations must not be used for pure helper calls that can
  lower directly to native functions.

## Types And Interfaces

`Agent[T]`
: BEAM-backed state process abstraction.

`Task[T]`
: BEAM-backed asynchronous result abstraction.

`NativeBridge`
: BEAM-supervised native resource bridge boundary.

`Bytes`
: BEAM-owned binary protocol frame.

`TcpSocket`
: BEAM-owned TCP socket handle.

`Port`
: BEAM-owned external process or port handle.

## Testing Notes

- Positive tests should live beside the owning module when source-level
  behavior is testable.
- Backend process behavior is validated through Erlang/BEAM build and runtime
  tests.
- Target-profile tests should reject BEAM modules for incompatible targets.
