# Native Boundary Glossary

Status: 0.0.7 baseline contract.

`NativeBoundary` is the compiler/runtime boundary for typed native modules,
host resources, and effects. It covers capability validation, resource
ownership, scheduler accounting, cancellation, cleanup, isolation, and typed
failure propagation.

The Terlan VM remains the runtime across this boundary. NativeBoundary code may
call maintained Rust protocol and OS libraries, but Tokio or another foreign
async executor must not become a Terlan runtime layer. Actor scheduling,
wakeups, timeouts, cancellation, resource lifecycle, backpressure, and
observability are VM-owned semantics.

`NativeModule` is a typed native module exposed to Terlan source.

`NativeResource` is a runtime-owned handle such as a database pool, file handle,
HTTP body, URI, path, vector storage, or sandboxed worker.

`HostCapability` is a declared effect such as filesystem, HTTP, database,
mobile shell, WASI, process, clock, random access, or network access.

## Contract

Renaming the old bridge terminology must not weaken the implementation
contract. Native boundary behavior keeps:

- typed manifests
- capability checks
- resource handles
- lifecycle cleanup
- scheduler accounting
- async isolation
- typed failure propagation

## Gate

The contract is guarded by:

```bash
make native-boundary-terminology-check
```
