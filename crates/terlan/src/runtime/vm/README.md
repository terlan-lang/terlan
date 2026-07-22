# Runtime VM Internals

This directory owns VM runtime helpers for compiler-emitted native code.
Admitted Terlan application images execute directly inside the execution shard;
unsupported program shapes fail during AOT lowering instead of entering an
interpreter or application-dispatch worker.

## Responsibilities

- Classify VM values and unsupported CoreIR shapes for stable diagnostics.
- Keep intrinsic, pattern, std-remote, and value helpers separate from the root
  VM evaluator.
- Preserve target-neutral language semantics across transitional and native
  execution during migration.

## Public Surface

- `value`: renderable VM values, closures, and type classification helpers.
- `intrinsics`: supported CoreIR intrinsic evaluation.
- `patterns`: pattern binding and matching helpers.
- `std_remote`: supported std module remote-call behavior.
- `kind`: compact CoreIR expression and pattern names for diagnostics.
- `checksum`: VM-owned Adler-32 and CRC-32 helpers for copied byte slices.
- `packet`: fixed-format packet length extraction for VM-owned byte streams.
- `bitstring`: UTF-8 scalar emission helpers for VM-owned bitstring work.
- `capability_worker`: bounded external adapter transport with VM-owned actor
  parking, deadlines, cancellation, and stale-reply suppression. Requests carry
  explicit capability and shard-epoch completion identity; replies and crashes
  identify the exact worker generation. Both request and response queues are
  bounded away from scheduler threads.
- `execution_shard_protocol`: coarse supervisor control commands limited to
  image admission, shard lifecycle, inspection, cross-shard routing, and
  recovery. Actor execution and local runtime transitions are not encodable.
- `execution_shard_supervisor`: explicit negotiation, sealed-image admission,
  epoch, readiness, signal, drain, stop, crash, restart-backoff, and quarantine
  lifecycle. A generation is routable only in its fully acknowledged ready
  phase. Crash reports identify the stable shard slot, failed epoch, reason,
  and observation tick directly. A test-only boundary matrix injects failure
  before and after admission, readiness, actor-effect publication, drain, and
  image replacement without adding production failpoint branches. The active
  `PureNativeExecutionShard` owns this state machine; REPL image changes drain
  and replace its existing epoch instead of constructing an unrelated shard.
- `execution_shard_epoch`: exact-generation fencing for routes, mailbox
  publications, continuation resumes, resource notifications, capability
  completions, timers, HTTP responses, database writes, and external effects.
  Its operation ledger survives shard restart: committed duplicates are
  suppressed, uncertain at-most-once effects are not retried, and replayable or
  idempotent retries require an explicit policy.
- `memory/publication`: typed proof that accounting, payload installation, and
  mailbox insertion complete before a recipient becomes scheduler-visible.
- `actor`: one execution shard's process table, scheduler, timers, resources,
  links, monitors, dynamic modules, database state, and image-generation
  registry. These services are stored by value and never reached through a
  process-global lock.
- `code_server`: lock-free shard-local image publication and actor bindings.
  Its optional mutex-backed wrapper is restricted to administrative
  publication and inspection and exposes no process-transition methods.
- `pure_native`: validates and invokes the direct-AOT artifact through the
  execution-shard backend.
  Direct-image execution exposes separate begin/resume steps: Yield returns
  while its exact VM actor remains parked, and native resume occurs only after
  the scheduler requeues that owner. Typed external I/O uses a pointer-free
  wait authority containing the admitted shard, actor, request, continuation,
  and `TvmBoundaryType`; only that shard can validate the wake, encode its
  owned value, consume the continuation lease, and enter generated resume
  code. A `.tvm` image supplies its callable IDs and typed signatures directly
  from its embedded descriptor.

## Core Model

The parent `runtime::vm` module owns native export routing. The architecture
keeps scheduling, mailboxes,
timers, failures, capabilities, and resources in the VM while compiled code
returns typed runtime transitions rather than an instruction stream.

Important invariants:

- Unsupported CoreIR forms must fail with stable diagnostic text.
- Runtime values must remain Terlan-facing values, not backend-specific terms.
- Helper modules must not introduce a second application code-generation
  backend or expose target-specific lowering as runtime semantics.
- Capability requests and results use the canonical bounded, versioned owned
  value codec; no legacy helper line protocol is admitted.

## Integration Points

- `runtime::vm`: calls these helpers while routing native exports.
- `formal_pipeline`: produces the checked CoreIR modules loaded into the VM.
- `vm/main.rs`: packages direct execution and external capability-worker
  supervision into the standalone runtime binary.

## Testing Notes

- Direct-AOT integration tests cover source-to-image execution.
- Add focused tests for every newly supported NativeIR expression or runtime operation.
