# Terlan N-Dimensional Array Roadmap

This roadmap defines the external `terlan-ndarray` package: a native,
Python-independent numerical array layer for Terlan and a stable interchange
boundary between data, numerical, image, and machine-learning packages.

This is a downstream roadmap. It does not block a Terlan release until the
active release roadmap promotes one of its named gates.

## Current State And Activation

Status as of 2026-08-13: the original contiguous CPU package baseline is
complete. NumPy-compatible CPU array behavior is now required for 0.0.7 and is
tracked by ND8 through ND19; remote immutable publication remains pending.

- The external `terlan-ndarray` package checkout now freezes ABI v1 and owns
  deterministic generated bindings, native/Terlan execution tests, and native
  dependency provenance.
- The generic C ABI generator now supports borrowed one-byte boolean arrays as
  a package-neutral `List[Bool]` contract without ndarray-specific compiler
  code.
- `make terlan-ndarray-package-check` passes the compiler contract suite, all
  42 C ABI generator tests, deterministic generation, adversarial metadata
  cases, warning-denied C/Rust builds, native sanitizers, generated helper
  lifecycle tests, and a revision-locked external Terlan consumer.
- `make terlan-ndarray-blas-check` passes real OpenBLAS LP64 provider
  admission, CBLAS `dgemm` semantics, package and immutable-consumer execution,
  stable provider rejection cases, and the non-gating five-size benchmark.
- `make terlan-ndarray-release-check` passes the complete package, CBLAS,
  DLPack, ndarray-to-PyTorch, Polars-to-PyTorch, transformed
  Polars-to-ndarray-to-PyTorch, and source-deleted immutable consumer lanes.
- ND7 completed the original executable technical baseline. Publishing the
  package repositories and replacing integration revisions with the resulting
  remote commits remains a release action.
- ND8 through ND19 replace the former MVP boundary with the required NumPy
  behavioral-compatibility program. The package is not complete for 0.0.7
  until those milestones pass.

The compiler release gate invokes the package closure gate. PyTorch autograd
remains PyTorch-owned, while NumPy-compatible CPU array semantics belong to
`terlan-ndarray`. CUDA is an optional device backend that must preserve the
same admitted array behavior without becoming a CPU installation dependency.

Progress is measured by the executable checkboxes under each milestone, not by
document creation, generated stubs, or native-only probes. A milestone status
changes to `Complete` only when every checkbox and its named gate pass together.

## Decision

Terlan will use a separate package and repository named `terlan-ndarray`.

- Repository: `terlan-lang/terlan-ndarray`.
- Terlan package: `terlan-ndarray`.
- Public namespace: `ndarray`.
- Primary public type: opaque `ndarray.Array`.
- Default execution profile: CPU arrays with NumPy-compatible strided views.
- Native package boundary: generated stable C ABI metadata and
  `NativeBoundary` resource handles.
- Tensor interchange: versioned pointer-free TNXP values with DLPack-compatible
  dtype, shape, stride, device, and ownership semantics.
- Columnar conversion: Polars-owned checked materialization into TNXP.
- Numerical kernels: CBLAS first and LAPACKE only when a public operation
  requires it.

The package is not a NumPy or CPython binding. It implements NumPy-compatible
array behavior through Terlan-native types and errors and must run without
Python or NumPy installed. NumPy is the behavioral reference for dtypes,
indexing, slicing, views, broadcasting, elementwise operations, reductions,
shape composition, creation, serialization, and CPU linear algebra.

Completion requires behavioral feature parity with the pinned NumPy CPU
numerical surface by module, not an MVP subset. Every NumPy operation must be
implemented, classified as a typed Terlan equivalent, or recorded as
non-applicable because it exists only for CPython integration, object dtype,
ndarray subclassing, or another runtime mechanism Terlan intentionally does
not expose. Unclassified omissions and placeholder implementations fail the
release gate.

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

## Compatibility Scope

The original package baseline established:

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

For 0.0.7, ND8 through ND19 extend that baseline to behavioral parity with
NumPy's homogeneous numerical CPU `ndarray` surface. Parity means equivalent
observable array results, shape rules, dtype promotion, indexing, views,
broadcasting, reductions, composition, random reproducibility contracts,
serialization, and linear-algebra behavior where the corresponding capability
is admitted. Terlan uses typed `Result` values and Terlan naming rather than
copying Python exceptions, dynamic dispatch, or Python-specific call syntax.

The following remain outside `terlan-ndarray` ownership:

