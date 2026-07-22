# Terlan N-Dimensional Array Roadmap

This roadmap defines the external `terlan-ndarray` package: a native,
Python-independent numerical array layer for Terlan and a stable interchange
boundary between data, numerical, image, and machine-learning packages.

This is a downstream roadmap. It does not block a Terlan release until the
active release roadmap promotes one of its named gates.

## Current State And Activation

Status as of 2026-07-19: ND0 through ND2 complete; ND3 active.

- The external `terlan-ndarray` package checkout now freezes ABI v1 and owns
  deterministic generated bindings, native/Terlan execution tests, and native
  dependency provenance.
- The generic C ABI generator now supports borrowed one-byte boolean arrays as
  a package-neutral `List[Bool]` contract without ndarray-specific compiler
  code.
- `make terlan-ndarray-package-check` passes the compiler contract suite, all
  35 C ABI generator tests, deterministic generation, seven adversarial
  metadata cases, warning-denied C/Rust builds, native sanitizers, generated
  helper lifecycle tests, and a revision-locked external Terlan consumer.
- ND3 through ND7 remain pending. ND8 is deliberately deferred.

Work begins with `ND0` immediately after the pinned CPU `terlan-pytorch`
package is remotely published and proven from a no-sibling checkout. CUDA and
general PyTorch autograd do not precede ndarray. Once ND0 starts, its ABI gate
should be promoted into the active release roadmap; later gates are promoted
only when their prerequisite milestone is complete.

Progress is measured by the executable checkboxes under each milestone, not by
document creation, generated stubs, or native-only probes. A milestone status
changes to `Complete` only when every checkbox and its named gate pass together.

## Decision

Terlan will use a separate package and repository named `terlan-ndarray`.

- Repository: `terlan-lang/terlan-ndarray`.
- Terlan package: `terlan-ndarray`.
- Public namespace: `ndarray`.
- Primary public type: opaque `ndarray.Array`.
- Default execution profile: owned, contiguous, row-major CPU arrays.
- Native package boundary: generated stable C ABI metadata and
  `NativeBoundary` resource handles.
- Tensor interchange: versioned DLPack C structures.
- Columnar interchange: the Arrow C Data Interface.
- Numerical kernels: CBLAS first and LAPACKE only when a public operation
  requires it.

The package is not a NumPy binding. NumPy's public C API is coupled to CPython
objects, interpreter initialization, reference counting, and NumPy version
compatibility. `terlan-ndarray` must run without Python or NumPy installed.

The package is also not a public wrapper over Rust `ndarray`. A Rust crate may
be evaluated as an internal implementation detail later, but Rust layouts,
generics, and crate ABI must not become the Terlan package contract.

## What This Roadmap Must Prove

The first complete CPU slice must prove all of the following together:

1. A separate Terlan project imports `ndarray.Array` through ordinary package
   metadata without changing compiler source.
2. Structured C metadata generates the package adapter through the generic C
   ABI machinery; no ndarray-specific compiler branch is added.
3. The native implementation allocates, owns, validates, and releases opaque
   arrays through `NativeBoundary` handles.
4. Terlan constructs non-constant arrays from owned scalar data plus a shape.
5. Shape, stride, dtype, element-count, and contiguous-layout inspection are
   exact and deterministic.
6. A real CBLAS implementation executes an observable matrix multiplication.
7. DLPack import/export validates version, dtype, shape, strides, device, and
   deleter ownership without exposing a pointer to Terlan source.
8. Polars exports selected numeric data to an owned array with explicit
   column, null, dtype, and shape policy.
9. PyTorch consumes an array through the DLPack boundary and executes an exact
   CPU result.
10. Stale handles, malformed shapes, overflow, incompatible dtypes, unsupported
    layouts/devices, double consumption, and missing native dependencies fail
    with stable diagnostic families.

Array unit tests alone are insufficient. DLPack structure tests alone are
insufficient. Completion requires fresh-package Terlan consumer execution and
at least one real Polars-to-array-to-PyTorch path.

## Scope And Non-Goals

The first release owns a deliberately small numerical-array model:

- homogeneous elements;
- rank-zero and arbitrary-rank shapes;
- checked signed dimensions at the Terlan boundary;
- checked element-count and byte-size multiplication;
- owned contiguous row-major CPU storage;
- `Bool`, `Int64`, and `Float64` construction and readback;
- exact shape, stride, dtype, rank, and element-count inspection;
- owned reshape/copy, transpose, basic elementwise arithmetic, reductions, and
  rank-2 matrix multiplication where implemented;
- deterministic disposal and stale-handle rejection.

The first release does not promise:

- NumPy API compatibility;
- CPython embedding;
- object, string, categorical, nullable, or heterogeneous array elements;
- arbitrary borrowed buffers exposed to Terlan source;
- autograd, models, optimizers, or training;
- DataFrame query operations;
- image decoding or computer-vision algorithms;
- CUDA execution, asynchronous streams, distributed arrays, or sparse arrays;
- automatic zero-copy conversion when ownership or layout cannot be proved.

Those responsibilities remain with specialized packages. In particular,
`terlan-pytorch` owns autograd and ML tensors, `terlan-polars` owns DataFrames,
and `terlan-opencv` owns image and computer-vision values.

## Frozen Baseline Semantics

The first implementation must use one deliberately narrow semantic model. This
prevents BLAS, DLPack, Polars, and PyTorch integrations from each inventing a
different interpretation of an array.

### Logical array model

Every live array has these observable properties:

```text
Array = {
  dtype: Bool | Int64 | Float64,
  device: Cpu,
  shape: List[non-negative Int],
  strides: canonical row-major element strides,
  offset: 0,
  storage: independently owned contiguous allocation,
  state: Live | Disposed
}
```

