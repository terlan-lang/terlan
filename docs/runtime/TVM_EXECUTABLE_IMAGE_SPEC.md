# TVM Executable Image Specification

Status: normative format-1 contract; later runtime slices remain incomplete.

Version: TVM executable image format 1, frozen for the 0.0.7 implementation.

This document defines the target executable format and loading model for the
Terlan VM. It is authoritative for new VM image, compiler AOT, loader, package,
and NativeBoundary work. Transitional JSON build metadata, where it still
exists for compiler-side migration tests, is not executable and must not be
renamed to `.tvm`.

The primary 0.0.7 architectural decision is that Terlan is a direct AOT native
compiler written in Rust. It emits native objects from checked Terlan compiler
IR without generating ordinary application Rust and invoking `rustc` as a
secondary compiler. The TVM executable image and runtime-kernel model are
consequences of that compiler architecture.

This is an execution-mechanics pivot, not a language-semantics reset. Every
already-ported Erlang-derived Terlan test retains its source fixture, assertions,
expected result, and public identity while its runner migrates to the AOT-native
TVM path. An implementation-specific BEAM, ERTS, opcode, or interpreter test may
change only after explicit classification; it may not be silently weakened to
make the new backend pass.

The JavaScript target is independent of this pivot. Existing JavaScript
lowering, emitted behavior, golden files, source maps, declarations, and
browser/server tests remain unchanged. Direct-AOT work touching shared compiler
IR must prove JavaScript byte stability where promised and behavioral
equivalence elsewhere; it may not regenerate expectations to hide a regression.

`TVM_NATIVE_DATA_ABI_SPEC.md` is normative for the value layouts, ownership,
call convention, actor identities, continuation identities, runtime safepoints,
and transition encoding used by those native objects.

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**,
and **MAY** are normative requirements.

## 1. Definition

A TVM executable image is a target-specific, ahead-of-time compiled native
program supervised by the Terlan VM. Its filename ends in `.tvm`.

The image is:

- native machine code in the platform executable format;
- linked to or launched with the matching precompiled Terlan execution-shard
  runtime and NativeBoundary ABI;
- accompanied by a small descriptor embedded in the native image;
- admitted, started, called, suspended, resumed, and stopped by the VM.

The image is not:

- Terlan bytecode;
- a binary encoding of Terlan instructions;
- serialized CoreIR, VMIR, HIR, AST, or source text;
- compiler interface metadata;
- JSON;
- a JIT input;
- a shared library loaded unsafely into the VM process by default.

The VM is a runtime kernel and supervisor, not a Terlan instruction
interpreter. It owns actors, scheduling, mailboxes, timers, failure propagation,
capabilities, NativeResources, and observability. The compiler AOT-compiles
Terlan control flow and computation into the image. Native code requests VM
services through the runtime ABI.

## 2. Platform Representation

Format 1 uses the operating system's native executable representation:

| Target | Representation |
| --- | --- |
| Linux and other ELF targets | ELF executable image |
| macOS and other Darwin targets | Mach-O executable image |
| Windows | PE executable image |

The `.tvm` suffix identifies the Terlan runtime contract; it does not replace or
reinterpret the underlying OS executable format. The compiler MUST produce a
separate image for each target triple and ABI. A portable or multi-target
distribution package MAY contain several `.tvm` images, but that package is not
itself a TVM executable image.

Format 1 does not define a portable instruction fallback. A loader MUST reject
an image for a different target rather than interpret it.

## 3. Logical Image Contents

A conforming image contains exactly two product-level concerns:

1. AOT-compiled native program code and immutable native data.
2. The minimal embedded TVM descriptor required to admit and invoke that code.

The embedded descriptor contains:

- TVM magic and executable-image format version;
- runtime ABI version range;
- NativeBoundary protocol version range;
- target triple, architecture, operating system, and calling convention;
- compiler identity and deterministic build identity;
- package and module identity;
- executable entry points and stable numeric export identifiers;
- argument and result boundary types for each exported entry point;
- actor entry and resume points where applicable;
- declared host capabilities;
- NativeResource ownership and cleanup declarations;
- native dependency identities and ABI fingerprints;
- code and immutable-data integrity digests;
- optional signature identity.

