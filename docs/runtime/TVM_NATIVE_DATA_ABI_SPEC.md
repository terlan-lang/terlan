# TVM Native Data ABI Specification

Status: current implemented normative semantic contract for Terlan 0.0.7; native ABI
version 1 and managed-layout profile 1 are pre-freeze.

The compiler and VM MUST use ABI 1 as the sole current native execution
contract. New compiler, runtime, image, and NativeBoundary work MUST conform to
this specification. ABI 1 is implemented but not yet frozen: its versioned
layouts and metadata may change before the 0.0.7 compatibility freeze, and
third-party precompiled objects MUST NOT assume compatibility across Terlan
releases until that freeze is declared.

ABI 1 exposes three deliberately separate guarantee classes:

1. **Language guarantee**: unrestricted low-level Terlan code has no
   language-level memory-safety guarantee.
2. **Managed VM guarantee**: admitted actor heaps, managed references,
   descriptors, roots, continuations, messages, and resources are validated by
   semantic identity, physical layout, owner, generation, and bounded work.
3. **NativeBoundary guarantee**: generated adapters convert declared external
   values explicitly, while unsafe Rust, C, C++, CUDA, and other foreign code
   executes outside the execution shard by default and exchanges only owned
   typed values or opaque capabilities.

No implementation, package, or release document may collapse these classes
into a claim that the Terlan language or arbitrary foreign code is memory-safe.

This document defines the data, call, memory, actor, continuation, and transport
contracts used by AOT-compiled Terlan code. It is subordinate to
`TVM_EXECUTABLE_IMAGE_SPEC.md` for image admission and execution.

Terlan is a state-of-the-art reimplementation and optimization of the Erlang
execution model, not an ERTS or BEAM compatibility layer. It preserves the
language-independent properties that make the model valuable--isolated
lightweight processes, immutable messages, independent failure, supervision,
preemptive fairness, selective receive, and observable runtime state--while
replacing instruction interpretation and dynamically tagged ordinary values
with direct AOT native code, static layouts, precise compiler metadata, and
specialized data paths.

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**,
and **MAY** are normative requirements.

## 1. Architectural Objective

The native architecture MUST optimize the complete actor system rather than a
single function-call benchmark. It is designed for:

- millions of independently scheduled lightweight Terlan processes;
- low-cost process creation, message send, and failure cleanup;
- bounded scheduler latency under allocation, receive, and native workloads;
- garbage collection whose ordinary pause scope is one Terlan process;
- native unboxed execution for statically typed code;
- efficient immutable sharing for large binaries, images, tensors, and other
  bulk storage;
- deterministic inspection, replay, hot reload, and capability enforcement;
- unsafe external code that cannot corrupt the scheduler or another actor.

No representation may be called state-of-the-art merely because it is native.
ABI 1 is not eligible to freeze until executable comparative gates measure
spawn, scheduling, message passing, selective receive, allocation, collection,
large-object sharing, crash isolation, and tail latency against a supported
Erlang/OTP reference on identical hardware.

The AOT and managed-memory pivot changes mechanics, not already accepted actor
semantics. Every Erlang-derived test already ported to Terlan MUST retain its
Terlan source fixture, behavioral assertions, ordering expectations, failure
expectations, and public test identity. Migration may replace only the execution
harness and artifacts required to run the same test through the native TVM path.
A test that genuinely asserted BEAM, ERTS, opcode, or interpreter implementation
detail must be explicitly classified with evidence; it cannot be weakened,
rewritten, deleted, or marked passing as an ordinary pivot migration.

The JavaScript backend is not part of the native TVM execution pivot. Existing
JavaScript lowering, emitted module behavior, browser and server execution,
golden output, source maps, declarations, and public tests MUST remain
unaffected. A shared compiler-IR change needed by direct AOT must prove existing
JavaScript fixtures byte-for-byte unchanged where output stability is promised,
and behaviorally identical everywhere else. The native pivot cannot make a
JavaScript regression, fixture regeneration, skipped test, or output rewrite an
accepted migration cost.

## 2. Representation Domains

There is no universal native representation for every Terlan value. Statically
typed Terlan computation uses statically typed native layouts. A universal
boxed `Value`, BEAM term word, or interpreter tag MUST NOT become the ordinary
application function ABI.

ABI 1 distinguishes four representation domains:

1. **Compiled value ABI**: values exchanged by separately compiled AOT Terlan
   objects in one admitted image generation.
2. **Managed layout profile**: runtime-private actor heaps, shared immutable
   storage, mailbox fragments, roots, and collection nodes inside one execution
   shard.
3. **TVM transport encoding**: canonical pointer-free values crossing a shard,
   OS-process, node, persistence, debugger, or hot-reload migration boundary.
4. **External NativeBoundary ABI**: generated adapters and isolated workers for
   Rust, C, C++, CUDA, and other native packages.

The compiled value ABI fixes what independently compiled objects must agree
on. The managed layout profile fixes runtime behavior and metadata interfaces,
but deliberately does not expose a public heap header. TVM transport is not
used for ordinary same-shard scheduling or message fast paths. External package
layouts never become Terlan layouts.

Raw native pointers MUST NOT cross from one execution shard or NativeBoundary
worker to another. Type identities and semantic conversion rules MAY be shared
between domains; physical addresses and allocator metadata MUST NOT.

### 2.1 Cranelift Code-Generation Boundary

Cranelift is the only conforming native code-generation backend for Terlan
0.0.7. `terlc` lowers checked CoreIR into compiler-internal **Terlan NativeIR**,
performs Terlan-owned ABI, ownership, safepoint, continuation, and representation
decisions there, then lowers NativeIR into Cranelift IR. Cranelift performs
target instruction selection, register allocation, frame finalization,
relocation production, and native object emission.

Cranelift does not define the Terlan language ABI, NativeIR, runtime ABI, type
identity, layout fingerprint, actor heap, stack-map encoding, descriptor, TVM
transport, or executable-image format. No Cranelift IR is stored in `.tvm`,
compiler interfaces, or runtime metadata.

The compiler uses `cranelift-object` in-process to emit relocatable native
objects. It MUST NOT start a Cranelift command-line compiler per module or
function. The exact Cranelift version, target ISA settings, CPU feature floor,
and relevant code-generation flags participate in object cache keys and build
provenance.

Terlan owns precise liveness and marks every managed reference that requires a
stack map. Cranelift user-defined stack maps may carry those roots through frame
layout, but their library data structures are not a stable TVM format. `terlc`
converts finalized code offsets and root locations into a compact, versioned
Terlan stack-map section validated by the TVM runtime.

LLVM is not a Terlan application code-generation backend. Product compilation
MUST NOT link or invoke LLVM libraries, LLVM IR, bitcode, `opt`, `llc`, ORC, or
LLVM stack-map/statepoint formats. An upstream Rust toolchain may itself have
used LLVM to build the already compiled `terlc` executable; that provenance does
not make LLVM part of Terlan application compilation.

## 3. Execution Shards And Actor Isolation

An execution shard is an OS process containing the precompiled TVM runtime,
one admitted native image generation, scheduler threads, actor heaps, local
mailboxes, and the local I/O reactor. A VM supervisor control plane admits,
observes, restarts, and replaces shards. A shard is not a Terlan actor: it is a
crash-containment and scheduling domain hosting many lightweight actors.

