# Serve Handler Internals

This directory owns development-server handler helpers. The implementation is
centered on routing requests to static assets or compiler-backed handlers. Its
most important boundary is that HTTP server plumbing stays separate from build
artifact discovery.

## Responsibilities

- Resolve dev-server requests to handler or static responses.
- Own one mutable native execution shard for every parked request invocation.
- Keep route diagnostics tied to build metadata.
- Avoid hand-rolled HTTP protocol behavior.

## Public Surface

- `mod.rs`: handler resolution used by the `serve` command.

## Core Model

The handler layer adapts compiler build outputs to the HTTP runtime used by the
development server.

Generated AOT handlers enter through a linear request invocation. Immediate
handlers release their actor and shard on return. A handler that requests typed
I/O retains only its shard, actor, and pointer-free `PureNativeSuspension`; the
transport receives an exact typed wait token and cannot invoke generated
resume code directly. Completion, cancellation, and invalid wake rejection all
release request-owned VM state before the invocation is discarded.

WebSocket and SSE routes may attach one complete static callback set to their
endpoint plan. Each admitted channel retains the exact native image generation
and permits only one generated callback to execute or wait at a time. Every
callback uses the same entry, typed wait, resume, and cancellation path as an
HTTP handler. Cancellation releases parked callback state before entering the
generated cancellation callback. Close, drain, and cancellation callbacks are
terminal and therefore cannot park: an attempted terminal wait is cancelled
under its actor and shard before the live protocol lease is closed.

WebSocket source callbacks observe open, admitted inbound text, writable,
close, and cancellation events. Ping, pong, and close control frames stay
inside the VM protocol runtime. SSE source callbacks observe open, ready event
data, VM keep-alive emission, graceful drain, and cancellation. The VM retains
the bounded event queue, wire encoding, keep-alive policy, and live stream
lease; generated code receives only owned values and typed wake authority.

After route admission, the finite HTTP exchange retains the channel session
until the production socket takes ownership. WebSocket traffic is decoded and
encoded by tungstenite before bounded frames enter generated callbacks. SSE
traffic uses the VM HTTP writer's chunked stream operations. Queue overflow is
returned as pressure, text or event data may satisfy only the exact parked
`String` wait, peer disconnect enters cancellation, and orderly close or drain
retains queued data until protocol completion.

## Entry And Resume Ownership

The asynchronous boundary has one ownership chain for ordinary requests,
WebSocket connections, and SSE streams:

| Stage | Owner | Retained state | Allowed transition |
| --- | --- | --- | --- |
| Router preparation | Compiler | Static route plans and qualified native callable identities | Materialize one evaluator-free endpoint plan |
| Generation admission | `AotHandlerRuntime` | Loaded native image, materialized router, and shared VM session runtime | Spawn an independently mutable execution shard |
| Generated entry | `AotHandlerInvocation` | Request shard, actor identity, and generated call | Complete and release, or park one suspension |
| External wait | `PureNativeIoWait` | Shard, actor, request, continuation, and boundary-type authority | Construct a wake carrying one owned value of the exact type |
| Generated resume | `PureNativeExecutionShard` | Active shard epoch and the parked suspension | Validate the complete wait identity before heap or continuation mutation |
| Channel serialization | `AotChannelInvocation` | At most one pending invocation and its exact lifecycle event | Reject parallel callback entry; complete, resume, or cancel the pending event |
| Protocol session | WebSocket or SSE callback session | Live VM protocol state, static callbacks, and an `Arc` to the admitted generation | Translate admitted protocol events into the shared channel invocation |
| Cancellation | Request or channel invocation | Parked actor, suspension, and shard | Cancel the actor, shut down the shard, then permit the channel cancellation callback |

Only typed `Receive` transitions cross the adapter boundary. Other generated
transitions remain shard-local and are driven before an observable step is
returned. A wake cannot select a callback or continuation: it can only complete
the exact wait from which it was created. There is no CoreIR module, evaluator,
closure environment, host future, native stack pointer, or untyped callback
handle in retained invocation state.

Cache replacement installs a new immutable `Arc<AotHandlerRuntime>` without
mutating the previous generation. Existing request and channel leases therefore
continue against their original code and router. Once replacement removes the
cache's ownership, the retired generation remains alive only through explicit
leases and becomes unreachable when the final lease drops; there is no retired
generation registry or hidden unload owner.

## AOT-5F Lifecycle Inventory

This matrix is the closure inventory for the native HTTP lifecycle. A complete
row has executable evidence in its named gate. The performance row requires all
three same-machine reports to exist and pass the versioned quantitative
comparison policy; partial report sets are invalid.

| Boundary | Runtime owner | Retained state | Terminal evidence | Status |
| --- | --- | --- | --- | --- |
| Generation replacement and unload | Handler cache and explicit request/channel `Arc` leases | Immutable admitted image, router plan, session runtime | `make tvm-aot-http-generation-lifetime-check` proves old/new execution and final weak-owner retirement | Complete |
| Bounded channel pressure and drain | WebSocket/SSE protocol session and socket-owning transport pump | Accounted frames/events and one linear callback invocation | `make tvm-aot-http-channel-transport-check` proves overflow, typed wake, close, drain, and disconnect cancellation | Complete |
| Request and channel cleanup | Request shard, actor runtime, protocol session, and VM resource tables | Parked continuation, buffers, sessions, timers, resources, and generation lease | `make tvm-aot-http-cleanup-check` proves completion, rejection, cancellation, shutdown, reload, and late-completion cleanup | Complete |
| Runtime fallback deletion | Native handler runtime and descriptor-admitted image cache | Native entry/resume identities and managed ABI values only | `make runtime-aot-only-check` rejects evaluator, command-spawn, synchronous wake injection, and temporary inventory rows | Complete |
| Same-machine performance comparison | Benchmark harness | Checked-CoreIR report, native-AOT report, strict comparison report, and versioned policy sharing one hardware/workload fingerprint | `make tvm-aot-http-performance-check` requires throughput, latency, memory, pressure, longevity, generation-overlap evidence, and in-budget ratios | Complete |

`make tvm-aot-http-lifecycle-inventory-check` composes every lifecycle gate
with the benchmark schema self-test and rejects a partial performance report
set. The complete row is closed by `make tvm-aot-http-performance-check`, which
embeds the committed policy and digest in the comparison report.

This closes AOT-5E orchestration plus all AOT-5F generation isolation, channel
transport pumping, cleanup, fallback deletion, and performance evidence.
Immediate native callbacks
cannot inject wake values: a suspension is cancelled under its exact actor and
shard and rejected. Only request/channel event pumps can resume generated code
through typed wake authority.

The main flow is:

1. Receive an HTTP request shape from the server.
2. Match it against generated route/static metadata.
3. Return a response plan for the runtime to execute.

Important invariants:

- HTTP parsing belongs to the Rust HTTP stack, not this module.
- Static assets must not escape the configured output root.
- Missing handlers must return stable dev diagnostics.
- Every parked generated callback has exactly one invocation owner and event.
- Transport code may complete typed waits but may not invoke continuation
  entries directly.
- Cancelling parked work releases its shard before a cancellation callback can
  enter generated code.

## Integration Points

- `commands::serve`: owns command lifecycle and server startup.
- Build artifacts: provide route and static asset metadata.

## Testing Notes

- Add focused serve command tests when route matching behavior changes.