- Rank zero is a scalar with shape `[]` and exactly one element.
- A shape containing any zero dimension has zero elements and owns a valid
  empty allocation representation.
- Strides are measured in elements, never bytes, in the public Terlan API.
- The scalar stride list is empty. Empty arrays still report canonical strides
  derived from their shape.
- `Bool` storage uses a documented one-byte `0`/`1` representation at the C
  boundary. It must not rely on C or C++ `bool` layout.
- `Int64` is signed two's-complement 64-bit storage. Terlan `Int` values must be
  range-checked before conversion.
- `Float64` preserves IEEE-754 payloads. Equality tests must distinguish exact
  structural checks from tolerance-based numerical checks and explicitly cover
  NaN, infinities, and signed zero.

### Ownership model

Baseline operations never return views. Constructors copy inputs, readback
copies outputs, and every transform or arithmetic operation returns a new
independently owned array. Disposing a result must never invalidate an input.

The native handle table and VM `NativeBoundary` resource table jointly enforce:

- typed resource identity, including rejection of a valid handle of the wrong
  native resource class;
- monotonically changing generations so a reused slot cannot revive a stale
  Terlan value;
- exactly-once destruction under explicit disposal, actor exit, native error,
  cancellation, and VM shutdown;
- no native callback while a handle-table lock is held;
- no pointer, allocator identity, or generation value visible to Terlan source;
- panic and foreign-exception containment before crossing the C ABI.

Views, borrowed arrays, copy-on-write storage, and shared DLPack-backed storage
are later features. They must not be simulated by weakening this ownership
model.

### Operation contracts

- `reshape` requires the same element count and performs an owned copy in the
  baseline even when metadata-only reshape would be possible.
- `transpose(axis0, axis1)` validates normalized non-negative axes and returns
  canonical contiguous row-major storage. Negative axis shorthand is deferred.
- Elementwise arithmetic initially requires identical dtype and shape. No
  implicit dtype promotion or broadcasting is allowed.
- `sum` rejects duplicate and out-of-range axes. The order of supplied axes
  does not change the result. Integer overflow behavior must be selected and
  documented before `Int64` reduction is public.
- `matmul` initially accepts rank-2 `Float64` arrays only. Batched matmul,
  vector promotion, and implicit copies of arbitrary strided inputs are not
  part of the first contract.
- Every fallible public operation returns a typed Terlan `Result`; native status
  values are never exposed as the public error API.

## Repository Ownership

| Repository | Owns | Must not own |
| --- | --- | --- |
| `terlan` compiler repository | Generic C metadata validation and generation, package execution, `NativeBoundary` resource and cross-package handoff contracts, stable generic diagnostics, and package-neutral fixtures | Numerical algorithms, a public `ndarray` API, BLAS linkage, or package-specific conversion code |
| future `terlan-ndarray` repository | Public array API, C metadata, package-owned native implementation, DLPack mapping, BLAS/LAPACK linkage policy, fixtures, package tests, and consumer tests | Compiler parsing/lowering forks, Polars query logic, PyTorch operations, or OpenCV algorithms |
| `terlan-polars` repository | `Series.to_array`, `DataFrame.to_array`, and Polars-owned `to_torch` convenience integration | Array storage or PyTorch implementation |
| `terlan-pytorch` repository | `Tensor.from_array`/DLPack consumption and tensor/model operations | DataFrame conversion or generic CPU array semantics |
| future `terlan-opencv` repository | Explicit `Mat`/array conversion policy where useful | Generic ndarray storage or DLPack ownership rules |

Expected sibling layout:

```text
/home/anatoly/Applications/terlan/
  terlan/                 # compiler repository
  terlan-ndarray/         # external numerical-array package
  terlan-polars/          # external DataFrame package
  terlan-pytorch/         # external PyTorch package
  terlan-opencv/          # external OpenCV package
```

The compiler may validate a sibling package checkout. It must not vendor
`terlan-ndarray`, silently fetch a BLAS implementation during ordinary
compiler tests, or link BLAS into the compiler itself.

## Dependency Direction

The base package must not depend on Polars, PyTorch, OpenCV, NumPy, or Python.

```text
                         +------------------+
                         | terlan-ndarray   |
                         | Array + DLPack   |
                         +------------------+
                           ^       ^      ^
                           |       |      |
               +-----------+       |      +-----------+
               |                   |                  |
       terlan-polars        terlan-pytorch       terlan-opencv
       DataFrame/Series      Tensor/models        Mat/images
```

`terlan-polars` owns the high-level conversion entrypoints because conversion
out of a DataFrame requires Polars-specific column selection, casting, null,
and chunk policy. `terlan-pytorch` remains usable without Polars.

The intended user paths are:

```text
List values + shape -> ndarray.Array
Polars DataFrame    -> DataFrame.to_array -> ndarray.Array
ndarray.Array       -> Tensor.from_array  -> pytorch.Tensor
Polars DataFrame    -> DataFrame.to_torch -> pytorch.Tensor
```

`DataFrame.to_torch` is a Polars-owned convenience surface. It should compose
the same checked array/DLPack machinery rather than implement a second buffer
protocol. If Terlan package metadata cannot express an optional PyTorch
dependency, the Polars repository should publish that convenience as a
Polars-owned integration package rather than forcing PyTorch on every Polars
consumer.

## Stable Native Foundations

No single dependency supplies NumPy's entire combination of array ownership,
indexing, broadcasting, algorithms, and ecosystem interoperability. The
package therefore composes small, durable boundaries.

