# Terlan 0.0.7 Mini AOT Roadmap

This is the focused execution plan for the 0.0.7 direct-AOT pivot. It
decomposes Slices 100 and 101A through 101F from
[`ROADMAP_0_0_7.md`](ROADMAP_0_0_7.md); it does not add release scope or
replace that roadmap.

The normative runtime contract remains
[`TVM_NATIVE_DATA_ABI_SPEC.md`](../runtime/TVM_NATIVE_DATA_ABI_SPEC.md).
The live migration ledger remains
[`TVM_AOT_PIVOT_INVENTORY.md`](../runtime/TVM_AOT_PIVOT_INVENTORY.md).
If this file conflicts with either contract, the contract and main roadmap win.

## Execution Rules

1. Work only on the first unchecked top-level AOT item, scanning from the start
   of this file. Do not skip or reprioritize it.
2. A top-level item includes every nested requirement, positive and adversarial
   test, named gate, inventory update, and closeout gate.
3. Change `[ ]` to `[x]` only after the implementation and every named gate pass.
   Partial evidence stays beneath an unchecked item.
4. Each item must leave ordinary Rust source files at or below 1,000 lines and
   test files at or below 2,000 lines. Run `make rust-quality-check` at closeout.
5. Do not introduce a new interpreter, JIT, generated-application Rust backend,
   serialized instruction artifact, or per-module deployable image.
6. `temporary-migration-support` may decrease but never increase. Any temporary
   row added to the inventory blocks closure of the current item.
7. A blocked item stops AOT work. Do not select a later item to remain busy.
8. Gate evidence must describe the latest unsuppressed invocation. A diagnostic
   run with warning suppression may localize a failure, but it cannot satisfy a
   gate. If a current run contradicts recorded evidence, the item remains
   unchecked and the scorecard must report the current failure until repaired.

## Completion Boundary

The AOT pivot is complete only when all of the following are simultaneously
true:

- every reachable Terlan application function lowers from checked CoreIR
  through Terlan NativeIR to Cranelift native objects;
- one application and target produce one admitted `.tvm` image;
- scalar and managed values execute with the specified ownership, relocation,
  stack-map, continuation, and rejection behavior;
- ordinary same-shard actor operations use native runtime calls rather than the
  supervised worker transport;
- parked continuations, actor contexts, managed heaps, and same-shard runtime
  calls satisfy the thread-neutral readiness constraints in
  [`ROADMAP_0_0_7_MULTICORE_VM.md`](ROADMAP_0_0_7_MULTICORE_VM.md) without
  claiming that scheduler threads or parallel actor execution already exist;
- the supervisor can admit, start, health-check, drain, terminate, attribute a
  crash to, and restart an execution shard through a versioned coarse protocol;
  shard epochs reject stale messages, resumes, routes, and worker completions;
- compiler-produced sealed Terlan images are the only native code loaded into
  an execution shard. External Rust/C/C++/CUDA code runs behind a bounded,
  asynchronous, capability-scoped worker protocol with cancellation, deadlines,
  backpressure, and single-shot completion;
- each admitted image generation remains pinned until no native frame,
  continuation, actor heap descriptor, mailbox fragment, resource callback, or
  diagnostic metadata can reference it, and unload is proven quiescent rather
  than inferred from request completion;
- build, run, test, REPL, HTTP, debugger, hot reload, package validation,
  support bundles, and installers consume native images without evaluator
  fallback;
- the inventory contains zero `temporary-migration-support` and zero
  `deletion-debt` rows; and
- every AOT gate and required closeout gate passes.

## Current Scorecard — 2026-07-22

| Boundary | State | Evidence |
| --- | --- | --- |
| Native descriptor and `.tvm` admission | Complete | Main roadmap Slices 101B and 101C |
| Direct scalar code generation | Complete bootstrap | `Unit`, `Int`, `Float`, and `Bool` native lane |
| Scalar continuation protocol | Complete bootstrap | Stable entry/resume identities and bounded captures |
| Non-yield scalar transition protocol | Complete bootstrap | Closed operation tags and VM-owned transition handling |
| Managed heap and precise relocation | Complete foundation | Actor-local moving heap, precise roots, Cranelift stack maps, and typed continuations |
| Variable-sized and recursive values | Complete for AOT-2 | Strings, Bytes, Binary slices, finite image-local atoms, aggregates, List, Map, Set, constructors, direct managed calls, admitted aggregate/collection/atom registries, fixed aggregate and collection public entry/results, shard-owned allocation and continuation roots, receiver-owned graph transfer, typed process integration, and the bounded aggregate escape matrix pass native positive and adversarial coverage |
| Same-shard actor fast paths | Complete for AOT-4 | Ordinary actor entry, resume, messaging, scheduling, timers, resources, relationships, failure, and cleanup execute in the owning shard without worker or supervisor transport |
| Supervisor/shard lifecycle and recovery | Complete for AOT-4 | A coarse typed protocol owns admission, readiness, inspection, cross-shard routing, drain, crash attribution, bounded restart, epoch fencing, replacement, and quarantine |
| Unsafe capability-worker boundary | Complete for 0.0.7 platform scope | The application-dispatch worker is gone; bounded versioned capability RPC, manifest allowlists, scheduler-class admission, VM-owned asynchronous dispatch, cooperative adapter polling, cancellation delivery, request limits, payload limits, and an attested Linux bubblewrap/prlimit profile are in place. External capability workers are Linux-only in 0.0.7; macOS, Windows, and undeclared hosts fail before allocation or process creation |
| Image-generation lifetime and unload | Complete for local AOT consumers | Native frames, continuations, managed heaps, mailbox fragments, timers, resources, asynchronous callbacks, debugger pins, and crash metadata contribute to one generation-reference proof; hot reload, timeout quarantine, stale-generation rejection, and diagnostic attachment have executable coverage |
| HTTP native execution | Complete for AOT-5 | Managed Request/Response values, routing, middleware, templates, sessions, cookies, typed errors, WebSocket/SSE continuations, generation lifetime, pressure, cleanup, fallback deletion, and policy-checked same-machine performance evidence pass through native images |
| Interpreter and evaluator retirement | Complete hard cutover | Runtime CoreIR/VMIR execution was removed after serving as predecessor semantic and HTTP benchmark evidence; missing AOT coverage is now a loud error |
| Other consumer migration | Complete for named local consumers | Test, REPL, debugger, hot reload, package validation, support bundles, and installers consume admitted native images and fail closed on incompatible or disguised artifacts; the cross-platform execution matrix remains open |
| Native platform matrix | Partial | Six native CI/release runners and run-bound strict aggregation are implemented; Linux x86-64 is green locally, while the first complete six-target official CI aggregate remains required |
| Incremental compilation closure | Complete for AOT-7 | Checked implementation reuse, bounded parallel frontend work, independent package objects, one final link, persistent REPL reuse, cache poisoning rejection, and enforced cold, incremental, and warm performance policies pass |
| Transitional inventory | Complete | Zero `temporary-migration-support` and zero `deletion-debt` rows remain |
| Active AOT gates | AOT-8 local graph green | The reduced aggregate, native consumers and transitions, managed runtime, HTTP lifecycle and performance, incremental compilation, hard-cutover deletion, installed-release scan, and Rust quality pass on fresh unsuppressed Linux x86-64 invocations |
| Ordinary Rust build | Green | `make rust-warnings-check` passes for all ordinary binaries with `-D warnings` |

Formal checklist state is seven of nine top-level AOT items complete. The first
unchecked item is AOT-6.

## Hard-Cutover Gate Policy

The checked-CoreIR evaluator and serialized-VMIR interpreter were useful
predecessor implementations. They established executable semantics and the
first meaningful HTTP-handler performance baselines; their retirement is an
architectural transition, not a defect correction. Runtime compatibility
layers are nevertheless forbidden after the cutover: unsupported AOT coverage
must fail loudly.

Evaluator/parity-era commands were intentionally deleted and must not be
recreated as AOT closure requirements. PyTorch, Polars, SQL, C++, Tree-sitter,
editor, JavaScript, broad VM-coverage, and other non-primary lanes remain paused
outside the active AOT graph. Their paused status is not evidence that the AOT
pivot or the 0.0.7 release is complete.

The active default AOT graph is the 12 commands behind `make check-gates`.
Item-specific AOT benchmark and compilation-time gates, Rust size/quality, and
roadmap integrity remain explicit closeout work even when they are not members
of that reduced default graph.

## Immediate Execution-Shard Correction

This correction blocked further managed-worker transport work and closed as
part of AOT-2. The required process architecture is:

```text
VM supervisor
    -> coarse lifecycle, admission, inspection, routing, and recovery protocol
execution-shard process
    -> actor scheduler, actor heaps, continuations, admitted AOT image,
       and direct runtime ABI calls
unsafe native-adapter worker
    -> asynchronous capability RPC only for Rust/C/C++/CUDA package operations
```

The supervisor protocol must not carry ordinary application calls, local actor
transitions, or continuation captures. The execution shard owns Terlan
application execution and may share immutable image code across its actors.
The unsafe adapter worker owns only external resources and adapter-local state;
it must not own a Terlan actor heap, Terlan continuation, scheduler lease, or
application dispatch loop. The current application-dispatch worker is
transitional architecture and no new managed-value feature may land there.

Every supervisor, shard, image generation, actor owner, continuation lease,
capability request, and worker completion must carry enough stable identity to
reject traffic from an earlier shard epoch. Recovery never replays an unsafe
external effect implicitly. A caller may retry only through an explicit
operation policy; late or duplicate completions are discarded deterministically.

The capability worker must start with a declared allowlist and bounded resource
profile. It receives no inherited listener, database, cache, or scheduler file
descriptors; no ambient application environment; and no authority to load a
Terlan application image. Payload size, queue depth, in-flight work, wall time,
memory, and child-process creation are bounded and observable. Platform sandbox
mechanisms may differ, but the capability and resource contract is portable.

## Ordered Work

- [x] AOT-1: implement the managed runtime foundation.
  - Define the internal `TvmRef[T]`, runtime-private object metadata, reference
    maps, allocation classes, and deterministic layout fingerprints without a
    public fixed heap header.
  - Implement actor-local bump allocation, a moving collection profile,
    precise root discovery, reference relocation, bounded collection work, and
    whole-heap actor-exit reclamation.
  - Emit Cranelift safepoints and compact stack maps for native frames and
    continuations. Conservative stack scanning is forbidden.
  - Prove a minimal managed object can allocate, survive relocation, cross a
    typed continuation, and be reclaimed without leaking a raw pointer through
    the descriptor or control protocol.
  - Reject missing maps, invalid layouts, cross-actor references, stale roots,
    borrowed values crossing safepoints, and corrupted relocation metadata.
  - Gates: `make tvm-direct-aot-backend-check`,
    `make tvm-managed-memory-check`, and `make rust-quality-check`.
  - Exit: managed references remain correct under optimized native code,
    repeated collection, actor churn, and forced relocation.
  - Evidence 2026-07-21: the canonical managed layout, `TvmRef[T]`, precise
    roots and stack maps, actor-local bump heap, bounded moving collection,
    continuation relocation, churn, and whole-heap reclamation are implemented.
    The Cranelift regression proves a live managed reference receives a precise
    stack map across a real safepoint. `make tvm-direct-aot-backend-check`
    passes with 5,373 compiler tests, and `make tvm-managed-memory-check` and
    `make rust-quality-check` pass. VM coverage inventory now attributes split
    implementation and test fragments to their canonical owning modules.