The descriptor MUST use the format-1 canonical little-endian binary encoding
specified below. It MUST NOT use JSON, Rust memory layout, `serde`
implementation detail, `bincode`, or another compiler-version-dependent object
layout. Unknown major versions MUST be rejected. Unknown optional records in a
compatible minor version MAY be ignored only when their record flags explicitly
permit it.

The descriptor MUST be extractable and validated without starting the image.
Its physical placement uses exactly one target-specific section:

| Native format | Descriptor section |
| --- | --- |
| ELF | `.note.terlan.tvm` |
| Mach-O | `__tvm_desc` in the `__TERLAN` segment |
| PE/COFF | `.tvm$D` |

The section contains the raw descriptor bytes, not JSON and not a native Rust
structure. The loader rejects a missing or duplicate descriptor section.
Changing a section name or an existing byte field requires a new major format.

### 3.1 Canonical Format-1 Bytes

All integers are unsigned little-endian values. The fixed 32-byte header is:

| Offset | Width | Field |
| ---: | ---: | --- |
| 0 | 8 | ASCII magic `TVMDSC01` |
| 8 | 2 | format major, value `1` |
| 10 | 2 | format minor, value `4` |
| 12 | 2 | header length, value `32` |
| 14 | 2 | record count |
| 16 | 4 | total descriptor length including footer digest |
| 20 | 2 | minimum runtime ABI |
| 22 | 2 | maximum runtime ABI |
| 24 | 2 | minimum NativeBoundary protocol |
| 26 | 2 | maximum NativeBoundary protocol |
| 28 | 4 | reserved zero |

The header is followed by the declared record count and then a 32-byte SHA-256
digest over every preceding descriptor byte. A descriptor is at most 1 MiB.
Each record has an 8-byte header: `kind: u16`, `flags: u16`, and
`payload_length: u32`, followed by exactly that many payload bytes. Records are
strictly ordered by kind and may not repeat. Flag bit 0 means optional; all
other flag bits are zero. An unknown mandatory record is rejected.

Text is encoded as `byte_length: u16` followed by nonempty UTF-8 bytes without
control characters. Lists start with `count: u16`. Format 1 records are:

| Kind | Required | Canonical payload |
| ---: | --- | --- |
| 1 | yes | target triple, architecture, operating system, calling convention as four texts |
| 2 | yes | compiler, build, package, module identities as four texts |
| 3 | yes | exports list |
| 4 | yes | sorted nonzero capability-ID `u64` list |
| 5 | yes | resource list of `(type_id, owner_capability_id, cleanup_export_id)` `u64` triples |
| 6 | yes | dependency list of `(id: u64, ABI SHA-256: [u8; 32])` |
| 7 | yes | code SHA-256 followed by immutable-data SHA-256 |
| 8 | no | signer text, signature length `u16`, signature bytes |
| 9 | no | native continuation list |
| 10 | no | canonical fixed aggregate layout table |
| 11 | no | canonical List, Map, and Set schema table |
| 12 | no | canonical finite atom identity table |

Each export is `id: u64`, name text, parameter count and boundary types, then
result count and boundary types. Format 1 permits zero or one result. Boundary
types use one byte: `0 Unit`, `1 Bool`, `2 Int`, `3 Float`, `4 Binary`,
`5 String`, `6 Json`, `7 NativeResource`, `8 Atom`, `9 Bytes`, and
`10 Managed`; tag 7 is followed by its declared resource type ID as `u64`, and
tag 10 by its 16-byte semantic type ID. No pointer, borrowed reference, or
physical collection-node tag exists.

Records 10 and 11 begin with `count: u16`. Every row then contains a 16-byte
semantic type ID, `encoded_length: u32`, and canonical bounded metadata bytes.
Rows are strictly ordered and unique by `(semantic_id, encoded_bytes)`. The
loader reconstructs each checked descriptor, verifies semantic identity, and
requires byte-for-byte canonical re-encoding before admitting the image.

Record 12 begins with `count: u16`, followed by that many canonical texts.
Identities are strictly ordered by UTF-8 bytes and unique. Empty identities,
control characters, duplicate entries, and noncanonical ordering are rejected.
Compiled and managed values carry only the zero-based `u32` index into this
immutable image-generation table; persistence and transport use atom text.