| Foundation | Role | Explicit non-role |
| --- | --- | --- |
| DLPack | Homogeneous tensor dtype, shape, strides, device, data pointer, versioning, and deleter-based exchange | Array implementation or numerical operators |
| Arrow C Data Interface | ABI-stable extraction of Polars/Arrow primitive column buffers, schemas, validity, and release callbacks | Dense multidimensional numerical operations |
| CBLAS | Stable CPU vector/matrix kernels, beginning with rank-2 matrix multiplication | Array ownership, arbitrary-rank semantics, or dtype policy |
| LAPACKE | Later decompositions/solvers when selected by a public API milestone | Required dependency for the first package slice |
| package-owned C ABI | Stable opaque handles, status codes, checked construction, copies, inspection, and disposal | A promise that internal structs are ABI-visible |

GNU Scientific Library is not the default foundation. It is mature, but its
vector/matrix model is not the arbitrary-rank contract required here and its
GPL license would materially constrain package distribution.

Every third-party native dependency must be pinned by released version or
immutable source revision, have its license recorded, and contribute exact
headers/library identity to the package execution report.

## Public Terlan Surface

The following is the proposed first surface. Exact syntax may change to match
the supported Terlan declaration grammar, but the semantics may not be
weakened.

```terlan
module ndarray.Types.

pub type DType = Bool | Int64 | Float64.
pub type Device = Cpu.

module ndarray.Array.

pub opaque type Array.

pub from_bools(values: List[Bool], shape: List[Int]): Result[Array, Error].
pub from_ints(values: List[Int], shape: List[Int]): Result[Array, Error].
pub from_floats(values: List[Float], shape: List[Int]): Result[Array, Error].

pub zeros(shape: List[Int], dtype: DType): Result[Array, Error].
pub full_float(shape: List[Int], value: Float): Result[Array, Error].

pub (array: Array) rank(): Int.
pub (array: Array) shape(): List[Int].
pub (array: Array) strides(): List[Int].
pub (array: Array) numel(): Int.
pub (array: Array) dtype(): DType.
pub (array: Array) device(): Device.
pub (array: Array) is_contiguous(): Bool.

pub (array: Array) to_bools(): Result[List[Bool], Error].
pub (array: Array) to_ints(): Result[List[Int], Error].
pub (array: Array) to_floats(): Result[List[Float], Error].

pub (array: Array) reshape(shape: List[Int]): Result[Array, Error].
pub (array: Array) transpose(axis0: Int, axis1: Int): Result[Array, Error].
pub add(left: Array, right: Array): Result[Array, Error].
pub subtract(left: Array, right: Array): Result[Array, Error].
pub multiply(left: Array, right: Array): Result[Array, Error].
pub sum(array: Array, axes: List[Int], keep_dims: Bool): Result[Array, Error].
pub matmul(left: Array, right: Array): Result[Array, Error].

pub dispose(array: Array): Unit.
```

Construction from Terlan lists is an owned copy. Readback is also a copy.
These operations establish useful behavior before borrowed or zero-copy native
memory is introduced.

The first public API should not expose raw pointers, byte addresses, deleter
callbacks, `DLManagedTensor` fields, Arrow capsules, BLAS integer widths, or
provider-specific handles.

## Package-Owned C ABI

The native implementation should expose a narrow C ABI described by structured
metadata and consumed by the generic Terlan C binder. The exact prefix may be
finalized with the package, but the contract should resemble:

```c
typedef int32_t TerlanNdArrayStatus;
typedef struct TerlanNdArrayOpaque *TerlanNdArrayHandle;

TerlanNdArrayStatus terlan_ndarray_from_f64(
    const double *values,
    int64_t value_count,
    const int64_t *shape,
    int64_t rank,
    TerlanNdArrayHandle *out);

TerlanNdArrayStatus terlan_ndarray_shape(
    TerlanNdArrayHandle array,
    const int64_t **out_shape,
    int64_t *out_rank);

TerlanNdArrayStatus terlan_ndarray_export_dlpack(
    TerlanNdArrayHandle array,
    struct DLManagedTensorVersioned **out);

TerlanNdArrayStatus terlan_ndarray_import_dlpack(
    struct DLManagedTensorVersioned *tensor,
    TerlanNdArrayHandle *out);

TerlanNdArrayStatus terlan_ndarray_delete(TerlanNdArrayHandle array);
```

Rules:

- status `0` is success and every failure is classified stably;
- all output slots are validated and initialized deterministically;
- input pointer/length pairs are borrowed only for the duration of the call;
- public list constructors copy before returning;
- borrowed shape/stride output is copied by the generated adapter while the
  source handle remains borrowed;
- native allocation size uses checked multiplication;
- no C++ exception or language panic may unwind through the C ABI;
- disposal is deterministic and the VM rejects stale handle use before native
  dispatch;
- implementation structs remain opaque and version-private.

## Ownership And Cross-Package DLPack Handoff

DLPack contains native pointers and a producer-provided deleter. Terlan source
must never serialize, inspect, or forge those values.

The safe cross-package path requires a compiler/runtime-owned handoff resource:

```text
producer Array handle
  -> producer exports DLManagedTensorVersioned
  -> NativeBoundary registers one-shot exchange resource
  -> consumer atomically claims exchange resource
  -> consumer validates and imports tensor
  -> final owner invokes producer deleter exactly once
```

The exchange resource is one-shot; that does not automatically consume the
source `ndarray.Array`. The baseline `Tensor.from_array` behavior should copy
and leave the source array readable. A later zero-copy shared mode must keep
the allocation alive independently for both handles. Destructive consume mode,
if ever added, must be a separately named operation that invalidates the source
atomically.

The handoff contract must prove:

- producer and consumer are in a compatible process/runtime boundary;
- supported DLPack major/minor version policy;
- checked dtype, rank, dimensions, strides, byte offset, and device;
- atomic unclaimed/claimed/closed state transitions;
- exactly one successful consumer claim;
- explicit copy/shared/consume semantics, with source readability preserved by
  the baseline copy mode;
- independent storage retention for any later shared mode and atomic source
  invalidation for any explicitly destructive consume mode;
- deleter execution exactly once on success, rejection, cancellation, helper
  failure, runtime shutdown, and stale-resource cleanup;
- stable rejection of double consumption and wrong resource kinds;
- no raw pointer encoded into Terlan strings, integers, lists, or package
  metadata.

Until this generic handoff is executable, Polars-to-array and array-to-tensor
conversion must use owned copies. A local native test that passes a pointer
directly between two adapters does not close this milestone.

## Arrow And Polars Conversion Policy

Arrow's C Data Interface supplies a stable columnar memory and lifetime
contract. It does not automatically turn a heterogeneous DataFrame into one
homogeneous row-major array.

`terlan-polars` must own:

- column and expression selection;
- feature/label separation;
- numeric dtype eligibility and explicit common-dtype casting;
- null rejection, filling, or masking policy;
- chunk consolidation policy;
- deterministic DataFrame row/column ordering;
- output shape `[row_count, selected_column_count]`;
- stable errors for missing columns, unsupported dtypes, nulls, empty
  selections, and overflow.

The baseline `DataFrame.to_array` implementation copies selected primitive
columns into one contiguous row-major `ndarray.Array`. Arrow release callbacks
must still be honored for every exported column, including partial failure.

Later zero-copy support may be added for a compatible single primitive Series
or an already compatible dense buffer. It must not be claimed for an ordinary
multi-column DataFrame whose columnar buffers require packing into a dense
matrix.

## BLAS And LAPACK Policy

The first accelerated operation is Float64 rank-2 matrix multiplication through
CBLAS `dgemm`.

The package must:

- validate rank and inner dimensions before calling CBLAS;
- validate row-major contiguity or materialize an owned compatible copy;
- reject unsupported dtypes without silent casting;
- verify CBLAS integer-width compatibility and reject LP64/ILP64 mismatch;
- record provider name, version, library path, and relevant build mode;
- test a non-square exact result, zero-size policy, incompatible shapes, and
  allocation overflow;
- keep BLAS loading and provider selection package-owned.

Basic elementwise loops, copies, shape transforms, and reductions may remain in
the small package-owned native core. LAPACKE enters only with a selected public
operation such as solve, QR, SVD, or eigen decomposition and must have its own
accuracy, workspace, convergence, and diagnostic policy.

## Stable Diagnostic Families

At minimum the package and generic handoff machinery must preserve these
machine-readable families:

| Family | Meaning |
| --- | --- |
| `ndarray.shape.negative_dimension` | A shape contains a negative dimension |
| `ndarray.shape.element_count_mismatch` | Shape product does not equal supplied value count |
| `ndarray.shape.overflow` | Element-count or allocation-size multiplication overflowed |
| `ndarray.axis.out_of_range` | An axis is invalid for the current rank |
| `ndarray.dtype.unsupported` | Requested or imported dtype is not supported |
| `ndarray.dtype.mismatch` | An operation requires compatible dtypes |
| `ndarray.layout.unsupported` | Strides, byte offset, alignment, or contiguity cannot be represented safely |
| `ndarray.device.unsupported` | The baseline package received non-CPU memory |
| `ndarray.handle.stale` | A disposed or invalid array handle was used |
| `ndarray.native.missing` | Required package native library is unavailable |
| `ndarray.blas.missing` | Selected BLAS provider is unavailable |
| `ndarray.blas.integer_width` | BLAS integer ABI does not match the package build |
| `ndarray.dlpack.version` | DLPack major/minor policy rejected an exchange |
| `ndarray.dlpack.already_consumed` | A one-shot exchange was claimed more than once |
| `ndarray.dlpack.deleter` | Exchange cleanup could not satisfy exactly-once ownership |
| `ndarray.arrow.schema` | Arrow primitive schema is unsupported or inconsistent |
| `ndarray.arrow.nulls` | Null policy rejected an Arrow/Polars export |

Diagnostics must identify the operation and relevant argument without printing
raw addresses, buffer contents, private native errors, or unbounded data.

## Package Layout

The external repository should begin with:

```text
terlan-ndarray/
  README.md
  LICENSE
  Makefile
  terlan.toml
  bindings/
    ndarray-c.json
  docs/
    ROADMAP.md
    ABI.md
    INTEROP.md
  native/
    include/
      terlan_ndarray.h
    src/
      array.c
      elementwise.c
      dlpack.c
      blas.c
  src/
    ndarray/
      Array.terl
      Types.terl
  test/
    ArrayTest.terl
    InteropTest.terl
    ErrorsTest.terl
  consumer/
    terlan.toml
    src/Main.terl
  scripts/
    check-package.sh
    check-consumer.sh
```

Generated package artifacts may be committed when they are reviewed source.
Downloaded BLAS distributions, build outputs, and generated scratch consumers
must remain ignored.

## Ordered Milestones

### ND0 - Freeze Package And ABI Policy

Status: Complete.

- [x] Create `terlan-lang/terlan-ndarray` with `terlan.toml`, package namespace
  `ndarray`, version `0.0.7`, license, README, and the package layout defined in
  this document.
- [x] Pin the supported DLPack header by released version or immutable revision;
  record source URL, revision, checksum, and license in package metadata.