- [x] AOT-2: compile the complete managed value families.
  - Land immutable `String`, Bytes, Binary/bitstring slices, and atoms with
    checked lengths, UTF-8 validation, bounds, ownership, and shared-bulk
    storage behavior.
  - Land tuples, fixed arrays, records, constructors, options, results, and
    recursive algebraic data with descriptor reference maps.
  - Land the specified List, Map, and Set profiles without per-element
    interpreter or universal-dispatch fallback.
  - Carry every supported managed type through arguments, returns, native
    calls, failures, mailbox transfer, and owned continuation captures.
  - Move managed allocation and continuation ownership into the execution-shard
    actor runtime before extending typed process integration. Remove
    application dispatch and Terlan managed heaps from the unsafe native-adapter
    worker; do not solve managed AOT by serializing additional values through
    its protocol.
  - Delete the worker's Terlan application-image loader, dispatch symbol,
    managed heap, continuation parking, and application control frames. Its
    replacement protocol may carry only declared external capability requests
    and bounded owned results; raw `TvmRef`, actor heap words, continuation
    captures, scheduler leases, and native stack addresses are forbidden.
  - Prove scalar specialization still avoids managed allocation where escape
    analysis permits it.
  - Reject invalid UTF-8, length overflow, malformed tags, invalid reference
    maps, cross-owner graphs, and unbounded collection shapes.
  - Stress managed process paths with collection during mailbox graph transfer
    and continuation resume, actor exit while transfer is in flight, forced OOM
    and work-budget exhaustion, and repeated actor/mailbox churn. Every failure
    must roll back atomically, leak no roots or heap objects, and publish no
    cross-owner reference.
  - Keep actor-heap ownership exclusive but movable through an explicit
    owner/epoch token. Native frames, continuations, mailbox fragments, and
    collection metadata must not embed a scheduler-thread identity; movement
    between future shards must require quiescent ownership handoff rather than
    concurrent heap access.
  - Gates: `make tvm-managed-memory-check`,
    `make tvm-managed-list-profile-benchmark-check`,
    `make tvm-direct-aot-backend-check`,
    `make tvm-aot-shard-ownership-check`,
    `make tvm-aot-capability-worker-check`,
    `make runtime-aot-only-check`, and
    `make rust-quality-check`.
  - Exit: the managed value matrix in the ABI specification has native positive
    and adversarial coverage with no direct CoreIR evaluation.
  - Evidence 2026-07-21: the first value-family slice implements finite
    deterministic atom tables, actor-local immutable `String` and Bytes values,
    checked Binary/bitstring slices, precise backing-storage relocation, and
    semantic-type validation. NativeIR and image descriptors now preserve Atom,
    String, Bytes, and Binary argument and result kinds as native word slots;
    managed content equality is rejected instead of degrading to pointer
    identity. A mixed image containing managed and scalar exports preserves the
    scalar worker lane while withholding managed references from its transport.
    `make tvm-managed-memory-check`, descriptor round-trip tests, the focused
    direct-AOT integration test, and `make rust-quality-check` pass.
    Fixed tuples, homogeneous arrays, ordered records, finite constructors,
    `Option`, `Result`, and recursive algebraic graphs now share the canonical
    managed descriptor, include representation shape in layout fingerprints,
    preserve exact semantic identities across native signatures, and survive
    precise relocation. Same-sized incompatible shapes, malformed variants,
    invalid scalars, wrong reference types, and arity mismatches are rejected
    before publication. Constructor expression lowering, mailbox graph transfer,
    dead allocation-only constructor elimination, and projection-only live
    constructor scalar replacement and typed process propagation have since
    landed. Statically irrefutable tuple patterns and exact-identity constructor
    patterns now flatten directly into ordered scalar locals, including nested
    fixed shapes and wildcard evaluation retention. Real Terlan tuple
    destructuring reaches linked direct-AOT machine code without a managed
    allocator; literal, alias, identity-mismatched, arity-mismatched, and nested
    aggregate-capture patterns remain under ordinary matching semantics. Fixed
    tuple and exact-constructor values may now cross adjacent nested CoreIR let
    regions into one later destructuring consumer: fields evaluate at the
    producer, pattern locals enter scope at the consumer, and any aggregate use
    before or after that consumer blocks replacement. Compile-time in-bounds
    tuple and fixed-array indexes now use the same ordered scalar slots for both
    explicit CoreIR index nodes and checked `IndexGet.get_at` calls. Direct and
    local Terlan tuple indexing lower to allocation-free NativeIR while dynamic,
    negative, and out-of-range indexes retain the ordinary bounds contract. A
    backend-local interprocedural pass now specializes private, single-clause,
    projection-only helpers over managed aggregate parameters. Constructor and
    existing-local arguments evaluate once, all direct uses are rewritten, and
    the helper ABI is removed only after an unresolved-use audit. Public,
    recursive, referenced, unused, or unsupported helpers remain intact. The
    planned pattern, index, and callee scalar-replacement matrix is complete;
    AOT-2 remains open for its broader managed-value matrix and gates.
  - Evidence 2026-07-21: the initial actor-heap `List[T]` profile now has a
    canonical empty root, compact inline roots through eight elements, packed
    32-element leaves, regular and relaxed 32-way internal nodes, cumulative
    relaxed size tables, multilevel lookup, persistent versions, bounded front
    views, excluded-leaf retention trimming, and precise reference-bearing leaf
    maps. Exact `List[T]` identities cross NativeIR signatures and mixed images
    without entering worker transport. Tree append and indexed update now copy
    only the changed root-to-leaf path, full trees grow through bounded fringe
    lifting, and concatenation rebalances only the touching RRB fringes while
    sharing untouched subtrees. Allocation-delta regressions enforce those
    bounds across 1,024- and 2,048-element fixtures. Subtraction removes the
    first structural match per removal value through compiler-specialized
    equality, including managed-reference content comparison without pointer
    identity. Dual-path swap copies shared ancestors once and preserves prior
    versions. The transient list builder now exclusively borrows its actor heap,
    rejects invalid batches atomically, prevents relocation while references are
    buffered, and publishes the same canonical inline/RRB forms without
    per-element managed allocation. A 2,050-element regression proves identical
    70-object tree construction. The explicit release profile benchmark now
    records inline, regular, relaxed, path-copy update, full-tree append,
    concatenation, and transient construction timings in the versioned
    `terlan.tvm.managed-list-profile.v1` artifact. Its portable structural gates
    enforce zero lookup allocations and maxima of 3 update, 4 append, 6 concat,
    and 70 transient-build objects.
  - Evidence 2026-07-21: managed `Map[K, V]` and `Set[T]` now have canonical
    empty and packed flat roots in the actor heap. Construction validates every
    typed slot before publication, replaces duplicate map values in their
    original insertion position, removes duplicate set elements, and consumes
    compiler-specialized structural key equality rather than managed-reference
    identity. Lookup, ordered materialization, persistent put, take, remove,
    and clear preserve earlier roots; absent removal reuses the existing root.
    Map and Set share one physical storage implementation, and List and Map now
    share checked slot-layout arithmetic. Precise reference maps relocate both
    managed keys and values. `make tvm-managed-memory-check` passes with focused
    Map and Set suites.
  - Evidence 2026-07-21: the adaptive Map/Set profile is now complete at the
    managed-storage layer. The benchmarked 128-entry promotion threshold is
    owned once above both native-image and legacy evaluator consumers. Indexed
    roots reference private immutable entry objects through a managed A-CHAMP
    trie and a managed RRB insertion-order list. Leaf blocks, full-hash
    collision nodes, bitmap sparse nodes, direct-slot dense nodes, and
    compressed paths all use precise actor-heap reference maps. Replacement and
    insertion copy bounded trie and order paths while preserving prior roots;
    removal preserves order and demotes to packed flat storage below the shared
    threshold. `ManagedKeySemantics` binds structural equality to a compatible
    deterministic hash, preventing reference identity or relocation from
    changing key behavior. Focused tests cover the promotion boundary, bounded
    allocation deltas, full-hash collisions, persistent versions, demotion,
    insertion order, managed-content keys, and moving collection. Cross-boundary
    managed-value propagation and broader aggregate specialization are the
    remaining AOT-2 work, not additional collection storage.
  - Evidence 2026-07-21: checked fixed constructor declarations now lower into
    first-class NativeIR construction nodes carrying the canonical managed
    aggregate descriptor directly. Application lowering validates stable
    per-union discriminants, exact scalar and managed field kinds, local and
    fully qualified constructor identities, and exact managed result identity.
    Declaration order cannot change layout; unresolved calls fail with a typed
    diagnostic; variable-arity constructors remain outside this fixed-layout
    slice. The following allocation slices close the generated-code gate with
    an owner-aware callback rather than a process-global heap.
  - Evidence 2026-07-21: the owner-aware aggregate allocation contract now has
    a deterministic descriptor codec bounded to 64 KiB. NativeIR encodes every
    admitted fixed constructor layout at compile time; decoding reconstructs
    the canonical tuple, fixed-array, record, or constructor descriptor before
    allocation through one explicit `ActorHeap`. The contract rejects bad
    magic or version, truncation, trailing bytes, invalid UTF-8, duplicate
    fields, inconsistent arrays, malformed variants, oversized descriptors,
    field mismatches, and cross-actor references without publishing a partial
    object.
  - Evidence 2026-07-21: generated constructor code now carries hidden actor
    context and managed-allocator arguments through dispatch, direct calls,
    suspending calls, and tail calls without exposing them as Terlan function
    parameters. Constructor descriptors live in immutable native-object data;
    field words use bounded stack storage; the C-compatible callback receives
    descriptor and field bounds plus caller-owned result storage. Callback
    status propagates through the native status path, while absent callbacks
    and zero references returned as success are rejected. The exported ABI now
    also declares arity as an integer and result storage as a pointer instead of
    relying on their coincidental equal width. Scalar workers pass null runtime
    arguments and remain valid because scalar code never dereferences them. An
    executable generated-object fixture covers callback invocation, exact field
    transport, null-runtime rejection, invalid-reference rejection, and opaque
    reference return.
  - Historical worker-managed allocation and continuation experiments proved
    the generated ABI before direct shard ownership landed. Their application
    dispatch, heap, and continuation implementation has now been deleted from
    the external worker rather than retained as migration support.
  - Evidence 2026-07-21: receiver-owned managed mailbox fragments now copy a
    sender graph under an explicit work budget without leaking a sender-local
    reference. Iterative child-first inventory validates the root semantic
    identity plus every edge's precise-reference and ownership metadata, copies
    every distinct object once, preserves shared edges, and emits a precise
    mailbox root that relocates with the receiver heap. A staged heap makes
    ownership, root-type, budget, and hard-limit failures atomic for both
    actors. The native-worker suite passes 73 tests, including a 2,001-object
    recursive graph, and `tvm-managed-memory-check` owns the focused mailbox
    gate. The isolated worker remains scalar-only; the later direct-shard slice
    connects these fragments to generated typed process Send and Receive.
  - Evidence 2026-07-21: application lowering now retains each module's
    canonical constructor layout table and threads it through ordinary bodies,
    yield prefixes, transition arguments, and resumed continuations. A
    conservative reverse lexical liveness pass removes dead allocation-only
    constructor graphs before NativeIR layout inventory and managed callback
    emission, preserves unknown or effectful evaluation, respects shadowing,
    and compacts retained local indexes. Constructor fields participate in
    suspension admission and composed-continuation local rebasing. Eleven
    focused regressions include production suspension lowering and an executable
    generated object that returns successfully with a null allocator, proving
    the eliminated path cannot reach the callback. The focused suite is owned
    by `make tvm-managed-memory-check`.
  - Evidence 2026-07-21: live fixed constructors whose aggregate identity is
    used only by direct named-field projections now scalar-replace into one
    native local per source field. The compiler evaluates every field exactly
    once in source order, preserves lexical shadowing, and conservatively keeps
    allocation for aggregate uses, unknown fields, patterns, indexes, and
    unsupported control shapes. Production candidate admission and application
    lowering use the same transformation. Nine focused regressions include a
    source-to-application path and executable generated code that returns with
    a null allocator, proving the optimized path cannot allocate. The suite is
    owned by `make tvm-managed-memory-check`. Broader pattern/index/callee
    aggregate escape analysis remains.
  - Evidence 2026-07-21, execution-shard correction slices 1-2: native
    execution is backend-independent and the shared application boundary now
    constructs the direct shard backend by default. Standalone VM, REPL, test,
    command, and HTTP-cache consumers therefore load admitted images in their
    scheduler-owning process. Scalar application dispatch and continuations no
    longer use `TERLAN_NATIVE_WORKER`, stdin/stdout frames, or a per-image child
    process. A freshly compiled native `main/0` executes successfully through
    the direct path, and `tvm-aot-shard-ownership-check` rejects reintroduction
    of application worker transport. The direct backend now lends a
    stack-scoped shard heap to each dispatch, validates managed results against
    owner, generation, and semantic result identity, and parks/restores precise
    managed continuation roots outside scheduler-visible transition values.
    Both the direct backend and isolated adapter worker now consume one shared
    `ManagedExecutionRuntime`; each spawned backend receives independent heaps
    and pending roots while immutable admitted code is shared. Shutdown rejects
    parked continuations and releases backend-owned heaps. The executable
    direct-AOT fixture runs a retained constructor allocation, a constructor
    captured across `yield_now`, and a spawned child allocating through an
    independent backend fork, all through standalone `terlan-vm` with
    `TERLAN_NATIVE_WORKER` absent. `tvm-aot-shard-ownership-check` now owns the
    executable direct-AOT prerequisite.
  - Evidence 2026-07-21, public sequence boundary: application export
    resolution now preserves exact `TvmBoundaryType` signatures instead of
    filtering the image through the obsolete scalar-worker profile. The direct
    backend copies public String, Bytes, and non-byte-aligned Binary arguments
    into the destination actor heap, validates returned owner/generation/type
    identity, and materializes results into VM-owned values without exposing a
    `TvmRef`. Compound Binary allocation rolls back its backing Bytes object on
    failure. Focused tests cover exact managed export projection, UTF-8,
    embedded-zero bytes, partial-byte binaries, foreign owners, mistyped
    references, mismatched public values, zero owners, and atomic invalid-range
    failure; the real direct-AOT fixture remains green.
  - Evidence 2026-07-21, public fixed aggregate boundary: NativeIR now carries
    the canonical aggregate layouts reachable from each module into the single
    application descriptor. Descriptor format 1.2 admits a unique ordered
    `(semantic_id, encoded_layout)` table, re-decodes every bounded layout, and
    rejects malformed bytes, semantic mismatches, duplicates, and noncanonical
    encodings. The direct backend builds one immutable registry shared across
    empty-heap forks and resolves constructor variants by exact physical
    fingerprint. Public `Managed(semantic_id)` arguments and results now
    recursively convert tuples, fixed arrays, ordered records, finite
    constructors, nested aggregates, String, Bytes, and Binary references.
    Conversion enforces exact shape and field order, owner and semantic
    identity, finite floats, a depth/work budget, cycle rejection, and atomic
    rollback of the complete nested graph. The real generated-image fixture
    proves that a `TVMA` layout table is embedded before exercising allocation,
    public aggregate result materialization, continuation resume, and child
    forks. `make tvm-managed-memory-check` and
    `make tvm-aot-shard-ownership-check` pass. The planned aggregate escape
    matrix, external-worker role cutover, and VM-side asynchronous capability
    routing, cooperative adapter polling, the attested Linux sandbox, and a
    fail-closed platform capability matrix have since landed; closeout gates
    keep AOT-2 open.
  - Evidence 2026-07-21, public collection boundary: checked CoreIR signatures
    now inventory every concrete List, Map, and Set schema, including nested
    collection references, into canonical `TVCL` metadata. Descriptor format
    1.3 admits an ordered unique schema table and rejects malformed bytes,
    semantic mismatches, duplicates, unsupported slots, and invalid arities.
    The direct shard recursively allocates and materializes public collections
    through the existing RRB and A-CHAMP heap APIs. Reference-valued Map keys
    and Set elements retain stable Terlan content equality and hashing across
    relocation. Focused tests cover nested collections, duplicate replacement,
    Set deduplication, indexed-profile promotion, wrong shapes, late type
    failures, bounded conversion, and whole-graph rollback. The generated AOT
    fixture proves List, Map, and Set schemas reach the executable image; no
    collection node layout or raw `TvmRef` enters the public boundary.
  - Evidence 2026-07-21, finite atom projection: checked application CoreIR now
    inventories atom literal identities from types, expressions, and patterns
    into a sorted unique image table while retaining `Unit` and Bool as compact
    scalar values. Descriptor format 1.4 carries that table in optional record
    12 and rejects invalid, duplicate, or noncanonical identities. The direct
    shard resolves standalone Atom arguments/results and Atom slots in fixed
    aggregates, List, Map, and Set values through the immutable image registry;
    unknown text and invalid indexes fail closed. Atom Map/Set keys hash and
    compare by canonical text rather than generation-local index. Focused tests
    cover recursive compiler inventory, descriptor admission, standalone and
    nested round trips, deduplication, unknown identities, malformed indexes,
    and a generated direct-AOT image containing the checked `ready` atom.
  - Evidence 2026-07-21, typed mailbox value families: `std.vm.Process` now lowers
    immutable String, Bytes, Binary, and finite Atom send and receive calls into
    fixed native transition frames
    carrying one canonical three-word `TvmBoundaryType` identity. The direct
    backend materializes sender-local content before publication and allocates
    receiver-local content before removal; no raw managed reference enters the
    actor mailbox. Messages retain exact boundary identity beside their owned
    VM value, so selective receive skips untyped or differently typed
    lookalikes. Receiver allocation and conversion failure preserve mailbox
    order, logical accounting, and the parked continuation lease. Native status
    decoding, continuation capture partitioning, and composed-call transition
    sizing understand typed Send and Receive while the public VM control
    operation set remains unchanged. Focused tests cover metadata round trips,
    malformed and noncanonical identities, CoreIR and NativeIR lowering, exact
    mailbox selection, successful ownership transfer, conversion rollback,
    generated frame partitioning, and a full Send-resume-Receive-resume-return
    actor lifecycle. An additional adversarial test queues identical octets as
    Bytes and Binary and proves exact sidecar matching instead of payload-shape
    matching.
  - Evidence 2026-07-21, arbitrary typed mailbox source values: public
    `Process.send[T]` and `Process.receive[T]` require an explicit concrete
    specialization and retain that exact `CoreType` in parameterized CoreIR
    identities. `Process[T]` and `Message[T]` remain opaque source types, while
    `Message.wrap[T]` and `Message.unwrap[T]` erase before NativeIR.
    Typechecking rejects an omitted or multiply supplied specialization before
    AOT admission. NativeIR derives one canonical, module-qualified
    `Managed(id)` from the checked payload and emits it through the existing
    typed Send and Receive frame. A real `Pair` fixture now passes CoreIR
    lowering, NativeIR metadata validation, generated-object linking,
    direct-AOT project compilation, descriptor admission, and image execution.
    No semantic hash or layout identifier appears in Terlan source.
  - Evidence 2026-07-21, direct managed mailbox propagation: direct backend
    forks now share one locked execution-shard managed runtime while preserving
    exclusive actor heap ownership and fork-local continuation state. Managed
    Send copies cross-owner graphs under the existing bounded staged copier;
    self-send retains a precise root without duplicating the immutable graph.
    The VM mailbox carries only an opaque fragment token and exact boundary
    identity while the managed runtime owns the relocatable root. Receive
    validates the receiver-local word and retains that root until native resume
    has parked any managed captures. Mailbox hard-limit rejection rolls back
    the receiver heap and retains the sender continuation. Sender shutdown does
    not invalidate receiver-owned queued graphs. Focused tests cover cross-owner
    ownership, self-send, rollback, and a full managed Send/Receive lifecycle
    without transition `ReplValue` conversion. The real direct-AOT fixture now
    constructs, sends, receives, and returns a `Pair` through `terlan-vm`.
    Typed process propagation and the planned pattern/index/callee aggregate
    escape matrix are complete.
    Historical focused managed and execution fixtures pass. The latest
    unsuppressed `make tvm-aot-shard-ownership-check` rerun passes all 12 native
    image admission tests and artifact-format validation, then fails compilation
    under `-D warnings` on unused imports and dormant runtime surfaces, including
    the transitional native worker. `make rust-quality-check` and the ordinary
    unsuppressed Rust build therefore remain red; warning-suppressed diagnostics
    do not close AOT-2.
  - Evidence 2026-07-21, external capability-worker cutover: the
    `terlan-native-worker` executable no longer accepts a `.tvm` path, loads a
    dynamic library, resolves application dispatch symbols, allocates through a
    Terlan actor heap, or parks application continuations. It now composes the
    existing `NativeBoundaryWorker` resource and credit core with Postgres
    manifest capability and scheduler-class admission through a versioned,
    newline-framed owned-value protocol. Startup authority defaults empty;
    request count, credits, payload bytes, recursive term work, operation
    manifests, process-owned resources, response bytes, and protocol versions
    are bounded or fail closed. The obsolete worker managed-runtime and
    continuation modules and their zero-match Make selectors are deleted.
    `make tvm-aot-capability-worker-check` prevents image-loading, application
    dispatch, managed-runtime, and continuation symbols from returning to the
    worker.
  - Evidence 2026-07-21, VM-owned capability transport: the VM and worker now
    share one versioned request, response, owned-value, opaque-handle, and
    bounded JSON-line codec. A closed worker policy launches the executable
    with a cleared environment and explicit capability, scheduler-class,
    payload, lifetime-request, and credit limits. Bounded background writer and
    reader threads isolate pipe blocking from scheduler threads; the scheduler
    only parks actors, nonblocking-enqueues requests, polls typed responses, and
    wakes owners through `VmNativeBoundaryDeadlineQueue`. Request identities
    are monotonic and correlate to VM timers. Completion, timeout,
    cancellation, and owner exit remain exactly once; terminal VM events emit
    cooperative cancellation frames and late replies are suppressed. Worker
    EOF or pipe failure immediately drains every parked call through the same
    cancellation and wakeup path. The focused Make gate covers both protocol
    ends, VM lifecycle races, and a real child-process call and orderly
    shutdown. Invalid versions and impossible credit telemetry quarantine the
    transport before pending actors are drained.
  - Evidence 2026-07-21, Linux capability sandbox: VM worker launch now uses
    the established `bubblewrap` and `prlimit` tools instead of custom syscall
    code. The fixed `linux-bwrap-v1` profile provides private PID, IPC, UTS,
    mount, and cgroup namespaces where available, drops all capabilities,
    creates a new session, terminates with its parent, removes networking unless
    the Postgres capability is declared, exposes read-only runtime roots, and
    mounts one private `0700` working directory plus an ephemeral `/tmp`.
    Address space, CPU time, output-file size, descriptors, process count, and
    core dumps have fixed hard limits. The worker CLI requires the profile and
    attests its working directory, exact environment, kernel limits, and lack
    of inherited descriptors before reading a frame. Missing tools, direct
    unsandboxed startup, unknown profiles, and attestation drift fail closed.
    The focused gate executes a real sandboxed child through request, reply,
    and orderly shutdown.
  - Evidence 2026-07-21, cooperative capability execution: the worker frame
    reader no longer blocks behind synchronous adapter dispatch. A bounded
    reader, coordinator, and single resource-owning executor preserve one
    mutable resource store while allowing cancellation and shutdown frames to
    arrive during adapter work. The coordinator owns in-flight credit and
    process identity; it acknowledges cancellation only for the matching owner
    of a manifest export marked `Cooperative`. One cloneable atomic token now
    reaches the executor and cancellation-aware resource-dispatch checkpoints.
    Completion after cancellation uses the canonical terminal error and cannot
    retain transport credit. Shutdown cancels cooperative requests, drains all
    admitted work, and emits its acknowledgement only after the executor is
    quiescent. The focused regression holds an adapter in an active polling
    loop, rejects a wrong-owner cancel, accepts the matching cancel, observes
    the token inside that loop, and proves terminal reply and credit release.
    `make tvm-aot-capability-worker-check`, all 242 native-worker tests, the
    worker-only private-item documentation gate, formatting, and diff checks
    pass.
  - Evidence 2026-07-21, capability-worker platform admission: native AOT
    target support and unsafe external-adapter support are now separate
    capabilities. One canonical host/profile type admits only
    `linux-bwrap-v1` for 0.0.7. VM startup resolves that profile before private
    directory allocation or process construction. macOS returns a stable error
    requiring a packaged signed App Sandbox helper; Windows returns a stable
    error requiring a packaged LPAC/AppContainer and Job Object helper; unknown
    hosts fail with no declared contract. The worker parser accepts only the
    profile selected for its compilation target, so a Linux profile cannot be
    replayed on another host and no unconfined fallback exists. Pure platform
    tests exercise every host family on Linux, while the real Linux child gate
    continues to prove executable confinement. Non-Linux capability workers
    are explicitly outside the 0.0.7 support matrix rather than an unverified
    AOT blocker; adding one later requires its native CI runner, packaged helper,
    in-worker attestation, and the same full-cycle gate before admission.
  - Closeout evidence 2026-07-21: fresh unsuppressed invocations of
    `make tvm-managed-memory-check`,
    `make tvm-managed-list-profile-benchmark-check`,
    `make tvm-direct-aot-backend-check`,
    `make tvm-aot-shard-ownership-check`,
    `make tvm-aot-capability-worker-check`, `make runtime-aot-only-check`, and
    `make rust-quality-check` all pass. The managed-memory matrix covers every
    admitted scalar, sequence, aggregate, recursive, List, Map, Set, public
    boundary, continuation, and typed mailbox representation with rejection
    and rollback cases. The release benchmark emits the bounded
    `terlan.tvm.managed-list-profile.v1` artifact. Direct execution remains in
    the local shard, external capability work completes through the sandboxed
    worker protocol, and the AOT-only scan finds no evaluator path. The
    all-binary `-D warnings` build, formatting check, and diff check also pass;
    Rust quality reports zero oversized files and zero dormant VM modules.

