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

## Core Model

VM modules model runtime behaviors that are meaningful only when the target
runtime can provide processes, supervision, and message passing. Terlan source
must import these modules explicitly, and target validation owns rejection for
non-VM-capable targets.

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

## Testing Notes

- Positive tests should live beside the owning module when source-level
  behavior is testable.
- Backend process behavior is validated through Terlan-owned runtime tests.
- Target-profile tests should reject VM modules for incompatible targets.