- [x] Select and document one default CPU CBLAS provider plus an explicit system
  provider override. Record LP64/ILP64 detection and supported host platforms.
- [x] Freeze native ABI version `1`, symbol prefix `terlan_ndarray_v1_`, status
  representation, scalar widths, bool representation, and calling convention.
- [x] Define structured C metadata for borrowed scalar arrays, borrowed shape
  arrays, copied array returns, opaque handles, status families, and consuming
  destructors.
- [x] Prove each required metadata shape with package-neutral compiler fixtures;
  add generic generator support only when a fixture demonstrates a reusable
  cross-package requirement.
- [x] Add negative metadata fixtures for missing lengths, mutable borrowed
  pointers, ambiguous ownership, missing destructors, unsupported scalar width,
  duplicate symbols, and ABI-version mismatch.
- [x] Generate bindings twice in clean directories and require byte-identical
  source, manifest, and report hashes.
- [x] Add a compiler-source scan proving no `ndarray`, DLPack provider, or BLAS
  provider special case enters parser, type checker, lowering, or codegen.

Exit: metadata validates, generated source is deterministic, and rejected
ownership shapes fail before code generation.

Gate: `make terlan-ndarray-abi-check`.

Completed progress: the standalone package freezes ABI v1 for owned contiguous
CPU `Bool`, `Int64`, and `Float64` arrays and pins DLPack v1.3 plus OpenBLAS
v0.3.33 by immutable revision and SHA-256. Its machine-readable provider policy
fixes LP64 as the default, defines explicit system-provider discovery and
LP64/ILP64 rejection, and lists supported host triples. The generic C binder
copies `List[Bool]` into explicit `uint8_t` storage rather than relying on Rust
or C boolean layout. The canonical gate passes 5 binding-contract tests, all 35
C ABI generator tests, deterministic generation of 14 files, 7 adversarial
metadata cases, warning-denied C/Rust builds, native lifecycle execution, and
Terlan execution through the generated helper.

### ND1 - Owned CPU Array Lifecycle

Status: Complete.

- [x] Implement a private native array header with checked dtype, rank, shape,
  canonical strides, element count, byte count, allocation, and live/disposed
  state. Keep its layout absent from the public header.
- [x] Implement checked shape-product and byte-size helpers with no unchecked
  signed conversion or multiplication.
- [x] Implement owned `Bool`, `Int64`, and `Float64` constructors from Terlan
  lists and shapes, including scalar and empty-array construction.
- [x] Implement rank, shape, strides, element count, dtype, CPU device,
  contiguity, and copied readback for all three dtypes.
- [x] Implement deterministic disposal and prove stale use, wrong resource kind,
  double disposal, and slot-generation reuse are rejected before native access.
- [x] Ensure every partial-construction failure releases shape and data
  allocations; add allocation-failure injection at each allocation boundary.
- [x] Execute package tests through generated bindings and the real native
  helper, not through Rust-only or C-only substitutes.
- [x] Execute a fresh Terlan consumer that constructs a non-constant `[2, 3]`
  Float64 array, verifies values and metadata, and disposes it.
- [x] Add the lifecycle rows from the conformance matrix below, including
  rank-zero, zero dimensions, high rank, negative dimensions, count mismatch,
  overflow, NaN/infinity, `Int64` limits, and malformed bool bytes.
- [x] Run native leak/error tooling on the lifecycle suite where supported and
  record a stable skip reason where the tool is unavailable.

Exit: an external Terlan consumer constructs a non-constant `[2, 3]` Float64
array, reads exact values and metadata, and releases all handles.

Gate: `make terlan-ndarray-package-check`.

Completed progress: the private native layout now owns dtype, CPU device,
rank, shape, canonical strides, element and byte counts, data, and lifecycle
state. Checked constructors and copied readback cover Bool, Int64, and Float64,
including scalar, empty, high-rank, overflow, allocation-failure, NaN/infinity,
integer-limit, and malformed-bool cases. The generated helper rejects stale,
wrong-kind, double-disposed, and stale-generation handles while reusing slots
with incremented generations. A fresh revision-locked Terlan consumer executes
a computed `[2, 3]` Float64 array and releases every handle. AddressSanitizer
and UBSan pass; LeakSanitizer and Valgrind record stable host-unavailable skips
in `reports/ndarray-package-report.json`.

### ND2 - Shape Operations And Basic Arithmetic

Status: Complete.

- [x] Implement owned `reshape` with exact element-count preservation and
  independent result storage.
- [x] Implement owned two-axis transpose for scalar, rank-1, rank-2, and
  arbitrary-rank arrays with canonical contiguous output.
- [x] Implement exact-shape add, subtract, and multiply for supported numeric
  dtypes. Reject bool arithmetic and implicit dtype conversion.
- [x] Select and document `Int64` overflow behavior consistently across
  elementwise operations and reduction before exposing those operations.
- [x] Implement deterministic `sum` axis normalization, duplicate-axis
  rejection, `keep_dims`, empty-input identities, and Float64 accumulation
  policy.
- [x] Prove source/result independence by mutating native test buffers after
  operation completion and by disposing values in every order.
- [x] Add table-driven Terlan tests across dtype, rank, axis, empty shape,
  aliasing, mismatch, overflow, and non-finite Float64 cases.
- [x] Add property tests comparing shape/index transforms to a small checked
  reference model for bounded generated shapes and values.
- [x] Keep broadcasting unavailable and require a stable rejection rather than
  accidental scalar or singleton expansion.

Exit: all results are independently owned, source arrays remain readable, and
shape/dtype failures have stable classifications.

Gate: `make terlan-ndarray-operations-check`.

