# Terlan 0.0.9 Accelerator And CUDA Integration Roadmap

This roadmap defines compiler and VM work required to make external accelerator
packages first-class AOT execution targets. `terlan-cuda` is the first proof
package, but all compiler and runtime mechanisms in this document must remain
backend-neutral.

CUDA APIs, NVIDIA version policy, kernels, memory management, streams, graphs,
and multi-GPU behavior belong to the external `terlan-cuda` package. The
compiler owns static semantics and artifact production. The VM owns generic
asynchronous execution, actor lifecycle, wakeup, cancellation, accounting, and
inspection.

## Release Objective

Terlan 0.0.9 must be able to:

1. resolve an external accelerator package without compiler source changes;
2. typecheck device resources and asynchronous operations through ordinary
   Terlan package declarations;
3. compile an admitted Terlan kernel subset ahead of time into a package-loadable
   accelerator artifact;
4. specialize host/device work and transfers from whole-program evidence;
5. suspend actors around asynchronous device work without blocking scheduler
   threads;
6. include accelerator runtime components only when reachable code requires
   them; and
7. emit reproducible manifests proving code, package, toolchain, target,
   capability, ownership, and cleanup decisions.

The release does not make CUDA part of the language or standard library. A
Terlan installation and ordinary CPU application must remain independent of
CUDA.

## Hard Contracts

- [x] Add no `cuda`, `gpu`, `kernel`, `device`, or accelerator-specific keyword
  to the Terlan grammar for this work.
- [x] Represent accelerator support through ordinary packages plus generic,
  versioned compiler metadata.
- [x] Keep accelerator artifacts AOT-only. Do not add a runtime JIT to the
  compiler or VM.
- [x] Keep host and accelerator execution ownership explicit; never silently
  fall back from a requested accelerator operation to CPU.
- [x] Compile ordinary pure host code through the existing native pipeline and
  send only admitted accelerator regions to an accelerator backend.
- [x] Use one canonical compiler-owned dtype, shape, layout, device, resource,
  operation, completion, and artifact identity model. Package adapters must not
  create parallel compiler types.
- [x] Preserve actor isolation: no accelerator pointer, handle, callback, or
  package thread may directly enter actor memory or execute actor code.
- [x] Preserve scheduler progress: blocking driver and package work must run
  outside scheduler workers, and asynchronous completion must wake actors
  through one VM API.
- [x] Make actor exit, cancellation, supervision restart, package failure,
  device loss, and VM shutdown converge on generic resource and completion
  cleanup contracts.
- [x] Reject unavailable targets, undeclared capabilities, unsupported kernel
  operations, unbounded resource requirements, and incompatible package
  artifacts at compile or load time with stable diagnostics.
- [x] Use maintained backend toolchains and libraries. The compiler must not
  hand-roll PTX parsing, CUDA drivers, BLAS, DNN kernels, collectives, or
  profilers.
- [x] Build the compiler, CPU packages, and CPU applications without CUDA
  headers, libraries, tools, drivers, devices, or package sources.
- [x] Inventory this section after its gate passes and revise downstream work
  if any contract requires a CUDA-specific compiler branch.

Gate: `make accelerator-hard-contract-check`.

Closeout inventory: the gate found no accelerator syntax or CUDA dependency in
the compiler workspace. CUDA names remain confined to the maintained LLVM
NVPTX backend adapter, quality fixtures, reports, and the external package.

## Ownership Boundary

| Layer | Owns | Does not own |
| --- | --- | --- |
| Parser and typechecker | Ordinary Terlan syntax, package declarations, generic resource/effect validation, kernel-subset admission, shape/type evidence | CUDA names, CUDA handles, launch implementation, or NVIDIA diagnostics |
| CoreIR and AOT compiler | Backend-neutral accelerator regions, static specialization, transfer planning, artifact descriptors, reproducibility, and source provenance | CUDA runtime APIs or package resource tables |
| Accelerator backend adapter | Conversion from admitted accelerator IR to one maintained target toolchain and target artifact | Terlan package API, actor scheduling, or runtime resource ownership |
| VM/runtime | Generic native-operation requests, suspension, wakeup, cancellation, cleanup, accounting, and inspection | CUDA context, stream, event, graph, module, or allocator implementation |
| `terlan-cuda` | CUDA resource and execution semantics plus target artifact loading | Compiler analysis, generic VM scheduling, or language syntax |