Export, capability, resource-type, and dependency IDs are nonzero `u64` values,
strictly sorted within their tables, unique within their identity domain, and
stable for the same logical declaration across deterministic rebuilds. The
compiler owns ID assignment and MUST reject a collision rather than renumber
unrelated declarations silently. Every resource owner and cleanup export, and
every NativeResource boundary type, must resolve inside the descriptor.

Each continuation is `id: u64`, parameter count and boundary types, then result
count and boundary types. Format 1 permits zero or one continuation result.
Continuation IDs are nonzero, strictly sorted, unique, and disjoint from export
IDs. Format-1 compiler-generated continuation IDs use the first little-endian
`u64` of `SHA-256("terlan-tvm-continuation-v1\\0" || module || "\\0" ||
function || "\\0" || arity_le || ordinal_le)`, with zero mapped to one. A
continuation descriptor carries owned resume values only; it never describes a
native stack address, borrowed value, or code pointer.

Format-1 Terlan function exports use the first little-endian `u64` of
`SHA-256("terlan-tvm-export-v1\\0" || module || "\\0" || function || "\\0" ||
arity_le)`, with zero mapped to one. The compiler rejects the resulting image
if any two declarations collide. Source order and cache layout therefore do
not participate in export identity.

For an application containing multiple source modules, identity record 2 names
the application compilation unit; it does not reduce the image to one source
module. Every export name remains fully source-module-qualified as
`module.function/arity`. The loader may accept a short function spelling only
when exactly one descriptor export with that spelling and arity exists;
otherwise it rejects the call as ambiguous and requires the qualified name.

## 4. Compiler Metadata Is Separate

Compiler interfaces and caches are not part of `.tvm`. In particular, an image
MUST NOT contain:

- source text or syntax trees;
- HIR, CoreIR, VMIR, or optimization records;
- generic constraints or trait-solving evidence not required at runtime;
- dependency-resolution traces;
- generated Rust, C, or C++ source;
- compiler cache keys or cache filesystem paths;
- quality, coverage, benchmark, or release reports.

Package interface metadata such as `.typi` remains a compiler concern. Debug
information MAY use the platform's native debug representation or a detached,
checksum-bound debug artifact. Release images SHOULD be strippable without
changing executable behavior or the TVM ABI.

Compiler builds embed `TVMDBG05` source identities in a non-loaded native debug
section (`.debug_terlan` on ELF, `__terlan` in `__DWARF` on Mach-O,
`.tdbg$D` on COFF, and `.tdbg` on PE). Records identify only emitted native
functions and bind module/function/arity to the real compiler input path,
UTF-8-safe declaration ranges, a SHA-256 digest of the exact compiler input,
generated/template source origin, compiler-generated continuation identities,
and exact source-expression ranges for resume entries whose spans survive
lowering. The section never embeds source text. This section is debugger
metadata, not executable IR or an admission fallback; stripping it cannot
change the canonical descriptor or runtime code.

## 5. AOT Execution Model

The compiler MUST AOT-compile every reachable Terlan function and control-flow
path that it can compile. It MUST NOT defer ordinary Terlan computation to a
runtime interpreter or JIT.

Pure functions have native implementations. If the VM invokes a pure export,
the invocation crosses the NativeBoundary using its stable export identifier
and typed signature. Calls that remain wholly inside one compiled native image
MAY be inlined or compiled as direct native calls; they must not bounce through
the VM merely to preserve source-level function boundaries.

Effectful and actor code is also native code. Ordinary same-shard actor
operations invoke typed runtime primitives directly. Spawn, local send,
receive, reduction polling, yield, timers, links, monitors, and actor exit MUST
NOT serialize through the supervisor control plane on their common path.

The execution-shard runtime owns:

- spawn, yield, park, resume, or terminate an actor;
- send or receive a typed message;
- register or cancel a timer;
- link, monitor, demonitor, or propagate failure;
- invoke a declared NativeBoundary capability;
- acquire, transfer, or release a NativeResource.