- CPython embedding and NumPy's Python object/C extension ABI;
- object, string, categorical, nullable, record, and heterogeneous arrays;
- Python subclass hooks, `__array_function__`, `__array_ufunc__`, and Python
  iterator/protocol integration;
- autograd, models, optimizers, and training, which belong to
  `terlan-pytorch`;
- DataFrame query operations, which belong to `terlan-polars`;
- image decoding and computer-vision algorithms, which belong to
  `terlan-opencv`;
- distributed arrays and sparse matrix packages;
- unsafe raw pointers or arbitrary borrowed host buffers exposed to Terlan
  source.

Those responsibilities remain with specialized packages. In particular,
`terlan-pytorch` owns autograd and ML tensors, `terlan-polars` owns DataFrames,
and `terlan-opencv` owns image and computer-vision values.

## Frozen Baseline Semantics

The first implementation must use one deliberately narrow semantic model. This
prevents BLAS, DLPack, Polars, and PyTorch integrations from each inventing a
different interpretation of an array.

### Original Logical Array Baseline

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

### Shared Storage And Ownership Model

Constructors copy inputs and readback copies outputs. Basic slicing, indexing,
reshape where layout permits, transpose, permutation, and broadcasting return
shared strided views. Explicit `copy` and `contiguous` operations return
independently owned storage. Storage remains live until its final array or view
is disposed, and disposing one handle must never invalidate another live view.
Writable views expose mutation to every alias of the same element;
zero-stride broadcast views are read-only.

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

Raw borrowed pointers remain forbidden at the Terlan boundary. Shared storage
is reference counted by package-owned code and external ownership callbacks are
retained exactly once until the final dependent view is released.

### Operation contracts

- `reshape` requires the same element count, returns a view when strides permit,
  and otherwise materializes according to NumPy-compatible order rules.
- `transpose(axis0, axis1)` validates normalized axes, including negative axis
  shorthand, and returns a shared strided view.
- Elementwise arithmetic applies the canonical NumPy-compatible promotion and
  trailing-axis broadcasting rules defined by ND9.
- `sum` rejects duplicate and out-of-range axes. The order of supplied axes
  does not change the result. Integer overflow behavior must be selected and
  documented before `Int64` reduction is public.
- `matmul` follows vector, matrix, and broadcast-batched NumPy behavior and may
  explicitly materialize arbitrary strided operands before maintained BLAS
  calls.
- Every fallible public operation returns a typed Terlan `Result`; native status
  values are never exposed as the public error API.

## Repository Ownership

| Repository | Owns | Must not own |
| --- | --- | --- |
| `terlan` compiler repository | Generic C metadata validation and generation, package execution, `NativeBoundary` resource and cross-package handoff contracts, stable generic diagnostics, and package-neutral fixtures | Numerical algorithms, a public `ndarray` API, BLAS linkage, or package-specific conversion code |
| future `terlan-ndarray` repository | Public array API, C metadata, package-owned native implementation, DLPack mapping, BLAS/LAPACK linkage policy, fixtures, package tests, and consumer tests | Compiler parsing/lowering forks, Polars query logic, PyTorch operations, or OpenCV algorithms |
| `terlan-polars` repository | `Series.to_array`, `DataFrame.to_array`, column selection, casting, null policy, and TNXP materialization | Array storage, PyTorch implementation, or direct tensor construction |
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
                         +-> terlan-ndarray -> transformed TNXP -+
terlan-polars -> TNXP ---+                                      +-> terlan-pytorch
                         +---------------------------------------+
                         +-> terlan-cuda (optional)
