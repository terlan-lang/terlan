# Cranelift Language and Runtime Ecosystem Survey

Survey date: 2026-07-24.

## Purpose

This survey tests the claim that Terlan uses Cranelift in a unique way. It
compares Terlan's 0.0.7 target architecture with public language compilers,
language runtimes, and WebAssembly runtimes that use or have seriously
evaluated Cranelift.

This is an architecture survey, not a maturity or performance claim. Terlan
entries describe the release contract in the 0.0.7 roadmap and runtime
specifications. They do not imply that every contracted capability is complete.
External entries describe behavior claimed by their primary sources; this
survey did not build or independently certify every reviewed project.

## Finding

The broad claim that Terlan is the only frontend language built around
Cranelift is false. Public direct-to-native language frontends include Raven,
Capy, Lumina, MinCaml, Haskelujah, Monty, pon, Soplang, and Lake. Several use
Cranelift as their only current backend, and several emit native object files or
standalone executables.

Independent per-actor garbage collection is also not unique to Terlan or
Erlang. Pony provides a per-actor tracing collector, but uses LLVM rather than
Cranelift and permits controlled cross-actor object references through its
reference-capability and ORCA protocols.

The narrower architectural result is:

> No public project identified by this survey combines a sole direct-AOT
> Cranelift backend, no bytecode/JIT/WebAssembly application path,
> compiler-owned native ABI and precise managed-root metadata, target-native
> executable images, and a multicore fault-tolerant actor runtime with
> supervision and isolated native-resource boundaries.

This is a dated negative search result, not proof that no private,
undocumented, or newly created project has the same architecture.

## Terlan Comparison Contract

A project is a complete architectural match only if it has all of these
properties:

1. A source-language frontend lowers checked language IR directly to Cranelift.
2. Cranelift is the sole application code-generation backend.
3. Application execution is AOT native code, without a required bytecode,
   WebAssembly, interpreter, or JIT tier.
4. The language/compiler owns its ABI, safepoints, stack maps, managed value
   descriptors, and executable-image semantics.
5. Each actor exclusively owns a precise, moving, independently collectible
   heap; a receiver never observes a reference into a sender's actor-local
   heap.
6. A runtime kernel schedules lightweight isolated actors over multiple cores.
7. Mailboxes, links, monitors, exits, timers, cancellation, and supervision are
   runtime semantics rather than library conventions.
8. Native and resource integrations cross typed, isolated runtime boundaries
   instead of an unrestricted in-process FFI.

The first four criteria describe the compiler and image boundary. The final
four distinguish Terlan from ordinary native languages and sandboxed bytecode
runtimes.

Terlan's comparison baseline comes from the
[0.0.7 roadmap](../roadmap/ROADMAP_0_0_7.md), the
[executable image specification](../runtime/TVM_EXECUTABLE_IMAGE_SPEC.md), the
[native data ABI specification](../runtime/TVM_NATIVE_DATA_ABI_SPEC.md), and the
[compiler build contract](../../crates/terlan/src/commands/build/README.md).

## Method

The survey used public primary sources: official project documentation,
repository READMEs, source repositories, and project issue trackers. Discovery
queries covered:

- programming languages using Cranelift, `cranelift-object`, AOT, or native
  executable emission;
- Cranelift JITs and language runtimes with garbage collectors;
- actor, process, mailbox, scheduler, or supervision runtimes using Cranelift;
- WebAssembly runtimes using Cranelift;
- language projects that evaluated but did not adopt Cranelift.

Projects were included when a primary source explicitly described executable
Cranelift code generation or a serious backend decision. Toy tutorials were
excluded from the main comparison. Abandoned projects remain relevant as
historical architecture evidence and are marked accordingly.

This method can establish known precedents and the absence of a match in the
reviewed set. It cannot establish universal uniqueness.

## Results Matrix