Native code enters a runtime slow path only when an operation cannot complete
inline, exhausts its reduction budget, parks, allocates, collects, or requires
control-plane work. A typed TVM transport transition is used when an operation
crosses a shard, OS-process, node, persistence, migration, debugger, or unsafe
NativeBoundary isolation boundary. It is not a Terlan opcode or an instruction
sequence for the VM to interpret.

Same-shard continuations use runtime-owned managed references plus precise stack
and continuation maps. Transported or persisted continuation identities are
stable numeric identities, never native pointers. Values that survive a park
have traced ownership. Native stack addresses are never persisted or sent to
the control plane.

## 6. Shard Supervision And NativeBoundary Invocation

Format 1 launches the admitted image in a supervised execution-shard OS process.
The VM supervisor control plane remains outside that process and uses a bounded,
versioned TVM control protocol for admission, lifecycle, cross-shard routing,
inspection, and recovery. It does not mediate ordinary local actor operations.

The VM MUST validate the embedded descriptor before starting the shard. The
shard and VM then perform a versioned handshake binding the running process to
the descriptor digest. Calls and responses require:

- bounded binary frames;
- monotonically unique or otherwise collision-safe request identifiers;
- stable numeric export identifiers;
- typed argument and result encoding;
- explicit success, typed failure, cancellation, and shutdown frames;
- declared capability identifiers for image-to-VM requests;
- backpressure and scheduler-credit accounting;
- deterministic rejection of malformed, oversized, duplicate, stale, or
  out-of-order frames.

The format-1 transition operation tag space assigns closed values to `Yield`,
`Send`, `Receive`, `Spawn`, `Timer`, `Link`, `Monitor`, `Resource`,
`Cancellation`, `Failure`, and `Scheduling`. Unknown tags fail closed.
Recognizing a tag does not authorize a scalar-only consumer to service it; the
initial pure driver accepts only `Yield` until it is attached to a scheduler
owner.
Every call, transition, and resume also carries a nonzero VM owner identity.
The worker binds a suspended continuation to the exact `(request, owner,
continuation)` triple; a matching request and continuation from another owner
fails with a distinct ownership diagnostic rather than being treated as a
stale identity.
Success and failure replies echo the same owner identity, and consumers verify
it beside the request ID before accepting a value or diagnostic. Ownership
therefore covers the complete call lifecycle, including paths that never park.
The VM actor runtime resolves that raw owner as a nonzero `VmProcessId` before
parking. Its scheduler registry indexes one pending native continuation by both
process and `(request, continuation)` identity. Only the exact owner can resume
the suspended process; stale or foreign-owner resumes leave it parked, and
actor exit removes the ownership entry.
Transition operation arguments and owned continuation captures are separate
bounded vectors. Operation arguments belong to the VM action being requested;
captures belong exclusively to the declared resume descriptor. Counts for both
vectors are validated against the frame length, and `Yield` requires an empty
operation-argument vector. Typed Send carries recipient, three canonical
boundary-identity words, and one backend-owned value word. Typed Receive carries
the same three identity words and injects a result of that exact descriptor type
into its continuation.

JSON MUST NOT be the production invocation protocol. Raw pointers, references,
borrowed lifetimes, C++ exceptions, Rust panics, and platform exception objects
MUST NOT cross the boundary. Opaque handles are numeric, generation-checked,
owner-associated NativeResource identities.

An image/shard crash, abort, invalid frame, timeout, or protocol disconnect MUST
become an attributed VM-owned failure. It MUST NOT crash or corrupt the
supervisor control plane or another shard. The VM owns shard termination,
resource reconciliation, actor notification, restart policy, durable recovery,
and diagnostics.

Memory-unsafe external package code uses separate supervised NativeBoundary
workers by default, so a C, C++, CUDA, or Rust-unsafe failure does not corrupt
the execution shard. A future trusted in-shard adapter profile requires an
explicit unsafe capability and separate conformance; it is not format 1's
default.

## 7. Loader Admission

Before execution, the loader MUST reject an image with any of the following:

- invalid native executable structure;
- absent, malformed, duplicate, or unsupported TVM descriptor;
- target or ABI mismatch;
- unsupported runtime or NativeBoundary protocol version;
- duplicate export identifiers or invalid boundary signatures;
- undeclared capabilities or unavailable required capabilities;
- raw-pointer or borrowed-lifetime boundary types;
- unresolved native dependency fingerprints;
- invalid code, immutable-data, descriptor, or package digest;
- signature failure when the package policy requires signatures;
- compiler metadata or instruction payloads presented as executable sections;
- a `.tvm.json` artifact presented as a `.tvm` image.