```

`terlan-polars` owns the high-level conversion entrypoints because conversion
out of a DataFrame requires Polars-specific column selection, casting, null,
and chunk policy. `terlan-pytorch` remains usable without Polars.

The intended user paths are:

```text
List values + shape -> ndarray.Array
Polars DataFrame    -> DataFrame.to_array -> ndarray.Array
ndarray.Array       -> Tensor.from_array  -> pytorch.Tensor
Polars DataFrame    -> DataFrame.to_pytorch_packet -> Tensor.from_packet
```

Direct packet consumption avoids a redundant ndarray import/export when no
array operation is required. ndarray remains the canonical general-purpose
array owner, not a mandatory transit package.

## Stable Native Foundations

No single dependency supplies NumPy's entire combination of array ownership,
indexing, broadcasting, algorithms, and ecosystem interoperability. The
package therefore composes small, durable boundaries.

| Foundation | Role | Explicit non-role |
| --- | --- | --- |
| TNXP with DLPack semantics | Pointer-free homogeneous tensor dtype, shape, strides, device, versioning, and checked copied exchange | Array implementation or numerical operators |
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

Polars does not automatically turn a heterogeneous DataFrame into one
homogeneous row-major array. The package must perform that conversion under an
explicit checked policy.

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
columns into one contiguous row-major TNXP value. Arrow C Data is not part of
this contract.

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

Status: Complete.

- [x] Acquire or locate the selected released CBLAS provider in the package
  build without adding BLAS linkage to `terlc` or the VM.
- [x] Validate provider version, library identity, architecture, calling
  convention, and LP64/ILP64 integer width before execution.
- [x] Implement rank-2 row-major Float64 matrix multiplication through CBLAS
  `dgemm` with checked dimension conversion and output allocation.
- [x] Define zero-dimension matmul behavior and prove the implementation does
  not pass invalid pointers to a provider for an empty operation.
- [x] Compare exact integer-valued fixtures and tolerance-based fractional
  fixtures with a package-owned reference implementation outside timed runs.
- [x] Exercise rectangular, identity, zero, non-finite, incompatible-shape,
  unsupported-dtype, integer-width, missing-provider, and allocation-failure
  cases.
- [x] Execute matmul from package Terlan tests and from a fresh immutable Git
  consumer after deleting access to the source checkout.
- [x] Record provider provenance and semantic result in deterministic reports;
  reject an unidentifiable or incompatible library rather than guessing.
- [x] Add a non-gating benchmark for sizes `1`, `16`, `64`, `256`, and `1024`
  with warmup and at least three samples. Benchmark results cannot replace
  semantic acceptance.

Exit: exact non-square matrix multiplication passes against the real provider;
missing provider, incompatible shapes, unsupported dtype, and ABI mismatch are
rejected deterministically.

Gate: `make terlan-ndarray-blas-check`.

Completed progress: the package resolver downloads the immutable OpenBLAS
source over HTTPS, verifies its SHA-256 before safe extraction, and reuses a
content-addressed cache. The generated package resolves the resulting static,
dynamic-architecture provider through generic `pkg-config` metadata while
keeping BLAS out of `terlc` and the VM. Provider admission records source and
library digests, version, architecture, C calling convention, LP64 integer
width, configuration, and fixed build options; missing, wrong-width,
wrong-architecture, and insufficient-version providers fail with stable
diagnostics. Rank-2 row-major Float64 `matmul` executes through `cblas_dgemm`,
skips provider calls for empty work, and passes package-owned reference,
allocation-failure, sanitizer, Terlan, and deleted-source immutable-consumer
tests. Deterministic semantic evidence is in
`reports/ndarray-blas-report.json`; non-gating measurements for all five sizes
are isolated in `reports/ndarray-blas-benchmark.json`.

### ND4 - DLPack Structure And Ownership Contract

Status: Complete.

- [x] Add a package-neutral one-shot native exchange resource to
  `NativeBoundary` with typed producer/consumer identities and atomic
  `Available -> Claimed -> Closed` transitions.
- [x] Prove cancellation, actor exit, helper crash, consumer rejection, VM
  shutdown, and abandoned-resource cleanup invoke exactly one producer cleanup
  action without holding runtime registry locks.
- [x] Implement package-native DLPack export for baseline CPU arrays using the
  pinned versioned structures and a package-owned manager context.
- [x] Implement DLPack import as an owned copy, preserving source readability
  and invoking the producer deleter after the copy or on every rejection path.
- [x] Validate DLPack version, device, dtype code/bits/lanes, rank, dimensions,
  nullability, strides, byte offset, pointer alignment, element/byte overflow,
  and supported contiguous layout before reading data.
- [x] Reject second claim, wrong exchange kind, stale token, forged scalar token,
  and mismatched producer/consumer capability with stable diagnostics.
- [x] Add adversarial native fixtures whose deleter increments an atomic audit
  counter; require exactly one event under success and each failure phase.
- [x] Run producer and consumer as independently built helpers through the VM
  broker. A direct in-process adapter call does not satisfy this item.
- [x] Preserve list construction/readback as an always-available owned-copy
  fallback independent from DLPack support.

Exit: two independently built native package fixtures exchange an array through
the runtime broker without exposing or serializing a pointer.

Gate: `make ndarray-dlpack-interop-check`.

Completed progress: the VM owns a package-neutral, authenticated one-shot
exchange registry and transports only validated pointer-free tensor packets.
The ndarray producer and an independently generated value-only C consumer build
as separate helpers, resolve from immutable Git revisions after their source
checkouts are removed, and pass success, duplicate claim, wrong consumer,
consumer rejection, helper crash, and abandonment flows. Native DLPack tests
cover exact deleter cardinality, malformed metadata, packet truncation and
trailing data, allocation failure at every owned phase, zero dimensions, and
source readability. Deterministic evidence is recorded in
`reports/ndarray-dlpack-report.json`.

### ND5 - Polars To Array

Status: Complete; implementation owner is `terlan-polars`.

- [x] Keep Polars independently installable and exchange pointer-free TNXP
  values with `terlan-ndarray`; no direct package dependency is required.
- [x] Implement `Series.to_array` for supported primitive bool/integer/Float64
  data with explicit Float64 casting and null policy. The public API emits the
  same pointer-free TNXP exchange token as DataFrame conversion.
- [x] Implement `DataFrame.to_array(columns, dtype, null_policy)` with stable
  column ordering and exact `[rows, columns]` row-major output.
- [x] Keep Arrow C Data out of the initial interoperability contract. Polars
  materializes supported columns into the checked pointer-free TNXP exchange;
  direct Arrow C Data ownership is deferred until benchmarks demonstrate that
  the copy is a material bottleneck.
- [x] Consolidate selected cast columns into one chunk before encoding. Reject
  nested, string, categorical, temporal, object, and other unsupported data
  without interpreting provider-private layouts; mixed admitted primitive
  columns follow the explicit Float64 conversion request.
- [x] Cover empty frames, empty selections, empty/missing/duplicate column
  names, chunked columns, nulls, mixed supported primitive numeric types, and
  representative temporal and nested physical layouts.
- [x] Record non-applicable failure classes explicitly: admitted primitive
  integer values remain finite when explicitly cast to Float64, valid Polars
  DataFrames enforce equal column lengths, and the owned TNXP path has no Arrow
  release callback.
- [x] Execute the Iris conversion path: select four Float64 feature columns from 150 rows,
  produce exact shape `[150, 4]`, verify bounded known values, and release all
  Polars resources after producing independently owned exchange data.
- [x] Run the integration from a fresh consumer using immutable revisions of
  both packages.

Progress evidence: the Polars native adapter now exposes
`polars.series.export_tensor_packet` and `Series.to_array`, reusing the checked
DataFrame encoder. Ten focused real-Polars tensor tests pass, including
boolean and integer casting, row-major layout, null rejection, unsupported
strings, empty frames and selections, missing and duplicate columns,
unsupported dtype and null policy, invalid consumer namespaces, explicit
multi-chunk consolidation without source mutation, mixed primitive numeric
conversion, temporal and nested rejection, and the exact 150-row Iris feature
conversion. Arrow C Data is intentionally deferred: neither the
Polars-to-ndarray path nor the DLPack-based ndarray-to-PyTorch path exposes raw
Arrow pointers or release callbacks. The current workspace compiler accepts
the complete Polars declaration surface, including negative constant defaults.

Exit: a fresh consumer reads the checked Iris CSV with Polars, selects four
Float64 feature columns, obtains an exact `[150, 4]` array, verifies selected
values, and cleans up every Polars and ndarray resource.

Gate: `make polars-ndarray-interop-check`.

### ND6 - Array To PyTorch

Status: Complete; implementation owner is `terlan-pytorch`.

- [x] Keep PyTorch independently installable and consume pointer-free TNXP
  values produced by `terlan-ndarray`; keep Polars absent from the PyTorch
  dependency graph.
- [x] Implement `Tensor.from_array` through the reviewed pointer-free DLPack
  exchange broker, not through a package-specific pointer tunnel or compiler
  branch.
- [x] Make the first operation an owned copy, preserve supported CPU dtype and
  shape, and leave the source `Array` readable after tensor construction.
- [x] Execute exact non-constant Bool, Int64, and Float64 conversions, then run
  one observable PyTorch operation on each supported tensor dtype.
- [x] Cover unsupported dtype/device/layout, malformed DLPack metadata, stale
  source, consumer rejection, native exception, double claim, and cleanup after
  actor cancellation.
- [x] Require producer and consumer cleanup audits to show one DLPack deleter
  call and independently valid source/tensor destruction in every order.
- [x] Expose `DataFrame.to_pytorch_packet` and `Tensor.from_packet` over the
  same TNXP protocol. Route through ndarray only when array operations are
  required; do not force a redundant import/export hop.
- [x] Execute the complete direct Iris feature path and verify tensor shape `[150, 4]`
  plus an exact deterministic model or reduction result.
- [x] Execute the transformed Iris feature path through three independent
  package helpers: import `[150, 4]` into ndarray, transpose to `[4, 150]`,
  export a new PyTorch-targeted packet, verify tensor shape and maximum `7.9`,
  and return ndarray live allocations to their starting count.

Progress evidence: four independent ndarray/PyTorch consumer tests pass. The
matrix lane creates a `[2, 3]` ndarray, imports an owned PyTorch tensor, runs
`mse_loss`, and verifies shape, dtype, values, and cleanup. Three additional
lanes export non-constant Bool, Int64, and Float64 vectors, run PyTorch
`narrow`, read exact typed scalar values, and return ndarray allocations to
their starting count. Six package-level adversarial executions reject duplicate
claims, wrong consumers, stale ndarray handles, malformed layouts, unsupported
devices, and unsupported dtypes with stable diagnostics. Six VM broker tests
cover native metadata admission, one-shot claims, forgery, actor exit, helper
failure, and shutdown cleanup; native failures remain typed adapter errors and
do not cross the boundary as exceptions. The PyTorch native DLPack suite passes
seven tests with the repository CPU fixture, including malformed header,
trailing data, truncation, broadcast stride, unsupported dtype, and scalar-type
cases. The seventh native packet test audits the package-private packet manager and
observes exactly one deleter call on both success and rejection, with no second
call when the independently owned tensor is dropped. Terlan consumer tests
dispose the tensor first and continue reading the ndarray source, then dispose
the ndarray source first and continue executing PyTorch operations on the
tensor.
The official `terlan-pytorch` `ndarray-interop-check` also passes with pinned
LibTorch 2.13 CPU, including C ABI generation, native helper compilation,
Terlan consumer execution, tensor reduction, and independent cleanup.
The direct Polars-to-PyTorch gate reads Iris, imports one PyTorch-targeted TNXP
packet without an ndarray relay, verifies `[150, 4]`, and reduces to the exact
maximum `7.9`.
The transformed gate imports a Polars-targeted packet into ndarray, transposes
the owned array to `[4, 150]`, exports a new PyTorch-targeted packet, verifies
the transformed tensor and the same exact maximum, and disposes both ndarray
owners independently.
The immutable variant resolves exact Polars, ndarray, and PyTorch Git snapshots,
deletes the source repositories, disables Cargo network access, and repeats the
same test using only the fresh consumer's package cache.

Exit: an array containing non-constant feature values becomes a PyTorch tensor,
executes an exact operation, and releases producer/consumer ownership exactly
once.

Gate: `make ndarray-pytorch-interop-check`.

### ND7 - Fresh Package And Adversarial Closure

Status: Technical closure complete; remote publication blocked externally.

- [ ] Publish and pin immutable revisions for ndarray and every participating
  integration package.
  Local HTTPS has no GitHub credentials, local SSH has no accepted key, and
  the connected GitHub app is not installed for `terlan-lang`. This is the only
  remaining ND7 action and cannot be satisfied by additional package code.
- [x] Fetch into a fresh Terlan consumer, remove sibling checkout access, disable
  network access, and execute only from the verified package cache.
- [x] Run the complete baseline, conformance, ownership, allocation-failure,
  malformed-input, and cross-package integration suites.
- [x] Run native sanitizers or equivalent platform memory tooling and fail on
  leak, use-after-free, double-free, undefined behavior, or foreign unwind.
- [x] Run package-generated Rust with warnings and deprecations denied, package
  C/C++ with warnings denied, and enforce repository file-size/complexity gates.
- [x] Validate deterministic ABI, package, BLAS, DLPack, Polars, and PyTorch
  reports against schemas and regenerate each twice for stability.
- [x] Prove no compiler path, `std.native` namespace, Python/NumPy installation,
  CUDA installation, unpinned download, or unpublished local dependency is
  required.
- [x] Prove every public Terlan function has positive, boundary, and adversarial
  coverage and that the package meets the enforced coverage baseline.
- [x] Record `x86_64-unknown-linux-gnu` as the validated 0.0.7 host and retain
  macOS, Windows, AArch64, and other hosts as explicitly unsupported until
  their release lanes pass.

Progress evidence: `make terlan-ndarray-release-check` passes the compiler C ABI
contract (42 tests), VM exchange broker (6 tests), generated package lifecycle,
native and adversarial suites, ASan/UBSan/leak checks, CBLAS execution, and
DLPack ownership. The same gate passes five ndarray-to-PyTorch cases, six
adversarial exchange cases, direct and ndarray-transformed Iris flows, and an
immutable three-package Iris consumer after removing source checkout access and
disabling Cargo network access. Five package report schemas regenerate
byte-identically across two runs, and the three-package integration report also
renders identically twice; variable benchmark timings are intentionally
excluded. The quality gate compiles generated Rust and package C with warnings
denied, enforces source-size and structural-complexity limits, rejects forbidden
runtime dependencies, and resolves positive, boundary, and adversarial evidence
for all 25 public Terlan functions. Remote package publication remains open.

Exit: `terlan-ndarray-package-check` passes in a fresh CPU-only workspace and
all required reports validate.

Gate: `make terlan-ndarray-release-check`.

### ND8 - Canonical Dtypes, Storage, Indexing, And Views

Status: Required for 0.0.7.

- [x] Replace per-array raw ownership with one reference-counted storage owner,
  byte offset, shape, and element strides; retain deterministic final release.
- [x] Admit `Bool`, signed and unsigned 8/16/32/64-bit integers, Float16,
  Float32, Float64, Complex64, and Complex128 with one canonical dtype table.
- [x] Define safe promotion, exact conversion, narrowing, overflow, NaN, and
  complex-to-real policies for every dtype pair.
- [x] Implement scalar indexing, negative indexes, indexed assignment, and
  bounds diagnostics for arbitrary rank.
- [ ] Implement basic slices with optional start/stop, nonzero positive or
  negative steps, axis insertion/removal, and ellipsis expansion.
- [x] Implement shared non-contiguous views, explicit owned copies, contiguous
  materialization, and source/view mutation visibility.
- [x] Make metadata, readback, DLPack, tensor packets, and disposal correct for
  offsets, arbitrary strides, zero-sized dimensions, and shared storage.
- [ ] Inventory this milestone after its gate passes and update ND9.

Gate: `make indexing-view-check`.

### ND9 - Broadcasting And Dtype Promotion

Status: Required for 0.0.7.

- [x] Implement one checked trailing-axis broadcast planner shared by every
  elementwise operation.
- [x] Implement `broadcast_to` as a zero-stride read-only view and reject
  mutation that would alias one storage element through multiple coordinates.
- [ ] Apply the canonical promotion table to mixed-dtype arithmetic,
  comparison, selection, concatenation, reduction, and linear algebra.
- [x] Implement scalar-array and array-array add, subtract, multiply, true
  divide, floor divide, remainder, and power with broadcasting.
- [ ] Cover scalars, zero dimensions, rank mismatch, incompatible dimensions,
  extreme rank, integer overflow, divide-by-zero, and allocation failure.
- [ ] Inventory this milestone after its gate passes and update ND10.

Gate: `make broadcasting-promotion-check`.

### ND10 - Elementwise Functions, Comparisons, And Reductions

Status: Required for 0.0.7.

- [x] Implement equal/not-equal and ordered comparisons with Bool outputs plus
  logical not/and/or/xor for Bool arrays.
- [x] Implement negate, absolute value, sign, square, square root, reciprocal,
  exponential, logarithm, trigonometric functions, floor, ceil, and round.
- [x] Implement `where` over broadcast-compatible condition/branch arrays.
- [x] Implement sum, product, mean, minimum, maximum, any, all, argmin, argmax,
  variance, and standard deviation over normalized axis sets with `keep_dims`.
- [ ] Freeze empty-input, NaN, infinity, integer accumulation, tie-breaking,
  degrees-of-freedom, and overflow semantics.
- [x] Ensure every operation executes directly over arbitrary strides or uses
  one explicit shared materialization helper when a native library requires it.
- [ ] Inventory this milestone after its gate passes and update ND11.

Gate: `make elementwise-reduction-check`.

### ND11 - Shape Composition

Status: Required for 0.0.7.

- [x] Implement two-input concatenate and stack, exact split, uneven
  array-split, indexed split, repeat, tile, flip, roll, squeeze, and
  expand-dims.
- [ ] Generalize concatenate and stack to arbitrary input lists and implement
  deterministic constant, edge, reflect, symmetric, wrap, and statistical
  padding modes.
- [x] Implement full axis permutations and a canonical transpose shorthand.
- [ ] Preserve dtype promotion, non-contiguous inputs, empty dimensions,
  independent output ownership, and deterministic result ordering.
- [x] Return multiple owned arrays through one reviewed generated resource-list
  ABI with complete partial-failure cleanup.
- [ ] Inventory this milestone after its gate passes and update ND12.

Gate: `make shape-composition-check`.

### ND12 - Creation And Deterministic Random Generation

Status: Required for 0.0.7.

- [x] Implement zeros, ones, full, identity, arange, linspace, and diagonal
  construction for all applicable dtypes.
- [x] Define and implement the safe `empty` construction contract without
  exposing uninitialized native memory.
- [x] Use maintained ChaCha8 generation with explicit seeds for uniform,
  normal, integer, Bernoulli, permutation, and functional shuffle operations
  without ambient process-global randomness.
- [x] Add an owned deterministic stream-state resource that composes the same
  random operations while advancing state explicitly.
- [x] Define reproducibility by package version, algorithm identifier, seed,
  dtype, and shape and persist deterministic vectors in the release report.
- [x] Cover invalid ranges, zero-sized outputs, non-finite bounds, rejection
  sampling limits, overflow, and allocation failure.
- [x] Inventory this milestone after its gate passes and update ND13.

Gate: `make creation-random-check`.

### ND13 - CPU Linear Algebra

Status: Required for 0.0.7.

- [x] Generalize dot and matmul across vector, matrix, and broadcast batched
  forms using maintained BLAS kernels where applicable.
- [x] Add strided, dtype-promoted trace with offsets and selectable axes.
- [x] Add determinant, square solve with vector or matrix right-hand sides, and
  inverse through pinned maintained `faer` kernels without a second BLAS
  provider in the process.
- [x] Add matrix/vector norms, least squares, Cholesky, QR, SVD, and symmetric
  eigen decomposition through the same maintained `faer` provider.
- [x] Define row-major conversion, workspace ownership, singularity,
  convergence, rank, tolerance, and non-finite-input diagnostics.
- [x] Execute numerical conformance against independent fixtures across square,
  rectangular, rank-deficient, ill-conditioned, empty, and non-contiguous data.
- [x] Inventory this milestone after its gate passes and update ND14.

Gate: `make linear-algebra-check`.

### ND14 - Serialization, Arrow, And Memory Mapping

Status: Required for 0.0.7.

- [ ] Implement bounded deterministic `.npy` read/write for every admitted
  scalar dtype, shape, byte order, and contiguous materialization policy.
- [ ] Implement package-owned raw-byte serialization with version, dtype,
  shape, integrity, and maximum-size validation.
- [ ] Implement read-only and read-write memory-mapped CPU arrays through a
  maintained mapping library with explicit flush and lifetime behavior.
- [ ] Implement Arrow C Data import/export for compatible primitive arrays,
  including offsets, validity, ownership callbacks, and exact null policy.
- [ ] Prove malformed headers, truncated data, path failures, oversized inputs,
  endian mismatch, callback failure, and partial construction release.
- [ ] Inventory this milestone after its gate passes and update ND15.

Gate: `make serialization-arrow-check`.

### ND15 - Advanced Indexing, Ordering, Searching, And Set Operations

Status: Required for 0.0.7.

- [x] Implement axis-based `take`, `nonzero`, `flatnonzero`, `argwhere`, and
  flattened `count_nonzero` over arbitrary-rank contiguous and strided arrays.
- [ ] Complete multidimensional integer-array and Boolean-mask indexing,
  assignment, `take_along_axis`, `put`, `put_along_axis`, and `compress`.
- [x] Implement stable `sort`, `argsort`, `searchsorted`, and flattened
  `count_nonzero` with one shared complex, signed-zero, infinity, and NaN-last
  ordering contract.
- [ ] Complete selectable unstable sorting, `partition`, `argpartition`,
  axis-aware `count_nonzero`, sorter-aware `searchsorted`, and lexicographic
  multi-key ordering.
- [ ] Implement `unique` with optional index, inverse, and count outputs plus
  intersect, union, difference, membership, and exclusive-or set operations.
- [x] Add the generated multi-resource return contract required by `nonzero`,
  `unique`, split families, and other operations that atomically return more
  than one owned array.
- [ ] Validate parity against version-pinned NumPy fixtures for empty inputs,
  duplicate values, repeated indexes, negative indexes, masks, non-contiguous
  views, all dtypes, and partial-allocation cleanup.
- [ ] Inventory this milestone after its gate passes and update ND16.

Gate: `make indexing-ordering-set-check`.

### ND16 - Statistics, Histograms, And Numerical Utilities

Status: Required for 0.0.7.

- [ ] Implement median, quantile, percentile, covariance, correlation,
  weighted average, peak-to-peak range, NaN-aware reductions, and cumulative
  sum/product with explicit interpolation and degrees-of-freedom policies.
- [ ] Implement histogram, histogram-bin edges, multidimensional histogram,
  bincount, digitize, gradient, difference, unwrap, interpolation, and
  trapezoidal integration over arbitrary strided arrays.
- [ ] Complete bitwise operations, shifts, gcd/lcm, sign-bit and floating-point
  decomposition, angle conversion, tolerance comparison, and missing
  transcendental ufunc families.
- [ ] Freeze warning/error behavior for empty slices, non-finite values,
  invalid bins, zero weights, integer overflow, duplicate coordinates, and
  interpolation outside the admitted domain.
- [ ] Inventory this milestone after its gate passes and update ND17.

Gate: `make statistics-numerical-check`.

### ND17 - Discrete Fourier Transforms

Status: Required for 0.0.7.

- [ ] Integrate a maintained CPU FFT provider and implement one-dimensional,
  multidimensional, real, Hermitian, inverse, frequency-bin, and shift APIs
  with NumPy-compatible axes and normalization modes.
- [ ] Reuse array storage, dtype, stride, materialization, and workspace
  ownership contracts without exposing provider plans or pointers publicly.
- [ ] Validate prime, composite, odd, even, empty, batched, non-contiguous,
  Float32/64, and Complex64/128 fixtures against version-pinned NumPy output.
- [ ] Inventory this milestone after its gate passes and update ND18.

Gate: `make fft-check`.

### ND18 - Polynomial And General Numerical Algebra

Status: Required for 0.0.7.

- [ ] Implement polynomial evaluation, roots, fitting, arithmetic,
  differentiation, integration, basis conversion, and companion matrices for
  the admitted numerical dtypes.
- [ ] Implement tensor contraction, Einstein summation, Kronecker product,
  outer/inner products, cross product, and generalized dot operations using
  maintained kernels where applicable.
- [ ] Freeze coefficient ordering, domain/window mapping, rank diagnostics,
  complex behavior, and ill-conditioned fit errors against NumPy fixtures.
- [ ] Inventory this milestone after its gate passes and update ND19.

Gate: `make polynomial-numerical-algebra-check`.

### ND19 - Device-Native Arrays And Feature-Complete Closure

Status: Required for 0.0.7.

- [ ] Extend the package-neutral array resource contract to CPU and admitted
  accelerator storage without exposing pointers or backend handles.
- [ ] Implement optional CUDA-native construction, transfer, elementwise,
  reduction, composition, and linear-algebra paths through `terlan-cuda` while
  keeping the CPU package independently installable.
- [ ] Implement stream-correct DLPack import/export and direct PyTorch/OpenCV
  exchange where device, dtype, layout, and ownership are compatible.
- [ ] Keep unsupported devices typed and ensure CPU-only installation never
  links or downloads CUDA.
- [ ] Run the complete public API matrix on contiguous CPU, non-contiguous CPU,
  and every admitted device backend; record behavioral parity and typed skips.
- [ ] Publish and pin immutable package revisions, run fresh offline consumers,
  and regenerate all evidence twice.
- [ ] Inventory the complete ndarray surface against this roadmap and reject
  release while any required 0.0.7 checkbox remains open.

Gate: `make feature-complete-release-check`.

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
    -> ND8 -> ND9 -> ND10 -> ND11 -> ND12 -> ND13 -> ND14 -> ND15
    -> ND16 -> ND17 -> ND18 -> ND19
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
- every admitted scalar dtype can be constructed, indexed, sliced, viewed,
  mutated, broadcast, transformed, reduced, serialized, and read back;
- mixed-dtype operations follow one tested promotion table and all operations
  handle contiguous and non-contiguous layouts correctly;
- creation, deterministic random, shape composition, and the required
  elementwise/reduction surface execute through public Terlan APIs;
- CBLAS matrix multiplication executes with verified provider provenance;
- maintained LAPACK-backed solve and decomposition operations execute with
  stable numerical and failure semantics;
- DLPack ownership transfer is safe across package helpers;
- Arrow C Data, `.npy`, raw serialization, and memory-mapped arrays satisfy
  bounded ownership and malformed-input contracts;
- CPU and every admitted device backend pass the same behavioral matrix;
- Polars-to-array and array-to-PyTorch consumers execute exact semantic checks;
- all required negative cases have stable diagnostic families;
- fresh consumers pass without Python, NumPy, CUDA, or network access;
- every owned native resource is released exactly once.

## Reference Contracts

- DLPack: <https://dmlc.github.io/dlpack/latest/c_api.html>
- Netlib BLAS: <https://www.netlib.org/blas/>
- LAPACKE: <https://netlib.org/lapack/lapacke.html>