Each Terlan actor owns:

- one logical process identity and generation;
- one independently collectible managed heap;
- one ordered mailbox and selective-receive state;
- one current native continuation;
- links, monitors, timers, reductions, and resource ownership;
- explicit soft and hard memory limits and observable accounting.

Only the scheduler thread holding the actor's mutator token may execute or
mutate its heap. An actor may migrate between scheduler threads at a safepoint,
but two threads MUST NOT execute the same actor concurrently. This makes
ordinary actor-heap allocation and mutation non-atomic.

A typed Terlan exception, pattern failure, allocation failure, or explicit exit
terminates only the affected actor and enters normal link/monitor/supervision
semantics. Unsafe NativeBoundary code runs outside the execution shard by
default. A compiler, runtime, or hardware fault may still terminate a shard;
the supervisor converts that event into attributed actor failures and applies
declared restart or durable-recovery policy. The architecture MUST NOT claim
that native machine code makes arbitrary memory corruption recoverable inside
one address space.

## 4. Target Model

Native ABI 1 initially supports 64-bit little-endian x86-64 and AArch64
targets, two's-complement integers, and IEEE-754 binary64 floating point. Terlan
`Int` remains fixed `i64`; it is never pointer-sized.

The compiled value ABI uses the target data layout. It MUST record pointer
width, endianness, scalar and vector alignment, object format, calling
convention, CPU feature floor, and native ABI version in the image descriptor.
A loader rejects a mismatch before code execution.

ABI 1 does not impose an eight-byte maximum alignment. The compiler may use
16-, 32-, or 64-byte alignment for target-supported vectors and specialized
numeric storage. Over-aligned allocation MUST use the matching runtime service,
and the alignment participates in the layout fingerprint. Portable transport
encoding never inherits target padding or alignment.

## 5. Type And Layout Identity

Every runtime-relevant concrete type has:

- a 128-bit semantic type identity derived from its package-qualified canonical
  type shape and language ABI major version;
- a canonical read-only type descriptor;
- a 256-bit native layout fingerprint for each target and representation
  profile;
- ownership, tracing, movement, destruction, transport, and inspection
  operations;
- optional specialized equality, hashing, copying, and message-transfer
  operations.

The semantic identity remains stable when a private physical optimization
changes. The native layout fingerprint changes whenever size, alignment, field
placement, union representation, reference map, call classification, or
specialization changes. A cross-object call or load rejects incompatible
fingerprints instead of reinterpreting memory.

A descriptor records at least:

- semantic identity, native fingerprint, size, and alignment;
- representation category and target feature requirements;
- ordered source fields and their physical locations when materialized;
- union variants and discriminants;
- precise managed-reference and shared-reference maps;
- copy, trace, relocate, finalize, equality, hash, inspect, and encode entries;
- valid safepoint and continuation metadata;
- optional debug name outside stripped release data.

The compiler MUST diagnose a semantic-identity collision. Rust `TypeId`, Rust
or C++ mangling, `repr(Rust)`, source paths, memory addresses, and cache paths
are not valid Terlan identities.

## 6. Primitive Compiled Layouts

ABI 1 primitive layouts are:

| Terlan form | Size | Alignment | Compiled representation |
| --- | ---: | ---: | --- |
| `Unit` | 0 | 1 | no payload |
| `Bool` | 1 | 1 | `u8`, exactly `0` or `1` |
| `Int` | 8 | 8 | signed two's-complement `i64` |
| `Float` | 8 | 8 | finite IEEE-754 binary64 bits |
| `Atom` | 4 | 4 | image-generation-local immutable atom index |
| `TvmRef[T]` | pointer width | pointer alignment | relocatable, actor-local managed reference |
| `SharedRef[T]` | pointer width | pointer alignment | shard-local immutable shared-storage reference |

`Bool` values other than zero and one are invalid. `Float` admits finite values,
including signed zero; NaN and infinity are rejected at constants, checked
operations, runtime entries, and transport decoding.

`Number` is a typing constraint and has no native layout. It is resolved to a
concrete numeric type or explicit union before code generation. `Never` has no
inhabitants. Compile-time-erased literal evidence allocates no storage.

## 7. Atoms

Atoms are finite compiler-known identities. Each image generation contains a
canonical immutable table ordered by normalized UTF-8 identity; compiled code
uses its zero-based `u32` index.

An atom index is generation-local. Transport and persistence encode canonical
UTF-8 identity or a collision-checked stable fingerprint, never the local
index. Dynamic text-to-atom creation is unsupported, preventing an unbounded
global atom table. Unknown atoms, invalid UTF-8, and identity collisions fail
atomically with typed diagnostics.

## 8. Actor-Local Managed Heaps

Ordinary variable-size, recursive, and escaping Terlan values live on the
owning actor's managed heap. A `TvmRef[T]` is one non-null, relocatable reference
valid only while that actor owns the referenced graph. It MUST NOT be retained
by another actor, shared arena, OS resource, NativeBoundary worker, persisted
record, or VM control plane.

The managed heap uses precise compiler metadata and MUST support:

- a bump-pointer young-allocation fast path;
- actor-local collection without stopping unrelated actors;
- a copying or evacuating young generation;
- promotion and an independently collectible old generation;
- precise root discovery from stack maps, registers, continuations, mailbox
  fragments, runtime frames, and actor state;
- relocation of all live `TvmRef` roots and fields;
- bounded collection work integrated with scheduler reductions;
- heap compaction or another demonstrated fragmentation bound;
- immediate whole-heap reclamation when the actor exits.

The first implementation MAY use a simple semispace collector before adding an
old generation, provided the public references and metadata already permit
movement. A non-moving collector is acceptable only as a measured temporary
profile and MUST NOT make addresses observable.

Ordinary actor-heap objects MUST NOT carry atomic strong and weak reference
counts. The runtime owns the physical header, forwarding state, age bits, mark
bits, size classes, side tables, and large-object metadata. It MAY use compact
headers, headerless typed regions, tagged references, or side metadata. None of
those bytes is a stable application ABI.

Finalizers are forbidden for ordinary values. OS and package resources use
`NativeResource` lifecycle ownership so collection timing cannot define program
behavior.

## 9. Roots, Safepoints, And Native Frames

The compiler emits precise stack maps for every safepoint. Each map identifies
live actor-local references, shared references, derived references, their base
objects, and values required to reconstruct the continuation. Conservative
native-stack scanning is non-conforming.

Safepoints occur at:

- allocation slow paths;
- bounded loop and call intervals required by the reduction budget;
- actor operations that may park or yield;
- explicit collection, inspection, migration, and cancellation polls.

The common scheduler poll SHOULD be an inline thread-local or actor-local budget
test. It enters the runtime only on exhaustion or a pending event. A safepoint
is native control flow plus metadata, not a stored Terlan instruction and not a
mandatory VM/worker IPC frame.

No borrowed stack address may survive a safepoint that parks, migrates, invokes
unknown native code, or permits collection. Derived interior references must be
recomputable from a traced base reference and checked offset.

## 10. Escape Analysis And Allocation Elision

The compiler SHOULD stack-allocate, scalar-replace, or eliminate aggregates,
closures, iterators, options, results, and collection intermediates proven not
to escape. It SHOULD specialize actor-local allocation sites when the type and
lifetime are known.