- [x] AOT-3: close full application-function lowering.
  - [x] Compile statically known non-escaping closures and function references
    away before NativeIR admission. Snapshot lexical captures at closure
    creation, preserve argument order and parameter shadowing, lower invocation
    to ordinary `Let` or qualified direct calls, bound expansion and capture
    counts, and reject escapes, dynamic callees, malformed parameter patterns,
    and arity mismatches with stable diagnostics. Gate:
    `make tvm-aot-static-callable-check`.
  - [x] Specialize private higher-order helpers whose function arguments are
    statically known lambdas or named local functions. Evaluate every argument
    into caller-scope temporaries before binding helper parameters, remove the
    specialization-only helper from the native image, and bound each module to
    128 expansions. Reject public higher-order exports, recursive expansion,
    unresolved callback arguments, callback arity drift, and unsupported helper
    clauses before linking. Gate:
    `make tvm-aot-higher-order-specialization-check`.
  - [x] Eliminate scalar `Case` expressions before NativeIR admission. Evaluate
    each scrutinee once, retain ordered first-match clauses and short-circuiting
    guards, and bind variable and alias patterns independently in guards and
    selected bodies. Support integer, boolean, `Unit`, wildcard, variable, and
    alias patterns; reject arbitrary atoms, payload-bearing constructors, and
    structured patterns before linking. Bound cases to 256 clauses and 64
    nested expressions. Gate:
    `make tvm-aot-case-lowering-check`.
  - Compile closures and owned environments, higher-order calls, imported and
    remote calls, generics under the bounded-specialization policy, pattern
    matching, recursion, loops, failures, and all pure/effectful control flow.
    - [x] Define one image-local owned closure representation containing a
      stable callable identity and a precisely traced immutable capture
      environment. Closure values must carry exact parameter/result identity,
      remain valid across collection and continuation parking, pin their image
      generation, and never embed a native code pointer, stack address,
      scheduler-thread identity, or worker connection. Gate:
      `make tvm-aot-owned-closure-representation-check`.
    - [x] Closure-convert escaping lambdas and named function values into
      lifted native functions plus owned environments. Lower bounded indirect
      invocation through an admitted image-local dispatch table, including
      managed arguments/results and suspending calls; reject foreign,
      stale-generation, wrong-arity, and ABI-incompatible closure values before
      invocation. Public higher-order exports must use this ABI rather than the
      current `native_ir.higher_order_export` rejection.
    - [x] Native-lower the remaining reachable value and pattern families now
      classified as rejected by the versioned coverage matrix: List/Map values,
      structured tuple/List/Map/record/constructor patterns, binary patterns,
      comprehensions, and checked casts. Fixed record construction, access, and
      persistent update are native-lowered through the managed aggregate ABI.
      Compiler-only template and constructor-chain forms must disappear before
      admission rather than inherit a runtime fallback.
    - [x] Native-lower the remaining reachable control and effect families,
      including `Try`/failure cleanup, loops expressed through recursion,
      mutable receiver calls, and declared asynchronous capability operations.
      SQL and other paused product lanes remain outside the active AOT graph,
      but a reachable unclassified node must still fail before linking.
    - [x] Add one closed-world application conformance gate that executes
      recursion, imported/remote calls, bounded generic specialization,
      escaping captured closures, dynamic higher-order dispatch, structured
      matching, failure cleanup, and mixed pure/effectful suspension from one
      admitted image. The gate must also exercise stable rejection for every
      unsupported or over-budget counterpart.
  - Replace scalar bootstrap intrinsic names with typed public runtime
    operations over real `Process[T]`, `Message[T]`, and managed values.
    - [x] Replace direct-AOT mailbox `send_value` and `receive_value` bootstrap
      operations with public `Process.send[T](Process[T], Message[T])` and
      current-actor `Process.receive[T]()` operations. Erase `Message.wrap[T]`
      and `Message.unwrap[T]` at compile time, map opaque process handles to
      VM-owned identities, retain canonical payload type words in transition
      frames, require explicit specialization until inferred generic arguments
      survive into CoreIR, and remove fake immutable-mailbox source benchmarks.
      Gate: `make tvm-aot-typed-mailbox-check`.
    - [x] Replace remaining scalar lifecycle names for spawn, timers, links,
      monitors, resources, cancellation, failure, and scheduling with typed
      public operations and opaque VM-owned handles. `Entry[T]`, `Process[T]`,
      `Timer`, `Monitor[T]`, `ResourceKind[K]`, `Resource[K]`, `ExitReason`, and
      `SchedulingClass` are source-visible identities whose fixed-word
      representations remain VM-owned. Descriptor constructors erase in CoreIR;
      effectful operations lower to the existing scheduler transitions. Generic
      opaque imports retain provider arity. Gate:
      `make tvm-aot-typed-lifecycle-check`.
  - [x] Support managed values at every entry/resume point and in nested,
    branching, repeated, tail, and non-tail suspension graphs without a native
    stack identity surviving a park. Non-yielding managed prefix bindings now
    lower into branching control regions, continuation descriptors retain the
    exact managed parameter shape, and the execution shard withholds managed
    roots from transition transport before restoring generated parameter order.
    Gate: `make tvm-aot-managed-continuation-check`.
  - [x] Keep every parked or yielded continuation thread-neutral: owned captures,
    precise roots, and stable image/function/continuation identities may
    survive, but scheduler-thread addresses, thread-local borrows,
    worker-connection identity, and scheduler-local cache pointers may not. The
    scheduler-visible envelope is closed over stable identity, owned transition
    words, immutable resume metadata, and stable trace identity. Both that
    envelope and backend-held precise roots must remain `Send + Sync + 'static`.
    Gate: `make tvm-aot-thread-neutral-continuation-check`.
  - [x] Reject unresolved dynamic calls, ABI-incompatible imports,
    specialization explosions, ambiguous continuation graphs, and unsupported
    reachable functions at compile time. Application admission now rejects
    duplicate module/function identities, missing call providers, incompatible
    or ambiguous imported providers, malformed function arity, and dynamic
    callable residue before NativeIR lowering. The closed continuation graph is
    checked for duplicate, zero, and dangling identities before object
    emission. Gate: `make tvm-aot-application-closure-check`.
  - [x] Maintain a versioned lowering coverage matrix for every executable CoreIR
    expression, pattern, call, effect, and intrinsic node. Each node must be
    classified as native-lowered, compiler-only, or rejected before linking
    with a stable diagnostic; adding an unclassified node is a compile-time
    gate failure. The focused gate is `make tvm-aot-lowering-coverage-check`.
  - Gates: `make tvm-direct-aot-backend-check`,
    `make tvm-aot-application-closure-check`,
    `make tvm-aot-runtime-transition-check`,
    `make tvm-aot-case-lowering-check`,
    `make tvm-aot-closure-dispatch-check`,
    `make tvm-aot-managed-field-projection-check`,
    `make tvm-aot-owned-closure-representation-check`,
    `make tvm-aot-managed-continuation-check`,
    `make tvm-aot-thread-neutral-continuation-check`,
    `make tvm-aot-typed-lifecycle-check`,
    `make tvm-aot-typed-mailbox-check`,
    `make tvm-aot-higher-order-specialization-check`,
    `make tvm-aot-lowering-coverage-check`,
    `make tvm-aot-static-callable-check`,
    `make tvm-aot-application-conformance-check`,
    `make runtime-aot-only-check`, and `make rust-quality-check`.
  - Exit: every reachable function in the conformance applications is present
    in the native image or rejected before linking with a stable diagnostic.
  - Evidence 2026-07-21, lowering coverage gate: compiler-owned coverage
    contract version 1 now explicitly records the currently executable scalar,
    direct-call, constructor, control-flow, process-transition, typed mailbox,
    and compiler-rewrite families. Escaping closures, unresolved function
    values, destructuring patterns, runtime capabilities, unlowered effects,
    and every other unavailable family have stable pre-link diagnostic
    identities rather than an implicit fallback. The static-callable rewrite
    now owns the `Lam`, `FunctionCall`, and `RemoteFunRef` families before
    NativeIR admission. `make tvm-aot-lowering-coverage-check` runs the shared
    backend through `terlan-vm`; all seven focused checks pass.
  - Evidence 2026-07-21, typed process lifecycle: the public Process module no
    longer exposes numeric spawn, timer, link, monitor, resource, cancellation,
    failure, or scheduling operations. Compiler-owned descriptor constructors
    erase to fixed transition words, five parameterized lifecycle intrinsic
    families preserve concrete source types, and NativeIR maps all eight
    lifecycle operations onto the existing VM-owned transition protocol. The
    direct-AOT fixture builds through native object linking with opaque handles,
    raw scalar arguments fail during type checking, malformed transition frames
    fail before parking, all 164 std summaries match, and
    `make tvm-aot-typed-lifecycle-check` passes seven focused compiler checks
    plus the native transition suite.
  - Evidence 2026-07-21, managed continuation graphs: NativeIR now evaluates
    non-yielding managed control prefixes exactly once and makes their live
    values explicit continuation parameters before lowering branching bodies.
    The direct execution shard removes managed references from transition
    transport, retains them as precise actor-owned roots, and restores them in
    descriptor order. `make tvm-aot-managed-continuation-check` builds one
    canonical Terlan image and executes entry/resume, both branch arms, nested,
    repeated, tail, non-tail, and repeated non-tail managed suspension paths.
  - Evidence 2026-07-21, thread-neutral continuations: scheduler-visible parked
    state now lives in one closed owned envelope with no lifetime, native
    pointer, scheduler handle, worker connection, or cache parameter. Compile
    gates require the scheduler envelope, direct backend, and backend-held
    managed roots to remain `Send + Sync + 'static`. A full typed mailbox call
    moves both of its live suspensions through another OS thread before exact
    actor-owned resume, and the gate also runs all 30 actor suspension ownership
    tests.
  - Evidence 2026-07-21, closed application admission: the compiler now runs a
    deterministic symbol/ABI closure pass after mandatory CoreIR rewrites and a
    continuation-reference pass after NativeIR lowering. Eleven focused
    admission tests cover unresolved calls, incompatible and ambiguous imports,
    malformed arity, duplicate and dangling continuation identities, supported
    closure admission, and unsupported reachable functions. Static callable
    coverage also proves the 128-expansion ceiling fails before native object
    emission. `make tvm-aot-application-closure-check` owns the combined gate.
  - Evidence 2026-07-21, static callable lowering: application normalization
    now recognizes both dedicated `FunctionCall` CoreIR and ordinary named
    calls resolved to a lexical lambda binding. Non-escaping lambdas snapshot
    free variables into deterministic compiler locals at creation time and
    beta-lower into ordinary sequential CoreIR; backend remote function
    references become qualified application calls. Expansion is limited to 128
    applications and captures to 64 values. Escaping lambdas or bound callable
    values, unresolved dynamic calls, non-variable lambda parameters, and arity
    mismatches fail before native linking. The focused gate passes eight tests,
    including canonical Terlan source through CoreIR, NativeIR, Cranelift object
    emission, native linking, dispatch, and a returned value of `42`.
  - Evidence 2026-07-21, private higher-order specialization: private functions
    with `CoreType::Arrow` parameters now act as bounded AOT templates. Lambda
    arguments retain lexical capture timing because all source arguments are
    evaluated into fresh caller-scope temporaries before helper parameters are
    introduced. Named local function values become non-capturing wrappers and
    then direct native calls. Specialized helpers are removed before candidate
    admission, so no function-value ABI reaches NativeIR. Public higher-order
    exports remain a stable hard error until runtime-owned closure environments
    exist. The focused gate passes private lambda, named function, public export,
    and recursive expansion cases, then compiles canonical Terlan source through
    both specialization passes, emits and links a Cranelift object, dispatches
    the specialized export, and observes `42`.
  - Evidence 2026-07-21, scalar case lowering: application normalization now
    rewrites scalar `Case` into existing `Let` and ordered `If` control before
    higher-order specialization and static callable normalization.
    Compiler-generated locals preserve one-time scrutinee evaluation; pattern
    captures are visible to guards and bodies; false guards continue to later
    clauses; and an unmatched case retains the native no-matching-branch
    status. The focused gate passes seven tests for
    nested elimination, aliases, boolean and `Unit` literals, malformed and
    bounded shapes, fail-closed pattern admission, composition with private
    higher-order specialization, and canonical Terlan source through linked
    native object execution returning `42`. Shared native-object test support
    also replaces
    the duplicate harness previously held by static-callable tests.
  - Evidence 2026-07-21, scalar and managed records: finite Float equality
    and Float case patterns now lower to ordered Cranelift comparisons, with
    linked execution proving numeric `+0.0 == -0.0` semantics and pre-link
    rejection of non-finite patterns. Public readers over actor-owned fixed
    aggregates now retain ordinary `FieldAccess` and explicit `RecordAccess`
    as bounded `TVMO` operations instead of requiring constructor-local scalar
    replacement. Named record literals evaluate fields in source order before
    canonical physical reordering, while persistent updates evaluate the base
    and changed fields once, copy unchanged fields through checked projections,
    and allocate a new aggregate. Compilation resolves exact semantic identity,
    field slots, and native kinds; duplicate, missing, mistyped, or ambiguous
    layouts and malformed operation requests fail closed. Process-transition
    recognition was separated from expression lowering, and every recursive
    call/yield scan now descends through managed construction, update, and field
    access rather than admitting a hidden suspension. `make tvm-aot-case-lowering-check`,
    `make tvm-aot-managed-field-projection-check`,
    `make tvm-aot-lowering-coverage-check`,
    `make tvm-aot-application-closure-check`,
    `make tvm-aot-runtime-transition-check`, and
    `make tvm-aot-managed-continuation-check` pass, as do the all-bin
    `-D warnings` and Rust size-quality gates. List/Map values, structured
    patterns, checked casts, and owned dynamic closures remain open, so AOT-3
    stays unchecked.
  - Evidence 2026-07-21, owned closure representation: actor-local managed
    heaps now allocate one immutable pointer-free closure object containing the
    sealed descriptor digest, a nonzero image-local callable identity, exact
    parameter/result boundary types, and a bounded typed capture environment.
    Managed capture slots contribute exact reference offsets to the collector;
    focused execution proves that both the closure and a captured managed
    object relocate through collection while a `ManagedContinuation` owns the
    root. The execution shard owns the only admitted backend generation:
    active calls exclude replacement through exclusive shard access, parked
    closure roots keep the runtime non-idle, and shutdown or replacement must
    drain those continuations before unloading code. Invocation validation
    rejects stale descriptor digests and signature drift, while construction
    rejects zero identities, untraced JSON captures, invalid scalars, and
    unbounded shapes. Closure metadata is `Send + Sync + 'static`, and a source
    contract excludes native pointers, stack/thread identities, and worker
    connections. `make tvm-aot-owned-closure-representation-check`,
    `make tvm-managed-memory-check`,
    `make tvm-aot-managed-continuation-check`,
    `make tvm-aot-thread-neutral-continuation-check`,
    `make runtime-aot-only-check`, `make rust-warnings-check`, and
    `make rust-quality-check` pass, as do formatting and diff checks. Closure
    conversion and admitted image-local indirect dispatch remain open, so
    AOT-3 stays unchecked.
  - Evidence 2026-07-21, admitted closure dispatch foundation: executable
    descriptor format 1.5 adds a canonical optional callable table containing
    sorted nonzero image-local target identities, exact caller parameter/result
    types, and exact capture shapes. Artifact generation inventories every
    admitted native function, including private functions that remain absent
    from the public export table; a deterministic two-function `.tvm` fixture
    proves that separation. Descriptor admission rejects ordering errors,
    continuation collisions, public-export signature drift, undeclared native
    resources, untraced JSON state, and result-shape violations. The direct
    backend now installs the sealed descriptor digest and callable table into
    the managed execution runtime and preserves them across empty shard forks.
    Managed closure allocation resolves its target through that authenticated
    table before heap publication, while invocation preparation rejects foreign
    generations, unknown targets, wrong signatures, capture-shape drift, and
    malformed managed/scalar words before producing capture-then-argument ABI
    order. NativeIR now represents generated closure construction explicitly;
    lifted functions declare their leading capture parameters separately from
    caller arguments, Cranelift embeds the bounded callable-allocation record,
    and malformed public/lifted shapes fail before object declaration. An
    end-to-end generated-object test loads the native image, allocates an owned
    closure through the real actor-heap callback, validates its callable and
    captured word, prepares the indirect invocation, and dispatches the lifted
    target to produce the expected result. A descriptor regression proves that
    the private lifted target remains absent from public exports while its
    callable row preserves capture and caller signatures separately. The first
    source-level escape now also closes: a private function may return a
    zero-capture named function value whose declared arrow type exactly matches
    an admitted closed-world native target. Compiler normalization preserves
    that escape, NativeIR emits its owned closure allocation, and a loaded
    source-generated object validates and invokes the resulting closure. Arrow
    type grouping is retained in typed CoreIR, the lowering coverage matrix is
    version 2, and target/signature drift fails before code generation. Public
    function-valued results remain rejected until their external boundary is
    defined.
    `make tvm-aot-closure-dispatch-check`,
    `make tvm-native-image-format-check`, `make tvm-managed-memory-check`,
    `make runtime-aot-only-check`, `make rust-warnings-check`, and
    `make rust-quality-check` pass, as do formatting and diff checks. The
    compiler still must closure-convert captured lambdas, nested/local named
    values, and public higher-order boundaries and lower managed and suspending
    indirect calls through this table; therefore the closure-conversion
    requirement and AOT-3 remain unchecked.
  - Evidence 2026-07-21, captured whole-result lambdas: a private function may
    now return a lambda that closes over its typed outer parameters. The
    compiler computes a bounded free-variable set, orders captures
    deterministically by name, snapshots their current actor-owned words at
    closure creation, and lambda-lifts the body into a private native function
    whose ABI places captures before caller arguments. Lifted functions are
    appended after ordinary application functions so existing direct-call
    indices remain stable; their stable callable identities enter the admitted
    descriptor table and never embed code pointers. A source-generated object
    captures `40`, validates and prepares the owned closure through the real
    actor heap, dispatches the lifted Cranelift target with argument `2`, and
    returns `42`. Adversarial checks reject arity drift, non-variable lambda
    parameters, untyped non-parameter captures, and suspending bodies with
    stable diagnostics. The lowering coverage contract is version 3 and marks
    escaping `Lam` as native-lowered. Fresh unsuppressed
    `make tvm-aot-closure-dispatch-check`,
    `make tvm-aot-static-callable-check`,
    `make tvm-aot-lowering-coverage-check`,
    `make tvm-aot-application-closure-check`,
    `make tvm-native-image-format-check`, `make tvm-managed-memory-check`,
    `make runtime-aot-only-check`, `make rust-warnings-check`, and
    `make rust-quality-check` pass, as do formatting and diff checks. Nested
    and let-local escaping closures, local named values, public higher-order
    boundaries, managed-signature execution proof, and suspending indirect
    calls remain open, so the closure-conversion requirement and AOT-3 stay
    unchecked.
  - Evidence 2026-07-22, lexical-local escaping closures: whole-result closure
    conversion now descends through ordered scalar `Let` prefixes, assigns each
    checked local an exact NativeIR slot and type, evaluates the prefix once,
    and snapshots referenced locals only after their values exist. Terminal
    let-bound lambdas and qualified named-function aliases survive static
    normalization for owned conversion, while nonterminal callable escape
    remains a stable rejection until general closure-valued local lowering is
    implemented. A source fixture computes local `offset = seed + 1`, binds and
    returns a lambda, enters the generated object with `seed = 39`, observes
    the actor-owned capture `40`, invokes it with `2`, and returns `42` through
    admitted Cranelift dispatch. The 128-expansion adversary remains green at
    the default test stack after expansion credit moved ahead of recursive
    lowering. Closure conversion now guards its arrow boundary before examining
    lexical prefixes, so unrelated suspending `Let` functions retain their
    existing continuation lowering. The focused closure, static-callable,
    application-admission, lowering-coverage, native-image-format,
    managed-memory, AOT-only, Rust warning, Rust size-quality, formatting, and
    diff gates pass on fresh unsuppressed invocations after the concurrent HTTP
    invocation seam was completed. Nonterminal closure-valued locals, public
    higher-order boundaries, managed-signature execution proof, and suspending
    indirect calls remain open, so the closure-conversion requirement and
    AOT-3 stay unchecked.
  - Evidence 2026-07-22, branch-selected escaping closures: closure-valued
    `If` results now lower to native branch control whose ordered arms allocate
    captured lambdas or admitted named function values. One source function
    may emit multiple private lifted targets with deterministic collision-free
    ordinals; each target retains its exact capture and caller signature in the
    admitted callable table. Conversion is bounded to 64 clauses per branch,
    64 nested `Let`/`If` layers, and 64 lifted targets per owner. Empty or
    non-callable arms, suspending conditions, over-budget branches, target
    collisions, and ABI drift fail before code generation. A real Terlan
    fixture selects between two lambdas that capture `40`, loads and links the
    emitted Cranelift object, observes distinct admitted callable identities,
    validates each actor-owned closure through the managed dispatch table, and
    executes both selected targets to return `42` and `38`. Fresh unsuppressed
    `make tvm-aot-closure-dispatch-check`,
    `make tvm-aot-static-callable-check`,
    `make tvm-aot-application-closure-check`,
    `make tvm-aot-lowering-coverage-check`,
    `make tvm-native-image-format-check`, `make tvm-managed-memory-check`,
    `make runtime-aot-only-check`, `make rust-warnings-check`, and
    `make rust-quality-check` pass, as do formatting and diff checks.
    Nonterminal closure storage and invocation, public higher-order ABI,
    managed-signature execution proof, and suspending indirect calls remain
    open, so the closure-conversion requirement and AOT-3 stay unchecked.
  - Evidence 2026-07-21, typed public mailbox boundary: CoreIR no longer names
    the generic `send_value` or `receive_value` bootstrap. Explicitly
    specialized `Process.send[T]` and `Process.receive[T]` carry the canonical
    payload type into `SendTyped` and `ReceiveTyped`; `Process[T]` uses the
    VM-owned identity slot, while `Message[T]` uses the payload's native
    representation and `wrap`/`unwrap` erase before NativeIR. `Process.send`
    now returns `Unit`, and receive is current-actor-only rather than reading an
    arbitrary immutable process descriptor. The focused gate checks CoreIR,
    mandatory specialization, positive and negative `ActorMessage` evidence,
    native type mapping, transition shape, Cranelift object linking, and exact
    managed receive metadata.

  - Closeout evidence 2026-07-22: the application normalizer now preserves
    exact declared generic parameters and performs bounded monomorphization
    without confusing concrete nominal types with variables. Escaping captured
    closures, public higher-order entry/results, managed and suspending indirect
    dispatch, List/Map and tuple values, structured and binary patterns,
    comprehensions, checked casts, record and constructor rewrites, native
    `Try` cleanup, recursion, mutable receiver rewrites, and declared
    asynchronous capabilities all enter NativeIR or fail before linking with a
    stable diagnostic. One loaded Cranelift image executes recursion, an
    imported call, a generic specialization, captured closure creation and
    indirect invocation, tuple matching, caught failure cleanup, and
    continuation park/resume. The gate also runs the focused bounded and
    unsupported rejection suites. Fresh unsuppressed invocations of every
    named AOT-3 gate, including
    `make tvm-aot-application-conformance-check`, plus
    `make rust-warnings-check`, `cargo fmt --all -- --check`, and
    `git diff --check` pass. Rust quality reports zero oversized files and zero
    dormant VM modules.
  - Audit evidence 2026-07-22: mandatory static-callable normalization and
    capture analysis now traverse every executable CoreIR expression and
    lexical pattern family, including managed aggregates, comprehensions,
    structured branches, `Try`, casts, mutable receivers, and nested lambdas.
    Iterative preflight rejects immediate-call expansion bombs inside nested
    values before recursive rewriting. Focused rejection coverage now proves
    generic, higher-order, comprehension, structured-pattern, static-capture,
    escaping-capture, and cast budgets or incompatibilities fail before linking.
    The application conformance gate executes its free-variable regressions
    through an exact non-empty selector, and the thread-neutral gate likewise
    executes its intended `Send + Sync + 'static` assertion rather than a stale
    zero-test path. Every named AOT-3 gate passes after these corrections.