The dependency direction is:

```text
Terlan source
  -> parser/typechecker/CoreIR
  -> compiler-owned accelerator plan and AOT artifact
  -> external package descriptor and loader
  -> maintained backend runtime and libraries

VM actor
  -> generic asynchronous native request
  -> external package worker
  -> generic completion/wakeup
  -> VM actor
```

Neither path permits the package to call into parser/typechecker internals or
the VM to import CUDA implementation types.

## Numerical Package Convergence

The numerical ecosystem uses one directional ownership graph:

```text
                         +-> terlan-ndarray -> transformed packet -+
terlan-polars -> TNXP ---+                                      +-> terlan-pytorch
                         +---------------------------------------+
                         +-> terlan-cuda when explicitly selected
```

Packages remain independently installable. They exchange the compiler-owned,
versioned, pointer-free tensor packet through ordinary `Bytes` values and the
VM broker; no package imports another package's native implementation. Polars
owns tabular selection and null policy, ndarray owns dense numerical shape and
storage, PyTorch owns tensors and models, and CUDA owns explicit NVIDIA device
execution. Arrow C Data, CUDA IPC, and zero-copy exchange are optional measured
optimizations, not release prerequisites. ndarray is used when array
computation is required; it is not a mandatory packet relay.

## Execution Order

```text
AC0 freeze current package boundary
 -> AC1 accelerator package metadata
 -> AC2 canonical tensor and resource contracts
 -> AC3 target and toolchain admission
 -> AC4 accelerator IR and kernel subset
 -> AC5 maintained AOT backend
 -> AC6 whole-program placement and optimization
 -> AC7 VM asynchronous integration
 -> AC8 specialized artifact assembly
 -> AC9 ecosystem proof
 -> AC10 correctness, performance, and release closeout
```

Each milestone must pass before a later milestone depends on its output schema.
Prototype artifacts and handwritten package-specific compiler branches do not
complete a milestone.

## AC0: Freeze The 0.0.7 Boundary

- [x] Capture the 0.0.7 `terlan-cuda` manifest, generated bindings, tensor
  packet, package graph, native helper, and execution reports as immutable
  compatibility fixtures.
- [x] Run the current direct CUDA, PyTorch CUDA, OpenCV integration, and typed
  unavailable-capability lanes and record exact compiler/package revisions.
- [x] Inventory all compiler code that names CUDA, NVIDIA, `cudarc`, PTX,
  LibTorch CUDA, or package paths and classify each reference as generic
  infrastructure, release gate, test fixture, or legacy special case.
- [x] Remove or generalize compiler special cases before defining the new
  metadata schema; tests and package-owned scripts may retain CUDA fixture
  names.
- [x] Persist `target/quality/accelerator-boundary-baseline.json` with package
  identities, public operations, ABI schemas, capabilities, reports, and known
  gaps.
- [x] Inventory the milestone after its gate passes and update AC1 with every
  discovered package-owned value currently duplicated in compiler source.

Gate: `make accelerator-boundary-baseline-check`.

Closeout inventory: the immutable 0.0.7 package contract remains reproducible.
The CUDA LibTorch lane executes against the pinned `2.13.0+cu129` distribution.
The admitted OpenCV build remains CPU-only and is classified explicitly rather
than being substituted into CUDA comparisons.

## AC1: Accelerator Package Metadata

- [x] Extend `terlan.toml` package metadata with a versioned generic
  accelerator capability descriptor.
- [x] Describe backend ID, device classes, supported artifact formats, dtypes,
  layouts, address spaces, resource classes, asynchronous operations, required
  host libraries, toolchain requirements, and target availability without a
  CUDA-only field.
- [x] Describe package-provided kernels and compiler-produced kernel entrypoints
  through one canonical descriptor schema.
- [x] Represent operation effects including allocation, transfer, execution,
  synchronization, host callback, randomness, nondeterminism, and blocking
  behavior.