An optimization may change physical placement but not identity, message,
inspection, failure, hot-reload, or resource semantics. Any value live across a
park must be represented in a traced continuation or actor-heap object. Escape
decisions and representation dependencies participate in incremental cache
keys and reproducible optimization reports.

## 11. Shared Immutable Storage

Large immutable byte buffers, strings, image planes, tensor storage, mapped
regions, and compiler-approved persistent nodes MAY use shard-local
`SharedRef[T]` storage. This is a separate memory domain, not the default actor
heap.

Shared objects:

- are deeply immutable;
- contain no actor-local `TvmRef`, mutable cell, borrowed address, continuation,
  or unowned OS handle;
- have an explicit size and type descriptor;
- use atomic reference counting, epoch reclamation, or another proven
  concurrency-safe reclamation scheme owned by the runtime;
- are charged proportionally or by declared policy to all retaining actors;
- are reclaimed independently of actor-local collection;
- never expose their native address as Terlan identity.

The compiler and runtime choose copy, move, or share using deterministic,
benchmark-backed thresholds. Large-object sharing MUST NOT force atomic
reference counting onto ordinary tuples, lists, closures, or actor-local
objects.

## 12. Strings, Bytes, And Binaries

`String` is immutable, valid UTF-8 and records byte length without requiring a
trailing NUL. Small strings MAY live in actor-local managed storage or an inline
specialized representation. Large strings MAY use shared immutable storage.
The selected form is described by the native layout fingerprint and is not a
transport promise.

Bytes are immutable byte sequences with the same actor-local or shared storage
choice. A `Binary` or bitstring slice semantically contains a storage reference,
bit offset, and bit length. The runtime proves checked addition and bounds.
Byte-aligned operations may select vectorized kernels and zero-copy shared
slices without changing bit semantics.

Cross-shard and external transport may use bounded inline bytes, shared-memory
capabilities, scatter/gather I/O, or chunked transfer. A zero-copy path MUST
authenticate owner, generation, length, permissions, and cleanup authority; a
raw pointer is never a transport value.

At an in-shard public application entry, the runtime resolves the exact export
descriptor before conversion. `String`, Bytes, and Binary inputs are copied
into the destination actor heap and passed to generated code only as validated
owner-local references. Binary backing storage and its checked slice are
published atomically. A successful result is validated against the same owner,
heap generation, and semantic boundary identity, then copied into a VM-owned
public value before actor-heap reclamation can occur. A raw managed reference is
never returned to the REPL, CLI, supervisor, or another actor.

An arbitrary `Managed(semantic_id)` value requires every reachable fixed
aggregate layout and collection schema in the admitted executable descriptor.
Descriptor format 1.4 stores unique canonical layout and collection tables
ordered by semantic identity and encoded metadata, plus the sorted finite atom
identity table. The loader decodes each bounded descriptor, verifies its
semantic identity and canonical bytes, and builds one immutable registry shared
by execution forks. Public entry/result conversion selects a
fixed aggregate layout by exact shape on input and exact physical fingerprint
on output. List, Map, and Set roots select one schema containing their exact
element, key, and value slot categories; their RRB and A-CHAMP node layouts
remain runtime-private. Conversion recursively handles aggregates, collections,
and sequence references under one explicit depth/work budget. Reference-valued
Map keys and Set elements use stable Terlan content equality and hashing rather
than heap-reference identity. Missing, ambiguous, cyclic, malformed, mistyped,
or unadmitted graphs are rejected atomically. Standalone atoms and Atom slots
inside aggregates and collections resolve through the same admitted table.
Unknown text and invalid local indexes fail atomically; Map and Set key
semantics use canonical atom text instead of generation-local indexes.

## 13. Aggregates, Unions, And Specialized Layouts

The default fixed aggregate layout places materialized fields in source order
using the target data layout. Whole-program optimization may reorder, erase,
split, scalar-replace, or vectorize a private aggregate that does not cross an
object, continuation, reflection, debug, persistence, hot-reload, or external
boundary. Every such decision changes its native fingerprint.

Fixed arrays use checked target stride and may select vector alignment.
Recursive edges use managed or shared references; inline infinite layout is
rejected.

Unions always have a stable semantic discriminant, but native layout MAY use:

- an explicit tag and payload;
- null, spare-bit, or alignment niches;
- pointer tagging;
- payload splitting or scalar replacement;
- specialized `Option` and `Result` representations.

The descriptor completely specifies the selected native representation and
precise reference map. Transport always uses the semantic discriminant and
active payload. Unknown tags and invalid niche bit patterns are rejected before
application observation.

## 14. Collections And Maps

Portable collections are Terlan values, not NativeResources. Their semantic
contract fixes immutability, equality, hashing, iteration order, message
behavior, and canonical encoding. Their physical representation remains
runtime- and compiler-private.

The implementation MAY choose persistent vectors, relaxed trees, hash-array
mapped tries, ordered trees, flat arrays, small inline forms, transient
uniquely-owned builders, or specialized primitive/vector storage. A compiler
may fuse pipelines or eliminate intermediates when behavior is unchanged.

Collection roots crossing separately compiled object calls use an opaque
managed or shared reference plus a matching descriptor. Nodes may not leak into
the compiled ABI. Randomized internal hash state MUST NOT affect observable
iteration, artifacts, replay, diagnostics, or canonical transport bytes.

### 14.1 A-CHAMP Ownership And The AOT Boundary

Terlan map semantics are language-owned. The compiler owns static typing,
purity, reachability, escape analysis, constant folding, fusion, scalar
replacement, and every decision that proves a map or intermediate map operation
need not be materialized. Once a portable map is materialized, its storage is
execution-shard-runtime-owned because its nodes participate in actor-heap or
shared-storage ownership, tracing, allocation accounting, collection, message
transfer, inspection, and failure handling.

The canonical materialized large-map implementation is the precompiled Terlan
runtime's adaptive CHAMP implementation. An AOT application image MUST NOT emit
an application-owned copy of the general CHAMP implementation, expose a CHAMP
node layout in a cross-object signature, or depend on Rust collection, `Arc`,
allocator, hasher, or enum layouts. Packaging the precompiled runtime with an
execution shard does not transfer ownership of that implementation to the
application image.

AOT-compiled code accesses a materialized map through direct, typed, in-shard
runtime entries. The runtime entry families include construction, construction
from entries, size and emptiness, lookup, containment, persistent put, remove,
take, and iteration. Their signatures use:

- an opaque `TvmRef` or eligible immutable `SharedRef` collection root;
- matching key and value type descriptors and native layout fingerprints;
- actor context for an operation that may allocate, collect, account memory, or
  reach a safepoint;
- caller-owned result storage and a typed status for failure-bearing results.

These entries are ordinary native calls inside the execution shard. They MUST
NOT use NativeBoundary transport, the supervisor control protocol, canonical
TVM serialization, symbol lookup on the common path, or a universal dynamic
dispatcher. Read-only lookup SHOULD use a non-allocating direct fast path when
the selected representation permits it. Persistent update MAY reuse uniquely
owned storage internally, but its observable input remains immutable.