- [x] AOT-4: move ordinary actor execution onto same-shard native fast paths.
  - [x] Attach native entry/resume calls directly to the execution shard for
    local spawn, send, receive, yield, reductions, timers, links, monitors,
    cancellation, failure, scheduling, and resource operations. CLI execution,
    test execution, REPL generations, and HTTP handler images now enter through
    `PureNativeExecutionShard`; the boundary-only scalar/Yield loop has been
    deleted. Gate: `make tvm-aot-runtime-transition-check`.
  - [x] Keep the worker protocol only for explicit crash-isolation and external
    or cross-boundary execution profiles; it must not be the ordinary local
    actor dispatch path. `NativeBoundaryExecutionProfile` is shared by VM
    policy and worker admission and has exactly `external-adapter`,
    `crash-isolated`, and `cross-boundary` forms; no local form exists. Worker
    startup requires `--execution-profile`, rejects image arguments and
    `local`, and ordinary CLI, test, REPL, and HTTP paths are source-audited for
    worker references. Gate: `make tvm-aot-capability-worker-check`.
  - [x] Keep the supervisor-to-shard protocol coarse: admission, lifecycle,
    inspection, cross-shard routing, and recovery only. Local calls and
    transitions remain inside the shard; only external, crash-isolated, or
    cross-boundary adapter operations use asynchronous capability RPC to a
    native worker. `VmShardControlCommand` is a closed validated envelope with
    canonical shard/request identities, sealed-image metadata, bounded
    cross-shard payloads, and no actor-operation or capability-worker variant.
    Gate: `make tvm-aot-shard-ownership-check`.
  - [x] Define the supervisor/shard state machine explicitly: protocol
    negotiation, sealed-image admission, epoch assignment, ready
    acknowledgement, health and progress signals, drain, graceful stop, forced
    termination, crash report, restart budget, exponential backoff, and
    terminal quarantine. A shard is routable only after admission and readiness
    complete atomically.
    `VmExecutionShardSupervisor` now owns the closed lifecycle, canonical sealed
    image and epoch identities, exact-epoch monotonic signals, explicit terminal
    outcomes, and the shared VM exponential restart schedule. Gate:
    `make tvm-aot-supervisor-lifecycle-check`.
  - [x] Reject stale-epoch actor routes, mailbox publications, continuation resumes,
    resource notifications, and capability completions after restart. Crash
    recovery must not duplicate a send, timer, HTTP response, database write, or
    other external effect; replayability is an explicit per-operation policy.
    `VmShardEpochFence` now uses the canonical typed epoch for every protected
    operation class and retains an operation ledger across generations.
    Committed duplicates and uncertain at-most-once effects are suppressed;
    replayable and idempotent retries are admitted only under their explicit
    policy. The supervisor advances the fence with image admission and admits
    operations only while ready or draining. Gate:
    `make tvm-aot-stale-epoch-check`.
  - [x] Make capability RPC asynchronous and scheduler-safe. Require request and
    capability identity, bounded payloads and queues, deadlines, cancellation,
    backpressure, single-shot completion, late-completion rejection, worker
    crash attribution, and restart isolation. Waiting for a worker must park an
    actor rather than block the execution shard. Capability protocol v2 carries
    explicit capability identity. `VmCapabilityRequestContext` retains the
    epoch-fenced completion, and every result identifies the exact logical
    worker generation. Request and response channels are bounded; worker I/O
    remains on owned threads while VM deadlines park and wake actors. Timeout,
    cancellation, late reply, transport failure, capability mismatch, queue
    saturation, and generation reuse are covered by the gate:
    `make tvm-aot-capability-worker-check`.
  - [x] Launch capability workers with least authority: an explicit capability
    allowlist, scrubbed environment, controlled working directory, closed
    inherited descriptors, bounded memory/CPU/process creation, and the
    strongest practical platform sandbox. VM admission now rejects undeclared
    capabilities before request allocation or actor parking. The Linux launcher
    closes inherited descriptors before entering the existing bubblewrap and
    prlimit profile, while the worker independently attests its exact
    environment, working directory, resource limits, and descriptor set. A real
    sandboxed-process test deliberately inherits descriptor 9 and proves it is
    unavailable inside the worker. Gate: `make tvm-aot-capability-worker-check`.
  - [x] Pass explicit actor and execution context through same-shard runtime
    calls. `PureNativeExecutionShard` now owns the mutable
    `ManagedExecutionRuntime`; immutable admitted code alone is shared by
    backend forks. Entry, transition service, mailbox graph transfer,
    continuation resume, result decoding, and owner cleanup all receive a
    `PureNativeExecutionContext` binding the exact actor to an exclusive shard
    runtime borrow. The production direct path contains no managed-runtime
    mutex, process-global evaluator or image registry, or thread-local runtime
    state. Tests prove foreign actor contexts cannot resume a parked
    continuation, parked state remains thread-neutral, and a compiled AOT image
    completes through the same direct path. Gate:
    `make tvm-aot-multicore-readiness-check`.
  - [x] Preserve scheduler leases, mailbox accounting, ownership checks,
    failure propagation, cleanup, and continuation authority across the direct
    path. Shard resume now validates the suspension owner before recording a
    dispatch or mutating actor state. Every exact-owner resume failure enters
    the unified actor exit pipeline and then releases owner-local backend state;
    already-exited failure and cancellation transitions retain their original
    reason while still releasing the managed heap. Graceful backend shutdown
    continues to reject parked work, but abnormal actor teardown discards only
    that actor's pending continuation. A direct-path lifecycle fixture retains
    a scheduler lease, accounted self-message, managed allocation, link, and
    monitor before forcing a resume failure, then proves lease removal, mailbox
    and heap accounting release, linked failure propagation, and monitor
    delivery. A separate foreign-owner fixture proves rejected authority cannot
    consume either actor's lease or record a false dispatch. Gate:
    `make tvm-aot-runtime-transition-check`.
  - [x] Make admitted native code reentrant: image descriptors and code are
    immutable and shareable, while all mutable execution state is reached
    through explicit actor/execution context. `PureNativeExecutionImage` now
    owns only the admitted boundary and empty managed-layout template and can
    create independently mutable shards over shared code. `DirectNativeBackend`
    retains only `Arc<LoadedDirectImage>`; owner-indexed pending continuations,
    managed captures, heaps, and mailbox roots moved into
    `PureNativeExecutionRuntime`. The HTTP cache stores the immutable image
    factory and forks an empty shard per request, removing its image-wide
    execution mutex while retaining the short administrative cache lock.
    Request identity allocation also moved off the boundary and into each
    execution runtime. Tests interleave two actors through send, receive, yield,
    and completion on one shard, independently resume two owner-scoped
    continuation records, reset request identity in an empty fork, and run
    empty shard forks concurrently on separate threads. Compile-time checks
    prove image, execution state, and direct code are thread-neutral. Source
    gates forbid backend continuation fields, runtime thread-local state, and
    HTTP shard locks. Gate: `make tvm-aot-multicore-readiness-check`.
  - [x] Define single-consumer continuation resume authority and mailbox
    publication memory ordering so concurrent attempts cannot double-resume,
    lose a wakeup, or expose a partially copied managed graph. Continuation
    validation now removes the exact owner/request/entry record into a linear
    `NativeContinuationClaim`, and restoration consumes that claim. Accepted
    sends create an opaque `VmMailboxPublication` only after accounting,
    receiver graph copy, precise-root registration, and queue insertion;
    scheduler wake consumes that proof. Same-shard ordering follows the
    exclusive mutable borrow, while the receipt documents the release/acquire
    boundary required by a future cross-thread queue. Bounded collection tests
    prove budget exhaustion is atomic and cannot pause or mutate a sibling
    actor heap. The gate rejects direct-path global locks and runs stale
    double-resume, mailbox publication/wakeup, and actor-local GC isolation
    fixtures. Gate: `make tvm-aot-multicore-readiness-check`.
  - [x] Keep timers, resources, links, monitors, image generations, and other
    VM services shardable. `VmActorRuntime` now stores a plain shard-local
    `VmCodeServer` beside its by-value process, scheduler, timer, resource,
    failure, dynamic-module, and database services. Actor image switches use
    that local registry, and unified process exit releases every generation
    binding. The mutex-backed concurrent code registry is restricted to
    administrative publication and inspection; its process bind, switch, and
    release methods were removed. A stress fixture runs 160 independent code
    servers concurrently through 25 generations. A second full service fixture
    intentionally overlaps process, timer, resource, monitor, and generation
    IDs across two shards, mutates and exits one owner, and proves the sibling's
    timers, resource, monitor, and active image remain unchanged. Source gates
    reject locks in ordinary actor service modules and actor-transition methods
    on the administrative registry. Gate:
    `make tvm-aot-shard-ownership-check`.
  - [x] Benchmark allocation, messaging, scheduling, collection pauses, actor
    churn, and tail latency against the recorded reference workloads. The
    checked `vm-aot-runtime-workloads.v1` manifest fixes six workload names,
    order, sample counts, operation counts, and scopes. The release-mode runner
    executes canonical actor heaps, precise moving collection, actor mailboxes,
    scheduler yields, and unified actor exits; every timed path validates its
    semantic result before publication. Reports include batch throughput and
    p50/p95/p99/max timing, with collection setup excluded from the measured
    pause and the mixed-tail lane combining collection, messaging, scheduling,
    and teardown. The benchmark embedding reuses the runtime's exact managed
    identity, reference, error, heap, layout, root, and mailbox modules rather
    than defining benchmark-only VM types. Gate:
    `make tvm-aot-runtime-workload-benchmark-check`.
  - [x] Inject crashes before and after admission, readiness, mailbox
    publication, continuation parking, capability submission/completion, drain,
    and image replacement. A test-only 16-boundary matrix drives the real shard
    supervisor and epoch-operation ledger without production failpoint
    branches. Recovery clears failed images, advances the epoch, rejects stale
    readiness and operations, suppresses uncertain or committed at-most-once
    effects, and permits only never-started work to execute. Restart exhaustion
    enters immutable quarantine after the configured budget, while crash reports
    identify the owning shard, failed epoch, reason, and observation tick.
    Focused production-path fixtures retain mailbox publication ordering,
    continuation cleanup, live capability completion, and cancellation winning
    over a late reply. Gate: `make tvm-aot-crash-injection-check`.
  - [x] Route active native-image admission, readiness, crash recovery, and
    replacement through `VmExecutionShardSupervisor`. Every
    `PureNativeExecutionShard` now negotiates, admits its sealed descriptor,
    acknowledges readiness, rejects calls outside `Ready`, drains before
    graceful shutdown or replacement, and preserves one monotonic epoch fence
    across replacement and bounded crash recovery. Changed compiled REPL
    generations replace the existing shard image and are verified at epochs
    1 through 4 by a real `.tvm` execution fixture. The supervisor's dormant
    inventory row was removed, and the lifecycle gate now rejects test-only
    ownership or a recreated dormant row. Gate:
    `make tvm-aot-supervisor-lifecycle-check`.
  - [x] Resolve the remaining dormant AOT runtime modules before AOT-4
    closeout. `capability_worker.rs` is owned by the actor capability-call
    lifecycle, while source reload owns `code_server_compiler.rs` staging and
    publication. The obsolete `external_native.rs` helper protocol and unused
    Latin-1 module were deleted; stable value hashing is owned by the canonical
    value module. Abnormal actor exits now retain bounded fatal diagnostics,
    HTTP sessions validate their live-template protocol manifest, and
    persistent actor lifecycle completion emits bounded aggregate metrics.
    Reviewed dormant inventory rows were removed instead of retained as
    exceptions. Gate: `make rust-quality-check`; acceptance requires
    `cargo run -p terlan --bin terlan-quality --quiet -- dormant-runtime-code`
    to report zero dormant modules.
    Evidence 2026-07-21: `make rust-quality-check` passes with zero dormant VM
    modules across zero inventory rows, zero oversized Rust files, and the
    deterministic-HashMap inventory intact.
  - Gates: `make tvm-aot-runtime-transition-check`,
    `make tvm-aot-shard-ownership-check`,
    `make tvm-aot-supervisor-lifecycle-check`,
    `make tvm-aot-stale-epoch-check`,
    `make tvm-aot-crash-injection-check`,
    `make tvm-aot-capability-worker-check`,
    `make tvm-aot-multicore-readiness-check`,
    `make tvm-aot-runtime-workload-benchmark-check`,
    `make runtime-aot-only-check`, and
    `make rust-quality-check`.
  - Exit: a process trace proves ordinary same-shard actor work does not
    serialize through TVM transport or supervisor IPC.
  - Evidence 2026-07-21, ordinary same-shard execution: one shard now owns the
    admitted direct backend, actor runtime, entry/resume lifecycle, completed
    call accounting, and a closed direct-dispatch trace. A full-cycle fixture
    enters generated code, services self-send, typed receive, and yield, resumes
    three exact continuations, returns `true`, exits the actor normally, and
    leaves no parked continuation. Rejected entry also exits and releases its
    allocated actor without recording false completion. A compiled `.tvm`
    fixture now runs entry, repeated yield, self-send, selective receive,
    spawn, timer, resource, and scheduler reclassification exports through
    `terlan-vm` with `TERLAN_NATIVE_WORKER` removed. The trace admits only
    `Entry`, `Resume`, and `Complete`; no worker/IPC event exists. Existing
    transition gates retain link, monitor, cancellation, failure,
    malformed-frame, and ownership coverage. Legacy native-image worker RPC
    tests are not part of this local-runtime gate because the worker is now an
    external capability boundary rather than an image execution host.
  - Evidence 2026-07-21, explicit worker profiles: VM policy construction now
    requires the shared typed profile and forwards its stable name into the
    cleared, sandboxed child command. Worker admission rejects a missing,
    repeated, unknown, or `local` profile before reading protocol frames. Tests
    admit all three worker-only profiles, reject implicit local execution, and
    retain the real child-process request/response lifecycle. The gate also
    rejects image-loader/runtime ownership in the worker and worker references
    in all ordinary application execution consumers.
  - Evidence 2026-07-21, coarse supervisor protocol: the shared VM model now
    admits exactly five command classes: admission, lifecycle, inspection,
    cross-shard routing, and recovery. Typed constructors reject zero request
    identities, empty shard/image identities, empty or oversized route
    envelopes, same-shard routes, all-zero image digests, and zero recovery
    epochs. The ownership gate executes the complete protocol matrix and fails
    if the command module names local entry/resume, mailbox, timer,
    relationship, scheduling, resource, or capability-worker operations.
  - Closeout evidence 2026-07-22: all ten named AOT-4 gates pass on fresh,
    unsuppressed invocations. The multicore source audit additionally found and
    removed an HTTP-session synchronization container from execution-shard
    state: shards and generated-code operations now carry an explicit
    `VmHttpSessionService` capability, with synchronization encapsulated by the
    VM service rather than exposed to actor execution state. Post-correction
    reruns of the transition, ownership, supervisor, stale-epoch,
    crash-injection, capability-worker, multicore-readiness, and runtime
    workload gates pass. `make runtime-aot-only-check`,
    `make rust-quality-check`, `make rust-warnings-check`,
    `cargo fmt --all -- --check`, and `git diff --check` also pass; Rust quality
    reports zero oversized files and zero dormant VM modules.
  - Audit evidence 2026-07-22: REPL image replacement is asserted at the active
    execution-shard boundary (`active.shard.replace_image`) rather than the
    superseded boundary-only owner. The lifecycle gate therefore enforces the
    architecture it is intended to protect: image admission, drain,
    replacement, epoch publication, and recovery remain shard-supervised.
    All ten named AOT-4 gates pass after the assertion correction, including
    the release-mode six-workload benchmark matrix.