- [x] Resolve accelerator dependencies through ordinary locked package
  resolution and reject unknown schema versions, duplicate owners, capability
  cycles, unsupported targets, and inconsistent descriptors.
- [x] Make package metadata available to typechecking and build planning without
  loading the package's native implementation.
- [x] Emit stable diagnostics with package, capability, target, and source-span
  provenance.
- [x] Add a second synthetic accelerator package fixture proving the schema is
  not tied to CUDA names or behavior.
- [x] Persist `target/quality/accelerator-package-metadata.json` with normalized
  descriptors, dependency closure, owners, and rejection evidence.
- [x] Inventory the schema after the gate passes and move no package-specific
  compatibility table into compiler types.

Gate: `make accelerator-package-metadata-check`.

## AC2: Canonical Tensor And Resource Contracts

- [x] Define one compiler-owned scalar dtype model used by native arrays,
  tensor packets, accelerator buffers, kernel signatures, and inter-package
  exchange.
- [x] Define one checked shape/layout model covering rank, dimensions, strides,
  byte offset, contiguous order, alignment, element count, and byte size.
- [x] Define generic host, device, and externally managed address-space
  identities without exposing backend pointers.
- [x] Define opaque linear resource ownership for device contexts, allocations,
  streams, events, modules, kernels, graphs, and imported tensors.
- [x] Define borrow, ownership transfer, disposal, stale-handle, and
  exactly-once-deleter semantics shared by compiler-generated adapters and VM
  resource tables.
- [x] Promote the versioned tensor packet into a package-neutral interchange
  contract with dtype, shape, strides, device, stream, ownership, and deleter
  metadata.
- [x] Reject integer overflow, invalid rank, negative dimensions, invalid
  strides, incompatible layouts, unsupported dtypes, cross-device aliases,
  escaped borrows, and double ownership transfer before dispatch.
- [x] Generate package declarations and native adapter codecs from the canonical
  schemas rather than handwritten per-package copies.
  - [x] Generate descriptor-filtered Terlan scalar, layout, packet, and resource
    declarations consumed by `terlan-cuda`.
  - [x] Generate the native scalar codec and checked row-major shape model
    consumed by `terlan-cuda`.
  - [x] Generate the native copied-host packet codec and remove the package-local
    packet metadata parser.
  - [x] Generate the native transferred/borrowed resource codec and connect VM
    resource-table ownership to package adapters.
- [x] Persist `target/quality/accelerator-value-contract.json` with schema
  versions, generated adapters, ownership transitions, and rejection evidence.
- [x] Inventory canonical types after the gate passes and delete repeated dtype,
  shape, device, packet, and resource structs from compiler modules.

Gate: `make accelerator-value-contract-check`.

## AC3: Target And Toolchain Admission

- [x] Extend target profiles with optional accelerator backend, architecture,
  driver API, artifact format, toolchain, native library, memory, and
  determinism requirements.
- [x] Separate driver-only package execution from toolkit-required AOT kernel
  compilation.
- [x] Discover maintained external toolchains through explicit configuration,
  environment, or package metadata; never search and select an ambient
  installation silently.
- [x] Verify toolchain executable, version, target support, libraries, headers,
  licenses, and immutable identity before compilation.
- [x] Reject host/device architecture mismatches, unsupported compute targets,
  missing tools, incompatible driver requirements, and package artifacts built
  for a different backend contract.
- [x] Support CPU-only checking and package resolution without probing a CUDA
  device or loading an NVIDIA driver.
- [x] Add deterministic target plans for at least Linux x86-64 CUDA and one
  synthetic non-CUDA accelerator fixture.
- [x] Emit `accelerator-target-plan.json` with target, toolchain, package,
  architectures, artifact formats, capabilities, source provenance, and
  rejected assumptions.
- [x] Inventory target admission after the gate passes and ensure no package
  can bypass it through a custom build script.

Gate: `make accelerator-target-admission-check`.

## AC4: Accelerator IR And Kernel Subset

- [x] Define a typed backend-neutral `AcceleratorIR` derived from checked
  CoreIR, not from source-text rewriting or generated CUDA source templates.