The compiler MAY eliminate a map, scalar-replace it, specialize a fixed small
map, precompute a constant map, or remove intermediate collection operations
when the transformation preserves the semantic contract. If optimized code
materializes the portable map, it MUST rejoin the canonical runtime
representation through the typed runtime entries. A compiler intrinsic MAY
inline a versioned validated fast path only when its slow path performs the same
semantic operation and its representation dependency participates in the
native layout fingerprint and incremental cache key; application code never
inspects CHAMP nodes directly.

Hashing used for a portable map MUST be defined by the matching Terlan type
descriptor and remain stable wherever language semantics, replay, persistence,
canonical transport, or deterministic diagnostics require stability. A host
language's default hasher is not a Terlan ABI or semantic definition.

Actor-local CHAMP nodes use managed-heap ownership and MUST NOT rely on ordinary
atomic strong or weak reference counting. Deeply immutable compiler-approved
nodes MAY use `SharedRef` under section 11. Existing flat-vector, bucket-index,
host-hasher, or reference-counted map implementations are migration profiles;
they do not define Native ABI 1 and must not leak into emitted `.tvm` images.

### 14.2 Adaptive RRB List Profile

Terlan `List[T]` is a portable ordered persistent sequence. Its semantic
contract does not expose linked cons cells, Rust `Vec`, an array address, a tree
branching factor, or another physical representation. Native ABI 1 uses an
adaptive relaxed-radix-balanced (RRB) vector as the canonical materialized
general-list profile because Terlan lists require efficient length, indexed
access and update, append, concatenation, splitting, head/rest traversal, and
iteration rather than only front insertion.

The initial adaptive representation has three forms:

1. one canonical empty-list value;
2. a compact inline leaf for lists of at most eight elements;
3. a 32-way RRB tree with packed leaves of at most 32 elements, cumulative size
   tables on relaxed nodes, a front cursor, and an append tail.

The thresholds and node bytes remain runtime-private. Changing them changes the
managed-layout profile and relevant cache fingerprints, not `List[T]` semantics.
The runtime MAY select a different measured threshold or branching factor after
conformance and benchmark evidence without changing the language ABI.

#### 14.2.1 Required Focused Node Structure

The canonical RRB implementation MUST distinguish regular nodes from relaxed
nodes. A regular node contains fixed-radix child slots and derives its child from
the index without storing cumulative sizes. A relaxed node contains only its
occupied child slots plus the cumulative subtree sizes required after
concatenation, split, insertion, deletion, or uneven construction. A regular
path MUST NOT pay the memory load, search, or storage cost of a relaxed size
table.

One materialized large-list root records its logical length and has four
conceptual components:

1. a front leaf and offset for head/rest and front-local operations;
2. a focused root-to-leaf path for spatially local access;
3. the regular or relaxed RRB root;
4. a rear leaf for append-local operations.

The physical encoding MAY merge, omit, or scalar-replace these components when
empty or redundant. Both affix leaves contain at most the selected leaf capacity
and MUST rejoin the canonical tree through the same balancing invariants as
ordinary leaves. Repeated front-local or rear-local operations SHOULD avoid a
root traversal until the active affix or focus is exhausted.

The focused path is an optimization cache, not an ownership escape hatch. Every
cached managed reference is a precise traced root or field. A cached derived
address MUST be invalidated before a relocating safepoint and reconstructed from
its traced base and checked indexes afterward. No untraced raw node pointer may
survive allocation, collection, actor park, migration, or an unknown native
call.

Regular and relaxed nodes MUST maintain a bounded-height RRB occupancy invariant
after construction, concatenation, split, and persistent update. Underfull
boundary nodes MAY exist where the RRB algorithm permits them; repeated
operations MUST NOT accumulate an unbounded rope-like chain or turn indexed
access into a function of concatenation history. Rebalancing copies only the
affected fringe and preserves all unaffected shared subtrees.

Relaxed size tables SHOULD use the narrowest validated unsigned width capable of
representing the subtree. Width, promotion, overflow checks, and table layout
participate in the native layout fingerprint. Overflow is a typed failure before
allocation or publication; it MUST NOT wrap, truncate, or partially update a
list.

RRB nodes and leaves use the ownership rules of section 14.1: actor-local nodes
are managed by `TvmRef`, and eligible deeply immutable nodes MAY use `SharedRef`.
A materialized list root crossing a native-object call is opaque. An application
image MUST NOT inspect RRB nodes or depend on their height, size-table encoding,
tail placement, element stride, or allocation layout.

The runtime provides direct typed in-shard entry families for empty and
from-elements construction, length, emptiness, first, rest, concatenation,
subtraction, iteration, append/push, clear, indexed lookup and update, and swap.
Entries use the list root, element type descriptor, native layout fingerprint,
actor context when allocation or a safepoint is possible, caller-owned result
storage, and typed status as required. They do not use NativeBoundary transport,
the supervisor, TVM serialization, or a universal dynamic dispatcher.

The expected operation profile is:

- constant-time length and emptiness;
- effectively constant `O(log32(n))` indexed lookup and persistent update;
- amortized constant-time append through the append tail;
- logarithmic concatenation, split, and structural rebalance;
- linear leaf-wise iteration with no per-element runtime dispatch;
- constant-time first/rest while advancing inside the focused front leaf, with
  one logarithmic trim or refocus when traversal crosses a tree boundary.

`rest` MAY return a structurally shared front-offset view, but an implementation
MUST bound retention of excluded content. Crossing a complete leaf or a measured
retention threshold trims the excluded prefix from the retained root so a short
suffix cannot indefinitely retain an otherwise dead large list. Older live list
versions remain valid through ordinary persistent structural sharing.

When ownership analysis proves a list uniquely owned and unpublished, the
compiler MAY request a transient edit token. The runtime MAY then update its
tail or uniquely owned paths in place while preserving actor ownership and
memory accounting. The token is actor- and generation-bound, cannot be stored in
a Terlan value or cross a safepoint that invalidates uniqueness, and MUST be
consumed when the list is frozen, published, returned through an unknown call,
captured, or sent. Failure to prove uniqueness uses persistent path copying.

The compiler SHOULD construct list literals and comprehensions through one
bounded transient builder rather than repeated persistent appends. It MAY
eliminate or scalar-replace fixed local lists and SHOULD compile traversal into
native leaf-wise loops. Homogeneous primitive lists MAY use unboxed specialized
leaves when the element descriptor and layout fingerprint identify that form;
generic or aggregate elements use descriptor-governed slots and precise
reference maps.

Leaf-wise AOT traversal MUST hoist representation validation, bounds checks, and
runtime dispatch out of the per-element loop whenever the static element and
list profile make that legal. `map`, `filter`, `fold`, comprehensions, equality,
hashing, copying, and message transfer SHOULD consume whole leaves or typed
chunks. A result-producing traversal SHOULD use one uniquely owned transient
builder and freeze once rather than publish a persistent version for every
element.

Actor-local nodes, affixes, and leaves MUST allocate through the actor managed
heap and its nursery/large-object policy. Allocation, path copying, transient
mutation, and focus changes remain charged to the owning actor. Specialized
primitive leaves contain no boxed dynamic values; reference-bearing and
aggregate leaves publish exact descriptor-derived reference maps before the
first safepoint that can observe them.