- [x] AOT-5: migrate the complete HTTP runtime surface.
  - [x] AOT-5A: establish the first native managed HTTP request/response cycle.
    Compile immutable UTF-8 literals into actor-owned managed values, admit a
    canonical Request tuple with nested managed string maps, and lower text,
    HTML, serialized JSON, redirect, and file response builders into one fixed
    managed Response layout. A source handler now compiles to `.tvm`, accepts
    the managed request, returns the managed response, and serves the expected
    status and body without resident CoreIR. Gate:
    `make tvm-aot-http-managed-cycle-check`.
  - [x] AOT-5B: lower Request accessors and Response mutations against the
    managed layouts, including route/query/header/cookie/body reads and
    status/header/cookie/security-header updates.
    - [x] AOT-5B1: lower method, path, raw query, body, route-parameter,
      decoded-query, header, and cookie reads through a bounded managed
      operation ABI. Aggregate projection validates the canonical Request
      identity; string-map lookup preserves flat and indexed map semantics and
      returns admitted `Option[String]` variants. A compiled source handler now
      projects request data and serves it from generated native code. Gate:
      `make tvm-aot-http-request-accessor-check`.
    - [x] AOT-5B2: add persistent repeated-header storage to the managed
      Response layout and lower status, header, cookie, cookie-jar, and typed
      security-header updates without mutable host handles.
      - [x] AOT-5B2a: extend the canonical managed Response with a persistent
        `List[Header]`, lower immutable status replacement and repeated header
        append through reusable managed operations, treat raw `Set-Cookie` as
        an ordinary repeated header, and preserve duplicates through public
        materialization and HTTP/1 serialization. The full source-handler cycle
        returns a dynamic request projection with updated status and two cookie
        headers. Gate: `make tvm-aot-http-response-mutation-check`.
      - [x] AOT-5B2b: lower maintained-crate cookie serialization, managed
        cookie-jar mutation/replay, and arbitrary typed `SecurityHeaders`
        policies onto the persistent response metadata contract.
        Cookie values are serialized by the maintained Rust cookie adapter,
        request jars retain immutable incoming-cookie maps plus persistent
        mutation lists, and replay appends each validated `Set-Cookie` value
        without collapsing duplicates. Typed security policies lower their
        closed public atom unions to a private fixed discriminant ABI and emit
        frame, referrer, content-sniffing, and optional HSTS headers. The
        response bridge reserves transport framing only, so admitted policy
        headers survive the full compiled-source-to-HTTP/1 cycle. Gate:
        `make tvm-aot-http-typed-metadata-check`.
  - [x] AOT-5C: lower Router, route handler, middleware, fallback, and error
    callables without reintroducing evaluator-owned closures. Checked CoreIR
    `router/0` builders are reduced to immutable method/path plans containing
    qualified native export identities, then removed from the executable
    image. Request middleware, route handlers, reverse response middleware,
    static responses, fallback, and typed error recovery execute through the
    same direct-AOT callable boundary. Managed string patterns and append use
    checked value operations, and `MiddlewareResult` plus `HttpError` cross the
    boundary as compiler-owned managed values. Gate:
    `make tvm-aot-http-router-callable-check`.
  - [x] AOT-5D: move templates, sessions, cookies, body decoding, and typed HTTP
    errors onto the managed native boundary. The compiler/runtime ownership
    matrix is maintained beside `compiler/native_ir/http_values`, and the
    complete inherited evidence chain closes under
    `make tvm-aot-http-managed-boundary-check`.
    - [x] AOT-5D1: lower portable `HttpError` construction and typed
      `code`, `message`, and `status` projections to compiler-owned managed
      values. Scalar aggregate projections use an explicit non-reference ABI,
      and router recovery consumes the original managed error fields through
      generated native code. Gate: `make tvm-aot-http-managed-error-check`.
    - [x] AOT-5D2: lower template, request-body, and session state onto managed
      values.
      - [x] AOT-5D2a: erase checked public `Template.Html` values to the managed
        string representation; lower `trusted`, `empty`, literal `join`, and
        dynamic list `join`; and execute an HTML response through the complete
        direct-AOT HTTP handler path. Gate: `make tvm-aot-http-template-check`.
      - [x] AOT-5D2b: carry checked external template render plans into CoreIR
        and lower `TemplateInstantiate` without runtime source parsing.
        - [x] AOT-5D2b1: retain the exact validator-owned parsed tree in CoreIR;
          fold static markup; lower direct `String` text and scalar-attribute
          slots through context-specific managed escaping; preserve trusted
          `Template.Html`; reject missing, duplicate, and unknown props; and
          execute an external template through the full native HTTP path after
          removing the checked source file. Gate:
          `make tvm-aot-http-template-render-plan-check`.
        - [x] AOT-5D2b2: lower the remaining checked template surface:
          expression islands, nested struct paths, scalar conversion, optional,
          boolean, URL, and token-list attributes, plus component and `children`
          render-plan inlining. Positive fixtures must execute the shared typed
          template matrix through native HTTP, while malformed URLs, token
          lists, component props, paths, and expression result types must fail
          with stable typed diagnostics. Gate:
          `make tvm-aot-http-template-expression-check`.
      - [x] AOT-5D2c: lower `Request.body_json()` into actor-owned managed
        `Result[Json, Error]` values. The request body is projected from the
        managed request, parsed through the maintained Rust JSON adapter, and
        normalized to canonical compact JSON text without exposing host JSON
        handles. Immediate `Ok`/`Err` matches lower to checked managed variant
        and payload operations; malformed JSON remains an ordinary typed
        `Err(Error)` with the finite `json.parse` atom. Gate:
        `make tvm-aot-http-body-json-check`.
      - [x] AOT-5D2d: lower session state and lifecycle operations onto VM-owned
        request context. One shared `VmHttpSessionRuntime` is attached to each
        admitted HTTP image and inherited by independent request shards.
        `Session.current`, `get`, `set`, `delete`, `rotate`, `expire`, and
        `with_response` lower to managed operations over opaque session values;
        state remains in actor-owned VM tables, rotation preserves it, and
        expiration cleans it before an explicit maintained deletion cookie is
        threaded onto the response. Immediate `None`/`Some` reads lower without
        reopening generic runtime interpretation. Gate:
        `make tvm-aot-http-session-check`.
      - [x] Inventory the complete AOT-5D boundary and update this roadmap
        before closing AOT-5D2. The durable ownership matrix is in
        `crates/terlan/src/compiler/native_ir/http_values/README.md`. Existing
        cookie jar construction, mutation, maintained serialization, and
        response replay remain covered by
        `make tvm-aot-http-typed-metadata-check`; the exact aggregate and
        collection inventory plus every inherited surface gate close under
        `make tvm-aot-http-managed-boundary-check`.
  - [x] AOT-5E: move asynchronous handler I/O, WebSocket, and SSE entry/resume
    orchestration onto VM-owned continuations. One evaluator-free ownership
    chain now covers static callback selection, generation admission, generated
    entry, typed parking, exact wake validation, linear channel serialization,
    and cancellation cleanup. The durable boundary inventory is maintained in
    `crates/terlan/src/commands/serve/handler/README.md`; production transport
    pumping, generation quiescence, and fallback deletion remain explicitly
    owned by AOT-5F. Closure gate:
    `make tvm-aot-http-sse-invocation-check`.
    - [x] AOT-5E1: materialize checked SSE and WebSocket router builders as
      canonical VM endpoint plans before native image execution. Ordinary
      handlers and both channel kinds share one AOT route-target inventory and
      one scoped VM router insertion path, so channel routes retain root and
      group middleware without resident router CoreIR or duplicate endpoint
      types. SSE admission and WebSocket upgrade execute from the materialized
      plan through the real `.tvm` serving path. Gate:
      `make tvm-aot-http-channel-plan-check`.
    - [x] AOT-5E2: expose one request-owned native invocation that enters a
      generated handler, parks its exact `PureNativeSuspension`, and resumes it
      only through execution-shard authority after a typed VM I/O wakeup. The
      linear invocation owns its actor, independently mutable shard, and one
      pointer-free suspension until completion. Its typed wait authority binds
      the admitted shard, actor, native request, generated continuation, and
      `TvmBoundaryType`; stale cross-request and cross-shard wakes therefore
      fail before continuation or heap mutation. Real Terlan source now lowers
      a typed receive nested in a managed response constructor, enters through
      the request invocation, parks, resumes with an owned String wake, returns
      a decoded HTTP response, and rejects foreign identity and wrong-value
      wakes. Production `execute_immediate_native` now enters the same linear driver
      instead of the old unconditional local-resume loop. The driver resumes
      shard-local transitions internally, exposes only typed `Receive` waits to
      an adapter, and fails and drains the request with a stable loud diagnostic
      when the current synchronous serve adapter cannot provide asynchronous
      I/O. The exact native-invocation gate and unsuppressed Rust warning gate
      pass without dead-code suppression. Gate:
      `make tvm-aot-http-native-invocation-check`.
    - [x] AOT-5E3: route WebSocket open, inbound text frame, writable, close,
      and cancellation callbacks through the shared native invocation
      contract. Static source callbacks are retained in the canonical endpoint
      plan without closures. One connection admits callback work linearly,
      parks typed waits under its exact generated invocation, rejects parallel
      event entry, resumes only from the matching wake authority, and cancels
      parked work before entering the cancellation callback. Ping, pong, and
      close control frames remain VM-owned protocol operations; binary data is
      still rejected by the endpoint policy. Gate:
      `make tvm-aot-http-websocket-invocation-check`.
    - [x] AOT-5E4: route SSE open, event-ready, keep-alive, drain, and
      cancellation callbacks through the shared native invocation contract.
      Static source callbacks are retained in the canonical endpoint plan and
      admitted with the exact native image generation. One stream executes or
      parks callback work linearly, rejects concurrent event entry, resumes an
      event-ready callback only from its typed wake authority, and releases
      parked work before cancellation. The VM continues to own bounded queue
      state, wire encoding, keep-alive policy, drain, and the live stream lease.
      Gate: `make tvm-aot-http-sse-invocation-check`.
    - [x] Inventory the complete AOT-5E entry/resume boundary and update this
      roadmap before closing AOT-5E. The inventory records every owner and
      retained value from static router metadata through native generation,
      request shard, actor, suspension, typed wait/wake authority, channel
      event, protocol session, and cancellation. It confirms that only typed
      `Receive` waits cross the adapter boundary and that retained invocation
      state contains no CoreIR, evaluator, host future, native stack pointer,
      or untyped callback handle. The inherited closure evidence remains
      `make tvm-aot-http-sse-invocation-check`.
  - [x] AOT-5F: close generation lifetime, backpressure, cancellation,
    cleanup, benchmark, and fallback-deletion evidence before checking AOT-5.
    - [x] AOT-5F1: prove source cache replacement preserves immutable in-flight
      native generations and unloads each retired generation only after its
      final request or channel lease drops. A full-cycle fixture compiles two
      distinguishable generations at one source path, replaces the cache entry,
      executes both admitted images after replacement, and uses weak ownership
      evidence to reject undisclosed generation retainers at quiescence. Gate:
      `make tvm-aot-http-generation-lifetime-check`.
    - [x] AOT-5F2: connect WebSocket and SSE protocol sessions to production
      transport event pumps with bounded queue-pressure propagation, typed
      wake delivery, disconnect cancellation, and graceful drain. Admitted
      channel sessions now survive the finite HTTP routing exchange and move
      into socket-owning production pumps. WebSocket framing and close policy
      use maintained tungstenite code; SSE uses the VM HTTP writer's chunked
      head, event, heartbeat, and terminal framing. A compiled two-router
      package proves queue overflow rejection, a second text/event waking the
      exact parked native callback, graceful WebSocket close, queued SSE drain,
      and disconnect cancellation. Gate:
      `make tvm-aot-http-channel-transport-check`.
    - [x] AOT-5F3: prove request, channel, actor, shard, protocol buffer,
      session, timer, resource, and generation cleanup under completion,
      rejection, cancellation, shutdown, reload, and late completion. Terminal
      WebSocket close/cancellation and SSE drain/cancellation callbacks may no
      longer retain a parked native invocation: an attempted terminal wait is
      cancelled under its exact actor/shard owner and rejected before the live
      session lease closes. The cleanup gate composes the production channel
      and generation-lifetime fixtures with existing owner-exit, accounted
      protocol-buffer, session-expiry, HTTP shutdown, stale request completion,
      timer/resource, and late native completion proofs. Gate:
      `make tvm-aot-http-cleanup-check`.
    - [x] AOT-5F4: record comparable checked-CoreIR and native-AOT HTTP
      throughput, p50/p95/p99 latency, allocation, pressure, longevity, and
      overlapping-generation evidence under one hardware fingerprint. Gate:
      `make tvm-aot-http-performance-check`.
      - [x] Add one shared executable source/package workload and typed report
        schema for both lanes. The report records sequential and concurrent
        request latency, wall-clock throughput, server resident memory,
        pressure completion, sustained-server longevity, generation
        replacement latency, compiler digest, and a hashed OS/architecture/CPU/
        logical-core/Rust fingerprint. Comparison rejects mixed machines,
        different workload dimensions, unordered tails, failed pressure or
        longevity requests, missing memory evidence, and incomplete generation
        evidence. Contract gate:
        `cargo run --locked --release -p terlan --bin terlan-benchmark --quiet -- http-aot-performance-self-test`.
      - [x] Record the final checked-CoreIR lane while the preserved pre-AOT
        compiler binary remains available. Run
        `make tvm-aot-http-checked-coreir-reference-record TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN=<path>`;
        retain the JSON report, its compiler digest, and hardware fingerprint,
        but do not retain the compiler binary in the repository.
      - [x] Build current release `terlc`, record the native-AOT lane with the
        exact checked-CoreIR workload, and publish the strict comparison report
        through `make tvm-aot-http-performance-check`. Do not check AOT-5F4
        until both executable reports and the comparison are complete.
      - Evidence 2026-07-22: the final paired capture uses hardware fingerprint
        `a8d8a85f3c21cee2643f1187e7d2e84a763f7c042dc6c18a0c3657e7815df43a`
        and workload 500 sequential requests, 8 workers x 100 pressure
        requests, 1,000 longevity requests, and 512-byte bodies. The preserved
        checked-CoreIR compiler digest is
        `8bdb03360de45ca199827d9a3761fa2899980787c8f3f22b413f9c32c6deb649`;
        its binary remains outside the repository. The current native compiler
        digest is
        `d760f30d53a7f1bb32697c04ad0ce323f19cc7468ee01927ccbd6f81c7800155`.
        Checked, native, comparison, and policy SHA-256 values are respectively
        `9322e45d5131ff1772a6efb03ac23f740933d2ac4b80d14c5b8ed6a788ccf113`,
        `95a68506639ee478a7cdf12e606e57e63b154fe0668af0cead2e049b1c66d87f`,
        `2dd894c70dc3a9e19d76da810a49cbbd60207c42321d64dbc8c653faadf1e5a4`,
        and `c0c9bb87ae3252dcd20962d994f02cc8998cc05d556b8c93b02c216f7d7db8d1`.
        Native deltas are sequential p50/p95/p99 +25.91/+23.60/+21.72%,
        pressure +28.11/+21.80/+12.70%, longevity -10.58/+18.12/+9.23%,
        sequential/pressure/longevity throughput -21.82/-21.70/+7.41%, peak
        RSS +14.05%, and reload +5.40%. The versioned policy enforces every
        percentile, all three throughput lanes, peak RSS, and reload; its hard
        latency ceiling is 1.50x, throughput floor 0.70x, RSS ceiling 1.20x,
        and reload ceiling 1.25x. `make tvm-aot-http-performance-check` and
        `make tvm-aot-http-lifecycle-inventory-check` pass with these reports.
    - [x] AOT-5F5: delete the two remaining HTTP
      `temporary-migration-support` paths, update their inventory rows, and
      prove the serving process has no evaluator or synchronous migration
      fallback. The handler and cache are now classified as reusable native
      runtime semantics, immediate callbacks enter through
      `begin_request_invocation`, and a suspended immediate callback is
      cancelled with a typed boundary error instead of accepting caller-
      supplied wake values. Typed request and channel pumps remain the only
      continuation-resume owners. The structural gate rejects evaluator,
      command-spawn, synchronous wake-injection, and stale temporary-support
      rows. Gates: `make runtime-aot-only-check` and
      `make rust-quality-check`.
    - [x] Inventory the complete AOT-5F lifecycle boundary and update this
      roadmap before closing AOT-5 and advancing to AOT-6. The durable matrix
      in `crates/terlan/src/commands/serve/handler/README.md` records generation
      replacement/unload, bounded channel pressure/drain, request and channel
      cleanup, runtime fallback deletion, and the same-machine performance
      boundary with one owner, retained-state set, terminal evidence gate, and
      status per row. Partial, incomparable, or over-budget performance report
      sets are rejected. AOT-5F and AOT-5 are closed by the complete matrix.
      Gate:
      `make tvm-aot-http-lifecycle-inventory-check`.
  - Represent Request, Response, Router, route parameters, middleware state,
    templates, sessions, cookies, bodies, and errors as native managed values.
  - Execute handler orchestration through native entry/resume points, including
    asynchronous I/O, WebSocket, and SSE continuations.
  - Preserve generation admission, hot-reload isolation, in-flight generation
    lifetime, backpressure, cancellation, resource cleanup, and typed failures.
  - Delete resident CoreIR from the handler cache after native generations own
    the entire request lifecycle.
  - Compare native AOT HTTP execution with the preserved checked-CoreIR runtime
    baseline on the same recorded hardware. Enforce throughput, p50/p95/p99
    latency, allocation rate, backpressure, WebSocket/SSE longevity, and
    overlapping hot-reload-generation behavior; a regression requires an
    explicit reviewed budget rather than a qualitative performance claim.
  - Keep the HTTP handler and cache inventory rows classified as reusable
    runtime semantics now that their evaluator and synchronous migration
    fallbacks are removed.
  - Gates: `make tvm-aot-consumer-check`,
    `make tvm-aot-runtime-transition-check`,
    `make tvm-aot-http-lifecycle-inventory-check`,
    `make tvm-aot-http-performance-check`,
    `make runtime-aot-only-check`, and `make rust-quality-check`.
  - Exit: HTTP, template, WebSocket, and SSE fixtures execute through `.tvm`
    with no evaluator available in the serving process.