Completed progress: ABI v1 now exposes independently owned reshape and
two-axis transpose plus exact-shape numeric add, subtract, multiply, and sum.
`Int64` arithmetic and reductions reject overflow; Bool arithmetic, implicit
dtype conversion, broadcasting, duplicate axes, and out-of-range axes have
stable status codes. Native table and bounded reference-model tests cover
scalar through arbitrary-rank layouts, empty arrays, non-finite Float64
values, disposal order, and allocation failure. Generated Terlan bindings run
four operation tests plus a revision-locked positive consumer and six isolated
negative consumers.

### ND3 - Real CBLAS Execution

Status: Active.

- [ ] Acquire or locate the selected released CBLAS provider in the package
  build without adding BLAS linkage to `terlc` or the VM.
- [ ] Validate provider version, library identity, architecture, calling
  convention, and LP64/ILP64 integer width before execution.
- [ ] Implement rank-2 row-major Float64 matrix multiplication through CBLAS
  `dgemm` with checked dimension conversion and output allocation.
- [ ] Define zero-dimension matmul behavior and prove the implementation does
  not pass invalid pointers to a provider for an empty operation.
- [ ] Compare exact integer-valued fixtures and tolerance-based fractional
  fixtures with a package-owned reference implementation outside timed runs.
- [ ] Exercise rectangular, identity, zero, non-finite, incompatible-shape,
  unsupported-dtype, integer-width, missing-provider, and allocation-failure
  cases.
- [ ] Execute matmul from package Terlan tests and from a fresh immutable Git
  consumer after deleting access to the source checkout.
- [ ] Record provider provenance and semantic result in deterministic reports;
  reject an unidentifiable or incompatible library rather than guessing.
- [ ] Add a non-gating benchmark for sizes `1`, `16`, `64`, `256`, and `1024`
  with warmup and at least three samples. Benchmark results cannot replace
  semantic acceptance.

Exit: exact non-square matrix multiplication passes against the real provider;
missing provider, incompatible shapes, unsupported dtype, and ABI mismatch are
rejected deterministically.

Gate: `make terlan-ndarray-blas-check`.

### ND4 - DLPack Structure And Ownership Contract

Status: Pending.

- [ ] Add a package-neutral one-shot native exchange resource to
  `NativeBoundary` with typed producer/consumer identities and atomic
  `Available -> Claimed -> Closed` transitions.
- [ ] Prove cancellation, actor exit, helper crash, consumer rejection, VM
  shutdown, and abandoned-resource cleanup invoke exactly one producer cleanup
  action without holding runtime registry locks.
- [ ] Implement package-native DLPack export for baseline CPU arrays using the
  pinned versioned structures and a package-owned manager context.
- [ ] Implement DLPack import as an owned copy, preserving source readability
  and invoking the producer deleter after the copy or on every rejection path.
- [ ] Validate DLPack version, device, dtype code/bits/lanes, rank, dimensions,
  nullability, strides, byte offset, pointer alignment, element/byte overflow,
  and supported contiguous layout before reading data.
- [ ] Reject second claim, wrong exchange kind, stale token, forged scalar token,
  and mismatched producer/consumer capability with stable diagnostics.
- [ ] Add adversarial native fixtures whose deleter increments an atomic audit
  counter; require exactly one event under success and each failure phase.
- [ ] Run producer and consumer as independently built helpers through the VM
  broker. A direct in-process adapter call does not satisfy this item.
- [ ] Preserve list construction/readback as an always-available owned-copy
  fallback independent from DLPack support.

Exit: two independently built native package fixtures exchange an array through
the runtime broker without exposing or serializing a pointer.

Gate: `make ndarray-dlpack-interop-check`.

### ND5 - Polars To Array

Status: Pending; implementation owner is `terlan-polars`.

- [ ] Add an optional immutable `terlan-ndarray` dependency to the Polars
  integration surface without forcing PyTorch on Polars-only consumers.
- [ ] Implement `Series.to_array` for supported primitive bool/integer/Float64
  Arrow data with explicit casting and null policy.
- [ ] Implement `DataFrame.to_array(columns, dtype, null_policy)` with stable
  column ordering and exact `[rows, columns]` row-major output.
- [ ] Consume Arrow C Data schema/array pairs through checked ownership wrappers;
  execute every release callback exactly once on success and partial failure.
- [ ] Consolidate chunks or copy across them deliberately. Reject unsupported
  nested, string, categorical, temporal, object, and mixed data without
  interpreting provider-private layouts.
- [ ] Cover empty frames, empty selections, missing/duplicate columns, chunked
  columns, nulls, cast overflow, mixed numeric types, schema mismatch, and
  release-callback failure injection.
- [ ] Execute the Iris path: select four Float64 feature columns from 150 rows,
  produce exact shape `[150, 4]`, verify bounded known values, and release all
  Polars, Arrow, and ndarray resources.
- [ ] Run the integration from a fresh consumer using immutable revisions of
  both packages.

Exit: a fresh consumer reads the checked Iris CSV with Polars, selects four
Float64 feature columns, obtains an exact `[150, 4]` array, verifies selected
values, and cleans up every Arrow, Polars, and ndarray resource.

Gate: `make polars-ndarray-interop-check`.

### ND6 - Array To PyTorch

Status: Pending; implementation owner is `terlan-pytorch`.

- [ ] Add an optional immutable `terlan-ndarray` dependency to
  `terlan-pytorch`; keep Polars absent from the PyTorch dependency graph.
- [ ] Implement `Tensor.from_array` through the reviewed DLPack exchange broker,
  not through a package-specific pointer tunnel or compiler branch.
- [ ] Make the first operation an owned copy, preserve supported CPU dtype and
  shape, and leave the source `Array` readable after tensor construction.