The canonical general-list profile MUST NOT require one separately allocated
cons cell per element. A compiler-private temporary cons chain is permitted only
when it is eliminated or frozen into the canonical list representation before
crossing a call, continuation, inspection, message, persistence, or hot-reload
boundary. Existing `Vec<ReplValue>` storage and copying `rest`, `concat`, or
`push` helpers are interpreter-migration representations; they do not define
Native ABI 1 and must not leak into emitted `.tvm` images.

#### 14.2.2 Representation Evidence Requirements

The 32-way tree and eight-element inline threshold are the initial profile, not
an unmeasured permanent claim. Before managed-layout profile 1 freezes, the same
implementation and workload harness MUST compare at least 16-, 32-, and 64-way
internal nodes, relevant inline thresholds, and type-specific leaf capacities.
The selected profile records its benchmark identity and layout fingerprint.

The representation benchmark matrix includes:

- empty, inline, one-leaf, multi-level, relaxed, and very large lists;
- primitive, managed-reference, and fixed aggregate elements;
- construction, transient construction, first/rest traversal, append, indexed
  lookup and update, split, concatenation, equality, hashing, and iteration;
- repeated concatenation and split histories, adversarial boundary occupancy,
  and front-view retention;
- warm steady-state cost, allocation bytes, copied bytes, live retained bytes,
  collection assists and pauses, and actor-message copy or transfer cost;
- AOT-generated literal, comprehension, and fused traversal consumers rather
  than runtime-helper microbenchmarks alone.

One profile wins only by a declared multi-workload policy; a single favorable
microbenchmark cannot establish the default. Threshold changes remain
runtime-private but require updated deterministic benchmark evidence and cache
fingerprints. Published performance claims identify the exact target, element
profile, compiler optimization profile, and workload distribution.

## 15. Message Transfer And Mailboxes

Same-shard send does not serialize through TVM transport. It uses a typed
compiler/runtime transfer plan selected from:

1. direct copy of scalars and small immutable graphs into receiver-owned
   mailbox fragments;
2. graph copying using descriptor-generated copy functions;
3. ownership transfer for a compiler-proven unique graph that the sender can no
   longer access;
4. retention of `SharedRef` bulk storage while copying the small envelope.

The receiver MUST never observe an actor-local reference into the sender's
heap. Publication to the mailbox is atomic: transfer validation and memory
reservation complete before the envelope becomes visible. Failed send leaves
both heaps and the mailbox logically unchanged.

The initial direct-copy implementation inventories the source graph
iteratively under an explicit byte-work budget, copies children before their
parents, and maps each distinct source object to exactly one receiver object.
It validates the root semantic identity and every edge through the precise
reference map and actor-heap ownership metadata, preserves sharing, and creates
a precise receiver-local mailbox root. Copying occurs in a staged receiver heap
and commits only after the complete graph succeeds, so invalid ownership, root
type, budget, or heap-limit failures cannot partially publish.

Generated typed Send and Receive frames carry a canonical three-word boundary
identity ahead of the payload. Source intrinsics expose immutable `String`,
Bytes, Binary, and finite Atom through this frame. For every managed-reference
type, Send copies the immutable graph directly into receiver-owned storage and
registers its precise root in the shared execution shard. The actor mailbox
contains an opaque fragment token plus exact boundary identity, never a sender
reference or public-value encoding. Receive selects only an exact identity
match, validates the receiver-local root, resumes native code with that word,
and releases the mailbox root only after continuation captures are parked.
Copy, accounting, validation, cancellation, or resume failure cannot publish a
cross-owner reference. The frame and mailbox transaction are
representation-neutral. Explicitly specialized
`Process.send_value[T]` and `Process.receive_value[T]` calls retain the concrete
aggregate or collection identity as `Managed(id)` metadata without
materialization through `ReplValue`. Omitting the explicit concrete type
argument is a compile-time error because the transition ABI never accepts an
erased or dynamically inferred managed identity.

Mailboxes are multi-producer, single-consumer queues. Producers may use atomic
queue operations, but they do not acquire the receiver's actor-heap mutator
token. The receiving actor integrates or consumes fragments at a safe point.
Selective receive preserves ordering and save-queue semantics without copying
unmatched messages repeatedly.

Cross-shard or remote sends use canonical TVM transport encoding, or a
versioned zero-copy shared-storage capability for eligible bulk payloads.

## 16. Functions, Closures, And Continuations

Statically resolved calls use native symbols or direct addresses internal to an
image generation. Persisted or transported function identity is a stable
numeric function identity plus image generation, never a code pointer.

A closure semantically contains image generation, function identity, and an
owned environment. Its physical layout may be specialized or eliminated. A
closure environment is traced actor-local storage unless all captures qualify
for shared immutable storage. Borrowed captures cannot outlive their proven
native scope.

An actor function that may park is AOT-lowered into native entry/resume points
and a typed managed continuation. Values live across a park are described by
precise continuation metadata; the native stack itself is never persisted.
Ordinary scheduler yield within the shard passes a continuation reference
directly to the runtime. Only cross-shard migration, durable persistence, or
hot-reload conversion uses pointer-free encoding.

The first executable transition proof is a deliberately smaller direct-shard
profile: a `std.vm.Process.yield_now/0` entry returns a stable numeric
continuation ID to the execution-shard runtime and writes bounded owned
`Int`/`Bool` captures into a caller-owned output buffer. Buffer capacity comes
from the admitted continuation descriptor and is checked before the native
entry runs. A matching resume dispatches a separate native entry with those
values and either produces the declared scalar result or yields the next
declared continuation. Linear functions may contain multiple suspension points.
Their stable identities include the source-order ordinal, and every point owns
a separately minimized capture signature; adjacent yields therefore require no
synthetic source local. This validates compiler splitting, pointer-free
identity, owned scalar capture, admission, typed protocol handling,
stale/wrong-type/cycle rejection, repeated transition driving, and consumer
execution.
The transport envelope carries a nonzero VM owner identity independently from
its request and stable continuation identities. Pending state is keyed by all
three fields, so a foreign actor cannot resume another owner's continuation
even when it knows the request and continuation numbers.
At the VM scheduler boundary the numeric owner is resolved to a live
`VmProcessId`. One process may own one parked native continuation, the exact
owner/request/continuation triple is required to resume it, and process exit
releases the lease. This registry is the scheduler-side ownership primitive;
the direct `.tvm` runner now returns a scheduler-visible suspension after
parking and does not send Resume until a separate exact-owner resume step
requeues the actor. Non-Yield operations still need the same integration.
The envelope also separates operation arguments from continuation captures.
This prevents send/spawn/timer/resource inputs from being mistaken for owned
resume values and permits each side to receive independent type and arity
validation. The initial Yield operation has zero operation arguments.
`Unit` is also admitted with the single canonical boundary word `0`. A direct
exported `yield_now/0` can therefore park with no captures and resume as its
declared `Unit` operation without a synthetic Int/Bool wrapper; nonzero Unit
words fail boundary validation. Pure scalar `let` prefixes use backward
free-variable selection so only locals live in the resume body appear in the
continuation descriptor and transport values. `Unit` may also be an export
parameter/result and an owned continuation capture. Its descriptor type
remains distinct from `Int`; initial calls and resumes reject any nonzero Unit
word before native execution. Pure-condition `if` regions may select pure or
suspending
bodies independently. Nested and repeated branch resumes remain terminal native
entries, and the entry buffer uses the maximum arm capture width while each
descriptor retains its exact zero-, one-, or multi-value signature. A branch
condition may suspend without exposing an intermediate Boolean to the VM: its
native continuation finishes the condition and the remaining ordered clauses.
The admitted scalar profile preserves source evaluation order through unary and
eager-binary wrappers only along their first-evaluated operand spine. It also
admits a suspending selected right-hand side of `and` or `or` when the left side
is scalar, so an unselected right side produces no transition. Nested lazy
condition composition is limited to eight right-hand decisions in this proof
profile. Earlier scalar work for an eager right-side suspension is materialized
before the transition and carried through typed captures. A non-linear `or`
condition may select a suspending body before or after its right-side condition
resumes. Checked failures before suspension are returned before any transition;
resumed false conditions continue ordered fallthrough and preserve no-match
failure.