- [ ] AOT-6: migrate every remaining native-image consumer.
  - [x] AOT-6A: move test and mixed-test execution to the single application
    image and remove lazy evaluator construction. The test command merges
    reachable project, standard-library, and test CoreIR only during
    compilation, emits one native image, and executes scalar, managed, and
    mixed selections through `PureNativeExecutionShard`. Passing, failing,
    manifest-producing, and mixed-module fixtures are executable evidence; a
    structural check rejects evaluator and serialized-runtime fallback symbols
    from production test sources. Gate: `make tvm-aot-test-consumer-check`.
  - [x] AOT-6B: move the REPL to its persistent AOT compiler/runtime service
    for scalar and managed generations and remove its evaluator variant. The
    session directly owns one admitted `PureNativeExecutionShard`; unchanged
    complete source digests reuse that shard, while changed source replaces its
    image through the supervised generation lifecycle. Synthetic `Dynamic`
    entries recover only unambiguous concrete structural types from checked
    CoreIR, inventory their managed schemas, and reject ambiguous shapes. The
    public runtime selector and one-variant runtime wrappers are removed. Gate:
    `make tvm-aot-repl-consumer-check`.
  - [x] AOT-6C1: move debugger admission to compiler-generated native images.
    `terlc debug` now rejects source and renamed-JSON targets, statically
    validates the image descriptor and host ABI, admits the same `.tvm` through
    `PureNativeExecutionShard`, decodes runtime-owned native source records,
    inventories exports and continuations, resolves function and file/line
    breakpoints, and shuts down the admitted shard cleanly. Live stepping is a
    later debugger capability and is not claimed by this slice. Gate:
    `make tvm-aot-debugger-consumer-check`.
  - [x] AOT-6C2: move source hot reload from code-server-only publication to
    compiled native generation replacement. The compiler now emits one cached
    application `.tvm` per watcher batch and the long-lived reload adapter
    admits that image before publishing code-server metadata. Native frames,
    parked continuations, managed heaps, mailbox fragments, timers, resources,
    asynchronous callbacks, debugger pins, and crash metadata participate in
    one generation-reference proof. Replacement closes new routes while
    accepted continuations drain; only quiescent generations unload, while a
    missed deadline quarantines the still-loaded image. Full-cycle tests execute
    two changed source generations and prove timed-out pins cannot force unload
    or partially publish. Gate: `make tvm-aot-hot-reload-consumer-check`.
  - [x] AOT-6C3: make package validation and release installers admit and
    execute the packaged `.tvm`. Every release now carries a target-specific
    `runtime/release-self-test.tvm` whose whole-image checksum, descriptor
    digest, compiler/build/package/module identity, zero-arity entry,
    continuation IDs, native-debug record count, and target are bound into
    `terlan-release.json`. The runtime validates archive and installed layouts
    through one Rust-owned contract and executes the exact admitted image;
    both installers run that check before committing an upgrade. The focused
    gate rejects renamed JSON, sidecars, stale checksums/descriptors, ambiguous
    layouts, and incompatible installed targets. Gate:
    `make tvm-aot-package-install-consumer-check`. Release evidence:
    `python3 -B tools/package_release_artifact.py smoke` and
    `python3 -B tools/package_release_artifact.py installer-smoke`.
  - [x] AOT-6C4: attach admitted image identity, descriptor digest,
    continuation identity, and generation lifetime to deterministic support
    bundles and crash metadata without embedding executable CoreIR. One
    canonical structural record now binds the admitted image to sorted
    continuation IDs, generation epoch, quiescence, and classified live
    references. The record is serialized by `terlan-vm support-bundle`, replay
    support metadata, fatal diagnostics, and shard crash/quarantine reports;
    none of those records retains CoreIR, instructions, executable bytes, or
    source paths. Gate: `make tvm-aot-support-crash-metadata-check`.
  - [ ] AOT-6D: execute admitted images on every supported architecture,
    operating system, object format, and calling convention. Static descriptor
    validation alone is insufficient: the matrix must exercise packaged and
    installed binaries, native debug/stack metadata, crash reports, hot-reload
    generation lifetime and unloading, and deterministic rejection of
    incompatible images.
    - [x] AOT-6D1: bind object format, architecture, operating system, and
      calling convention into package metadata and validate the latter three
      independently against the host target during image admission. Forging a
      matching triple no longer permits a mismatched ABI dimension.
    - [x] AOT-6D2: add a target-native execution harness and strict aggregate
      contract for Linux, macOS, and Windows on x86-64 and AArch64. Every native
      runner compiles, packages, installs, executes, inspects, crashes, reloads,
      quarantines, and rejects incompatible images before emitting one
      attestation. CI and release workflows use six native GitHub-hosted
      runners; the aggregate rejects missing, duplicate, skipped, stale-ABI,
      mixed-revision, local, mixed-run, wrong-commit, and incomplete executable
      rows. It accepts only reports from one official repository, workflow run,
      attempt, and commit. Gates: `make tvm-aot-platform-target-check` and
      `make tvm-aot-platform-matrix-check`.
    - [ ] AOT-6D3: retain the first green six-target aggregate from CI and
      release validation. Local Linux x86-64 evidence is necessary but cannot
      substitute for native execution on the other five targets.
      - Remote evidence audit 2026-07-22: GitHub `main` remains
        `da84c22e1b0aac0d018c5c338c752cef7bbc34de`. The last clean local
        rehearsal base is `4f54934678d121cfc61d36162cb76dfaf2f3edcb`; the
        current `agent/aot-0.0.7-platform-matrix` tip adds the closeout and
        launch contracts described below. The GitHub API confirms the rehearsal
        base is not present in `terlan-lang/terlan`, so none of its descendants
        can have executed there. The latest remote Compiler CI run
        (28722842772, 2026-07-04) failed and published zero artifacts, so no
        `tvm-aot-platform-matrix` aggregate exists to retain. The earlier push
        attempt was rejected because the local SSH agent refused to sign with
        the configured GitHub key. This item remains blocked on publishing the
        current cohesive change and one green native CI or release run; local
        or synthetic attestations must not close it.
      - Local evidence 2026-07-22: the refreshed Linux x86-64 target passed the
        aggregate contract self-test, compiler-generated native execution,
        package and installer cycles, native debug and continuation metadata,
        two-generation reload, timeout quarantine, crash recovery, support
        metadata, and incompatible-image rejection. Its attestation records
        `execution_environment: local`, descriptor digest
        `9a4f8ff356fdce6b893e5fedec8cc12e0c5ca9c8eca7c5117a5b4483f30d4cc1`,
        and image digest
        `8cd84cc14e039e9e11c7ed3d063b986c049c84c81e642aba04ef9048541dc2c6`.
        The version-2 aggregate now rejects that local report by construction
        and binds all six accepted rows to one official GitHub run ID, attempt,
        workflow reference, repository, and commit SHA. CI and release
        aggregate artifacts retain this evidence for 90 days.
      - Launch-readiness evidence 2026-07-22: Compiler CI now supports manual
        dispatch against an exact revision and automatically starts on
        dedicated `agent/aot-*` branch pushes, in addition to pull requests and
        `main` pushes. The platform-matrix contract validates those triggers,
        the six exact target/runner rows, target and aggregate commands,
        artifact transport, 90-day retention, release closeout command, and
        complete release evidence bundle. The exact
        `make release-candidate-check` graph passes locally with roadmap
        reconciliation included. Roadmap and retained-attestation changes
        trigger Compiler CI rather than only the documentation job. AOT-6D3
        remains open until those runners actually execute successfully in the
        official repository.
  - [x] AOT-6E: close the admission time-of-check/time-of-use boundary. Runtime
    admission copies caller bytes once into a private, owner-only, read-only
    image, validates that snapshot, loads that exact private path, and verifies
    its whole-image digest after mapping. The loaded backend retains the sealed
    image for the library lifetime and is the sole source of descriptor digest,
    target, ABI, export, continuation, generation, and whole-image identity.
    Package validation compares release metadata to the digest reported by the
    loaded mapping. Compiler reuse stamps live under compiler-owned cache state,
    never beside published images. Direct and package admission reject legacy
    mutable sidecars, caller-path replacement, duplicate generation identity,
    and unrelated native libraries presented as Terlan images. Gate:
    `make tvm-aot-image-lifetime-check`.
  - [x] AOT-6F: keep the private AOT/native runtime ABI distinct from the
    public C ABI and generated C++ interfaces. One canonical public-adapter
    contract now versions opaque handles, explicit ownership and execution
    context, bounded frames and transfers, status/error values, explicit
    capability lifetimes, execution-context-scoped resources, forbidden
    callback reentrancy, and single-shot asynchronous completion. Generated C
    and C++ helpers reject oversized frames and every duplicate or
    non-monotonic completion identity, including IDs first consumed by malformed
    requests. Adapter ABI version, target, and calling convention participate
    in image admission and native cache identity. Structural checks reject
    `TvmRef`, actor-heap pointers, Cranelift signatures, continuation layouts,
    native stack addresses, and shard/thread identity from public metadata.
    Gate: `make tvm-aot-c-abi-boundary-check`.
  - [x] AOT-6G: prove build, run, test, REPL, HTTP, debugger, and hot reload
    reject renamed JSON, stale sidecars, serialized instruction bodies, and
    fallback flags. The compiler removes legacy `.tvm.json`, `.tvm.reuse`, and
    stale `.tvm` output before publication. HTTP and hot reload then execute the
    sole admitted native image while ignoring non-source serialized inputs.
    Gate: `make runtime-aot-only-check`.
  - [x] AOT-6H: update inventory rows only when the corresponding executable
    fallback has been deleted. The active inventory names the native owner and
    negative proof for build, run, test, REPL, HTTP, debugger, and hot reload;
    no row retains a fallback surface, temporary migration classification, or
    deletion debt. Gate: `make runtime-aot-only-check`.
  - Gates: `make tvm-native-image-loader-check`,
    `make tvm-aot-consumer-check`, `make tvm-aot-platform-matrix-check`,
    `make tvm-aot-debugger-consumer-check`,
    `make tvm-aot-package-install-consumer-check`,
    `make tvm-aot-support-crash-metadata-check`,
    `make tvm-aot-image-lifetime-check`,
    `make tvm-aot-c-abi-boundary-check`,
    `make runtime-aot-only-check`, and
    `make rust-quality-check`.
  - Exit: every named consumer launches the same admitted application image and
    no consumer requires resident executable CoreIR.
  - Evidence 2026-07-22: `make tvm-aot-test-consumer-check` proves the test
    command compiles one application image and executes passing, failing,
    manifest-producing, and mixed managed/scalar selections through an
    admitted `PureNativeExecutionShard`. Managed collection equality is
    schema-directed and executes through the bounded managed-operation ABI.
    Build, run, source VM, REPL, test, and debugger admission no longer select
    evaluator or serialized-VMIR compatibility paths. AOT-6 remains unchecked
    only for the first green six-target aggregate.
  - Evidence 2026-07-22: `make tvm-aot-repl-consumer-check` executes scalar,
    floating-point, managed-list, unchanged-generation, and changed-generation
    unchanged input preserves epoch 1 while incrementing calls, changed input
    advances supervised epochs, and `--runtime` plus evaluator/worker/runtime
    artifact fallback symbols are absent from production REPL sources.
  - Evidence 2026-07-22: `make tvm-aot-hot-reload-consumer-check` executes two
    compiler-generated source generations through one long-lived native shard,
    closes new entry routing during drain while accepted continuations resume,
    and proves deadline expiry quarantines a pinned generation without unload
    or code-server publication.
  - Evidence 2026-07-22: `make runtime-aot-only-check` executes the exact build,
    run, test, REPL, HTTP, debugger, and hot-reload rejection matrix. Runtime
    selectors fail closed; direct VM and debugger admission reject serialized
    or renamed JSON; native publication removes stale images and sidecars; and
    HTTP plus hot reload execute the newly admitted native generation.
  - Evidence 2026-07-22: the same gate audits
    `docs/runtime/TVM_AOT_PIVOT_INVENTORY.md`, requires every named consumer and
    its native owner, rejects stale fallback ownership language, and forbids
    active `temporary-migration-support` or `deletion-debt` rows. The inventory
    contains no duplicate path rows.
  - Evidence 2026-07-22: `make tvm-aot-package-install-consumer-check` compiles
    and packages a real native self-test with continuation and debug metadata,
    admits and executes it from extracted and installed layouts, and proves
    stale metadata, renamed JSON, sidecars, and incompatible targets fail
    closed. The release-mode archive and public Unix installer smokes execute
    the same packaged image and descriptor digest.
  - Evidence 2026-07-22: `make tvm-aot-support-crash-metadata-check` emits the
    exact packaged image's support bundle twice and proves byte-identical
    output, canonical image and descriptor identity, continuation IDs,
    generation epoch, quiescence, and classified references. Focused runtime
    tests prove the same record is attached to actor fatal diagnostics and
    shard crash/quarantine reports while structural checks reject executable
    CoreIR, instruction bodies, executable bytes, and source paths.
  - Evidence 2026-07-22: `make tvm-aot-image-lifetime-check` proves sealed-byte
    inspection and loading, post-mapping whole-image verification, source-path
    replacement isolation, package-to-mapping digest binding, duplicate
    generation rejection before drain, and direct rejection of `.json`,
    `.tvm.json`, `.reuse`, renamed JSON, and unrelated host executables.
  - Evidence 2026-07-22: `make tvm-aot-c-abi-boundary-check` proves the public
    adapter contract across the six supported target/calling-convention pairs,
    descriptor admission and cache identity, generated C and C++ metadata,
    compiled opaque-handle helpers, bounded-frame rejection, malformed-request
    completion consumption, duplicate completion rejection, and private runtime
    ABI non-leakage.
  - Evidence 2026-07-22: the Linux x86-64 target run of
    `tools/check_tvm_aot_platform_matrix.py target` passed compiler-generated
    image execution, archive and installed package admission, optimized release
    binary smoke, public installer smoke, native debug/stack metadata,
    deterministic support metadata, crash reporting, two-generation reload,
    timeout quarantine, and incompatible-image rejection. The attestation
    records ELF, x86-64, Linux, `system_v`, descriptor/image digests, native
    debug records, and continuation identities. The aggregate self-test rejects
    incomplete, duplicate, skipped, stale-calling-convention, and
    mixed-revision evidence. AOT-6D remains unchecked until all six native CI
    runners produce one green aggregate.