- [ ] Execute exact non-constant Bool, Int64, and Float64 conversions, then run
  one observable PyTorch operation on each supported tensor dtype.
- [ ] Cover unsupported dtype/device/layout, malformed DLPack metadata, stale
  source, consumer rejection, native exception, double claim, and cleanup after
  actor cancellation.
- [ ] Require producer and consumer cleanup audits to show one DLPack deleter
  call and independently valid source/tensor destruction in every order.
- [ ] Add Polars-owned `DataFrame.to_torch` only as composition of
  `DataFrame.to_array` and `Tensor.from_array`; do not implement a second
  Polars-to-tensor buffer protocol.
- [ ] Execute the complete Iris feature path and verify tensor shape `[150, 4]`
  plus an exact deterministic model or reduction result.

Exit: an array containing non-constant feature values becomes a PyTorch tensor,
executes an exact operation, and releases producer/consumer ownership exactly
once.

Gate: `make ndarray-pytorch-interop-check`.

### ND7 - Fresh Package And Adversarial Closure

Status: Pending.

- [ ] Publish and pin immutable revisions for ndarray and every participating
  integration package.
- [ ] Fetch into a fresh Terlan consumer, remove sibling checkout access, disable
  network access, and execute only from the verified package cache.
- [ ] Run the complete baseline, conformance, ownership, allocation-failure,
  malformed-input, and cross-package integration suites.
- [ ] Run native sanitizers or equivalent platform memory tooling and fail on
  leak, use-after-free, double-free, undefined behavior, or foreign unwind.
- [ ] Run package-generated Rust with warnings and deprecations denied, package
  C/C++ with warnings denied, and enforce repository file-size/complexity gates.
- [ ] Validate deterministic ABI, package, BLAS, DLPack, Polars, and PyTorch
  reports against schemas and regenerate each twice for stability.
- [ ] Prove no compiler path, `std.native` namespace, Python/NumPy installation,
  CUDA installation, unpinned download, or unpublished local dependency is
  required.
- [ ] Prove every public Terlan function has positive, boundary, and adversarial
  coverage and that the package meets the enforced coverage baseline.
- [ ] Record supported host triples and execute the baseline on Linux, macOS,
  and Windows release artifacts or retain an explicit unchecked platform item.

Exit: `terlan-ndarray-package-check` passes in a fresh CPU-only workspace and
all required reports validate.

Gate: `make terlan-ndarray-release-check`.

### ND8 - Later Numerical And Device Surface

Status: Deferred.

- Additional primitive dtypes and reviewed conversion rules.
- Broadcasting, views, slicing, concatenation, stacking, and advanced
  reductions.
- Random generation with explicit deterministic seeds.
- LAPACKE solve/decomposition operations with numerical tolerance policy.
- Memory mapping and serialization, including optional `.npy` compatibility.
- DLPack device import/export for CUDA and other devices after stream and
  synchronization ownership is executable.
- Optional OpenCV and other package-owned conversion conveniences.

None of these later surfaces may weaken CPU ownership, diagnostic stability, or
package independence.

## Required Conformance Matrix

The package should keep this matrix as structured test data and generate
package tests from it where practical. A milestone may mark a row complete only
when the public Terlan path executes it; native-only coverage is supporting
evidence, not completion.

| Area | Required positive cases | Required adversarial cases |
| --- | --- | --- |
| Shape | scalar `[]`, `[0]`, `[2,3]`, `[2,0,3]`, rank 8 | negative dimension, element-count mismatch, element-count overflow, byte-size overflow, excessive rank |
| Bool | empty, scalar false/true, mixed matrix, copied readback | non-`0`/`1` native byte, wrong readback dtype, stale handle |
| Int64 | min, max, zero, negative, matrix | Terlan-to-Int64 range failure, selected overflow policy, dtype mismatch |
| Float64 | finite values, `-0.0`, infinities, NaN payload path | tolerance misuse, non-finite reduction policy, dtype mismatch |
| Metadata | rank, shape, canonical strides, numel, dtype, CPU, contiguous | wrong resource kind, disposed value, malformed native metadata |
| Reshape | scalar, empty, same shape, rank change | changed element count, negative/overflowing shape, stale input |
| Transpose | scalar, vector no-op, matrix, high rank, empty axis | duplicate axes where disallowed, axis out of range, stale input |
| Arithmetic | exact shape add/subtract/multiply, empty arrays | shape mismatch, dtype mismatch, bool arithmetic, overflow, allocation failure |
| Reduction | all axes, selected axes, `keep_dims`, empty policy | duplicate axis, bad axis, overflow, unsupported dtype |
| Matmul | rectangular, identity, zero, fractional | rank mismatch, inner mismatch, dtype mismatch, provider missing, integer ABI mismatch |
| Handle lifecycle | dispose inputs/results in every order, actor cleanup | double dispose, use after dispose, slot reuse, wrong type, cancellation during call |
| DLPack | owned CPU copy for each dtype, source remains readable | version, lanes, dtype, shape, strides, offset, alignment, device, double claim, throwing helper |
| Arrow/Polars | single Series, multi-column frame, chunks, Iris `[150,4]` | nulls, missing columns, mixed/nested dtype, cast overflow, malformed schema, partial release |
| PyTorch | each dtype, exact shape, observable tensor operation | rejected import, stale exchange, cancellation, double claim, deleter audit failure |

Additional invariants apply to every matrix row:

- inputs remain readable unless an operation is explicitly documented as
  consuming;
- outputs are either fully initialized and owned or absent;
- every allocation and foreign ownership callback has a counted cleanup event;
- diagnostics use the documented stable family and contain no raw address;
- repeated execution is deterministic apart from explicitly benchmarked timing;
- tests include disposal after success and after every injected failure point.