Direct `yield_now` composition is not condition-only: unary and eager scalar
value expressions and any argument of a scalar native call use the same typed
prefix capture layout. Checked left-side or earlier-argument work completes
before suspension, later arguments remain unevaluated until resume, and the
continuation returns the final value.

A suspending native function may be called in tail position. Fixed-point
classification propagates suspension and maximum capture capacity through
direct, transitive, and branch-selected tail calls. The internal ABI forwards
the selected continuation's exact capture length through caller-owned output,
so it does not confuse a zero-capture path with a sibling of larger capacity.

The bounded non-tail profile composes a callee with one to eight proven-linear
suspension stages and no suspending callee of its own. It requires one statically
known initial continuation and one guaranteed next continuation at every
nonterminal stage. Immediate completion remains inside the caller entry. If the
callee yields, native code verifies its expected stable continuation identity
and exact capture count, appends only caller scalars live after the call, and
returns the corresponding distinct stable caller continuation.

Each intermediate wrapper rewrites the callee's next stable suspension to the
next caller wrapper and carries caller-live scalars forward. Callee temporary
indices are rebased when caller parameters are appended, so computed locals and
caller captures cannot alias. The terminal wrapper inlines the terminal callee
resume and then evaluates the saved scalar continuation; no wrapper stores a
native stack or code pointer. Supported evaluation contexts are the
first-evaluated operand spine of unary and eager binary expressions, any single
argument of a pure call, any sequential `let` binding or body, selected branch bodies, and
branch conditions. Earlier call arguments are materialized and checked before
suspension; later arguments remain after resume. Sequential scalar `let`
bindings before the suspending call are materialized under the same rule. A tail
caller may forward the composed wrapper chain unchanged.

Eager binary expressions with a direct right-side `yield_now` or proven-linear
suspending callee first evaluate and check the scalar left operand, then carry
its exact result beside other live values in the declared continuation
signature. Resume completes the eager operation without recomputing the left
operand. Up to eight proven-linear suspending calls may compose in one scalar
expression. The completed path may enter another `CallThen`; resumed wrapper
paths use the same stable nested continuation IDs and typed capture signatures.
Ambiguous callee branch graphs, chains deeper than eight, and a ninth suspending
call in one expression remain outside admission. This profile also does not yet
satisfy the final same-shard actor fast path, managed
captured-value layout, scheduler parking, or full actor-transition requirements
in sections 16 and 21.

Direct `yield_now` regions and proven-linear `CallThen` regions may compose in
either order. The compiler selects the call region first when both appear in one
expression, then lowers the direct-yield resume, so source-order prefix work and
typed live captures are never reordered.

## 17. Dynamic And Term Values

`Dynamic` and `Term` are explicit language forms for heterogeneous data. They
do not force statically typed calls into a universal boxed representation.

Their native representation is descriptor-driven and MAY inline common
scalars, use tagged words, or refer to managed storage. The exact tag and word
layout belongs to the managed-layout profile and fingerprint. It is not frozen
as a 32-byte public structure.

Dynamic dispatch uses immutable descriptors or compiler-generated tables, not
source names, mutable global registries, Rust `Any`, or interpreter expression
tags. Transport encodes semantic type identity plus the typed payload.

## 18. Actor And Resource Identities

Logical identities are pointer-free and generation checked. A local actor
identity contains VM or shard identity, actor identity, generation, and typed
capability information. A continuation identity additionally binds image and
actor generations. A `NativeResource` identity binds resource kind, owner,
generation, lifecycle state, and capability.

The exact in-shard packed representation may be optimized, but the canonical
transport record and its field widths are independently versioned. No identity
contains a heap address, code pointer, OS handle, thread identifier, or
allocator slot without a generation check.

Every operation rejects stale, foreign, wrong-owner, wrong-kind, disposed, or
unauthorized identities before state mutation. Distributed identities require
an explicit node/cluster profile; local programs do not pay for remote routing
fields on every fast path.

## 19. Generic Code

Go-class compilation speed prohibits uncontrolled specialization. Generic
lowering uses shared descriptor-driven code where representation permits,
finite scalar and vector specializations, and deterministic profile-guided or
compiler-budgeted specialization.

The compiler publishes the budget and content-addressed decision key. Source
order, parallel scheduling, cache warmth, and hash iteration cannot change the
decision. Hot paths MAY receive additional specialization only from an explicit
reproducible profile input.

## 20. Native Call ABI

Calls inside one compilation unit may be inlined or freely optimized. Calls
between independently cached native objects use the versioned
`terlan-native-v2` classifier:

- `Unit` has no payload;
- primitive scalars and managed/shared references pass directly;
- target-supported small trivial aggregates may use registers;
- larger, over-aligned, dynamically represented, or failure-bearing values use
  caller-owned result storage;
- ownership and borrow mode are part of the signature;
- no native unwinding crosses a Terlan or runtime boundary.

The target-specific classifier, register assignment, stack alignment, and
layout fingerprints are normative generated tables and executable fixtures,
not prose assumptions. Cross-object link rejects a signature or fingerprint
mismatch.

Stable runtime entries use a narrow C-compatible wrapper:

```text
status = entry(actor_context, argument_storage, result_storage)
```

The format-1 generated dispatch entry receives the actor-runtime context and
managed allocator callback as hidden leading arguments. Source parameters do
not include either value, and every generated native-to-native call forwards
them unchanged. Managed construction calls the allocator with this exact
shape:

```text
status = allocate(
  actor_context,
  immutable_layout_bytes,
  layout_byte_count,
  field_word_storage,
  field_count,
  managed_reference_result
)
```

The callback MUST allocate through the actor identified by `actor_context`,
validate the complete descriptor and field shape before publication, and write
one nonzero opaque reference word only on success. Generated code propagates a
non-success status, rejects an absent callback, and rejects a zero reference
returned with success. Scalar-only entries may receive null runtime arguments
because they never call the managed allocator.