Admission is fail-closed. Filename suffix alone never establishes that a file is
a TVM executable image.

## 8. Determinism and Relocation

Given the same checked sources, dependency lock, compiler version, target,
profile, and declared environment inputs, compilation MUST produce the same
logical descriptor, export inventory, native program behavior, and build
identity.

Paths, timestamps, process identifiers, temporary directories, random build
identifiers, and cache locations MUST NOT affect the logical image identity.
Platform toolchain output that prevents byte-for-byte reproducibility MUST be
identified by the compiler and covered by reproducibility gates before format 1
is stable.

The descriptor MUST refer to exports, dependencies, and resources by stable
identity or digest. It MUST NOT contain absolute paths or paths into the
compiler cache. A validated `.tvm` can be moved without rewriting it.

## 9. Compilation-Time Requirements

AOT compilation increases cold-build work. That cost must not be multiplied by
an artifact-per-function or compiler-process-per-module design.

The development-build objective is Go-class compilation speed: direct,
predictable, package-parallel AOT compilation with inexpensive dependency
interfaces and aggressive reuse of unchanged native objects. This objective is
architectural. It cannot be satisfied by hiding a slow secondary-language build
behind a warm benchmark.

Ordinary Terlan functions MUST be emitted through a compiler-owned native-object
backend. Generated Rust, C, or C++ plus a general-purpose secondary compiler MAY
be used for bootstrap experiments, binding adapters, or explicit external native
packages, but MUST NOT be the final product code-generation path for Terlan
application functions.

Cranelift is the sole 0.0.7 application code-generation backend. The compiler
lowers checked CoreIR through Terlan-owned NativeIR and ABI classification into
Cranelift IR, then uses `cranelift-object` in-process to produce relocatable
ELF, Mach-O, or PE/COFF objects. Cranelift is an emission implementation detail:
Cranelift IR, serialization, cache formats, and stack-map structures are not TVM
image formats.

The backend MUST NOT link or invoke LLVM, emit LLVM IR or bitcode, call `opt`,
`llc`, or ORC, consume LLVM statepoint/stack-map formats, or present LLVM as a
second release profile. The fact that an upstream Rust compiler may have used
LLVM to build the `terlc` executable does not add LLVM to the Terlan application
compiler architecture.

A conforming implementation MUST:

- load checked package interfaces without reparsing or re-typechecking unchanged
  dependency sources;
- compile independent packages or bounded dependency components in parallel;
- emit native objects directly from checked compiler IR;
- emit through an in-process, version-pinned Cranelift library integration;
- generate compact Terlan-owned stack maps from finalized Cranelift code
  locations rather than exposing backend-native metadata;
- use content-addressed internal caches for generated native sources, objects,
  descriptors, and native dependency results;
- reuse unchanged native objects across builds;
- retain the sealed application image in the content-addressed cache and
  materialize deployable output from that cache entry, never by trusting an
  existing same-named output image;
- avoid starting `rustc`, a C/C++ compiler, or a native linker once per Terlan
  function;
- group code generation into bounded compilation units and perform one final
  application-image link per target;
- skip both native code generation and linking on a true no-op build;
- provide a fast development profile without release-only optimization or LTO;
- keep release optimization and link-time optimization profile-controlled;
- prevent unbounded generic monomorphization through a documented shared-code,
  bounded-specialization, or equivalent strategy;
- keep cache intermediates outside the deployable output directory;
- report cache hits, code-generation time, and link time in compiler timing
  diagnostics.

The REPL remains AOT-only. It MUST use a persistent compiler service and
content-addressed incremental compilation rather than introduce a JIT or fall
back silently to instruction interpretation. A changed declaration MAY produce
a new incremental native object and image generation; unchanged declarations
must be reused. REPL latency must be tracked separately from cold release-build
latency.

Required performance baselines are:

- cold development build;
- one-function incremental development build;
- no-op development build;
- cold release build;
- package-level relink;
- first REPL declaration and unchanged REPL reuse.