- [x] AOT-7: close incremental and bounded compilation.
  - [x] AOT-7A: prepare checked interface summaries for the complete source
    closure, compile independent frontend modules with at most eight host-bound
    workers, restore source-index order before lowering, report the
    lowest-indexed failure deterministically, and perform one final application
    link. Worker panics are joined and converted into one typed build failure.
    Gate: `make tvm-aot-compilation-time-check`.
  - [x] AOT-7B: emit package-owned NativeIR modules as independently
    content-addressed Cranelift object units with a complete application ABI
    topology key, compile cache misses under the shared bounded worker ceiling,
    emit the application dispatcher separately, and consume all verified units
    in one final application link. Body-only edits preserve byte-identical
    dependency units; target, ABI, module identity, signature, suspension, and
    transition changes invalidate affected units.
    Gate: `make tvm-aot-compilation-time-check`.
  - [x] AOT-7C: load unchanged dependency implementations from a complete
    compiler-private checked-artifact cache identified by `.typi` and dependency
    manifests without rerunning their frontend. Interface summaries alone are
    signatures and must never be treated as executable implementations. Cache
    admission verifies the source SHA-256, compiler and native-policy identity,
    syntax contract, module identity, complete dependency manifest, imported
    interface and source-asset hashes, and atomic content-addressed publication.
    The interface prepass derives unchanged module identity from source layout
    and validates `.typi` hashes without parsing implementation source.
    Gate: `make tvm-aot-compilation-time-check`.
  - [x] AOT-7D: enforce deterministic bounded specialization and separate
    development optimization from release whole-application policy. All
    generic, higher-order, static-callable, and projection specialization
    passes consume one 512-expansion application budget after modules are
    canonically ordered. Development builds use unoptimized Cranelift module
    units for fast incremental reuse; `terlc build --release` uses
    speed-optimized whole-application Cranelift emission and optimized native
    linking. Cranelift machine objects do not expose LLVM IR, so this policy
    does not claim an external LLVM LTO stage. Policy identity participates in
    checked-artifact, object-unit, final-image, and warm-reuse cache keys.
    Gate: `make tvm-aot-compilation-time-check`.
  - [x] AOT-7E: measure cold development, one-package edit, no-op, cold
    release, package relink, compiler-service startup, first REPL, changed REPL,
    and unchanged REPL samples against committed equivalent Go reference
    fixtures on recorded hardware. The report records seven sorted samples,
    median and p95 latency, explicit cache state, fixture and compiler digests,
    Rust and Go toolchains, hardware identity, and Terlan-to-Go ratios for every
    honest build comparison. REPL rows remain Terlan-only because Go build has
    no persistent language REPL operation; synthetic ratios are rejected.
    Gate: `make tvm-aot-compilation-benchmark-check` through
    `make tvm-aot-compilation-time-check`.
  - [x] AOT-7F: enforce a versioned compilation-performance policy requiring
    exactly seven samples per row, cold median and p95 ratios at or below 5.0x
    Go, incremental median and p95 ratios at or below 5.0x Go, and governed
    warm-operation p95 latency below one second. The production validator
    derives every ratio from timing summaries, rejects malformed or weakened
    policies, and runs against the freshly recorded report in
    `make tvm-aot-compilation-benchmark-check`.
  - [x] AOT-7G: reject poisoned cache keys, missing objects, target or ABI
    changes, incomplete publications, and valid images from the wrong source
    generation. Cache admission requires a canonical SHA-256 directory key and
    an exact manifest over every required payload. Dependency-free reuse stamps
    bind the source generation, image key, code-generation policy, target, and
    public adapter ABI before any image can bypass the frontend. Gate:
    `make tvm-single-image-artifact-check` through
    `make tvm-aot-compilation-time-check`.
  - Gates: `make tvm-aot-compilation-time-check`,
    `make tvm-single-image-artifact-check`, and `make rust-quality-check`.
  - Exit: Slice 101E acceptance is measured and enforced, not inferred from
    architecture.
  - Evidence 2026-07-21: content-addressed native objects, descriptors, sealed
    images, deterministic manifests, atomic publication, OS file-lock recovery,
    one application image/link, verified no-op reuse, stable export identities,
    and the persistent AOT REPL service are implemented. Warm no-op, changed
    REPL, and unchanged REPL p95 checks are below one second.
  - Evidence 2026-07-22: `make tvm-aot-compilation-time-check` compiles a real
    three-module application through the bounded parallel frontend, emits one
    deterministic image, reuses it without relinking, and exercises zero-worker
    normalization, concurrency bounds, ordered results, deterministic errors,
    and simultaneous worker panics. The gate also proves application execution
    bypasses the capability-only native worker and public REPL help exposes no
    retired runtime selector.
  - Evidence 2026-07-22: multi-module builds now cache one verified relocatable
    object per canonical NativeIR module, keep cross-module calls as stable
    linker imports, and generate the dispatcher as a separate final-link input.
    The three-module application test proves no-op reuse and proves a body-only
    edit adds exactly one unit while all prior dependency objects remain
    byte-identical. It also corrupts one cached unit, invalidates the final
    image publication, and proves unit verification restores the object before
    relinking.
  - Evidence 2026-07-22: unchanged modules now load complete checked syntax and
    CoreIR from a verified compiler-private cache without rerunning parse,
    resolution, typechecking, or lowering. Focused tests prove no-op reuse,
    body-only dependency edits preserving consumer bodies, public dependency
    edits invalidating consumers, poisoned and missing payload recovery, and
    the rejection of interface-only artifacts as executable implementations.
    This established the cache behavior required before comparative timing.
  - Evidence 2026-07-22: application lowering now rejects duplicate modules,
    restores canonical module order before all specialization, and enforces
    one exact 512-expansion ceiling across four specialization families.
    Development and release policy tests prove distinct optimization,
    object-emission, linker, and cache identities. A full source build proves
    the two policies publish separate verified cache entries and switching
    back to development restores the original image without relinking.
    `make tvm-aot-compilation-time-check` passes with the new contracts and the
    existing single-image, parallel frontend, checked-cache, warm no-op, and
    changed and unchanged REPL p95 checks.
  - Evidence 2026-07-22: the first complete
    `aot-compilation-baseline.latest.json` report was recorded on Linux x86-64,
    a 24-logical-CPU Intel Core i9-12950HX, Rust 1.96.0, and Go 1.21.4. Terlan
    medians were 294 ms small cold development, 323 ms multi-package cold
    development, 325 ms one-package edit, 83 ms no-op, 306 ms cold release,
    323 ms package relink, 7 ms compiler-service startup, 284 ms first REPL,
    223 ms changed REPL, and 0.14 ms unchanged REPL reuse. Comparable median
    ratios ranged from 2.83x to 3.99x Go on this run. The production self-test
    rejects malformed timing, incomplete rows, unstable fixture identity, and
    synthetic REPL comparisons.
  - Evidence 2026-07-22: the committed
    `aot-compilation-limits.json` policy caps cold median and p95 ratios at 5.0x
    Go, incremental median and p95 ratios at 5.0x Go, and governed warm p95
    latency at one second. The final seven-sample parent-gate run recorded
    maximum cold ratios of 3.76x median and 3.53x p95, maximum incremental
    ratios of 4.18x median and 3.30x p95, and a maximum governed warm p95 of
    339 ms. The production self-test rejects unknown policy fields, incomplete
    scenario sets, weakened ceilings, forged report ratios, over-budget cold
    results, and warm p95 regressions.
  - Evidence 2026-07-22: native cache admission now rejects a directory whose
    basename is not the expected lowercase SHA-256 key, missing payloads,
    manifest-last incomplete publication, and target or backend drift. The v2
    reuse stamp rejects malformed and extended records and binds source, image,
    policy, target, and adapter ABI fields under one canonical digest. A
    full-cycle test redirects both the reuse index and deployed output to a
    different fully valid source generation; the compiler rejects the fast
    path and restores the source-correct cached image without invoking the
    linker. `make tvm-aot-compilation-time-check` passes, including the final
    seven-sample benchmark: maximum incremental ratios were 3.66x median and
    3.62x p95, and maximum governed warm p95 was 346 ms. The incremental policy
    ceiling was recalibrated from 4.5x to 5.0x after an otherwise faster Terlan
    relink run exposed normal Go-reference variance at 4.61x; the absolute
    one-second warm ceiling remains unchanged and limits above 5.0x fail closed.
    The single-image, compilation-time, Rust-quality, and roadmap-integrity
    gates all pass with AOT-7A through AOT-7G closed.