The compiler removes a constructor allocation before layout inventory and
callback lowering only when reverse lexical liveness proves its result dead and
all recursively evaluated fields allocation-only and effect-free. Calls,
intrinsics, and unrecognized expression forms conservatively retain source
evaluation. Removing an allocation compacts subsequent local indexes without
changing lexical shadowing. Live constructors keep their canonical descriptor
through native calls and suspended continuation captures unless a separate
field-use proof establishes that the aggregate identity cannot escape. The
current proof scalar-replaces a fixed constructor used only by direct named
field projections into one local per source field. It evaluates every field
exactly once in source order, including unprojected fields. Aggregate uses,
unknown projections, and refutable patterns retain the canonical managed
allocation. Statically irrefutable tuple and exact-constructor patterns,
compile-time in-bounds tuple and fixed-array indexes, and private single-clause
projection-only helpers participate in the same scalar replacement contract.
The helper specialization preserves argument evaluation once, rewrites every
direct use transactionally, and removes the managed function ABI only after an
unresolved-use audit. Public helpers, recursive helpers, function references,
and unsupported argument shapes retain the canonical managed call. These
proofs are independent of dead-allocation elimination.

The shared managed execution runtime binds `actor_context` to stack-scoped
dispatch state and lazily owns exactly one heap for each nonzero actor owner.
The ordinary direct backend owns this runtime inside the execution shard;
independent backend forks share immutable code and descriptors but never heaps,
pending roots, allocation contexts, or managed references. Context and
allocator pointers are valid only during one synchronous dispatch.
Descriptor-directed word decoding admits only canonical Unit, Bool, finite
Float, bounded Atom, Int, and same-owner managed-reference fields. Failure
leaves caller result storage untouched and publishes no partial object. A
successful managed result is validated against its owner, live heap generation,
and admitted semantic result identity before it leaves native dispatch. The
isolated external worker has no Terlan image dispatch or managed runtime. Its
bounded capability protocol carries owned adapter terms and opaque resource
handles, never `TvmRef`, actor-heap metadata, or application results.

Suspension does not serialize actor-heap references. Before publishing a
scheduler-visible transition, the direct backend partitions continuation
captures using the declared parameter types. Unit, Int, finite Float, and Bool
captures remain in transition state; String, Bytes, Binary, and typed managed
captures become owner-scoped `ManagedContinuation` roots inside the shard.
Resume supplies only injected runtime results plus scalar captures. The backend
validates the exact request, owner, continuation, scalar shape, managed semantic
type, and live heap generation before reconstructing generated parameter order.
Moving collection updates retained roots, so restoration uses relocated
references and rejects stale, foreign, missing, or mistyped captures. External
capability workers never park or restore Terlan continuations.

Pointers in this wrapper are validated shard-local addresses and never cross an
OS-process boundary. Typed failures use result storage or runtime failure
records. A panic, C++ exception, SEH exception, or foreign unwind that reaches
the wrapper terminates the responsible unsafe worker or execution shard and is
converted by the supervisor; it never unwinds through TVM runtime frames.

## 21. Scheduler And Runtime Fast Paths

Spawn, local send, receive scan, reduction polling, yield, timer registration,
links, monitors, and actor exit use typed native runtime calls within the
execution shard. They MUST NOT require canonical serialization, control-plane
IPC, symbol lookup, or a universal dynamic dispatcher on their common path.

The runtime may implement validated compiler intrinsics for the hottest paths,
provided slow paths share the same semantic operation and inspection trace.
Scheduler queues use work stealing or another measured M:N policy. Fairness is
defined by reduction and latency guarantees, not host-thread scheduling luck.

Blocking I/O and native work park the actor rather than a scheduler thread.
Completion resumes only the matching live actor and continuation generation.

## 22. TVM Transport Encoding

TVM transport is canonical binary data, not bytecode and not a sequence of
Terlan instructions. It is used only when a value crosses an isolation,
persistence, inspection, migration, or network boundary that cannot share
managed references safely.

Frames are bounded and include protocol, request, image, actor, continuation,
operation, capability, and typed payload identities as needed. Scalar encoding
is fixed little-endian. Statically typed payloads omit redundant tags; structs
use semantic field order, unions use semantic discriminants, and collections
use canonical element order.

Closures, managed pointers, borrowed slices, descriptor pointers, OS handles,
threads, exceptions, and arbitrary platform structs are not transport values.
Eligible bulk storage crosses by bytes, chunks, or a separately authenticated
shared-memory capability.

Decoding is atomic. Invalid UTF-8, unknown types or atoms, non-finite floats,
invalid discriminants, duplicate keys, overlong lengths, arithmetic overflow,
stale identities, trailing bytes, and ownership violations reject the entire
frame before actor or resource state changes.

## 23. External NativeBoundary

Generated Rust, C, C++, CUDA, and package adapters convert between Terlan values
and declared external ABI values. External aggregate layout is never inferred
to equal a Terlan layout.

Unsafe or memory-unsafe adapters execute in supervised NativeBoundary workers
outside the execution shard by default. Calls are capability checked, bounded,
backpressured, cancellable according to manifest policy, and attributed to the
owning actor. Worker crash, abort, timeout, invalid response, panic, or exception
becomes a typed actor-visible failure and resource cleanup event without
corrupting actor heaps or scheduler state.

An adapter defines conversion, ownership, allocation/deallocation authority,
encoding, thread affinity, blocking behavior, cancellation, error conversion,
resource creation, transfer, and disposal. Raw pointers, C++ references,
inferred lifetimes, Rust references, unresolved templates, exceptions, and
overload ambiguity do not cross the boundary. A future trusted in-shard adapter
profile requires explicit unsafe capability and separate conformance; it is not
ABI 1 default behavior.

## 24. Constants And Read-Only Data

Scalars and immutable aggregates MAY be embedded in native read-only sections.
Managed constants are copied or mapped into the appropriate actor/shared domain
without exposing immortal-header assumptions. Read-only constants contain no
actor-local mutable state, continuation, resource, OS handle, timestamp, cache
path, or nondeterministic address identity.

## 25. Hot Reload And Migration

Native memory is never reinterpreted under a different layout fingerprint.
Existing actors may continue on their admitted image generation while new
actors enter the new generation. Direct compatible migration is allowed only
when generated conversion code proves semantic compatibility and constructs
valid destination objects.

Cross-layout, cross-shard, durable, and user-defined migrations use typed
semantic conversion, optionally through canonical transport. They do not copy
heap headers or native object bytes. Function, closure, actor, continuation,
resource, and shared-storage generations are independently validated.

## 26. Debugging And Observability

Native debug metadata maps code, safepoints, stack maps, continuations, type
identities, optimized locations, and runtime events to Terlan source. Inspection
runs at safepoints and uses descriptors; it does not conservatively scrape
arbitrary memory.

The runtime exposes per actor allocation, live bytes, young/old collection
counts, pause and assist time, mailbox fragment bytes, shared-retained bytes,
reductions, run-queue delay, and failure cleanup. Release benchmarks report
median and tail behavior rather than throughput alone.

## 27. Rejection Requirements

The compiler, linker, loader, runtime, transport decoder, or adapter rejects:

- target, feature, calling-convention, or ABI mismatch;
- semantic identity collision or native fingerprint mismatch;
- invalid size, alignment, aggregate, union, or reference-map metadata;
- missing or inconsistent precise stack maps at a safepoint;
- a foreign, stale, unaligned, or cross-actor managed reference;
- an actor-local reference stored in shared memory or another actor's mailbox;
- borrowed data retained across a park, collection, or unknown call;
- native address used as actor, closure, continuation, or resource identity;
- transport containing a native pointer, closure, OS handle, or trailing data;
- unbounded or nondeterministic specialization;
- finalizers used for observable NativeResource lifecycle;
- native unwinding across a runtime frame;
- hot-reload byte reinterpretation;
- unsafe native execution admitted into the shard without its explicit profile;
- generated Rust, C, or C++ application code presented as direct ABI 1 object
  emission.