## Coverage And Property Strategy

The package must enforce coverage as a non-decreasing baseline, with 100% as
the requirement for package-owned safety and ownership logic. Generated glue is
covered through generator fixtures and representative generated execution
rather than excluded wholesale.

Property-based tests should generate bounded cases for:

- shape product and canonical stride derivation;
- flatten/unflatten index round trips;
- reshape element-count preservation;
- transpose permutation and inverse transpose;
- elementwise arithmetic against a checked scalar reference;
- reduction shape derivation and reference values;
- matmul against a simple non-BLAS reference for small dimensions;
- handle state-machine transitions;
- DLPack claim/deleter state-machine transitions.

Generators must bias toward zero, one, maximum supported rank, dimensions near
multiplication overflow, repeated axes, empty storage, `Int64` boundaries,
non-finite Float64 values, and malformed external metadata. Every discovered
failure must be minimized and retained as a deterministic regression fixture.

Fuzz targets should cover C metadata decoding, shape validation, DLPack import,
Arrow schema validation, and serialized package reports. Fuzzing is additive;
it does not replace deterministic adversarial rows.

## Gate Topology And Runtime Budget

The compiler repository may expose convenience targets, but one package-owned
orchestrator must own test selection, process reuse, reports, and timing. Do not
create one shell/Make process per individual test.

### Fast developer gate

`make terlan-ndarray-check` should finish from a warm build in a target of 60
seconds and run:

- metadata/schema validation and deterministic generation;
- package-native unit and property tests with bounded case counts;
- generated adapter tests;
- package Terlan tests using an already available native helper;
- warnings/deprecations-as-errors and source policy scans.

### Integration gates

Integration gates are independently selectable and share build artifacts:

```text
make terlan-ndarray-package-check
make terlan-ndarray-blas-check
make ndarray-dlpack-interop-check
make polars-ndarray-interop-check
make ndarray-pytorch-interop-check
```

Each gate must print elapsed time and identify reused versus rebuilt artifacts.
No integration gate may silently download an unpinned dependency. Missing
optional host tooling must produce a stable skip only where the roadmap
explicitly allows a skip; required CPU dependencies fail the gate.

### Release gate

`make terlan-ndarray-release-check` runs the union once through the shared
orchestrator, plus fresh immutable consumers, offline execution, sanitizers,
coverage, report-schema validation, and platform packaging. It must not invoke
each lower gate as a new nested build when their cases can be selected in the
same process.

The first canonical run records phase timings. Any phase exceeding its budget
must be profiled before raising the budget. Reports and benchmark outputs are
not written into the compiler repository's build tree unless that gate owns
them.

## Implementation Sequence And Parallelism

The critical path is:

```text
ND0 -> ND1 -> ND2 -> ND3 -> ND4 -> ND5 -> ND6 -> ND7
```

Safe parallel work is limited:

- BLAS provider acquisition/provenance can proceed while ND1 is implemented,
  but public matmul waits for ND1 ownership and shape validation.
- DLPack native structure fixtures can proceed during ND2/ND3, but cross-package
  exchange waits for the generic `NativeBoundary` broker.
- Polars and PyTorch may design their public conversion APIs during ND4, but
  neither may land a pointer shortcut before the broker gate passes.
- ND5 and ND6 may run in parallel after ND4; ND7 waits for both.

The first implementation work package is ND0. It is complete only as one
feature-sized change containing the external repository scaffold, frozen ABI
metadata, deterministic generated fixture, adversarial metadata cases, and the
passing ABI gate. Do not commit each checkbox as a separate checkpoint.

## Required Reports

The executable gates should produce deterministic machine-readable artifacts:

- `ndarray-abi-report.json`: C metadata, generated manifest hashes, supported
  dtypes/layouts, and native ABI version;
- `ndarray-package-execution-report.json`: package revision, compiler revision,
  exact operations, semantic results, and handle cleanup;
- `ndarray-blas-report.json`: provider, version, library identity, integer
  width, and executed kernel cases;
- `ndarray-dlpack-report.json`: DLPack version policy, claim transitions,
  deleter events, copy/share mode, and rejection cases;
- `polars-ndarray-interop-report.json`: selected columns, source schema, null
  policy, output dtype/shape, copy policy, and cleanup;
- `ndarray-pytorch-interop-report.json`: source array metadata, tensor metadata,
  operation result, ownership transfer, and cleanup.

Reports must use sorted keys, stable reason codes, bounded redacted errors, and
no raw addresses or user data dumps.

## Completion Criteria

The roadmap's baseline is complete only when:

- `terlan-ndarray` exists as an external package and ordinary Git dependency;
- the compiler contains no ndarray-specific parser, lowering, or runtime
  branch;
- the generated C adapter and package-owned native implementation execute on
  a real CPU path;
- arbitrary owned Float64 data plus shape can be constructed and read back;
- CBLAS matrix multiplication executes with verified provider provenance;
- DLPack ownership transfer is safe across package helpers;
- Polars-to-array and array-to-PyTorch consumers execute exact semantic checks;
- all required negative cases have stable diagnostic families;
- fresh consumers pass without Python, NumPy, CUDA, or network access;
- every owned native resource is released exactly once.

## Reference Contracts

- DLPack: <https://dmlc.github.io/dlpack/latest/c_api.html>
- Arrow C Data Interface:
  <https://arrow.apache.org/docs/format/CDataInterface.html>
- Netlib BLAS: <https://www.netlib.org/blas/>
- LAPACKE: <https://netlib.org/lapack/lapacke.html>
