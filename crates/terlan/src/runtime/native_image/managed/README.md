# Native Image Managed Memory

This module owns the actor-local managed-memory contract used by direct-AOT
Terlan images.

## Ownership

- `layout.rs` owns semantic type identities, physical layout fingerprints,
  allocation classes, and precise object reference maps.
- `roots.rs` owns native safepoint maps, runtime root locations, and managed
  continuation captures.
- `atoms.rs` owns the finite, canonically ordered image-generation atom table.
- `aggregates.rs` owns fixed tuples, arrays, records, and active algebraic
  constructor layouts over precise managed-reference fields.
- `aggregate_abi.rs` owns the bounded, deterministic aggregate descriptor
  crossing from generated native code into an actor heap. Decoding rebuilds
  the canonical descriptor through checked constructors before allocation;
  trailing data, partial layouts, invalid UTF-8, inconsistent array fields,
  malformed variants, and descriptors above 64 KiB are rejected.
- `collection_abi.rs` owns bounded, deterministic schemas for materialized
  List, Map, and Set roots. It identifies the existing storage family and its
  exact scalar or managed-reference slot types without exposing node layouts.
- `layout_registry.rs` owns the immutable aggregate layouts, collection schemas,
  and finite atom identities admitted with one executable image. It groups
  constructor variants by semantic identity, resolves live objects by exact
  physical fingerprint, resolves each collection through one canonical schema,
  and maps canonical atom text to generation-local compact indexes.
- `compiler/native_ir/cranelift/managed.rs` inventories those descriptors into
  immutable object data and lowers constructor expressions to the VM allocator
  callback. The callback receives an actor context, exact descriptor bytes,
  word-sized field values, and caller-owned result storage. Missing callbacks,
  callback failures, and zero managed references are rejected before a result
  can escape generated code.
- `execution.rs` binds each synchronous dispatch to a stack-scoped allocation
  context and lazily reuses one `ActorHeap` per nonzero actor owner. The callback
  decodes every field word according to its admitted descriptor and publishes
  only a complete owner-local object. Scalar calls do not materialize a heap,
  callback panics become a typed failure, and successful managed results are
  validated against their admitted owner and semantic identity. It also owns
  the checked `String`, Bytes, and Binary entry allocation and result-copying
  operations used by the direct application boundary. Binary argument
  allocation is transactional across backing storage and slice creation.
- `runtime/vm/pure_native/direct_backend.rs` owns this execution context on the
  ordinary application path. Independent backend forks share immutable loaded
  code and managed schemas but begin with empty heaps and no parked
  continuation state. Its `managed_values` module recursively converts fixed
  tuples, arrays, records, constructors, List, Map, and Set values at public
  entry/result boundaries. Atom and reference-valued collection keys use stable
  Terlan content equality and hashing rather than local indexes or references.
  Conversion is owner-checked, depth/work bounded, cycle rejecting, and
  transactional across the complete nested graph.
- `execution.rs` partitions generated suspension captures by descriptor type.
  Scalar captures remain in scheduler-visible transition state; managed
  captures remain runtime-private `ManagedContinuation` roots, survive
  actor-local relocation, and return to their exact generated parameter
  positions only when the matching owner resumes the matching continuation.
  The isolated worker reuses this mechanism while projecting its bounded
  external control protocol to scalars only.
- `mailbox.rs` owns immutable receiver-local message fragments. `heap.rs`
  inventories each sender graph iteratively under an explicit work budget,
  copies distinct objects once while preserving sharing, and publishes the
  receiver heap only after the complete graph and precise mailbox root exist.
  A failed transfer changes neither heap. The same-shard actor runtime owns
  typed queue publication and exact type selection; direct `Managed(id)` graph
  transfer remains to be connected to that queue transaction. The isolated
  native-worker transport remains scalar-only.
- `lists.rs` owns adaptive inline and 32-way regular/relaxed RRB list storage.
  Tree updates and appends copy only the selected path, while concatenation
  rebalances the touching fringes and shares untouched subtrees. Subtraction
  consumes compiler-specialized structural equality, and swaps copy the union
  of both changed paths without mutating prior versions. Transient builders
  exclusively borrow their actor heap, validate values before buffering, and
  publish through the same canonical list constructor.
- `maps.rs` owns insertion-ordered immutable map roots, typed key/value slots,
  compiler-specialized structural equality and stable hashing, and persistent
  put, take, remove, and clear operations. Small maps remain packed and flat;
  larger maps use actor-heap A-CHAMP nodes plus an RRB insertion-order list.
  Indexed replacement and insertion copy only affected index and order paths,
  while collision, sparse, dense, and compressed nodes carry precise reference
  maps and survive relocation.
- `sets.rs` owns the typed Set surface while delegating storage, equality,
  hashing, and adaptive profiles to the canonical unit-valued managed map
  representation.
- `slots.rs` owns shared checked alignment and packed-slot layout arithmetic
  used across managed collection families.
- `sequences.rs` owns actor-local immutable strings, bytes, and checked
  bitstring slices. Public result conversion copies semantic content into
  runtime-owned values; actor-local references never become public values.
- `heap.rs` owns actor-local bump allocation, precise graph tracing, moving
  semispace collection, relocation, accounting, limits, and actor-exit
  reclamation.
- `core.rs` owns the opaque pointer-width `TvmRef<T>`, actor identity, and
  shared managed-memory errors. The runtime and safe benchmark embedding reuse
  these exact types.
- `mod.rs` owns the public managed-memory module surface.

Application code cannot construct or inspect a `TvmRef<T>`. Object metadata is
runtime-private side metadata rather than a public fixed heap header. Collection
is actor-local, precise, and bounded by an explicit work budget. Any reference
that belongs to another heap or predates relocation is rejected before access.
One actor's collection never traverses a heap registry or waits for another
actor. Budget exhaustion is detected before relocation and leaves that heap's
roots and bytes unchanged; it cannot pause or mutate a sibling actor heap.

## Extension Rule

Managed value families must reuse these descriptors, references, roots, and
heap operations. They must not introduce another object identity, ownership
token, collector, stack-map representation, or continuation-root type.

## List Profile Benchmark

Run `make tvm-managed-list-profile-benchmark-check` from the compiler root to
measure inline, regular-tree, relaxed-tree, path-copy update, append,
concatenation, and transient construction profiles in release mode. The command
writes the versioned report to
`target/quality/tvm-managed-list-profile.json`. Timing values are observational;
the operation-specific managed-object maxima are deterministic regression
budgets.