For the 0.0.7 reference development profile, the warm one-function incremental
build, true no-op build, changed REPL declaration-to-registered-generation loop,
and unchanged REPL reuse loop MUST each complete in less than one second at the
95th percentile. The release gate must record the reference hardware, operating
system, toolchain, cache state, sample count, median, and 95th percentile. Cold
compiler-service startup and cold release compilation are measured separately
and do not weaken the warm-loop requirement.

The release benchmark suite MUST include equivalent Terlan and Go reference
projects covering a small command, a multi-package application, a one-package
edit, and a no-op build. It must publish cold and warm ratios on the same
machine. The first conforming backend establishes the explicit permitted ratios;
0.0.7 cannot close with only Terlan-internal timing numbers or with a backend
known to require one `rustc`, C/C++ compiler, or linker process per module.

No format or loader slice is complete if it improves runtime execution by
silently introducing unbounded compiler subprocess or file fan-out.

## 10. Artifact Count

The deployable output for one application and target is one `.tvm` image.
Generated source, object files, fingerprints, compiler metadata, and quality
reports are cache or build-evidence files, not neighboring runtime artifacts.

A detached debug artifact and required platform signature files are the only
format-1 exceptions. A distribution package containing several targets is a
separate packaging layer.

## 11. Current Migration

The current implementation directly emits Cranelift objects for the supported
scalar-pure CoreIR region across all modules in a package, performs one link,
and emits one descriptor-bearing `.tvm`. Source-module-qualified exports from
that image are statically admitted and invoked through a descriptor-bound
binary worker protocol. `terlan-vm run` and `terlan-vm load` consume the image
without a JSON sidecar. Native objects, descriptor objects, and sealed images
live in a content-addressed compiler cache outside deployable VM output; stale
same-target `.tvm` outputs are removed when the application image is
materialized.

Each complete native cache entry ends with a deterministic `manifest.v1` line
protocol. The manifest binds the cache-input SHA-256, full target triple,
backend identity, exact filenames, byte lengths, and SHA-256 digests of the
native object, descriptor object, and sealed image. It is an internal compiler
artifact, not part of the deployable `.tvm` format. A cache hit requires an
exact manifest match, successful static image inspection for the current
target, and an embedded descriptor build identity matching the cache key.
Missing, partial, corrupted, or semantically mis-keyed entries are cache misses
and are regenerated; the manifest is written last so an interrupted build is
never accepted as complete. The cache-input identity includes the Terlan
compiler version, native-codegen schema, backend, image format, full target ABI,
and deterministic NativeIR fingerprint.

The implemented format-1 control subset has descriptor-bound hello/ack,
strictly increasing request IDs, typed scalar call success/failure, a
descriptor-declared `Transition(Yield)` / matching `Resume` exchange, and a
shutdown/ack exchange followed by verified worker exit. The initial transition
profile carries bounded owned `Int` and `Bool` captures: AOT entry code returns
a stable continuation ID and writes captures into a caller-owned buffer whose
capacity was derived from record 9; the VM validates the identity, arity, type,
and Bool domain before AOT resume code finishes the typed result. For pure
scalar `let` chains, the compiler traces resume free variables backward through
their dependencies, emits the required prefix calculations, and transports only
the live locals; dead locals do not enter the continuation signature. Linear
code may cross multiple suspension points, including adjacent source yields.
Each point uses its stable source-order ordinal in the format-1 continuation
identity and recomputes its own live capture signature. The worker updates the
one pending `(request, continuation)` identity after every yielded resume, and
the VM rejects repeated continuation IDs rather than following a cycle. Stale
IDs, wrong capture types, and insufficient output capacity fail closed. An
ordered `if` may contain independently pure or suspending bodies when its
conditions are pure. Nested branches, distinct yields in both arms, a resume
body that yields again, and a selected short-circuit RHS use the same terminal
status protocol. The native entry reserves the maximum capture width across its
reachable arms; each continuation descriptor still declares only its own live
values. A condition with a linear scalar prefix may suspend: its continuation
owns the values needed to finish that condition and the remaining ordered
clauses, rather than returning an intermediate Boolean to the VM. This supports
first and later clauses, repeated condition yields, nested conditionals, a
selected body that suspends again, and a suspending left operand whose resumed
Boolean preserves `and`/`or` short-circuit suppression and failure propagation.
Scalar condition-prefix calculations run before suspension, propagate checked
failures immediately, and retain only values live in the composed resume.
The same extraction applies to ordinary unary and eager scalar value
expressions and to any argument of a scalar native call, so native code resumes
the value computation directly instead of returning an intermediate scalar to
VMIR. Earlier call arguments are materialized and checked before suspension;
later arguments remain in the resume and are evaluated only afterward.
Non-linear scalar conditions may compose suspension through unary and eager
binary operators on their first-evaluated operand spine. A scalar left operand
may also select a suspending `and`/`or` right operand; an unselected lazy operand
emits no transition, while a resumed false value follows the original ordered
fallthrough or no-match path. The proof profile bounds nested lazy composition
at eight right-hand decisions and rejects deeper shapes. A non-linear `or`
condition may select a suspending body directly or after its right-side
condition resumes; distinct stable continuations preserve the suppressed,
one-transition, and two-transition paths. Eager right-side suspension preserves
earlier scalar calculations as typed captures rather than lifting across them.

