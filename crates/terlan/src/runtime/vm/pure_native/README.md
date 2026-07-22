# Direct Native Execution

This module runs admitted Terlan AOT images inside the VM execution-shard
process.

## Ownership

- `direct_backend.rs` loads immutable admitted image code and invokes its fixed
  dispatch ABI through an explicit actor execution context.
- `execution_shard.rs` owns both the actor runtime and its mutable
  `ManagedExecutionRuntime` under an active `VmExecutionShardSupervisor`. It is
  the only ordinary admission, entry/resume, replacement, crash-recovery, and
  shutdown path used by the CLI, tests, REPL, and HTTP handler cache.
- `execution.rs` turns native transitions into scheduler-visible actor
  suspension and resume state.
- `pure_native.rs` owns exact image export resolution and the runtime-facing
  typed call contract.

Every direct backend operation receives a `PureNativeExecutionContext` that
binds one exact actor to an exclusive borrow of shard-owned managed state. The
backend supplies generated code with a stack-scoped allocation context backed
by that actor's heap. Managed results are validated against the admitted result
identity before returning from dispatch. Managed continuation captures become
precise owner-local roots and are removed from transition values until the same
request, owner, and continuation resumes.

Resume uses a linear `NativeContinuationClaim`. Exact identity validation
removes the parked record and creates the claim in one exclusive shard borrow;
restoration consumes that claim. A stale or concurrent second resume therefore
cannot obtain authority, and validation failure leaves the original record
parked.

Typed mailbox transitions use the descriptor's canonical boundary identity.
Managed sender words are copied directly into receiver-owned graph storage;
the VM mailbox carries only an opaque fragment token and exact boundary type.
The shared shard runtime retains each precise mailbox root until receive has
resumed native code and parked any resulting managed capture. Failed graph
copy, mailbox admission, or receiver validation preserves the continuation and
rolls back receiver allocation. Finite Atom remains an image-table scalar.
Explicitly specialized `send_value[T]` and `receive_value[T]` calls therefore
carry arbitrary admitted aggregate and collection identities without an
intermediate public runtime value.

Accepted actor sends produce an opaque mailbox publication receipt only after
accounting, graph copy, precise-root registration, and queue insertion are
complete. Scheduler wakeup consumes the receipt after those writes. The current
same-shard queue obtains this ordering from Rust's exclusive mutable borrow; a
future cross-thread queue must use release publication and acquire consumption
at the same receipt boundary.

Public `String`, Bytes, and Binary arguments are copied into the destination
actor heap according to the admitted export descriptor. Results are copied back
into VM-owned public values before the heap can be released. Arbitrary
`Managed(id)` aggregates remain private until the executable descriptor carries
the complete semantic layout needed for public materialization.

`PureNativeExecutionImage` shares immutable loaded code and admitted metadata.
Each fork receives a new actor runtime and `PureNativeExecutionRuntime` while
reusing the image's code and managed layouts. Mutable heaps, mailbox roots, and
owner-indexed pending continuations remain in that execution runtime and are
reachable only through an exclusive actor context borrow. Shard-local request
identity allocation is owned there as well. `DirectNativeBackend` retains no
actor or request state, and the direct path has no managed-runtime mutex,
thread-local state, or image-wide execution lock. Graceful shard shutdown
rejects parked work, while abnormal owner shutdown discards only the exact
owner's pending continuation and heap. Receiver-owned messages from exited
senders remain valid.

Every spawned shard negotiates the local lifecycle protocol, admits the sealed
descriptor identity and digest, and acknowledges readiness before calls can
enter. Deliberate image replacement drains the ready epoch and advances the
same supervisor to a new sealed generation without consuming restart budget.
Abnormal recovery is separate: it records attributed crash state, enforces
bounded backoff, clears failed actor/runtime state, and admits a strictly newer
epoch. The REPL uses deliberate replacement for changed compiled generations.

The HTTP handler cache stores the immutable image factory rather than a live
shard. Each request forks an empty shard before executing, so cache admission
may use its administrative lock without serializing handler execution.

The isolated native worker remains available only for unsafe external adapter
execution. It has no Terlan image loader, managed execution runtime, actor heap,
or continuation state.

The removed boundary-only driver could service only scalar completion and
`Yield`; all other transitions failed outside an actor runtime. Ordinary
consumers now allocate an actor in `PureNativeExecutionShard`, enter the image
directly, and route every resume through VM-owned transition services. The
shard's typed dispatch trace contains only entry, resume, and completion events
and therefore cannot encode a worker round trip. Entry rejection exits the
allocated actor and releases its backend-owned state before returning the
typed error.

Resume authority is checked against the suspension owner before dispatch or
actor mutation. Once an exact-owner resume begins, every error returns through
the unified actor exit pipeline. That path forgets scheduler state, releases
the continuation lease, mailbox accounting, resources, timers, and managed
heap, propagates linked exits, and publishes monitor notifications before the
original native error is returned.