- [x] Admit a deliberately bounded first kernel subset: scalar arithmetic,
  comparisons, structured control flow, static loops, indexing, local values,
  typed buffer loads/stores, and package-declared math operations.
- [x] Represent execution dimensions, address spaces, mutability, aliasing,
  alignment, synchronization, and shared-memory requirements explicitly.
- [x] Reject recursion, actor operations, allocation with unbounded size,
  exceptions, dynamic dispatch, closures, host I/O, package calls without an
  accelerator implementation, and unsupported effects inside kernels.
- [x] Require statically proven or checked bounds for buffer indexing; unsafe
  unchecked indexing must not be inferred from performance intent.
- [x] Preserve source spans, generic specializations, dtype/shape evidence, and
  diagnostics through AcceleratorIR.
- [x] Serialize a deterministic textual or structured form suitable for
  snapshots, cache keys, and backend differential tests.
- [x] Add an interpreter or verifier for the first pure subset so compiler tests
  can validate semantics without CUDA hardware.
- [x] Persist `target/quality/accelerator-ir-report.json` with admitted and
  rejected constructs, normalized IR hashes, and source provenance.
- [x] Inventory the subset after the gate passes and expand only from real
  package or application pressure.

Gate: `make accelerator-ir-check`.

## AC5: Maintained AOT Backend

- [x] Evaluate maintained AOT toolchains capable of lowering the canonical IR
  to CUDA-loadable artifacts and record the selection criteria, support status,
  licenses, target coverage, and reproducibility behavior.
- [x] Pin one backend implementation for the first CUDA lane; do not expose the
  selected toolchain's API as Terlan language or package semantics.
- [x] Lower AcceleratorIR types, control flow, address spaces, memory accesses,
  kernel parameters, launch metadata, and source locations without textual
  substitution.
- [x] Validate produced artifacts with established tooling before packaging and
  reject unsupported instructions, target features, malformed metadata, and
  backend diagnostics.
- [x] Generate a versioned artifact descriptor accepted by the ordinary
  `terlan-cuda` module/kernel loader.
- [x] Cache artifacts by compiler, normalized IR, backend, toolchain, package,
  target architecture, and build-option identity.
- [x] Build identical kernels in isolated directories and require deterministic
  descriptors and byte-identical artifacts where the selected toolchain can
  provide them.
- [x] Keep all backend libraries optional and outside normal CPU compiler
  artifacts when no accelerator target is selected.
- [x] Persist `target/quality/accelerator-aot-backend.json` with toolchain
  identity, inputs, outputs, validation, reproducibility, and rejected cases.
- [x] Inventory backend-specific code after the gate passes and keep all CUDA
  implementation details behind the generic backend trait and artifact schema.

Gate: `make accelerator-aot-backend-check`.

## AC6: Whole-Program Placement And Optimization

- [x] Extend whole-program analysis with host/device placement, transfer,
  synchronization, shape, alias, lifetime, and effect evidence.
- [x] Begin with explicit package operations as the semantic reference path;
  every optimization must preserve that path's observable result and failure
  behavior.
- [x] Specialize kernels for statically known dtype, rank, shape, layout,
  constants, launch bounds, and target architecture.
- [x] Fuse compatible pure elementwise regions only when aliasing, ordering,
  numerical, error, and resource semantics are preserved.
- [x] Eliminate redundant host/device transfers and synchronization only when
  ownership and dependency analysis proves them unnecessary.
- [x] Keep buffers device-resident across compatible package calls and actor
  suspension without extending a borrow beyond its admitted lifetime.
- [x] Select maintained library operations instead of generated kernels when a
  package descriptor proves semantic equivalence and target availability.
- [x] Reject an optimization when determinism, floating-point behavior,
  overflow, error timing, cleanup, or actor isolation would change.
- [x] Emit an explainable placement plan with each region, target, transfer,
  synchronization, fusion, library selection, rejection, and source reason.
- [x] Add differential CPU reference, unfused accelerator, fused accelerator,
  and package-library fixtures for exact and tolerance-based semantics.
- [x] Persist `target/quality/accelerator-placement-report.json` with plans,
  decisions, artifact identities, transfer counts, and differential outcomes.
