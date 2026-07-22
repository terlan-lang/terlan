# NativeBoundary Internals

This directory owns deadline and actor-parking mechanics for VM-managed native
calls. The parent module owns typed NativeBoundary dispatch and VM memory
accounting; this directory coordinates asynchronous worker completion without
exposing native pointers or host-runtime state to Terlan actors.

## Responsibilities

- Park a runnable process while a native worker request is outstanding.
- Bind each request to one VM timer and enforce worker credit limits.
- Wake the owner exactly once after completion, cancellation, or timeout.
- Return typed NativeBoundary replies for timeout, cancellation, and owner exit.
- Correlate external worker replies by monotonic request identity and suppress
  every completion that arrives after a terminal VM event.

## Core Model

`VmNativeBoundaryDeadlineQueue` indexes pending requests by timer, process, and
request identifier. Starting work reserves worker credit, installs a one-shot
VM timer, and blocks the owner. Terminal timer or worker events validate all
identities, release pending state, and resume only a still-live parked owner.
`runtime/vm/capability_worker.rs` owns bounded background pipe I/O so scheduler
threads only enqueue requests and poll decoded responses. It starts workers
with a cleared environment and explicit capability, worker-class, frame,
lifetime-request, and credit limits.

Every worker policy must also select one closed execution profile from the
shared `NativeBoundaryExecutionProfile` type: `external-adapter`,
`crash-isolated`, or `cross-boundary`. There is deliberately no local profile.
Ordinary actor entry and continuation resume stay in
`PureNativeExecutionShard`; a capability worker cannot load a `.tvm` image or
dispatch application exports.

Inside the worker, a bounded reader thread and one resource-owning executor
thread meet at a coordinator-owned in-flight map. The reader can therefore
deliver a cancellation frame while adapter work is executing without sharing
the mutable resource store. Cooperative manifest exports receive one atomic
request token; wrong-owner, stale, and non-cancellable requests reject the
cancellation acknowledgement. Shutdown cancels cooperative work, drains every
admitted request, and acknowledges only after transport credit returns.

On Linux, `capability_worker/sandbox.rs` uses `bubblewrap` and `prlimit` rather
than custom syscall code. The profile creates PID, IPC, UTS, mount, and cgroup
namespaces where available, drops capabilities, isolates networking unless an
explicit Postgres capability needs it, exposes read-only system roots, mounts
one private writable working directory, and installs fixed address-space, CPU,
file, descriptor, and process limits. Startup fails closed when either tool is
missing. A POSIX exec wrapper closes every inherited descriptor above standard
I/O before bubblewrap starts. The worker independently attests the exact
working directory, environment, limits, and descriptor set before reading
requests. VM admission also rejects undeclared capabilities before allocating a
request identity or parking an actor.

## Invariants

- A process and request identifier may each have at most one pending call.
- Deadline overflow and zero-duration requests fail before actor state changes.
- Worker credit, timer state, and pending indexes are released together.
- Late or mismatched events cannot resume an unrelated process.
- Pipe reads and writes never run on a VM scheduler thread.
- Timeout, cancellation, and owner exit publish cooperative cancellation while
  retaining VM-side exactly-once completion even when the worker replies late.
- Worker EOF or pipe failure immediately cancels all parked requests through
  the deadline queue; actors never wait for their original timeout after a
  known worker-process failure.
- Malformed protocol versions or credit telemetry quarantine the transport,
  reject future requests, and drain parked actors through the same path.
- Linux workers cannot start through the production CLI without the attested
  `linux-bwrap-v1` profile.
- A worker cannot start without an explicit worker-only execution profile, and
  the protocol rejects `local` as an unsupported profile.
- Version 0.0.7 deliberately rejects external capability workers on macOS,
  Windows, and undeclared hosts before work-directory allocation or process
  creation. Native AOT execution remains a separate platform capability.
- macOS admission requires a packaged signed App Sandbox helper. Windows
  admission requires a packaged LPAC/AppContainer helper with Job Object
  limits. Neither platform may substitute the Linux profile or an unconfined
  process.

## Testing Notes

- `deadline_test.rs` covers success, timeout, cancellation, owner exit,
  duplicate identities, credit rejection, stale events, and overflow.
- Parent NativeBoundary tests cover request/reply memory pressure, cleanup,
  typed errors, and reduction accounting.