A suspending native function may be called in tail position because its result,
status, stable continuation ID, and owned captures can be returned directly
without retaining a caller stack. Suspension classification and maximum capture
capacity propagate to tail callers by fixed point, including transitive chains
and branch-selected callees. The internal native ABI carries a caller-owned
transition-length output separately from the statically bounded capture buffer;
this reports the selected continuation's exact arity even when sibling callees
have different capacities.

Tail recursion is compiler-owned before Cranelift. After application-global
continuation materialization, typed tail-position analysis converts direct
self recursion to a loop-header backedge and statically resolved mutual
components to bounded tagged dispatch. Arguments are evaluated before the
backedge and replace the parameter frame in parallel. Managed parameter slots
remain precise stack-map roots across safepoints. Terminal suspending edges
forward the selected transition and resume identity without retaining a caller
frame. Non-tail calls and calls followed by cleanup remain ordinary calls.
Native object units for a lowered recursive component must not contain
relocations to that component's function symbols.

Every recursive tail edge also owns a stable reduction continuation. Generated
code counts backedges in its current invocation frame and returns a `Yield`
transition no later than 1,024 edges after entry or resume. The transition
frame contains the already-evaluated next arguments; its continuation
descriptor re-enters the selected direct or mutual-recursion target with a
fresh budget. This is the VM preemption boundary for otherwise busy native
loops. Cancellation, shutdown, failure delivery, inspection, and peer
scheduling happen while the exact actor continuation is parked; generated code
does not poll a host executor or retain a shard lock across the handoff.

The same tail and bounded non-tail rules apply to `Unit`-returning suspending
functions. This permits an effect-shaped call to yield and then continue in its
native caller; only caller values live after the Unit call are appended to the
wrapper signature.
Sequential Unit-shaped effect calls use the same distinct nested continuation
layout. Up to eight such effects may compose in one scalar native region; each
stage carries only values live after it, and a ninth effect keeps the function
outside this bounded admission profile.

A bounded non-tail call may target a callee with one to eight proven-linear
suspension stages and no suspending callee. Admission requires one known initial
continuation and one guaranteed next continuation from every intermediate
stage. The immediate result continues in the current native entry. Each yield
is rewritten to its own distinct stable caller continuation after native code
verifies the callee continuation identity and capture count. Every record-9
signature concatenates the stage's exact callee captures with only caller
scalars live after the call.