- [x] Inventory optimization ownership after the gate passes and remove any
  package-side optimizer that duplicates compiler analysis.

Gates:

```text
make accelerator-placement-check
make accelerator-fusion-check
make accelerator-transfer-elision-check
```

## AC7: VM Asynchronous Integration

- [x] Reuse one VM-native asynchronous operation request, completion, and
  cancellation model for accelerator, network, storage, and other package work.
- [x] Dispatch blocking package/driver calls outside scheduler threads.
- [x] Suspend an actor with an explicit continuation while accelerator work is
  pending and wake it only through the scheduler-owned completion API.
- [x] Preserve actor mailbox, links, monitors, supervision, reductions,
  cancellation, and tracing behavior while an accelerator operation is
  outstanding.
- [x] Route successful results, package errors, device loss, worker failure,
  timeout, cancellation, and late completion through one typed resume path.
- [x] Register all live package resources against actor and runtime ownership so
  actor exit, restart, and VM shutdown release or transfer them deterministically.
- [x] Add bounded outstanding-operation and device-memory budgets per actor,
  supervisor, application, and target profile.
- [x] Expose accelerator state in generic runtime inspection and support
  bundles without storing raw pointers, user buffer content, or backend handles.
- [x] Execute concurrent actors on independent streams and prove one blocked or
  failed package worker does not stop scheduler progress.
- [x] Persist `target/quality/accelerator-vm-integration.json` with suspension,
  wakeup, cancellation, cleanup, accounting, isolation, and failure evidence.
- [x] Inventory VM integration after the gate passes and remove CUDA-specific
  scheduler, actor, timer, or supervision code.

Gate: `make accelerator-vm-integration-check`.

## AC8: Specialized Artifact Assembly

- [x] Connect accelerator package reachability to the 0.0.9 runtime capability
  graph and whole-program requirement analysis.
- [x] Exclude accelerator backend adapters, package workers, artifacts,
  descriptors, native libraries, and diagnostics when no reachable operation
  requires them.
- [x] Include only reachable kernels, maintained-library operations, dtypes,
  architectures, and runtime adapters in a selected accelerator artifact.
- [x] Generate static operation and kernel registries; do not ship a universal
  dynamic registry on constrained targets.
- [x] Reject an artifact whose target lacks required driver, memory, threading,
  blocking, cancellation, or native-library capabilities.
- [x] Record host code, accelerator code, transfers, package dependencies,
  native libraries, runtime capabilities, memory budgets, and cleanup policy in
  the ordinary application artifact manifest.
- [x] Prove excluded accelerator modules are absent through symbols, sections,
  registries, package closure, and link/import inspection.
- [x] Build CPU-only and CUDA-selected fixtures twice and require deterministic
  plans plus reproducible artifacts where supported.
- [x] Persist `target/quality/accelerator-specialized-artifact.json` with
  included/excluded closure, artifact hashes, size, imports, and provenance.
- [x] Inventory assembly after the gate passes and merge generic capability
  evidence into the main 0.0.9 specialization reports.

Gate: `make accelerator-specialized-artifact-check`.

## AC9: Ecosystem Proof

- [x] Execute a pure Terlan-generated elementwise kernel through
  `terlan-cuda` and compare it with the CoreIR reference result.
- [x] Execute a generated matrix or image preprocessing kernel and exchange its
  output with `terlan-pytorch` through the canonical tensor packet.
- [x] Execute OpenCV preprocessing, compiler-generated CUDA work, PyTorch CUDA
  inference, and CPU result extraction without an unnecessary intermediate
  host transfer.
- [x] Execute `terlan-ndarray` exchange when its CUDA contract is available,
  while keeping every base package independently installable.
- [x] Prove package failures, incompatible packets, rejected layouts, device
  mismatch, actor cancellation, actor crash, worker crash, and runtime shutdown
  each release ownership exactly once.
- [x] Run all package integrations through fresh checkouts and locked dependency
  resolution rather than sibling-path assumptions.
- [x] Add a synthetic second backend package through the same metadata,
  AcceleratorIR, artifact, and VM contracts to expose CUDA-specific leakage.
