# Native compiler and runtime optimizations

These optimizations improve Terlan's existing managed actor model without
changing language syntax, persistent collection semantics, or image formats.

## Executable optimization contracts

`scalar_replacement_differential_test.rs` generates 96 deterministic constructor
programs. Each is lowered with scalar replacement enabled and disabled, emitted
to a native object, linked, and executed with the real managed allocator and
resolver under development, serving and release code-generation policies.
Both executions must match an independent scalar oracle, including exact
division-by-zero, overflow and unmatched-branch statuses. Cases include unused
fields and lexical shadowing. Optimizable projections must also contain no aggregate
allocation descriptors. Behavioral equivalence alone cannot satisfy that check.

Public-source fixtures additionally pass through parsing, interface resolution,
type checking, application lowering, native linking and execution. These tests
are ordinary workspace tests, so the existing precommit workspace test command
discovers them without a second hook-specific test inventory.

These regressions exposed two compiler bugs. Dead constructor elimination
treated checked arithmetic as harmless and could suppress its failures when the
result was discarded or shadowed. It now retains expressions without a totality
proof, including arithmetic and conditional fallthrough. String concatenation
also lowered an empty argument as `List[Never]` instead of applying the
consumer's `List[String]` type. Its intrinsic now uses the existing typed-value
adaptation and inventories the resulting collection schema.

## Construction into final storage

RRB empty roots, inline roots, tree roots, leaves and internal nodes initialize
their final actor-heap slots. They no longer allocate a temporary byte vector
for each node and copy it into the heap. Reference maps use bounded stack-backed
buffers. All element types and reference owners are validated before publication;
the canonical allocator installs reference slots and retains its rollback rules.

Bulk list construction runs inside one allocation transaction. Failure in a
later leaf or allocation rolls back every earlier node of that construction,
while retaining the caller's preexisting heap objects. Tests cover malformed
late elements, heap limits, retry and moving collection of reference lists.

This does not extend borrowed initialization storage across a safepoint or
suspension. The runtime remains responsible for materialized list storage.

## Compact immutable batches and traversal

Image-local atom tables store all names in one UTF-8 buffer with 32-bit start/end
offsets. The constructor validates identities, canonicalizes ordering and
duplicates, and checks byte-offset arithmetic before reserving the buffer.
Offsets are private and constructed at character boundaries; lookups and
iteration borrow the immutable owning table. Cloning copies two buffers without
pointer relocation. Sorted atom ordinals and serialized image metadata remain
unchanged. The index storage is eight bytes per atom on the native target,
compared with the previous sixteen-byte boxed-string entry, and the retained
buffer count is two rather than one per atom plus the index array.

String-list concatenation gathers an array of checked managed string references.
It does not clone each fragment into a host `String`. Small local inputs copy
directly from heap ranges to the final result. Larger results, or inputs using
external storage, copy into the existing immutable shared byte representation.
Singleton concatenation reuses the validated immutable string.
The copy phase borrows validated text views rather than resolving every managed
reference again. Those views are consumed before the result allocation can
mutate the heap. The external-string hard-limit check includes both the aligned
header and its text storage; rejection leaves the heap unchanged.

List materialization and string batching walk leaves in order. Root and leaf
bounds are checked at their respective boundaries instead of restarting an RRB
lookup for every element. The traversal borrows the heap immutably, preventing
collection or mutation while its ranges are in use. Node heights, child totals,
relaxed cumulative sizes, scalar categories and references remain checked.
Persistent sharing and existing native/transport layouts remain intact.

## Compiler index facts

`FixedIndex` proves that a closed integer index expression evaluates without
overflow or division failure and is within one fixed aggregate's length. The
scalar-replacement pass uses this fact to select the corresponding scalar local.
Alias selection rejects a proof for a different shape. Evaluation is bounded in
depth and supports checked addition, subtraction, multiplication, division,
remainder and unary negation. Unknown, negative, overflowing, failing and
out-of-range expressions retain their ordinary checked path.

This is intentionally limited to compile-time fixed aggregate projections.
General dynamic collection-index proofs and loop-range propagation require a
separate analysis of collection identity, control-flow joins and invalidation.

## Validation and measurement

Run `make tvm-native-optimizations-check` for focused correctness and optimization
contracts. The complete workspace suite includes these tests as well.

Run `make tvm-native-optimizations-benchmark` for three complementary reports:

* `target/quality/tvm-atom-table-profiles.json` compares the original individually
  owned names with packed UTF-8 names at three sizes. Paired samples include
  input ownership, validation, sorting, construction, all lookups, cloning,
  verification and destruction. Retained text and index storage are measured;
  allocator overhead and temporary construction storage are excluded.
* `target/quality/tvm-string-join-profiles.json` compares the previous owned
  fragment representation with the managed-reference batch at three sizes.
  Thirty-two paired samples alternate execution order after warming both lanes.
  Each sample includes materializing inputs, constructing a list, joining,
  collecting, checking exact output bytes and dropping the heap and registry.
  Input-text generation is excluded equally from both lanes.
* `target/quality/vm-aot-runtime-workloads.json` retains the existing complete
  actor workload family: allocation, local message round trips, scheduling,
  collection pauses, spawn/exit churn and mixed tail latency.

Both representation reports identify the native source revision and build
policy and include p50/p95/p99/max latency. Join reports measure semispace occupancy,
object counts and retained output bytes. Scratch metadata and text sizes are
explicitly modeled logical storage, excluding allocator overhead and spare
capacity; they are not process RSS. Both join lanes use current list construction
and traversal, so their timing difference isolates the join representation,
not every change in this work. Timing has no machine-independent pass threshold.

These reports measure Terlan's internal representations. Whole-application
comparisons require matched workloads, hardware, toolchains and interfaces.

## Local observations

The debug paired runs confirmed the storage reduction but showed higher CPU
cost for packing. A separate optimized probe compiled the actual production
atom and managed-core modules with `rustc --edition=2021 -O -C codegen-units=1`.
Its control retains the previous owned-name layout, validation and lookup
behavior. It runs 128 alternating pairs after eight warmups per lane, including
construction, all lookups, cloning, verification and destruction.

The Linux x86-64 run with Rust 1.96.0 measured these median microseconds:

| Atoms | Owned-name control | Packed names |
|---:|---:|---:|
| 8 | 3.865 | 3.515 |
| 128 | 71.738 | 65.111 |
| 1,025 | 690.627 | 648.128 |
| 16,384 | 12,812.196 | 11,521.924 |

This isolated probe supports retaining the packed representation; it does not
measure a complete optimized Terlan service. Source, exact flags, CPU model,
source digest and raw results are retained under
`target/quality/atom-release-probe/`. The permanent release-mode paired tests in
`make tvm-native-optimizations-benchmark` remain the comparison workflow for later runs.
For 1,025 names, measured index storage fell from 16,400 to 8,200 bytes and the
structural retained-buffer count fell from 1,026 to two; text bytes were unchanged.