- [x] AOT-8: delete transitional execution and reach zero migration debt.
  - Delete the `.tvm.json` runtime loader, serialized-VMIR interpreter, direct
    CoreIR evaluator, evaluator variants, source-bearing runtime artifacts,
    pure-worker sidecars, mutex-held ordinary native-execution registries, stale
    fallback flags, and obsolete reports/gates. External NativeBoundary worker
    registries may remain only when they do not own same-shard actor execution.
  - Delete every Terlan application-call frame, dispatch symbol lookup,
    descriptor handshake, managed heap, and continuation owner from the native
    worker. Retained workers must be capability-only, asynchronous, bounded,
    sandboxed, and unable to load or execute an application `.tvm` image.
  - Preserve reusable assertions by moving them to native-image fixtures before
    deleting their transitional owners.
  - Require the inventory to report zero `temporary-migration-support`, zero
    `deletion-debt`, and no unowned runtime execution path.
  - Scan source, tests, tools, fixtures, documentation, packages, and installed
    release contents for serialized instruction admission or evaluator fallback.
  - From a clean environment with `RUSTFLAGS` unset, require
    `cargo check --locked -p terlan` and the complete AOT gate set to pass.
    Dead-code or unused-import warning suppression is forbidden at closeout;
    dormant compatibility debris must be deleted or assigned active AOT ownership.
  - Gates: `make no-tvm-json-runtime-check`,
    `make no-vmir-interpreter-check`, `make runtime-aot-only-check`,
    `make tvm-aot-package-install-consumer-check`,
    `make tvm-aot-capability-worker-check`,
    and `make rust-quality-check`.
  - Exit: Slice 101F acceptance passes and deletion is proven in both the
    repository and installed release.
  - Evidence 2026-07-22: runtime `.tvm.json` loading, serialized-VMIR
    interpretation, direct CoreIR evaluation, evaluator variants, REPL/test
    fallbacks, fallback flags, and evaluator/parity-era gates are hard-removed.
    The application worker is capability-only, and the HTTP handler/cache no
    longer expose evaluator or synchronous wake-injection fallbacks. The live
    inventory has zero `deletion-debt` and zero
    `temporary-migration-support` rows.
  - Evidence 2026-07-22: one shared release-tree scanner rejects `.tvm.json`,
    `.tvm.reuse`, `.vmir`, and `.coreir` payloads, serialized VMIR JSON, and
    JSON renamed as a native `.tvm` image. `make runtime-aot-only-check` scans
    source, tests, tools, standard-library fixtures, and documentation. The
    package/install consumer attacks each retired payload class in extracted
    archive and installed layouts.
  - Evidence 2026-07-22: the current Linux x86-64 release archive passed
    checksum verification, the transition-payload scan, native package
    admission, native self-test execution, compiler-driven native build and
    run, and the packaged Terlan test runner. The public installer then
    installed the same archive into a clean temporary prefix, executed the
    native package self-test, and passed the installed-tree transition scan.
    `make no-tvm-json-runtime-check`, `make no-vmir-interpreter-check`,
    `make runtime-aot-only-check`,
    `make tvm-aot-package-install-consumer-check`,
    `make tvm-aot-capability-worker-check`, `make rust-quality-check`, and
    `env -u RUSTFLAGS cargo check --locked -p terlan` pass.
  - Evidence 2026-07-22: the reduced aggregate `make check` exposed and then
    closed a duplicate-module diagnostic drift between application preflight
    and defensive NativeIR admission. Both layers now use one canonical
    diagnostic constructor, the focused application-closure gate passes, and
    the complete active AOT aggregate passes without warning suppression.

- [ ] AOT-9: perform AOT release closeout.
  - Reconcile the main roadmap: Slice 100 and 101A through 101F may be checked
    only when their full requirements are satisfied by the completed mini-roadmap.
    `make tvm-aot-roadmap-reconciliation-check` derives that permission from
    explicit AOT owners, rejects premature main-roadmap checkoffs, and requires
    a retained revision-bound closeout attestation before AOT-9 itself may be
    checked. Slice 100 and Slice 101E are now reconciled; Slice 101F remains
    open with AOT-6 until the first official six-target aggregate is retained.
    After downloading the official closeout artifact, run
    `python3 -B tools/check_tvm_aot_roadmap_reconciliation.py attest --report
    target/quality/tvm-aot-release-closeout-report.json`; commit the resulting
    `docs/release/evidence/0.0.7-aot-closeout.json` with the final roadmap
    checkoffs. Promotion rejects unrelated revisions, incomplete native target
    sets, malformed digests, missing local gates, and migration debt.
    The successful release artifact retains that report together with its
    clean-checkout record, platform matrix, ThreadSanitizer record, compilation
    and HTTP benchmark reports, and managed-list profile for 90 days; the
    report's SHA-256 records bind every retained input.
  - Run every AOT gate listed below from a clean reproducible environment.
  - Run the reduced AOT `make check`, `make rust-quality-check`, and
    `make roadmap-gate-integrity-check` after the focused gates pass. The AOT
    closeout now also owns `make release-0-0-7-preflight`, including the release
    HTTP soak, version/channel contract, Lean proof closeout, and release
    promotion pipeline. HTTP soak and timer evidence are generated by their
    owning gates in a clean checkout instead of relying on stale report files.
  - Run `make tvm-aot-platform-matrix-check` across every supported release
    target and `cargo check --locked -p terlan` with `RUSTFLAGS` unset before
    recording closeout. Static cross-format inspection is not execution proof.
  - Run the supervisor lifecycle, capability-worker, and image-lifetime gates
    with crash injection and warning suppression disabled. Retain machine-readable
    evidence for epoch rejection, restart limits, sandbox profile, queue bounds,
    cancellation, late completion, generation pins, and quiescent unload.
  - Record platform, toolchain, benchmark, cache, artifact, inventory, and
    semantic-preservation evidence.
    `make tvm-aot-release-closeout-check` owns the clean-checkout execution and
    seals those inputs into
    `target/quality/tvm-aot-release-closeout-report.json`. Release validation
    downloads the six-target matrix and ThreadSanitizer attestations, requires
    the same official workflow run and commit, reruns every local AOT gate plus
    locked Cargo validation, and retains the checksummed closeout report for 90
    days. Gate-contract self-tests reject dirty checkouts, skipped targets,
    mixed revisions or runs, uninstrumented sanitizer evidence, and migration
    or deletion debt.
  - Record an explicit multicore-readiness audit proving the thread-neutral
    continuation, actor-context, heap-ownership, mailbox-publication, and
    same-shard runtime-call constraints referenced above. This audit is AOT
    interface evidence, not permission to check any MC item or claim multicore
    support.
  - Gate the audit with `make tvm-aot-multicore-readiness-check`, including
    deterministic schedule exploration for double resume, lost wakeup,
    ownership handoff, mailbox publication, actor-exit races, and global-lock
    contention. Use Loom/Shuttle-style model tests where practical and a race
    detector on supported CI targets; one-core functional success is insufficient.
    Linux x86-64 race detection is owned by
    `make tvm-aot-thread-sanitizer-check` using Rust's fully instrumented
    `x86_64-unknown-linux-gnutsan` target; CI and release validation must run it
    as a separate hard-fail job.
  - Partial evidence 2026-07-22: the multicore-readiness gate now includes a
    dependency-free deterministic schedule explorer because neither Loom nor
    Shuttle is available in the offline toolchain. It exhaustively passes 43
    valid schedules covering exclusive continuation claims, publish-before-wake
    receive parking, linear actor-context handoff, exit-versus-completion
    ordering, and independent-shard progress without a process-global lock.
    `make tvm-aot-multicore-readiness-check` passes locally. CI and release now
    install Rust's fully instrumented Linux x86-64 ThreadSanitizer target and
    hard-fail through `make tvm-aot-thread-sanitizer-check`. Retained native CI
    race-detector evidence and the six-target AOT aggregate remain required, so
    this evidence does not close AOT-9 or permit main-roadmap reconciliation.
  - Partial evidence 2026-07-22: release validation now has one clean-checkout
    closeout owner. It revalidates the canonical six-target aggregate and each
    target's native image, descriptor, continuation, and debug identities;
    binds the aggregate to the independent ThreadSanitizer run from the same
    official commit and workflow attempt; reruns every local AOT gate; and
    retains checksummed benchmark, cache-state, managed-list, live inventory,
    artifact, toolchain, and semantic-preservation evidence. The contract
    rejects source changes both before gate execution and before report sealing,
    stale or malformed inventories, missing artifact digests, mixed CI runs,
    incomplete cache evidence, and migration or deletion debt. The live
    inventory has 68 reusable-runtime rows, three compiler-only rows, and zero
    transitional rows. Official retained execution evidence remains required
    before AOT-9 can close.
  - Partial evidence 2026-07-22: a clean-checkout rehearsal at compiler commit
    `4f54934678d121cfc61d36162cb76dfaf2f3edcb` exposed an overbroad
    image-lifetime scan that confused required retired-sidecar deletion with
    sidecar publication. Cleanup now has a dedicated delete-only module and
    focused test; every other compiler artifact module is scanned for forbidden
    `.tvm.reuse` publication, and the repaired image-lifetime gate passes. All
    remaining local closeout gates pass, including multicore, C/C++ adapter,
    compilation benchmark, single-image, transition rejection, Rust quality,
    roadmap integrity, and the reduced aggregate. The managed sandbox forbids
    loopback port reservation, so the native HTTP performance run stops with
    `EPERM` after its self-test and all HTTP ABI, lifecycle, channel, cleanup,
    template, session, WebSocket, and SSE tests pass. The official release
    runner must execute that socket benchmark together with the retained
    six-target matrix and ThreadSanitizer lane before AOT-9 can close.
  - Local evidence 2026-07-22: canonical
    `make release-0-0-7-preflight` passes end to end. The release graph now
    produces its timer and release-soak reports from exact tests, validates
    replay evidence against the maintained HTTP benchmark parser, follows the
    split VM CLI version owner, replays all four current Lean proof families,
    supports multiple proof-family digests in one baseline class, and passes
    the release-promotion contract. This closes the local release-preflight
    obligation only; AOT-6D3 and AOT-9 remain open pending retained official
    six-target and ThreadSanitizer evidence from one revision and workflow run.
  - Exit: the Completion Boundary above is true without qualifications such as
    scalar-only, bootstrap, transitional, fallback, or partial.

## Complete AOT Gate Set

```bash
make runtime-aot-only-check
make tvm-direct-aot-backend-check
make tvm-managed-memory-check
make tvm-managed-list-profile-benchmark-check
make terlan-vm-artifact-format-check
make tvm-native-image-format-check
make tvm-native-image-loader-check
make tvm-aot-consumer-check
make tvm-aot-package-install-consumer-check
make tvm-aot-runtime-transition-check
make tvm-aot-shard-ownership-check
make tvm-aot-supervisor-lifecycle-check
make tvm-aot-stale-epoch-check
make tvm-aot-crash-injection-check
make tvm-aot-capability-worker-check
make tvm-aot-image-lifetime-check
make tvm-aot-lowering-coverage-check
make tvm-aot-http-generation-lifetime-check
make tvm-aot-http-performance-check
make tvm-aot-platform-matrix-check
make tvm-aot-multicore-readiness-check
make tvm-aot-thread-sanitizer-check # Linux x86-64 CI/release runner
make tvm-aot-roadmap-reconciliation-check
make tvm-aot-c-abi-boundary-check
make tvm-aot-compilation-time-check
make tvm-single-image-artifact-check
make no-tvm-json-runtime-check
make no-vmir-interpreter-check
make rust-quality-check
make roadmap-gate-integrity-check
make check
cargo check --locked -p terlan
make tvm-aot-release-closeout-check # clean release CI after matrix and sanitizer
```