- [x] Persist `target/quality/accelerator-ecosystem-report.json` with package
  revisions, pipeline topology, transfer decisions, results, failures, and
  cleanup evidence.
- [x] Inventory package pressure after the gate passes and assign every gap to
  the compiler, VM, or owning external package before changing code.

AC9 inventory: the compiler owns typed IR, AOT artifacts, placement, and static
registries; the VM owns suspension, cancellation, worker failure, shutdown, and
exactly-once resource cleanup; each external package owns its maintained native
adapter and packet conversion. `terlan-cuda` remains a local immutable snapshot
until its package repository is published, while the gate proves the same
source-resolution contract through temporary Git repositories and
`terlan.lock`.

Gate: `make -C ../terlan-cuda ecosystem-check`.

## AC10: Correctness, Performance, And Release Closeout

- [x] Define exact semantics for integer and Boolean kernels and tolerance,
  NaN, infinity, signed-zero, reduction-order, and deterministic-mode policy
  for floating-point kernels.
- [x] Run CPU reference, accelerator IR reference, package-kernel, generated
  kernel, and maintained-library differential suites over the same fixtures.
- [x] Validate generated artifacts and native execution with the strongest
  maintained backend sanitizers, analyzers, and profilers available for the
  pinned toolchain.
- [x] Benchmark compile time, artifact size, cold startup, warm launch,
  transfers, synchronization, kernel execution, package dispatch, actor
  suspension/wakeup, and concurrent actor throughput separately. Benchmark
  graph replay when the package exposes an admitted graph closure; otherwise
  record the capability as unavailable without substituting a different DAG.
- [x] Compare Terlan-generated execution with equivalent direct CUDA,
  `terlan-cuda`, LibTorch CUDA, and OpenCV CUDA lanes only when operation,
  dtype, shape, algorithm, synchronization, and hardware are equivalent.
- [x] Set performance budgets from measured baselines and require explicit
  reviewed changes rather than claims that the VM inherently outperforms a
  maintained executor or library.
- [x] Run CPU-only CI on every change and self-hosted CUDA CI on package,
  compiler-generated kernel, VM integration, ecosystem, and performance lanes.
- [x] Produce one signed release report containing compiler, VM, package,
  toolchain, driver API, hardware, kernel, native-library, artifact, benchmark,
  and source identities.
- [x] Promote accelerator support only for OS, architecture, device, driver,
  toolchain, and package combinations actually exercised by release gates.
- [x] Inventory the complete roadmap after all gates pass; archive completed
  execution detail and carry only measured gaps into the next release.

Gates:

```text
make -C ../terlan-cuda cuda-library-execution-check
make -C ../terlan-cuda cuda-observability-performance-check
make -C ../terlan-cuda cuda-release-candidate-check
```

Closeout inventory: `accelerator-release.json` binds twelve milestone reports
to the compiler, VM, package, LLVM NVPTX, NVIDIA driver, RTX A4500, generated
kernel, native-library, and benchmark identities. Its detached Ed25519
signature is verified by the release gate. Promotion is experimental and
limited to Linux x86-64, `sm-30` PTX executed on compute capability 8.6,
driver 580.173.02, LLVM 14.0.0, and the recorded local package snapshots.

Measured package-owned follow-up work remains in `terlan-cuda/docs/ROADMAP.md`:
one-shot transferred-resource packets, CUDA-enabled OpenCV resource exchange,
compiler/VM trace identities for NVTX correlation, and a second executed
compute-capability lane. The equivalent Terlan CUDA and LibTorch CUDA benchmark
now executes with separated cost classes; no unavailable lane is represented
as passing evidence.

## Completion Definition

The compiler roadmap is complete when an external accelerator package can
declare its capabilities, a Terlan program can compile an admitted kernel ahead
of time, whole-program analysis can place and optimize the work, the VM can
suspend and resume actors around execution without blocking scheduler progress,
and the final artifact contains only the proven accelerator closure.

The proof must work for `terlan-cuda` without any CUDA-specific parser,
typechecker, CoreIR, scheduler, or resource-table branch. A synthetic second
backend must pass the generic contract far enough to demonstrate that CUDA is a
package implementation, not a hidden language feature.
