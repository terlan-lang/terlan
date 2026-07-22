# VM HTTP Internals

This directory owns focused HTTP lifecycle, deadline, response-memory, and
template-response helpers for the VM HTTP server. The parent `http.rs` module
owns protocol dispatch; these modules own bounded subsystem state.

## Responsibilities

- Enforce process-owned request deadlines through VM timers.
- Account response memory before bytes enter the transport queue.
- Render typed template responses into HTTP output.
- Cancel and shut down handlers without leaking VM streams or process state.

## Core Model

HTTP work remains attached to a VM process and TCP stream. Completion cancels
its timer and releases response memory; timeout or shutdown closes the stream
and exits the handler with a stable typed reason.

## Integration Points

- `runtime::vm::http`: owns server polling and protocol state.
- `runtime::vm::timer`: supplies deterministic process-owned deadlines.
- `runtime::vm::tcp`: owns stream lifecycle and queued bytes.

## Testing Notes

- Adjacent `*_test.rs` modules cover deadline races, memory limits, and template
  responses.
- Lifecycle changes require success, cancellation, and owner-exit regressions.