| Project | Input and execution model | Cranelift position | Relevant overlap | Decisive difference |
| --- | --- | --- | --- | --- |
| **Terlan 0.0.7 contract** | Terlan source to NativeIR to native object and `.tvm` image; separate JS shared/browser/worker targets | Sole native backend; AOT only | Compiler-owned ABI, safepoints, stack maps, managed values, actors, runtime kernel, JS compiler, integrated web packaging | Comparison baseline |
| [Raven](https://martian56.github.io/raven/) | Source to one static native binary | Native backend | Closest surface syntax; static types, traits, sum types, matching, `Result`/`Option`, tracing GC, goroutines, channels | Mutable bindings, global collector, channel concurrency, C FFI, no documented JS backend or browser build pipeline, and no documented actor fault/supervision model |
| [pon](https://github.com/can1357/pon) | Python to native JIT or AOT executable | Shared backend for JIT and AOT | Runtime ABI, GC, safepoints, precise Cranelift stack maps | Dynamic Python compatibility and tiered JIT; no actor runtime |
| [Lake](https://github.com/morphqdd/lake-native-compiler) | Process-oriented source to x86-64 ELF | Direct AOT backend | Processes, PIDs, mailboxes, waiting, reductions, scheduler | Single-thread cooperative runtime, fixed mailboxes, per-process arenas, no documented links/monitors/supervision |
| [Capy](https://github.com/capy-language/capy) | Statically typed source to native executable | Direct native backend | Language-owned lowering of arrays, structs, and first-class functions | Conventional executable and C interop; no managed actor kernel |
| [Lumina](https://github.com/luminalang/lumina) | Functional source to native code | Direct native backend | Language IR lowered to Cranelift | GC is unfinished and allocations may leak; no actor runtime |
| [MinCaml](https://github.com/osa1/mincaml) | MinCaml source to object code | Direct AOT backend | CFG lowering and native object emission | Learning project, x86-64 Linux only, no GC |
| [Haskelujah](https://haskelujah.org/) | Haskell to native executable or Wasm | Cranelift default; LLVM and Wasm also supported | Rust runtime, mark-sweep GC, native objects | Multiple backends, Haskell compatibility, no documented actor runtime |
| [Monty](https://github.com/mental32/monty) | Typed Python subset with compile-time interpreter | Only implemented backend | Direct language code generation | Archived toy project; planned LLVM/GCC alternatives |
| [Soplang](https://github.com/soplang/soplang) | Source through JIT or AOT paths | JIT and AOT backend | Standalone native binary path | JIT remains a first-class execution mode; no actor kernel |
| [frawk](https://github.com/ezrosent/frawk) | AWK-like source through JIT or bytecode interpreter | Optional JIT backend beside LLVM | Mature language workload using Cranelift | LLVM is recommended and bytecode interpretation remains |
| [rustc_codegen_cranelift](https://github.com/rust-lang/rustc_codegen_cranelift) | Rust through rustc to native code | Alternative nightly backend | Large-language frontend integration and native objects | Explicitly an alternative backend; Rust runtime semantics are not Cranelift-centered |
| [Inko](https://docs.inko-lang.org/manual/latest/design/compiler/) | Concurrent language to native objects | Uses LLVM; Cranelift replacement was evaluated | Lightweight processes, ownership, message passing, Rust runtime | The Cranelift replacement issue was closed without adoption |
| [Pony](https://www.ponylang.io/use/performance/pony-performance-cheat-sheet/) | Actor language to native code | Uses LLVM, not Cranelift | Per-actor heaps, independent tracing GC, multicore actor scheduler | Cross-actor references use capability and ORCA accounting; no Cranelift backend |
| [Wasmtime](https://docs.wasmtime.dev/stability-platform-support.html) | WebAssembly to AOT/JIT native code or Pulley bytecode | Default native backend where supported | Production hardening, AOT, runtime embedding, isolation | WebAssembly is the frontend contract; alternative compilers and interpreter exist; no language actor semantics |
| [Wasmer](https://docs.wasmer.io/runtime/features/) | WebAssembly to native code | One of Cranelift, LLVM, and Singlepass | AOT caching, runtime embedding, multiple targets | Pluggable Wasm runtime rather than a language/compiler-owned runtime |
| [Lucet](https://github.com/bytecodealliance/lucet) | WebAssembly to native sandboxed modules | AOT Cranelift compiler | Native module format, isolation, production deployment | WebAssembly input, no actor model, archived in favor of Wasmtime |
| [Gleam proposal](https://github.com/gleam-lang/gleam/issues/109) | Proposed native or Wasm backend | Not implemented | Fault-tolerant language domain would have been relevant | Proposal closed as not planned |

## Closest Precedents

### Lake

Lake is the closest public match on execution semantics. It directly compiles a
process-oriented language with Cranelift and models process spawning, PIDs,
mailboxes, blocking receive, and reduction-like scheduling. It is not a
complete match: its documented runtime is single-thread cooperative, uses
fixed-size mailbox rings and per-process arenas, targets x86-64 ELF, and does
not document links, monitors, structured exits, or supervision.

Lake invalidates any claim that Terlan is the first Cranelift language with
lightweight processes or message passing. It does not invalidate the combined
Terlan compiler, multicore, managed-memory, fault-recovery, and native-boundary
claim.

### pon

pon is the closest public match on compiler/runtime metadata integration. Its
Python pipeline shares one IR and runtime ABI between Cranelift JIT and AOT,
uses `cranelift-object`, and documents precise Cranelift stack maps in its typed
tier.

pon invalidates any claim that Terlan is the first language to combine direct
Cranelift AOT, a custom GC, safepoints, and precise stack maps. Terlan differs
by rejecting JIT and dynamic-language compatibility while integrating those
mechanics with actor isolation and supervision.

### Raven

Raven is the clearest counterexample to the simple frontend claim. It compiles
a statically typed language through Cranelift into a static native binary and
includes a tracing collector plus lightweight goroutines and channels.

Raven is also the closest surface-language peer found by this survey. Both
languages use `let`, PascalCase type names, typed functions, `struct`, nominal
keyed construction, generics, traits and implementations, `Result` and
`Option`, exhaustive pattern matching, and `->` match arms. This overlap is
large enough that comparative language-design claims must acknowledge Raven,
not only its Cranelift backend.

The grammars remain distinct. Raven uses `fun`, `match`, conventional brace
blocks, mutable `let` bindings, `self` methods, enums, goroutines, channels,
`defer`, and C FFI. Terlan uses declaration-ending periods, expression and
clause bodies, immutable bindings, explicit receiver methods, constructors and
unions, atoms, pipes, actor mailboxes, supervision, and isolated native
capabilities.

Terlan also has compile-time structural forms that Raven does not document.
Shape synonyms define reusable, hygienic pattern-and-guard expansions without
creating a runtime wrapper or nominal type. Implication constraints such as
`T => {name: String}` require compiler-proven structural evidence and permit
only the declared projections inside the generic scope. Raven instead uses
nominal trait bounds and offers type reflection; reflection does not provide
the same fail-closed structural proof.

Construction and composition also differ. Terlan constructor declarations are
typed, overloadable construction APIs with defaults, varargs, visibility, and
pattern identity; they are separate from the type's representation and from
ordinary receiver methods. Struct `includes` performs checked compile-time
field and eligible-method composition, traits may extend other traits, and
constructor chains compose construction explicitly. This is inheritance-like
reuse without classes, subtyping, parent-object identity, implicit coercion, or
virtual dispatch. Raven documents direct struct literals, enum variants, and
ordinary associated functions such as `Type.new()`, but no corresponding struct
inclusion or inheritance model.

The type abstraction surfaces are not equivalent either. Raven `enum`
declarations provide closed nominal sum types with unit, tuple, and named-field
variants. Terlan supports first-class type unions such as `A | B`, closed
valued unions with an explicit representation, constructor-defined variants,
and opaque nominal types whose representation is visible only inside the
defining module. Raven's public language reference does not document an opaque
type declaration or general union type expression.

Terlan also supports higher-kinded type parameters. `F[_]` binds a unary type
constructor, `F[_, _]` binds a binary constructor, and signatures may apply
those constructors as `F[A]` or `F[A, E]`. The compiler preserves kind arity
through interfaces, checks constructor applications during trait resolution,
and uses the feature for standard `Functor`, `Applicative`, and `Monad`
contracts. Raven documents first-order generic parameters on functions,
structs, enums, and implementations, but no syntax for abstracting over a type
constructor. Terlan does not yet provide type lambdas or partial type
application, so a partially applied constructor such as `Result[_, E]` still
requires a binary higher-kinded contract.

Both languages support integer range values, iteration, and interval matching,
but their contracts differ. Raven follows the Rust convention: `a..b` excludes
`b`, `a..=b` includes it, and either form may appear directly as a `match`
pattern. Terlan makes `a .. b` inclusive and lowers it to the public
`std.range.Range.Range` value. Terlan ranges choose an ascending or descending
default step, expose checked explicit steps, and participate in membership
guards, shape guards, comprehensions, and iterator traversal. An interval case
is currently written as a guarded pattern such as
`status where status in 200 .. 299`, rather than as a bare range arm.

Both languages support macros, but through different contracts. Raven provides
`@derive`, hygienic pre-parse token-rule macros, and compile-time and runtime
reflection. Terlan provides typed AST macros using `quote` and `unquote`, plus
raw embedded-language forms whose parser, expansion, and result contract can be
owned by a package or compiler profile. Raven's token manipulation and
reflection are broader general metaprogramming surfaces; Terlan's direction is
more type-directed and oriented toward checked syntax and embedded languages.

Terlan also owns a broader application build stack. In addition to the native
Cranelift lane, the compiler emits JavaScript for shared, browser, and worker
profiles through an Oxc-backed backend. Browser builds integrate an Rsbuild
pipeline backed by Rspack, generate or accept build configuration through the
Terlan project contract, package static assets and templates, emit a Terlan web
manifest, and feed `terlc serve`. TypeScript declaration surfaces can generate
Terlan browser and package bindings. Raven's public language and toolchain
documentation does not describe a JavaScript backend or comparable integrated
browser bundler.

This full-stack difference is relevant to product scope but is not evidence
that Terlan's use of Cranelift is unique: Cranelift participates in Terlan's
native lane, while Oxc and Rsbuild/Rspack own distinct JavaScript and asset
stages.

Raven explicitly describes itself as having no VM or interpreter. Its
concurrency model therefore does not establish the supervised runtime-kernel
and executable-image contract Terlan is building.

### Inko

Inko is the closest mature language-design peer because it combines native
compilation with lightweight processes, message passing, and a Rust runtime.
Its compiler currently lowers to LLVM. Inko considered replacing LLVM with
Cranelift as its only backend, but the tracking issue was closed without an
implementation.

Inko is evidence that Terlan's actor-native direction is not unique by itself,
and that using Cranelift rather than LLVM remains a meaningful implementation
distinction.

### Pony

Pony is the closest public match on per-actor memory management. Each actor has
its own heap and performs tracing collection independently, so collection of
one actor does not impose a global stop-the-world pause. Pony also has a
multicore actor scheduler and actor-cycle collection.

Pony differs from BEAM and Terlan's 0.0.7 contract in two decisive ways. It
uses LLVM rather than Cranelift, and objects allocated by one actor may remain
reachable from other actors under reference-capability restrictions. ORCA
therefore exchanges GC accounting messages between actors. Terlan instead
requires receiver-owned copies, unique ownership transfer, or explicit shared
immutable storage; a receiver cannot hold an actor-local reference into the
sender's heap.

### Wasmtime and Lucet

Wasmtime and the retired Lucet project are the strongest production precedents
for embedding Cranelift, AOT-compiling untrusted input, validating artifacts,
and maintaining a hardened native runtime boundary. They compile WebAssembly,
not a source language whose type system, actor semantics, GC roots, and native
resource model are designed together.

These projects are implementation references for compiler hardening and image
loading, not direct product analogues.

## Claims Policy

The following claims are rejected:

- "Terlan is the only language frontend using Cranelift."
- "Terlan is the only language using Cranelift as its native backend."
- "Terlan is the first Cranelift language with AOT compilation."
- "Terlan is the first Cranelift language with garbage collection or precise
  stack maps."
- "Terlan is the first process-oriented language compiled with Cranelift."
- "Terlan is the first native actor language with per-actor garbage
  collection."

The following wording is supported by this survey:

> Terlan integrates direct AOT Cranelift compilation with a compiler-owned
> native ABI and a supervised fault-tolerant actor runtime. No matching public
> architecture was identified in the 2026-07-24 ecosystem survey.

Shorter public wording may say:

> Terlan is building a Cranelift-native actor system: application code is
> AOT-compiled while the runtime owns isolation, scheduling, supervision, and
> recovery.

Do not turn the negative search result into an unqualified "first", "only", or
"unique" claim.

## Engineering Consequences

1. Treat Cranelift as infrastructure, not the product differentiator by itself.
2. Keep Terlan ABI, safepoint, stack-map, descriptor, and image formats
   independent from CLIF and Cranelift serialization.
3. Study pon for stack-map and AOT/GC integration, while preserving Terlan's
   no-JIT contract.
4. Track Raven as the closest surface-language and direct-AOT compiler peer,
   while keeping syntax comparison separate from runtime equivalence.
5. Study Lake for process-to-native lowering and explicit scheduler boundaries,
   while retaining Terlan's multicore, fault-recovery, and managed-memory
   requirements.
6. Study Wasmtime and Lucet for artifact validation, fuzzing, target support,
   and hostile-input hardening.
7. Re-run the survey before making public comparative claims and before each
   major compiler/runtime architecture release.

## Primary Sources

- [Cranelift project overview](https://cranelift.dev/)
- [Wasmtime platform and compiler support](https://docs.wasmtime.dev/stability-platform-support.html)
- [Wasmtime architecture](https://docs.wasmtime.dev/contributing-architecture.html)
- [Wasmer runtime backends](https://docs.wasmer.io/runtime/features/)
- [Lucet repository and end-of-life notice](https://github.com/bytecodealliance/lucet)
- [rustc Cranelift backend](https://github.com/rust-lang/rustc_codegen_cranelift)
- [Inko compiler design](https://docs.inko-lang.org/manual/latest/design/compiler/)
- [Inko Cranelift evaluation](https://github.com/inko-lang/inko/issues/674)
- [Pony per-actor garbage collection](https://www.ponylang.io/use/performance/pony-performance-cheat-sheet/)
- [Pony ORCA collector](https://www.ponylang.io/learn/papers/)
- [Raven language overview](https://martian56.github.io/raven/)
- [Raven language reference](https://martian56.github.io/raven/v2/guide/language-reference/)
- [pon compiler and runtime](https://github.com/can1357/pon)
- [Lake native compiler](https://github.com/morphqdd/lake-native-compiler)
- [Capy compiler](https://github.com/capy-language/capy)
- [Lumina compiler](https://github.com/luminalang/lumina)
- [MinCaml compiler](https://github.com/osa1/mincaml)
- [Haskelujah compiler](https://haskelujah.org/)
- [Monty compiler](https://github.com/mental32/monty)
- [Soplang compiler](https://github.com/soplang/soplang)
- [frawk compiler](https://github.com/ezrosent/frawk)
- [Gleam native-backend proposal](https://github.com/gleam-lang/gleam/issues/109)