Intermediate wrappers rewrite the next callee suspension to the next caller
wrapper and preserve caller captures across repeated resumes. Callee temporary
indices are rebased around appended caller parameters. The terminal wrapper
inlines the final callee continuation before the caller's saved scalar
expression, so no stack address, code pointer, or dynamic continuation identity
is transported. Fixtures cover unary, binary, nested, any single pure-call
argument, any sequential `let` binding or body, condition, and selected-branch
contexts. Earlier call arguments and `let` bindings are materialized before
suspension. Zero/`Int`/`Bool` and combined callee/caller captures; computed
locals over multiple resumes; checked
arguments; tail forwarding; distinct wrapper identities; and protocol
rejection. A boundary fixture executes eight distinct wrappers and rejects a
ninth stage. Eager binary expressions may evaluate a checked scalar left operand
before a right-side `yield_now` or proven-linear suspending callee, carry that
exact result as a typed caller-owned continuation capture, and finish after
resume without recomputation. A scalar expression may sequence up to eight
proven-linear suspending calls. Immediate completion enters the next native call;
each yielded path retains one stable nested wrapper layout across initial and
resumed execution. Ambiguous branch graphs, callee chains deeper than eight, and
a ninth suspending call in one expression remain rejected.
Direct yields and proven-linear calls may occur in either order within that
bound. Call extraction runs before yield extraction, preventing a suspending
call from being treated as a pure prefix; nested transitions retain source-order
captures across both directions.

Content-addressed native cache entries have one OS-backed file-lock owner at a
time. A concurrent compiler that misses the same entry waits for that owner,
then revalidates the complete manifest and image identity before deciding
whether code generation or linking is still required. The empty lock anchor is
internal cache metadata, never deployable output; ownership is released by the
OS even if the compiler is killed.
Objects, descriptor objects, sealed images, and the manifest are published from
same-directory temporary files. The manifest is published last, so readers
never credit a partial new generation; temporary link and publication files
are removed when their owner exits normally or returns an error.
For a dependency-free incremental build, the compiler may skip parsing and
typechecking only after validating exact source text, artifact schema/compiler/
target identity, the checksum-covered CoreIR contract and metadata, a safe
image path, deployed image digest and static descriptor, embedded build key,
and the complete content-addressed cache manifest. Any failed check returns to
the normal compiler pipeline and repairs the output.

The direct `.tvm` runner now creates a VM process for its native entry and
services each validated `Yield` through the scheduler-owned continuation
registry before emitting `Resume`. Native execution is split into explicit
begin/resume steps: begin returns an owned suspension while the exact
process/request/continuation owner remains parked, and resume first requeues
that owner before advancing native code. Repeated yields produce new suspended
steps rather than being consumed inside one worker callback. Managed values,
non-Yield transition dispatch, same-shard fast paths, cancellation,
backpressure credits, other actor operations, and restart policy remain
migration work; this is not yet the final common-path effect mechanism.

The standalone runtime never falls back to serialized JSON or VMIR, and it
never executes CoreIR at runtime. Source commands whose managed-AOT path is incomplete fail
loudly with a stable `error[vm.aot_required]` diagnostic. The retired
checked-CoreIR runtime was valuable preliminary implementation and benchmark
evidence—including the HTTP-handler baseline—but it is no longer an executable
compatibility path.

Migration proceeds in this order:

1. Extend package-wide bounded compilation from the scalar-pure region to all
   reachable functions while preserving one link and one `.tvm` per target.
2. Complete execution-shard lifecycle supervision, crash attribution,
   cancellation, backpressure, and scheduler-credit frames.
3. Compile effectful control flow into native continuation entry points and
   same-shard runtime calls, reserving transport transitions for real isolation
   boundaries.
4. Move source and compiler metadata to compiler interfaces or detached debug
   artifacts.
5. Delete the `.tvm.json` execution loader and serialized-VMIR interpreter.

The transitional artifact MUST NOT gain new architecture that assumes
serialized VMIR is the final execution format.

## 12. Conformance

Before format 1 is declared stable, executable gates must prove:

- native format and descriptor extraction for every supported target;
- rejection of JSON, bytecode, VMIR, and compiler-metadata payloads as `.tvm`;
- deterministic descriptor and export identity;
- target, ABI, capability, dependency, digest, and signature rejection;
- typed pure export invocation;
- actor yield, transition, park, resume, and completion through native entry
  points;
- NativeResource ownership, transfer, cleanup, and stale-handle rejection;
- worker crash, abort, timeout, cancellation, malformed-frame, and oversized-
  frame isolation;
- relocation without path rewriting;
- cold, incremental, no-op, release, relink, and REPL compilation-time
  baselines;
- exactly one deployable `.tvm` output per application and target.

The conformance gates must exercise a Terlan consumer end to end. Metadata-only
fixtures, generated-source inspection, and mocked native calls are insufficient.