Validation and ownership transfer are atomic before observable publication
wherever possible.

## 28. Conformance And Performance Gates

### 28.1 Current 0.0.7 Pre-Freeze Gate

ABI 1 remains the current implementation only while all of these invariants
are executable and fail closed:

- one canonical descriptor codec and identity model is shared by compiler
  emission and VM admission;
- descriptor, target, ABI, signature, fingerprint, stack-map, reference-map,
  capability, and ownership mismatches reject before image publication;
- malformed admission leaves no executable generation, actor state, resource,
  or partial value visible;
- C, C++, and Rust bindings reject unknown ownership, escaping references,
  unresolved lifetimes, missing disposal authority, ambiguous calls, and
  uncontained unwinding;
- unsafe external adapters remain process-isolated by default;
- descriptor bytes, recursive conversion, transport frames, requests, worker
  credits, retained resources, and cancellation work have explicit limits;
- adversarial cases cover successful counterparts plus malformed metadata,
  forged or stale handles, cross-owner access, worker failure, panic,
  exception, timeout, late reply, cancellation, and cleanup;
- release evidence records throughput and p50, p95, p99, and p99.9 latency for
  actor and NativeBoundary workloads without using performance as a substitute
  for correctness.

A failed invariant removes the corresponding safety or support claim; it must
not silently downgrade ABI validation or select an in-shard unsafe path.

Before native ABI 1 or managed-layout profile 1 freezes, executable gates prove:

- direct object compilation and calls on x86-64 and AArch64;
- exact primitive and target classifier behavior;
- adaptive RRB semantic equivalence across empty, inline, regular, relaxed,
  focused, transient, sliced, and specialized-leaf forms;
- adaptive RRB structural invariants, persistent-version isolation, bounded
  front-view retention, transient-token invalidation, precise relocation of
  focused paths, and deterministic typed failure under size overflow;
- AOT list literals and leaf-wise list traversal through the native image with
  no interpreter or per-element universal dispatch;
- the required adaptive RRB representation benchmark matrix and recorded
  managed-layout selection evidence;
- precise stack-map root enumeration and relocation under optimized code;
- stack allocation and scalar replacement of non-escaping values;
- actor-local young collection, promotion, compaction/fragmentation bounds, and
  whole-heap exit reclamation;
- no atomic reference counting on ordinary actor-heap objects;
- safe shared immutable retention and cleanup under concurrent actor churn;
- copy, unique transfer, shared-buffer, same-shard, and cross-shard message
  paths without cross-actor managed references;
- bounded scheduler latency during allocation, collection, selective receive,
  native calls, and I/O;
- actor-local typed failure isolation and attributed shard-crash recovery;
- stale continuation/resource rejection and hot-reload conversion;
- canonical transport round trips and malformed-frame matrices;
- isolated Rust/C/C++ adapter crash, cancellation, and cleanup behavior;
- a Terlan consumer end to end with no serialized VMIR, interpreter, JIT, or
  secondary application compiler.
- the complete already-ported Erlang-derived semantic corpus through the native
  TVM path with unchanged source fixtures, assertions, expected outcomes, and
  public test identities; only the classified runner/artifact harness may
  change.
- every existing JavaScript lowering, golden, runtime, declaration, source-map,
  browser, and server gate unchanged; shared-IR changes prove byte-for-byte
  stability where promised and behavioral equivalence elsewhere.

The benchmark gate compares a pinned Terlan build with a pinned supported
Erlang/OTP build on identical hardware and workload definitions. It records at
least process memory, spawn throughput, scheduler fairness, local message
latency/throughput, selective-receive scaling, small-object allocation,
per-process GC pause p50/p95/p99/max, large-binary fan-out, actor-crash cleanup,
and mixed-workload tail latency. Any accepted regression requires an explicit
owner, explanation, expiry, and release waiver; a marketing claim is not
conformance evidence.

Metadata-only fixtures, mocked calls, JSON round trips, Rust layout assumptions,
and interpreter execution are insufficient.

## 29. Freeze Boundary

ABI 1 uses the following lifecycle:

1. **current-pre-freeze**: sole implemented ABI; normative semantics with no
   cross-release binary-compatibility promise.
2. **release-candidate**: layouts and metadata are locked while cross-target,
   adversarial, fuzz, sanitizer, and performance gates run.
3. **frozen**: the declared compatibility range accepts conforming ABI 1
   objects and rejects incompatible inputs with stable diagnostics.
4. **deprecated**: loading remains available only for an explicit migration
   period with replacement guidance.
5. **rejected**: loaders fail before admission and identify the required
   supported ABI.

Terlan 0.0.7 MUST remain at current-pre-freeze. A later release may enter
release-candidate or frozen status only through the complete gate set in this
section; changing a status label without that evidence has no normative effect.

ABI 1 freezes observable compiled-object and runtime-entry behavior only after
the direct backend, two target classifiers, moving-root tests, actor heap,
message transfer, scheduler, NativeBoundary isolation, hot reload, and
comparative benchmark gates pass.

The following remain runtime-private across ABI 1: object header bytes, GC mark
and forwarding bits, generation layout, allocation regions, collection nodes,
mailbox queue nodes, shared-storage reclamation metadata, dynamic-value packing,
and optimization thresholds. A change to them requires a managed-layout profile
or dependency-fingerprint change, not a language ABI change, unless it alters
compiled-object assumptions.

Changing a frozen scalar representation, call classifier, ownership mode,
stack-map contract, semantic transport field, or identity meaning requires an
explicit compatible extension or a new native ABI major version.

## 30. Architectural Lineage

Terlan treats Erlang/OTP behavior and test history as a semantic and adversarial
reference, not as an implementation constraint. The actor-local heap and
large-binary split build on the process isolation demonstrated by ERTS. Precise
safepoints, relocation maps, escape analysis, native specialization, and
isolated unsafe workers are the optimization layer made possible by Terlan's
typed direct-AOT compiler.

Primary background references:

- Erlang system documentation, process memory and garbage collection:
  <https://www.erlang.org/docs/17/efficiency_guide/processes.html>
- Erlang system documentation, reference-counted large binaries:
  <https://www.erlang.org/doc/system/binaryhandling.html>
- Cranelift native object emission:
  <https://docs.rs/cranelift-object/latest/cranelift_object/struct.ObjectModule.html>
- Cranelift user-defined safepoints and stack maps:
  <https://docs.rs/cranelift-codegen/latest/src/cranelift_codegen/ir/user_stack_maps.rs.html>
- Go garbage-collector guide, including escape-analysis implications:
  <https://go.dev/doc/gc-guide>
- Bagwell and Rompf, *RRB-Trees: Efficient Immutable Vectors*:
  <https://rtheunissen.github.io/bst/docs/references/2012_bagwell_rompf.pdf>
- Stucki, Rompf, Ureche, and Bagwell, *RRB Vector: A Practical General Purpose
  Immutable Sequence*:
  <https://rtheunissen.github.io/bst/docs/references/2015_stucki_rompf_ureche_bagwell.pdf>
