# Terlan 0.0.7 Roadmap

This roadmap is the active closeout plan for the 0.0.7 development line.
Completed implementation inventory belongs in
`docs/roadmap/RELEASE_NOTES_0_0_7.md`; this file intentionally tracks only
remaining release-blocking work and the gates that must prove it cannot
regress.

Detailed completed-slice history is archived in
[`archive/ROADMAP_0_0_7_IMPLEMENTED.md`](archive/ROADMAP_0_0_7_IMPLEMENTED.md).
Completed entries and embedded implementation-progress narration are removed
from this file so the active roadmap stays focused on unfinished work.

The release-blocking Rust structural-stabilization work in
[`ROADMAP_0_0_7_CODE_QUALITY.md`](ROADMAP_0_0_7_CODE_QUALITY.md) closed on
2026-07-29. Its numbered-fragment removal, clean Rust gates, build-graph
validation, typed failure boundaries, and permanent closeout aggregate apply
to all remaining 0.0.7 slices.

## Release Boundary

0.0.6 was the last released version before the VM-default pivot. 0.0.7
makes Terlan VM the default runtime direction. BEAM bytecode, Erlang source
lowering, ERTS packaging, EUnit, and OTP runtime execution are not product
contracts for this line.

Terlan still wants fault-tolerant runtime resiliency, but expressed in
Terlan-owned terms:
process isolation, supervised recovery, message passing, runtime inspection,
hot reload, resource ownership, cancellation, and typed NativeBoundary calls.

### 0.0.7 Primary Pivot: Direct AOT Compilation

0.0.7 now makes a second, release-blocking correction to the VM-default pivot.
The primary decision is that Terlan becomes a direct AOT native compiler written
in Rust. Checked Terlan compiler IR is emitted directly as native objects;
ordinary Terlan application modules are not regenerated as Rust and passed to a
second compiler. The VM is consequently a runtime kernel and supervisor for
AOT-compiled native Terlan images, not an interpreter for serialized compiler
IR. The normative forward contract is
[`TVM_EXECUTABLE_IMAGE_SPEC.md`](../runtime/TVM_EXECUTABLE_IMAGE_SPEC.md).

Cranelift is the sole 0.0.7 native code-generation backend. Terlan owns
NativeIR, ABI classification, safepoints, compact stack maps, descriptors, and
image semantics; Cranelift owns instruction selection, register allocation,
frame finalization, relocations, and object emission. LLVM is not a Terlan
application backend or optional release profile.
The dated
[`CRANELIFT_ECOSYSTEM_SURVEY.md`](../compiler/CRANELIFT_ECOSYSTEM_SURVEY.md)
records the public precedents, closest architectural peers, and comparative
claims this decision can support.

This decision supersedes the earlier assumption that `.tvm.json`, serialized
VMIR, or a binary encoding of Terlan instructions could become the final TVM
execution format. A `.tvm` is a target-native ELF, Mach-O, or PE executable
image with a minimal embedded TVM descriptor. Compiler interfaces, source text,
CoreIR, VMIR, generated source, cache fingerprints, and quality reports are not
runtime image contents.

The hard release boundary is:

- no JIT and no Terlan bytecode;
- no serialized CoreIR or VMIR execution in the final runtime path;
- all reachable Terlan computation and control flow are AOT-compiled;
- same-shard actor operations use typed native runtime fast paths, not
  instruction sequences, canonical serialization, or control-plane IPC;
- pointer-free TVM transport is reserved for real shard, OS-process, node,
  persistence, migration, debugger, and unsafe-native isolation boundaries;
- format 1 images run in supervised execution shards, while memory-unsafe
  Rust/C/C++/CUDA adapters run in separate NativeBoundary workers by default;
- one application and target produce one deployable `.tvm` image;
- generated sources, objects, fingerprints, and reports remain internal cache
  or ephemeral evidence;
- no LLVM library, LLVM IR, bitcode, `opt`, `llc`, ORC, or LLVM stack-map format
  participates in Terlan application compilation;
- `.tvm.json` and its interpreter are transitional removal paths and cannot be
  extended as forward architecture.
- every already-ported Erlang-derived semantic test keeps the same Terlan
  fixture, assertions, expected behavior, and public test identity; the pivot
  changes only its classified runner/artifact harness unless the test is proven
  to assert a BEAM, ERTS, opcode, or interpreter implementation detail.
- all existing JavaScript lowering, emitted behavior, golden output, source-map,
  declaration, browser, server, and package tests remain unaffected; shared-IR
  edits must prove byte-for-byte stability where promised and behavioral
  equivalence everywhere else.

All previous VMIR execution checkmarks record historical implementation and
migration evidence only. They do not satisfy the revised 0.0.7 release boundary.
Reusable compiler semantics, runtime ownership, NativeBoundary isolation,
actor/resource machinery, and adversarial tests must be retained while their
execution path is moved to AOT-native images.

## Runtime Principles

- The compiler owns language semantics and AOT-emits target-native TVM
  executable images; CoreIR and VMIR are compiler-internal only.
- The VM owns actor scheduling, mailboxes, timers, wakeups, cancellation,
  resource lifecycle, backpressure, failure propagation, and runtime
  observability.
- The execution-shard runtime performs local actor mechanics directly; the VM
  supervisor control plane admits, observes, restarts, and replaces shards.
- Actor heaps are independently collectible, moving, precise, and non-atomic on
  the owning actor fast path; atomic reclamation is limited to shared immutable
  bulk storage.
- Portable collections are VM values, not NativeBoundary handles.
- Tokio, OTP, and BEAM-derived paths must not define default runtime
  semantics.
- Release validation must prove Terlan behavior through Terlan-owned tests and
  gates.

## Implementation Order

0.0.7 is intentionally large, but the work must land as complete vertical
slices rather than disconnected parser/runtime fragments.

1. Freeze new serialized-VMIR execution work and audit every VM gate, runtime
   entry point, REPL path, debugger path, HTTP path, and package consumer as
   reusable runtime semantics, transitional compatibility, or deletion debt.
2. Implement the compiler-owned direct native-object backend and prove a typed
   Terlan function can compile and link without generated application Rust,
   `rustc`, a C/C++ compiler, or a per-module linker invocation.
3. Freeze the TVM descriptor byte layout and platform section mappings, then
   prove one statically inspectable target-native `.tvm` image with one typed
   AOT export.
4. Replace compiler-process-per-module output with content-addressed native
   objects emitted directly by a compiler-owned backend, bounded compilation
   units, incremental reuse, package-parallel compilation, and one final image
   link per application and target. Go-class development compilation speed is
   the release objective; generated Rust plus `rustc` is bootstrap evidence,
   not the product application backend.
5. Make the VM validate, launch, observe, restart, and terminate execution
   shards through a bounded control protocol; implement park/resume, scheduling,
   mailbox, timer, and actor heap mechanics on typed same-shard native paths.
6. AOT-compile pure, effectful, and actor control flow, migrate REPL/test/HTTP/
   debugger consumers, then delete `.tvm.json` execution and the serialized-
   VMIR interpreter.
7. Resume dependent language, package, PyTorch, Polars, OpenCV, CUDA, web, and
   ML closure only on the conforming TVM execution path. Work that does not
   deepen the transitional runtime may proceed independently.
8. Finish proof, coverage, performance, and release gates after the AOT-native
   execution surface stops moving.
9. Only after Slices 100 and 101A through 101I are complete, replace the
   handwritten parser with a LALRPOP-generated token-stream parser. This work
   must not share the critical path with the AOT pivot or cause parser churn
   while native-image semantics are still changing.
10. Close the direct-AOT developer loop with failed-edit-safe hot reload before
    release. Development reload is a 0.0.7 product contract; production
    generation admission, rollout, and rollback belong to 0.0.8.

Each slice must include syntax/parser coverage where relevant, typechecker
coverage, VM/default execution coverage, adversarial diagnostics, formatter
coverage, editor/tree-sitter coverage when syntax changes, and a Make gate
before it can be marked complete.

The pivot migration uses a same-corpus rule: ported Erlang-derived tests are run
unchanged through both the last accepted transitional baseline and the new
AOT-native path until the native path replaces it. The comparison is by stable
test identity and source/expectation digest, not merely by aggregate pass count.
Any missing, renamed, weakened, skipped, or expectation-edited test fails the
pivot gate unless an explicit implementation-specific classification names the
evidence, owner, replacement coverage, and expiry.

The same gate records the complete pre-pivot JavaScript suite and output digest
inventory. Direct-AOT changes may not remove, skip, weaken, rename, or regenerate
those fixtures. A shared-compiler change is admitted only after all existing
JavaScript gates pass and the output inventory is unchanged except for a
separately reviewed JavaScript feature or bug fix unrelated to the TVM pivot.

## Quality Enforcement Rule

0.0.7 must not trade correctness for checklist progress. A slice can be marked
complete only when the implementation is usable, the tests exercise real
behavior through the intended user path, and the gate proves the behavior fails
for the right reasons.

Hard rules:

- Do not mark a slice complete because source files exist, generated files
  exist, strings appear in an artifact, or a declaration typechecks.
- Do not accept marker checks as a substitute for executing the feature through
  the intended user path.
- Do not accept declaration-only tests, fake surface tests, `assert(true)`,
  identity assertions, or tests that prove only that a symbol exists.
- Do not accept syntax-only implementation for a language feature unless the
  slice is explicitly a reservation slice and rejects use with stable
  diagnostics.
- Do not accept VM/runtime code that is tested but not wired into the active
  runtime path unless it is listed as dormant implementation debt with a
  follow-up gate.
- Do not accept `.tvm.json`, serialized VMIR execution, or an interpreter test
  as evidence that the TVM executable-image path is complete.
- Do not add new runtime behavior only to the transitional artifact evaluator.
  A required temporary fix must be labeled removal debt and paired with its
  AOT-native migration owner.
- Do not place generated native source, object files, fingerprints, compiler
  metadata, or quality JSON beside deployable TVM images.
- Do not accept AOT work that starts a native compiler once per function or
  module, relinks a true no-op build, or regresses the required cold,
  incremental, no-op, release, relink, and REPL timing baselines.
- Do not accept generated Rust, C, or C++ plus a secondary compiler as the final
  code-generation route for ordinary Terlan application functions. Native
  binding adapters remain allowed and must be separately cached.
- Do not accept generated bindings unless unsupported source constructs are
  accounted for in a skipped/unsupported manifest with stable reasons.
- Do not accept std adversarial coverage that avoids table-driven or
  property-based testing when the failure surface is a matrix, invariant,
  parser/renderer pair, sequence, or generated-value space.
- Do not accept a slice that grows Rust files past the reviewed file-size
  baseline or adds inline tests to implementation modules. Slice closeout must
  run `make rust-quality-check`, which owns file-size, separated-test, and
  dormant-runtime-code enforcement.
- Do not accept a slice until the touched Rust code has been inspected for code
  smells: redundant types, duplicate helper functions, dead wrappers,
  near-identical match arms, one-off abstractions, and concepts that should be
  shared across feature modules.
- Do not accept a roadmap checkbox until its gate passes locally and the
  feature has positive tests, adversarial tests, and documentation or generated
  docs where user-facing.

If a gate is too weak to prove the feature, strengthen the gate before marking
the slice complete.

## Completed Work Cleanup

The active roadmap intentionally excludes implemented checklist slices.
Completed execution history belongs in `docs/roadmap/RELEASE_NOTES_0_0_7.md`
or gate-specific documentation, not in this file.

Completed areas no longer tracked here include executable documentation,
ACME/TLS behavior, formal type spec seed work, target inference, standard
library source naming with uppercase-leading final std module segment
enforcement, package Git source contracts, native binding generator contract
work, and golden runtime ownership cleanup. Previous VM runtime expansion and
performance results remain useful behavior baselines, but they are reopened for
execution-path conformance under the AOT-native TVM pivot.

## Active Closure Requirements

### Value Lifecycle And Compile-Time Constants

- [x] Close the 0.0.7 value-lifecycle inventory and add typed module constants.
  - Contract: `docs/compiler/TERLAN_0_0_7_VALUE_LIFECYCLE_INVENTORY.md` is the
    authoritative matrix for evaluation time, storage, identity, mutability,
    visibility, effects, portability, and artifact behavior of value-producing
    source forms.
  - Hard decision: add `[pub] const NAME: Type = Expr.` with mandatory
    `SCREAMING_SNAKE_CASE`, explicit types, deterministic typed compile-time
    evaluation, semantic substitution, no runtime storage, and no observable
    address identity.
  - Hard decision: add closed nominal valued unions using
    `[pub] type Name: Representation = CONSTANT = ConstExpr | ...`. The
    representation type is explicit once; every arm must const-evaluate to it,
    inherits the type's visibility, and defines a type-owned constant accessed
    as `Name.CONSTANT`. Reject duplicate names/values, incompatible or
    non-constant arms, and implicit representation-to-union conversion. Parsing
    a representation is checked, and patterns/exhaustiveness operate over the
    closed set of arms.
  - Hard decision: valued-union arms are Terlan's inherent type-constant form.
    Do not add `enum`, inherent `impl` blocks, repeated arm-level `const`, or
    free-form qualified declarations such as `const Status.OK: Status = 200`.
    Standalone constants remain module-owned; trait-associated constants remain
    contract-owned.
  - Hard decision: atoms/bodyless aliases remain symbolic singleton identity;
    local runtime values remain `let`; 0.0.7 does not add local constants or
    `let mut`. Terlan has no runtime `static` declarations, mutable globals,
    runtime module variables, implicit module initializers, or load-order
    effects. Existing `static` configuration metadata remains tooling input,
    not a runtime value declaration.
    The REPL cannot declare constants, but it can import public module,
    valued-union, and trait-associated constants and substitute their exported
    typed values while compiling submitted expressions.
    Receiver-declared `mut` operations authorize compiler-controlled writeback
    to assignable local places while portable aliases retain prior values;
    shared runtime identity and mutation require explicit resource/actor
    identity. Identity-free values use constants, and large immutable data uses
    assets rather than global storage.
  - Hard decision: Terlan has no language-level lazy value declarations or
    `lazy` binding syntax. Repeatable deferred work uses explicit closures;
    memoized one-time work uses a typed library/resource API that exposes its
    ownership, synchronization, failure caching, retry, and cancellation
    behavior. Constants are compile-time evaluated and never lazy.
  - Hard decision: constants have no runtime reflection. Public declarations
    and exported typed values are inspectable by compiler interfaces, imports,
    generated documentation, editor tooling, and REPL compilation, but runtime
    code cannot enumerate constants or look one up by name. Runtime registries
    must be declared explicitly as ordinary constant aggregates.
  - Hard decision: qualified, directly imported, and aliased public constants
    may appear as value patterns. Resolve and substitute them before applying
    ordinary pattern-comparability, duplicate/unreachable-pattern, and
    exhaustiveness rules; constants add no separate equality semantics. A
    changed exported value invalidates dependent pattern analysis and caches.
  - Hard decision: refutable `let` patterns are accepted as explicit match
    assertions. Compiled failure produces structured `MatchError` and follows
    catch/supervision semantics; REPL matching is transactional and commits no
    partial bindings on failure. Exhaustive recovery uses `case`, while const
    evaluation reports a failed assertion at compile time.
  - Hard decision: add explicit kind-checked const generic parameters such as
    `Buffer[const SIZE: Int, T]`. Initial const kinds cover `Int`, `Bool`, and
    `Atom`; arguments accept literals, resolved constants, in-scope const
    parameters, and const-function calls over those inputs. Inline type
    arithmetic, float/string const parameters, runtime-dependent types, and full
    dependent typing remain deferred.
  - Hard decision: initially allow constant references in ordinary expressions,
    guards, constant initializers, defaults, value patterns, and explicitly
    const-kinded type arguments. Reject them in annotations, annotation schemas,
    target/native/machine/static config, paths, declaration names, conditional
    compilation, runtime configuration, and secret providers.
  - Phase decision: hygienic syntax macros expand before constant evaluation.
    Macros may generate or preserve constant syntax but cannot initially inspect
    or branch on evaluated constant values; constant resolution/evaluation then
    precedes ordinary typechecking and runtime substitution.
  - Hard decision: add compile-time-only const functions using
    `[pub] const lower_name(...): Type -> Expr`. Const functions return typed
    values, call only const-safe operations/functions, export portable evaluator
    IR when public, and emit no runtime callable, closure, VMIR body, or runtime
    symbol. Runtime use is a stable compile error.
  - Hard decision: traits may require or default associated constants and
    implementations provide missing values. Access is canonically
    `TraitName[TypeArgs].CONSTANT`; values are const-evaluated with no runtime
    storage or dynamic dispatch. Valued-union alternatives provide the separate
    inherent type-constant form through `TypeName.CONSTANT`.
  - Requirement: replace the one-off literal predicate currently used by
    parameter and constructor defaults with one typed const evaluator. Const
    contexts must opt in explicitly rather than acquiring compile-time behavior
    from parser shape alone.
  - Requirement: resolve constant/type/constructor namespace interaction,
    imports and aliases, constant value patterns, public interface payloads,
    const generic declarations/applications and kinds, compile-time-only const
    functions, trait-associated constants, valued unions and their explicit
    representation/checked-conversion rules, cycles and evaluator resource
    limits, REPL imports of public constants, source provenance,
    incremental-cache invalidation, hot reload, package
    compatibility, and cross-target deterministic behavior.
  - Requirement: close the adjacent P0 blind spots in the inventory:
    module initialization/global storage, hygienic typed user expression macro
    semantics, mutation/aliasing semantics, fallible binding contexts, and
    runtime configuration/secret separation. User raw-text, declaration, type,
    and pattern macros remain explicitly staged behind the expression-macro
    contract.
  - Security requirement: const evaluation cannot read environment variables,
    files, clocks, randomness, networks, NativeBoundary resources, runtime
    configuration, or secret providers. Private constants are still treated as
    artifact-embedded content.
  - Acceptance: public/private constants parse, format, resolve, import,
    typecheck, evaluate, substitute, and execute consistently on supported
    targets; defaults reuse the evaluator; negative fixtures reject invalid
    naming, effects, mutation, handles, cycles, ambiguous names, target-host
    dependence, secret/config access, evaluator exhaustion, and stale dependent
    artifacts with stable diagnostics.
  - Gate: add `make value-lifecycle-contract-check` and include it in the
    canonical 0.0.7 language/release gate before marking this item complete.

### Bitstring And Binary Processing

- [x] Add VM-native HTTP middleware and router composition on top of the VM
  HTTP transport façade.
  - Problem: transport alone does not define an application architecture for
    shared concerns, route trees, or cross-cutting request behavior.
  - Requirement: add composable middleware primitives in `std.http` for request
    pre/post-processing (authn/authz stubs, trace context propagation, panic
    boundary, header normalization, timeout, and recovery).
  - Requirement: support router composition primitives that are typed and pattern
    compatible with existing Terlan pattern matching, including method/path
    segments and parameter captures.
  - Requirement: provide explicit middleware order and short-circuit semantics:
    request enters middleware chain in declaration order, can terminate early,
    and can continue with typed request/response context.
  - Requirement: route handlers must be pure-by-default where practical;
    side-effectful stages must be explicit by type contract or capability.
  - Requirement: support scoped middleware composition per route group and app
    wide middleware chains.
  - Requirement: include diagnostics for ambiguous matches, missing routes, and
    middleware conflicts with stable machine-readable codes.
  - Requirement: preserve backpressure semantics through middleware layers and
    ensure response buffering policies are explicit.
  - Requirement: integrate with existing `std.http` transport abstraction and use
    same typed request/response models to avoid duplicate decoding.
  - Requirement: add docs and examples for nested routing trees, middleware
    chaining, and typed response short-circuit behavior.
  - Tests:
    - parser/typecheck tests for router DSL and middleware type constraints.
    - executable `.terl` routing tests covering method/path matching, captures,
      middleware ordering, short-circuit/recovery, and error mapping.
    - adversarial tests for ambiguous route resolution, double-consume request
      body, unsupported backend results, and invalid middleware state transitions.
    - concurrency/perf smoke tests for many concurrent routes under bounded
      request load.
  - Implemented evidence: materialized SSE and WebSocket endpoint plans retain
    their source-declared queue, message-size, and keep-alive policies in
    production live-session admissions; bounded many-route concurrency coverage
    is anchored in the HTTP stack gate.
  - Gate: extend `make vm-http-stack-check` with
    `vm_http_router_middleware` anchor.
  - Make integration: run this gate after VM transport façade and before
    richer HTTP features such as static assets and websocket/stream handlers.
  - Acceptance: users can express production-like route graphs with typed
    middleware composition without backend runtime-specific handler glue.

- [x] Add protocol stack benchmarking and regression harness for VM-native
  binary/transport workloads.
  - Problem: we now have protocol parsing, framing, and transport slices, but no
    reproducible benchmark harness that proves stability across binary workload
    shapes and transport concurrency regimes.
  - Requirement: add a benchmark driver under `scripts/benchmarks/protocol` with
    stable workload definitions for:
    - binary construction/decode roundtrips,
    - protocol shape parse/recompose suites,
    - TCP framing decode/encode throughput and latency,
    - VM HTTP request lifecycle, header/method/path parsing, and body read/write.
  - Requirement: each workload must run cold and warm phases:
    - cold: first-classload/spawn/compile path measured once,
    - warm: steady-state run loop measured at fixed iterations.
  - Requirement: execute each workload at at least `{1, 10, 100, 1_000}` scale
    points, with fixed seeds and deterministic payload generation.
  - Requirement: persist results as stable JSON/TSV artifacts in-repo for regression
    comparison containing:
    - runtime lane and profile,
    - commit/platform/rust-version,
    - workload and phase,
    - iterations and concurrency,
    - mean/median/p95/p99,
    - error rate and typed decode-failure counts,
    - winners and relative percentages versus baseline.
  - Requirement: include at least one adversarial workload set (truncated
    payloads, invalid UTF-8, malformed framing, impossible widths, duplicate
    captures, and unsupported-backend paths).
  - Requirement: baseline snapshots must include:
    - VM default lane,
    - comparable legacy runtime lane (where available),
    - clearly documented unsupported comparisons.
  - Requirement: add a benchmark comparison report format in
    `docs/benchmark_reports/` and enforce that every run prints and stores the
    lane winner and delta.
  - Gate: extend `make vm-semantics-vs-otp-check` and
    `make vm-http-vs-axum-check` with anchors:
    `binary_protocol_benchmark` and `binary_protocol_concurrency_benchmark`.
  - Make integration: run this harness after `binary-bitstring-processing-check`
    and before distributed protocol transport acceptance.
  - Acceptance: benchmark artifacts are machine-readable, versioned, deterministic,
    and include explicit degradation/winner percentages for both success and
    adversarial workload classes.
- [x] Add distributed fault semantics, partition recovery, and migration rollback
  guarantees.
  - Problem: scheduler-level migration and transport primitives exist, but we do
    not yet have a coherent fault-recovery contract that governs failure
    sequencing across nodes, partitions, and in-flight process moves.
  - Requirement: define VM-owned fault-state semantics for distributed execution:
    `Suspected`, `Degraded`, `Isolated`, `Recovering`, and `Recovered`, with
    explicit transition rules and monotonic constraints.
  - Requirement: add typed failure envelopes for distributed operations:
    failed heartbeat, suspected partitions, migration timeout, migration partial
    commit, stale placement updates, and recovery window expiry.
  - Requirement: define deterministic recovery policy:
    - heartbeat and suspicion thresholds are explicit,
    - migration rollback and replay policies are deterministic,
    - partial/duplicate outcomes are idempotent under retry.
  - Requirement: ensure that actor/process migration is not completed until all
    required state and in-flight message contracts are satisfied.
  - Requirement: add VM diagnostics that are stable and machine-readable for:
    partition onset, suspect quorum, node role demotion, recovery completion,
    and migration rollback decisions.
  - Requirement: define compatibility behavior for non-partition-tolerant nodes:
    explicit `feature_unsupported`/`fallback_local_only` outcomes, not silent
    undefined behavior.
  - Tests:
    - parser/typecheck tests for fault-state declarations and recovery policy
      declarations.
    - VM executable `.terl` tests for suspect→isolation, isolation→recovery,
      migration timeout rollback, duplicate-heartbeat suppression, and policy
      fallback.
    - adversarial tests for partition oscillation, stale rejoin, rollback loops,
      and mismatched recovery policy resolution.
    - deterministic replay tests for out-of-order fault events and duplicate
      recovery messages.
  - Gate: extend `make vm-distributed-scheduling-check` with
    `vm_distributed_fault_recovery` anchor.
  - Make integration: run this gate after
    `vm_distributed_scheduler_and_migration` and before any distributed state
    replication persistence slices.
  - Acceptance: one `.terl` distributed scenario demonstrates deterministic fault
    classification, bounded recovery, and migration rollback without orphaned state.
### Compiler Purity And Optional Effects

- [x] Add compiler-inferred purity, `@pure` invariant assertions, and optional
  effect values for advanced composition.
  - Requirement: the compiler must infer purity/effects for all functions
    where the body is available. Users should not have to annotate ordinary
    pure helper functions before guards, templates, or lowering can use them.
  - Requirement: `@pure` is not a new language declaration kind and not a
    runtime syntax form. It is compiler metadata attached to ordinary
    functions and means "this function must remain side-effect free."
  - Requirement: accepted source shape:
    ```terl
    @pure
    pub normalize(value: Int): Int ->
        value * 100.
    ```
  - Requirement: the compiler must validate `@pure` as an invariant contract;
    it cannot be a trusted unchecked promise. A pure function cannot perform
    side effects: send messages, mutate VM state, mutate resources, call
    NativeBoundary effects, allocate external handles, read clocks/randomness,
    start processes, perform IO, query databases, or call functions that
    perform effects.
  - Requirement: changing an annotated `@pure` function so it performs an
    effect must fail compilation. `@pure` is a regression detector and public
    API promise, not the only way purity is known.
  - Requirement: pure helper calls are allowed inside flexible shape guards
    and typed template interpolation when the compiler can prove purity,
    whether purity is inferred or asserted with `@pure`.
  - Requirement: optional monadic/effect values may describe effects without
    performing them. This must remain an advanced opt-in style, not mandatory
    for normal Terlan application code:
    ```terl
    @pure
    pub handler_plan(req: Request): Effect[Response] ->
        Users.find(req.user_id)
        |> Effect.map(Response.json).
    ```
  - Requirement: a function returning `Effect[T]` may still be pure if it only
    constructs an effect description. The VM/runtime boundary that executes
    the effect is effectful and must be tracked separately.
  - Requirement: ordinary direct-effect code remains valid outside pure
    contexts. Terlan must not force Haskell-style effect wrapping on beginner
    or normal application code.
  - Requirement: purity metadata must be emitted into module summaries, docs,
    LSP hover, and any deploy/native metadata that uses purity for lowering or
    capability analysis.
  - Requirement: `@pure` may enable constant folding, native lowering, VM
    bypass for pure helper code, guard calls, and template slot validation, but
    those optimizations must not change observable semantics.
  - Gate: add `make compiler-purity-metadata-check`.
  - Remaining gaps: VM scheduling and cancellation for pending/external
    `Effect[T]` descriptions, plus native-lowering use beyond completed values.
  - Make integration: run `compiler-purity-metadata-check` before
    `flexible-shape-guards-check`, `typed-template-interpolation-check`, and
    native lowering gates.
  - Acceptance: executable `.terl` tests prove inferred pure helper calls and
    `@pure` asserted helper calls work in guards and templates, while ordinary
    direct-effect function calls still work outside pure contexts.
  - Acceptance: executable `.terl` tests prove a pure function can return an
    `Effect[T]` description without executing it, and that effect execution is
    tracked at the VM/runtime boundary.
  - Acceptance: adversarial tests prove every disallowed effect inside
    `@pure` reports a stable diagnostic, inferred impure helpers are rejected
    from guards/templates, and optional `Effect[T]` APIs are not required for
    normal direct-effect code.

### Comprehension Guards

- [x] Add guard/filter clauses to comprehensions, including VM-scheduled async
  guard forms.
  - Requirement: comprehension guards do not use `where`. After generators,
    any comma-separated expression that typechecks as `Bool` is a filter.
    `where` remains reserved for pattern/function-head/case guards where it
    separates a pattern from a condition.
  - Requirement: pure comprehension guards may run inside ordinary list/set/map
    comprehensions:
    ```terl
    [
        user.name
        | user <- users,
          user.active,
          user.age in 18..120
    ].
    ```
  - Requirement: async/effectful guards have no special keyword and are not
    hardcoded to one effect type. A guard expression is accepted when its
    result type implements the core `GuardResult` contract. `Bool` is the
    built-in pure guard result. Effectful result types, including a possible
    `Effect[Bool]`, must opt in through that same contract instead of being
    syntax-special.
  - Requirement: if any comprehension guard returns an effectful
    `GuardResult`, the whole comprehension result is lifted through the
    guard result's declared effect container. For example, a guard result that
    lifts through `Effect` turns `List[T]` into `Effect[List[T]]`. A function
    whose declared return type is pure must reject that comprehension with a
    stable diagnostic.
  - Requirement: async guard execution is VM-owned. The VM schedules guard
    effects, preserves source order by default, propagates
    cancellation/backpressure, and returns stable typed errors when an async
    guard fails. Parallel guard evaluation is not implicit in 0.0.7; it must
    be requested later through an explicit std helper or stream combinator.
  - Requirement: async guard syntax must stay Terlan-shaped and must not add a
    JavaScript-like `async`/`await` model or a `where effect` form. The guard
    result contract determines whether the comprehension is pure or effectful:
    ```terl
    pub visible_users(current_user: User, users: List[User]): Effect[List[User]] ->
        [
            user
            | user <- users,
              user.active,
              Permissions.can_view(current_user, user)
        ].
    ```
    where `Permissions.can_view(current_user, user)` returns a type that
    implements `GuardResult` and lifts the comprehension into `Effect`.
  - Requirement: multiple async guards run in source order after earlier pure
    guards have passed for the same candidate:
    ```terl
    [
        user
        | user <- users,
          user.active,
          Permissions.can_view(current_user, user),
          Audit.allowed_for_report(current_user, user)
    ].
    ```
  - Requirement: pattern guards and function-head guards remain pure-only and
    accept only the pure `Bool` guard result in 0.0.7. Effectful
    `GuardResult` implementations are accepted only in comprehensions whose
    result type is lifted by the guard contract.
  - Requirement: the compiler must reject non-guard-result expressions,
    effect containers without a `GuardResult` implementation, conflicting
    guard-result lift containers in one comprehension, and untracked effects
    with stable diagnostics.
  - Requirement: comprehension guards must support variables bound by earlier
    generators and must reject references to variables introduced by later
    generators.
  - Requirement: formatter, parser, typechecker, CoreIR, VM execution,
    JS/backend diagnostics, LSP, tree-sitter, and language feature coverage
    must agree on pure and async comprehension guard support.
  - Gate: add `make comprehension-guards-check`.
  - Remaining implementation state: async/effectful `GuardResult` guards,
    lifted result containers and VM scheduling/cancellation, mixed stacked
    pure/effectful guard planning, set/map comprehension execution, and broader
    JS guarded-comprehension lowering for imported helpers and effectful
    results are still open.
  - Make integration: run `comprehension-guards-check` after
    `compiler-purity-metadata-check` and before language feature coverage.
  - Acceptance: executable `.terl` tests prove pure list/set/map
    comprehension guards, multiple guards, generator ordering, range
    membership, pure helper calls, and empty-result behavior through the
    default VM path.
  - Acceptance: executable `.terl` tests prove effectful `GuardResult`
    guards in the explicit stream/effectful comprehension form, including
    success, false filter, typed error propagation, cancellation, ordering
    semantics, and the absence of syntax-special `Effect[Bool]` handling.
  - Acceptance: adversarial tests prove effectful guards are rejected when the
    surrounding comprehension result type is pure, effectful guards are
    rejected in pattern guards, unknown guard variables fail, later generator
    variables are unavailable, non-guard-result expressions fail, conflicting
    guard-result lift containers fail, `where` inside comprehensions reports a
    stable diagnostic, and unsupported backend lowering reports stable
    diagnostics.

### Shape Synonyms

- [x] Add `shape ... = ...` compile-time pattern aliases.
  - Requirement: use `shape`, not `pattern`, in source code. `pattern`
    remains an implementation/spec term; `shape` is the user-facing Terlan
    construct.
  - Requirement: use `=` for the shape definition body. `shape` already marks
    the declaration as a compile-time matching alias, so `=>` would be
    redundant here. Runtime function bodies, case branches, lambdas, and
    handlers continue to use `->`.
  - Requirement: keep `=>` reserved for a future feature where the arrow
    itself carries semantics. Shape synonyms must not consume it.
  - Requirement: support reusable named match shapes with bound variables and
    optional guards:
    ```terl
    shape OkResponse(body) =
        {status, body} where status in 200..299.

    case response {
        OkResponse(body) -> Ok(body);
        _ -> Err(BadResponse)
    }.
    ```
  - Requirement: shape synonyms must not allocate, construct runtime values,
    or introduce wrapper types. They expand into pattern/guard logic during
    compile-time validation.
  - Requirement: exported shapes become part of the module's public matching
    API and must be visible to docs, LSP hover, completion, tree-sitter, and
    generated module summaries.
  - Requirement: shape synonyms may bind variables, include wildcard slots,
    compose with constructor, tuple, list, map, and future struct patterns,
    and participate in function-head pattern parameters.
  - Requirement: extractor-backed shapes are allowed only when the extractor
    is a compiler-visible pure function that returns a typed match result.
    The extractor must not perform IO, mutate state, allocate external
    handles, or depend on request-global hidden state.
  - Requirement: extractor shape binding must be deterministic: the same input
    value and extractor arguments produce the same match success/failure and
    the same bound variables.
  - Requirement: HTTP route matching must not introduce route-specific
    syntax. A route matcher is just a matchable shape/extractor in ordinary
    pattern matching:
    ```terl
    case request {
        Route("GET", "/users/${id: Int}") -> Users.show(id);
        Route("POST", "/users") -> Users.create(request);
        _ -> NotFound
    }.
    ```
  - Requirement: the web layer must build route dispatch from normal `case`,
    normal function-head patterns, normal shape definitions, normal guards,
    and normal functions. No `routes {}` grammar, route branch arrow, or
    web-only declaration form belongs in 0.0.7.
  - Requirement: route-shaped matches must type path captures from the
    literal path pattern, for example `${id: Int}`, and must fail with stable
    diagnostics for unknown capture types, duplicate capture names, malformed
    path patterns, and branch handlers receiving the wrong capture type.
  - Requirement: recursive shapes and ambiguous overlapping shape expansions
    are rejected in 0.0.7 unless the compiler can prove a deterministic
    expansion.
  - Requirement: parser, formatter, typechecker, CoreIR, VM pattern binder,
    JS/backend diagnostics, LSP, tree-sitter, docs, and coverage inventories
    must agree on `shape ... = ...`.
  - Gate: `make shape-synonyms-check`.
  - Make integration: run `shape-synonyms-check` from `make check` before
    `flexible-shape-guards-check` and `pattern-matching-support-check`.
  - Remaining gap: explicit implication contracts for imported or otherwise
    opaque predicates.
  - Acceptance: executable `.terl` tests prove local shapes, exported/imported
    shapes, guard-bearing shapes, nested shapes, route-shaped extractor
    patterns, function-head shape parameters, and wildcard fallback behavior
    through the default VM path.
  - Acceptance: adversarial tests prove recursive shapes, unknown shape names,
    arity mismatch, duplicate bindings, impure shape guards, ambiguous
    expansion, runtime constructor misuse, and unsupported backend lowering
    report stable diagnostics.

### Shape Implications

- [x] Add the `=>` implication arrow for compile-time structural evidence.
  - Requirement: `=>` is called the implication arrow. It is not a runtime
    execution arrow, not a conversion operator, not a generator hook, and not a
    macro system.
  - Requirement: `where` is reserved for runtime/value guards only. Implication
    evidence must not use declaration `where` clauses.
  - Requirement: the initial supported form is positive structural implication
    in generic parameter constraints only. `where` is reserved for runtime
    guards and is not an implication surface.
  - Requirement: generic parameter lists use implication shorthand when the
    evidence is local to that generic parameter:
    ```terl
    pub display_name[T => {name: String}](value: T): String ->
        value.name.

    pub struct Page[T => {title: String}] {
        model: T
    }.

    pub impl Render[T => {title: String}] for T {
        render(value: T): Html ->
            h1(value.title).
    }

    pub type Named[T => {name: String}] =
        T.
    ```
    This shorthand is the canonical implication syntax and must not desugar to
    any declaration `where` implication form.
  - Requirement: `T => Shape` means the compiler can prove that values of `T`
    expose at least the required structural shape. It does not allocate,
    construct a wrapper, call user code, or convert the value.
  - Requirement: implication checking is fail-closed. If the compiler cannot
    prove the left side entails the right side from explicit evidence, the
    program is rejected. There is no best-effort implication inference and no
    runtime fallback.
  - Requirement: every accepted implication must produce typed compiler
    evidence with provenance. Accepted evidence sources are built-in core
    rules, explicit user declarations, generated binding manifests,
    shape definitions with guards, concrete closed types, and already-proven
    trait/type facts. Ad hoc name matching is not evidence.
  - Requirement: implication evidence is scoped. The typechecker must attach
    implication evidence to the local constraint environment introduced by the
    owning generic parameter list and must not leak it outside that
    lexical/typechecking scope.
  - Requirement: implication diagnostics must be stable and specific:
    `unproven_implication` when no evidence exists, `ambiguous_implication`
    when more than one incompatible evidence source matches,
    `implication_violation` when a later operation contradicts proven negative
    evidence, and `implication_scope_error` when evidence is used outside its
    valid scope.
  - Requirement: implications are allowed only in generic parameter lists on
    functions, receiver methods, structs, type aliases where legal, and impls.
    Field-level implication decorators and declaration `where` implication
    clauses are not supported.
  - Requirement: implication targets start with closed structural field shapes,
    including field names, field types, optional visibility rules, and nested
    shapes. The compiler must reject open/dynamic maps, `Dynamic`, unknown
    fields, private fields from outside the defining module, and ambiguous
    generated fields unless the source type is explicitly known closed.
  - Requirement: implication evidence may be inferred from concrete structs,
    records, generated JS/DOM bindings, database row descriptors, OpenAPI
    generated types, and std wrapper types only when those types expose a
    stable typed field/method surface.
  - Requirement: implication evidence must compose with field access. Inside a
    scope where `T => {name: String}` holds, `value.name` is legal and typed as
    `String`; outside that scope, generic field access remains rejected unless
    another rule proves it.
  - Requirement: implication evidence must compose with shape synonyms,
    function-head pattern parameters, template props, route extractors, and std
    APIs without becoming a parallel trait system.
  - Requirement: negative capability implications such as
    `SecretKey => not Log` are allowed only after the capability/trait
    operation being denied has a compiler-known contract. Negative structural
    absence remains separate and is not part of the initial shape implication
    implementation.
  - Requirement: negative structural implication is future work only. Use
    negative traits for denied capabilities in 0.0.7; do not implement
    `not (T => Shape)` until closed-type absence proofs are specified.
  - Requirement: update the canonical EBNF before implementation. The grammar
    must define implication constraints as generic-parameter shorthand in
    type/evidence positions, not as an expression-level binary operator and
    not as a declaration `where` clause. EBNF must reject implication in runtime
    expressions, declaration `where` clauses, parameter type annotations such as
    `value: T => {title: String}`, field declarations, shape bodies, case
    branches, lambdas, and ordinary type aliases unless those forms explicitly
    own a generic-parameter constraint list.
  - Requirement: update parser fixtures, syntax output fixtures, formatter
    fixtures, and tree-sitter grammar from the same EBNF change. There must be
    no duplicate implication grammar in golden/scratch copies.
  - Requirement: update the formal type specification and Lean proof track.
    Proof obligations must include implication well-formedness, evidence
    soundness for field access, non-conversion semantics, constraint scoping,
    private-field rejection, closed-shape requirements, fail-closed unproven
    implication rejection, evidence provenance preservation, and preservation
    of branch/function result typing when implication evidence enters the local
    environment.
  - Requirement: the Lean inventory must explicitly classify implication
    proofs as current before this slice is complete. Stale or missing proof
    artifacts must fail `make lean-proof-track-check`.
  - Requirement: parser, formatter, typechecker, CoreIR, VM diagnostics,
    JS/backend diagnostics, generated summaries, docs, LSP hover/completion,
    tree-sitter, and coverage inventories must all agree on implication
    syntax and supported positions.
  - Requirement: seek std-library adoption while implementing the feature.
    Candidate APIs include generic render/template helpers requiring
    `{title: String}` or `{name: String}`, data/JSON helpers requiring
    structural object fields, table/row helpers requiring `{id: Int}`, and
    collection helpers that can operate on structurally known key fields.
  - Gate: add `make shape-implications-check`.
  - Gate: extend `make syntax-contract-check` or the equivalent grammar gate
    so the EBNF, parser, formatter, tree-sitter, and syntax docs agree on
    implication syntax.
  - Gate: extend `make lean-proof-track-check` with implication proof
    inventory and proof execution.
  - Make integration: run `shape-implications-check` from `make check` before
    `language-feature-coverage-100-check`, `shape-synonyms-check`,
    `typed-template-interpolation-check`, and std package coverage.
  - Acceptance: executable `.terl` tests prove implication-constrained
    functions, receiver methods, structs, impls, imported concrete types,
    generated std types, nested shape evidence, and field access through
    implication evidence on the default VM path.
  - Acceptance: adversarial tests prove missing fields, wrong field types,
    private field access, dynamic/open maps, unsupported target evidence,
    ambiguous generated fields, implication outside generic parameter
    constraints, field-decorator syntax, attempted runtime conversion,
    unproven implication,
    scoped-evidence leakage, and attempted negative structural implication all
    report stable diagnostics.
  - Acceptance: at least one std-library API uses shape implication evidence
    before the slice is marked complete, so the feature is proven against real
    library code and not only synthetic fixtures.

### Typed Template Interpolation

- [x] Add typed interpolation for Terlan templates.
  - Requirement: templates must support embedded Terlan expressions in text
    and attribute positions while preserving source-level type checking:
    ```html
    <h1>{user.name}</h1>
    <a href={profile_url(user.id)}>{user.display_name}</a>
    ```
  - Requirement: interpolation is not string concatenation. The compiler must
    know the slot context, expected output kind, escaping rules, and source
    location for each embedded expression.
  - Requirement: text slots escape by default. Trusted HTML must require an
    explicit `std.template` trusted fragment type, not a raw string.
  - Requirement: attribute slots must reject values that cannot be rendered
    safely for the target attribute. URL, boolean, token/list, numeric, and
    optional attributes must have typed rendering rules.
  - Requirement: interpolation must work with template components, typed
    props, typed children/slots, live actor-bound snippets, static-site output,
    and HTTP response rendering.
  - Requirement: parser, formatter, template contract validation, CoreIR,
    VM rendering, JS/browser rendering, LSP, tree-sitter injections, source
    maps, diagnostics, and std docs must agree on interpolation syntax and
    slot typing.
  - Requirement: template files stay close to their target format. HTML
    templates remain HTML-like with `{expr}` interpolation and Terlan imports;
    do not introduce HEEx-style or framework-specific custom tag grammar for
    0.0.7.
  - Requirement: template target kind must be inferred from file extension or
    explicit build metadata, for example `.terl.html`, `.terl.xml`,
    `.terl.yaml`, and `.terl.json`. Unknown target extensions fail with a
    stable diagnostic instead of defaulting to unsafe string rendering.
  - Requirement: imports inside templates must use explicit Terlan import
    forms and must not weaken target-format syntax highlighting more than
    necessary.
  - Requirement: non-HTML template targets such as XML, YAML, JSON-like
    generated documents, and OpenAPI output must reuse the same typed
    interpolation model with target-specific escaping/rendering rules.
  - Gate: `make typed-template-interpolation-check`.
  - Make integration: run `typed-template-interpolation-check` from
    `make check` after the HTTP runtime stack gate and before release/runtime
    dependency and Angular.ts integration gates.
  - Current coverage: typed text and attribute interpolation validation,
    component prop expression slots, scalar field and receiver-method slots,
    arithmetic expression slots, module-aware inferred/asserted pure helper
    calls with effectful-helper rejection, URL attribute validation for string-like and
    `std.net.Uri.Uri` values, boolean attribute validation for `Bool` values,
    optional attribute validation for wrapped scalar, URI, and boolean values,
    token/list attribute validation for string-like token collections,
    `quick-xml`-backed XML artifact structure validation with typed text and
    quoted-attribute interpolation boundaries,
    multiline source-map diagnostic locations for text and attribute template
    slots,
    trusted fragment std behavior, HTML boundary escaping, static
    text/attribute escaping, artifact template suffix and malformed
    interpolation diagnostics, and JSON/YAML/Text template artifact validation.
  - Closure coverage: executable live actor-bound snippets, full VM HTTP
    response rendering, JS/browser rendering parity, LSP completion, and
    tree-sitter/TextMate injection parity are enforced by
    `typed-template-interpolation-check`.
  - Acceptance: executable `.terl` tests prove text interpolation, attribute
    interpolation, component props, children slots, trusted fragments, optional
    attributes, list/token attributes, static rendering, and VM HTTP rendering.
  - Acceptance: adversarial tests prove wrong slot types, unsafe raw HTML,
    unsafe URLs, missing variables, unsupported target renderers, malformed
    template syntax, and source-map diagnostic locations fail stably.

- [x] Slice 4: harden template interpolation parity for JS/browser rendering,
  VM artifact pipelines, and static-site output.
  - Requirement: JS/browser template runtime interpolation must route through the
    same typed slot contracts as VM/HTTP rendering for text slots, attribute
    slots, list/token attributes, optional attributes, URL slots, trusted
    fragments, and boolean slots.
  - Requirement: the compiler must refuse using untyped string concatenation as a
    substitute for interpolation in HTML-like templates; all `${...}`/`{...}`
    forms must stay typed and typed-safe end-to-end.
  - Requirement: interpolation for static-site/artifact generation must preserve
    slot typing across formatters and output backends so template artifacts fail
    deterministically when slot contracts are violated in generated files.
  - Requirement: template-target inference for `.terl.json`, `.terl.yaml`, and
    `.terl.xml` must remain explicit and stable for browser/VM/codegen modes;
    unknown/mismatched target combinations must use a stable diagnostic family and
    include span location of the interpolation node.
  - Requirement: browser/VM/JS renderers must share the same slot escaping policy
    matrix and produce isomorphic behavior for equivalent inputs.
  - Requirement: add cross-backend adversarial tests proving that unsafe URLs,
    unsafe raw HTML, and unsupported slot types fail with the same canonical
    diagnostics in VM, JS/browser, and static artifact paths.
  - Gate: split `make typed-template-interpolation-check` into
    `typed-template-interpolation-vm-check` and
    `typed-template-interpolation-js-check`, then add
    `typed-template-interpolation-backend-check` that composes:
    - `artifact-template-check`
    - `template-contract-check`
    - `terlc test std/template/TemplateInterpolationBackendTest.terl`
    - JS/browser renderer parity fixtures for HTML/XML/JSON-like artifacts.
  - Gate: `make typed-template-interpolation-vm-check`.
  - Gate: `make typed-template-interpolation-js-check`.
  - Gate: `make typed-template-interpolation-backend-check`.
  - Verified 2026-07-19: external HTML template instantiation in `js.shared`
    and `js.browser` now calls generated renderers instead of returning an
    untyped props object. Renderer descriptors come from the parsed
    `HtmlTemplate` tree and carry URL, boolean, token-list, and scalar attribute
    kinds selected by the same Rust policy matrix used by static/VM paths.
    Oxc and Node execution coverage proves text/attribute escaping, boolean
    presence, token-list rendering, and canonical unsafe-URL/invalid-token
    rejection. The composed `typed-template-interpolation-check` passes, and
    its Rust selectors are grouped by module so the gate no longer rebuilds
    every binary once per individual assertion.
  - Completed 2026-07-19: trusted/optional slots and nested components execute
    from the same external HTML fixture files in VM, `js.shared`, and
    `js.browser`. JSON, XML, YAML, TOML, and text artifacts use one structured
    descriptor for static and browser rendering, with identical output,
    target-specific escaping, stable slot telemetry, and span-bearing target
    mismatch diagnostics. The VM/JS/backend gate split is wired into `make
    check` in the required order.
  - Make integration: `typed-template-interpolation-backend-check` runs after
    `typed-template-interpolation-tooling-check` and before
    `vm-http-concurrency-investigation-check`.
  - Acceptance: the same template fixture set reports identical acceptance/rejection
    outcomes across VM and JS/browser rendering modes.
  - Acceptance: static artifact generation for each supported template target emits
    structured output that includes typed diagnostics and stable slot-type telemetry.

### Angular.ts Terlan Integration

- [x] Make the Angular.ts Terlan integration equivalent to the Dart
  integration, not just a todo smoke harness.
  - Problem: the current `integrations/terlan` package is not a usable
    integration. It proves that a generated Terlan todo module can be imported
    from JavaScript, and it proves that a handful of namespace files exist,
    but it does not expose a real typed Angular.ts facade to Terlan users.
  - Requirement: generated `pub type` aliases from `@types/namespace.d.ts`
    are not enough. The integration must generate usable Terlan types from
    `.d.ts` declarations: structs/shapes for object types, callable function
    types, generic type parameters, union/result classifications, optional
    fields, readonly fields, method surfaces, constructors where supported,
    and documented skipped cases where Terlan cannot yet express the source
    declaration safely.
  - Requirement: Terlan must learn to consume a real `.d.ts` namespace and
    produce a usable `terlan.angular.*` package whose generated code can be
    compiled, imported, and used by handwritten Terlan app code. The gate must
    fail if generation produces aliases to unresolved `T*` TypeScript names
    without a Terlan-side usable wrapper or documented skip.
  - Requirement: match the practical integration level of
    `/home/anatoly/Applications/ng/angular.ts/integrations/dart`: typed module
    creation, token/DI helpers, component registration, controller
    registration, service/factory/value registration, directive registration,
    scope access, template/cache helpers, HTTP/service wrappers where exposed
    by Angular.ts, WebSocket/SSE/worker/wasm registration surfaces where the
    source namespace provides them, and tests that instantiate or exercise
    those wrappers.
  - Requirement: the user-facing goal is to build Angular.ts applications
    entirely in Terlan. JavaScript may exist as generated glue and runtime
    packaging, but ordinary app authors must not need to write `todo.js`,
    manual `angular.module(...)` calls, or handwritten JS controller glue for
    the default workflow.
  - Requirement: Terlan source must own the app boundary: module declaration,
    imports, component/controller registrations, DI tokens, state model,
    event handlers, template references, and build profile. The generated
    Angular.ts JavaScript must be treated as a compiler artifact.
  - Requirement: the example must be rewritten from a JavaScript-owned todo
    harness into a Terlan-owned app. A valid example should look conceptually
    like a Terlan module defining the app, its component/controller state, and
    handlers, with Angular.ts used as the rendering/runtime target.
  - Requirement: app assets and templates must be declared from Terlan project
    metadata or Terlan imports, then packaged by the existing JS/browser
    pipeline. The Angular.ts integration must not require authors to maintain
    a separate hand-written JS bootstrap file.
  - Requirement: generated Terlan wrappers must preserve Angular.ts naming and
    source intent while following Terlan naming rules at the call site. Keep
    the raw JS/Angular.ts boundary explicit; do not hide unsafe dynamic calls
    behind fake typed APIs.
  - Requirement: documentation from the source `.d.ts` must be preserved in
    generated Terlan docs where available, with normal Terlan doc formatting.
  - Requirement: every unsupported `.d.ts` construct must be classified in a
    generated skip manifest with a stable reason. Missing generated output
    without a skip reason is a gate failure.
  - Requirement: the generated package must include at least one handwritten
    Terlan app example using the generated Angular.ts facade directly. The
    example must define a module, register a component/controller boundary,
    bind state into an Angular.ts template, and execute in the Angular.ts
    harness.
  - Requirement: the browser harness must be executable. String-marker checks
    are insufficient; tests must run Angular.ts with the generated Terlan
    facade in a browser or browser-equivalent integration gate.
  - Requirement: namespace generation must use the real external
    `@types/namespace.d.ts` input, not a tiny fixture, for the release gate.
    Fixture tests may remain only as unit tests for small parser cases.
  - Requirement: the materialized `integrations/terlan` package must pass
    package-local `make check` against the real external Angular.ts checkout.
    The generated namespace manifest must use the canonical TypeScript input
    package identity while preserving `@types/namespace.d.ts` as the Angular.ts
    source resolution, and the gate must fail on source-package drift before a
    confusing binder error leaks out.
  - Historical gate: `angular-ts-terlan-facade-parity-check` supplied the
    recorded completion evidence and is paused with the non-AOT graph during
    the focused hard cutover.
  - Completed 2026-07-19: the generated package consumes the real external
    `@types/namespace.d.ts`, emits typed namespace/facade modules plus a stable
    skip manifest, builds a Terlan-owned todo application and typed template,
    and executes the create/toggle/edit/filter/delete workflow through the
    external Angular.ts runtime in Playwright. The generated adapter uses
    explicit DI annotation for strict Angular.ts runtimes. SSE validation now
    follows the current `$sse`/`createSseService` factory contract rather than
    the retired `SseProvider` class shape.
  - Make integration: run `angular-ts-terlan-facade-parity-check` after
    `angular-ts-terlan-integration-check` and before Wasm Angular.ts
    integration validation.
  - Acceptance: the gate fails if `Angular`, `NgModule`, `Component`,
    `Directive`, `Scope`, `HttpService`, `TemplateCacheService`, `Worker`,
    `WebSocket`, `Sse`, `Machine`, or `Workflow` surfaces are generated only
    as unresolved aliases, fake declarations, or existence-only tests.
  - Acceptance: the gate includes positive and adversarial tests for generated
    wrappers: wrong argument type, wrong arity, missing required option,
    optional field omission, unsupported union shape, unresolved source type,
    and unsafe raw boundary use.
  - Acceptance: the generated Terlan facade can be used by handwritten Terlan
    code without manually writing JavaScript glue for the normal module,
    component, controller, and DI workflows.
  - Acceptance: a fresh app generated from the Angular.ts/Terlan profile can
    be built, served, and tested with only `.terl`, `.terl.html`/template, and
    project metadata files as user-authored source.

- [x] Slice 3: harden Angular.ts facade parity and package-wide smoke for browser
  path.
  - Requirement: extend the facade generator checks to include parity surfaces for
    remaining high-value Angular.ts constructs (`Machine`, `Workflow`, `Sse`,
    `WebSocket`, `Worker`, `Directive`, `TemplateCacheService` integration
    helpers) with at least one positive and one adversarial test per family.
  - Requirement: generated wrappers must preserve constructor/method shape, option
    defaults, and callback lifecycles in a way that compiles to runnable Terlan
    and behaves predictably under `npm`-backed harness execution.
  - Requirement: integration package should validate namespace drift before
    materialization and prove deterministic regeneration after drift (hash-based
    manifest comparison, not text-only checks).
  - Requirement: remove reliance on fixture-only `@types` by defaulting `make
    angular-ts-terlan-facade-parity-check` to real namespace input when present
    in environment or sibling repository.
  - Requirement: browser-equivalent integration test must exercise lifecycle
    behavior (component/controller mount, directive-like interaction, event handler
    callback invocation, DI token resolution), not only compile checks.
  - Requirement: skip manifest must be stable and versioned; adding a construct
    without docued reason fails the gate.
  - Historical gate: `angular-ts-terlan-facade-parity-hardening-check`
    supplied the recorded completion evidence and is paused with the non-AOT
    graph during the focused hard cutover.
  - Completed 2026-07-19: the generated facade now provides callable Machine,
    Workflow, SSE, WebSocket, Worker, Directive, and TemplateCache helpers,
    including explicit default/config overloads and worker unsubscribe
    lifecycles. The gate compiles the focused generated package, rejects seven
    wrong-typed handwritten Terlan call families with stable source spans,
    rejects stale namespace hashes before binding, proves hash-identical
    regeneration, and adversarially validates every facade family and the
    versioned skip-reason policy. Environment-selected and sibling Angular.ts
    checkouts use the real namespace by default. Playwright additionally proves
    controller/component mount, strict-DI token resolution, directive linking,
    and browser event callback invocation.
  - Make integration: run this gate after `angular-ts-terlan-app-ownership-check`
    and before `angular-ts-terlan-integration-check`’s external-materialization
    branch or equivalent final parity branch.
  - Acceptance: façade parity failure for a supported flow cannot silently degrade
    to `std.lsp` or runtime fallback; it must fail with stable diagnostics.
  - Acceptance: gate proves callback-lifecycle wrappers can be triggered from
    handwritten Terlan code and that adversarial shapes fail with deterministic
    span diagnostics.

### Native Package Completion

Cross-package numerical arrays have a separate downstream architecture in
[`ROADMAP_NDARRAY.md`](ROADMAP_NDARRAY.md). The agreed boundary is an external
`terlan-ndarray` package with an opaque `ndarray.Array`, owned contiguous CPU
storage, a generated stable C ABI, DLPack tensor exchange, Arrow C Data
interchange, and CBLAS/LAPACKE kernels. It is not part of core `std` or the
compiler. `terlan-polars` owns `Series.to_array`, `DataFrame.to_array`, and the
Polars-facing `DataFrame.to_torch` convenience; `terlan-pytorch` owns
`Tensor.from_array` and remains independent of Polars. These downstream gates
do not block 0.0.7 until explicitly promoted into this active roadmap.

- [x] Finish `terlan-polars` as a feature-complete external package.
  - Requirement: complete the external
    `/home/anatoly/Applications/terlan/terlan-polars` package instead of
    moving Polars into core `std` or the compiler crate.
  - Requirement: the public Terlan namespace remains `polars`, not
    `std.native.polars`, and imports must work from an ordinary package
    dependency.
  - Requirement: the package must link the real maintained Rust `polars`
    crate inside the package adapter, not through a compiler dependency.
  - Requirement: the release surface must cover DataFrame construction,
    `read_csv`, dimensions, column names, schema inspection, selecting columns,
    filtering, sorting, grouping/aggregation where supported by the first
    package version, lazy query execution where supported, and stable error
    conversion.
  - Requirement: every public `polars` Terlan API must have executable Terlan
    tests and Rust adapter tests. Declaration-only package tests do not count.
  - Requirement: adversarial tests must cover missing files, malformed CSV,
    missing columns, type mismatches, empty data, unsupported lazy operations,
    stale handles, and stable diagnostic/error codes.
  - Requirement: the core Terlan repo may carry package-boundary gates, but
    persistent package implementation and tests belong in `terlan-polars`.
  - Completed slice: `make terlan-polars-package-check` is now a permanent
    Rust quality gate that uses `TERLAN_POLARS_DIR` or the sibling
    `/home/anatoly/Applications/terlan/terlan-polars` package, validates the
    package manifests through typed TOML parsing, verifies the public
    `polars.DataFrame` surface, checks generated NativeBoundary operation
    metadata, proves the old core-native namespace does not leak into package
    text, and runs the package-owned Rust adapter tests.
  - Completed slice: the public package now exposes typed schema inspection as
    `DataFrame.schema(): List[ColumnSchema]`; the adapter reads real Polars
    dtype metadata, the correlated helper protocol transports ordered schema
    entries, and the VM reconstructs field-addressable Terlan records.
  - Completed slice: scalar equality filtering is executable through
    `DataFrame.filter_eq(column, value)` with a compact
    `String | Int | Float | Bool` scalar union. Native argument encoding keeps
    the concrete scalar type, real Polars materializes a new filtered frame,
    and package/Rust tests cover successful integer filtering, missing columns,
    and incompatible scalar types.
  - Completed slice: single-column ascending/descending sorting is executable
    through `DataFrame.sort_by(column, descending)`. Its acceptance test proves
    row order by composing descending sort, `head`, and typed filtering, and
    also covers missing-column failure.
  - Completed slice: grouping and aggregation are now represented by
    `DataFrame.group_count(keys)`, which performs stable grouping over one or
    more columns and materializes a `count` column. Rust tests assert exact
    counts, while executable Terlan tests cover successful grouping, empty-key
    rejection, and missing-column failure.
  - Completed slice: real lazy execution now uses helper-owned opaque
    `LazyFrame` resources. `lazy`, `where_eq`, and `project` build immutable
    plans; `collect` materializes a DataFrame; `release` deterministically
    invalidates a plan handle. Executable tests prove deferred missing-column
    errors and stale-plan rejection. Distinct lazy verbs avoid the compiler's
    current same-module native name/arity metadata collision.
  - Completed slice: direct construction is available as
    `DataFrame.from_rows(columns, rows)` for string-valued data, including
    zero-row frames. A length-prefixed nested-list helper encoding preserves
    empty rows, and stable `invalid_row_width` validation rejects ragged input.
  - Completed slice: bounded row inspection is available as
    `DataFrame.rows(limit)`. The native protocol materializes nested string
    lists, renders nulls deterministically as `"null"`, and rejects negative
    or greater-than-10,000 limits with stable errors.
  - Completed slice: `examples/iris_dataset_audit` is an executable,
    Polars-only ML data preparation experiment over the attributed UCI Iris
    dataset. It proves the 150-by-5 shape, four `Float64` feature columns,
    three balanced 50-row species classes, eager feature projection, lazy
    Setosa partitioning, bounded row inspection, and complete handle cleanup.
    The package gate runs it against the real helper; numerical-array and tensor
    conversion remain explicitly deferred until the `terlan-ndarray` owned-copy
    path and the PyTorch DLPack consumer path are ready.
  - Completed slice: the public package now has an immutable opaque `Expr`
    value model and reusable expression contexts for projection/expansion,
    computed columns, predicates, stable grouped aggregation, date operations,
    string splitting, aliases, and name transforms. Expressions cross the
    NativeBoundary as private values and require no handle lifecycle.
  - Completed slice: `examples/polars_getting_started` ports every DataFrame
    example from the official Polars getting-started guide with its exact
    people/family fixtures. The executable proves CSV write/read with date
    inference, both selection forms, `with_columns`, both filters, both group
    forms, the chained query, a left join, and vertical concatenation.
    Generated metadata and the permanent boundary gate now cover all 49 public
    package operations.
  - Completed slice: adversarial executable coverage now requires missing and
    malformed CSV errors, empty constructed data, duplicate columns, ragged
    rows, missing filter/sort/group columns, incompatible scalar filters,
    deferred lazy schema errors, and stale DataFrame/LazyFrame handles.
  - Completed slice: `make terlan-polars-package-check` now executes all twelve
    package-owned Terlan tests through `terlc test` with the real native helper,
    in addition to both consumer projects and both Rust adapter feature
    profiles. Package tests are no longer credited for typechecking alone.
  - Completed slice: immutable Git dependencies now resolve through explicit
    `terlc package fetch`, which writes deterministic `terlan.lock` Git entries
    and populates a revision-addressed cache. Normal build/run/test traversal
    is network-free and verifies origin URL, exact revision, clean checkout,
    package identity, and Git tree checksum. Focused fixtures prove offline
    reuse, transitive path-to-Git resolution, missing revisions, absent locks,
    and poisoned-cache rejection without Polars-specific compiler behavior.
  - Completed slice: `terlc package fetch --artifact <archive.tar.zst>` now
    admits target-specific prebuilt package runtimes into the same lockfile and
    content-addressed cache. Build, run, and VM test resolution reverify the
    archive and complete payload inventory, select only the active target, use
    the artifact's Terlan source tree, and propagate its runtime bindings
    without process-global environment mutation or a Cargo source fallback.
    The OpenCV gate proves deterministic archive replay and an environment-free
    full-cycle threshold test from the locked cache.
  - Completed slice: the OpenCV package now pins the compiler revision used for
    releases and has a stable-tag workflow that rebuilds the package from exact
    compiler and OpenCV commits, replays the target archive, binds its manifest
    to the release commit, records promotion evidence, attests its checksum,
    and refuses to replace an existing staged release. External publication is
    outside this roadmap and is not a completion requirement.
  - Gate: `make terlan-polars-package-check`.
  - Completed 2026-07-20: `make check` runs
    `terlan-polars-package-check` after the generated C ABI binding gate and
    before release packaging. The named gate delegates its Rust invocations to
    a permanent focused target so completed-slice ownership remains auditable.
  - Completed 2026-07-20: source resolution prefers `TERLAN_POLARS_DIR`, then
    the sibling checkout, then a full-revision verified cache. When the cache
    is absent, `TERLAN_POLARS_SOURCE` materializes the exact revision through
    atomic staging; offline replay, abbreviated revisions, and revision-mismatched
    cache poisoning have executable coverage. Publishing or committing the
    package from this workspace and live network access are not completion
    requirements. Local path consumers import and execute `polars.DataFrame`
    through package metadata with the current compiler.
  - Acceptance: the gate fails if generated source or docs mention
    `std.native.polars` as the user-facing namespace.

- [x] Add a PyTorch package through generated stable C ABI bindings.
  - Requirement: create an external package boundary for PyTorch/LibTorch;
    do not link PyTorch into the core compiler or standard library.
  - Requirement: the package should live in an external repository/workspace
    named `terlan-pytorch` unless a better package name is chosen before
    implementation starts. The golden compiler repo may validate it, but must
    not own the package implementation.
  - Requirement: target LibTorch's released stable C/AOTI ABI and versioned
    `StableIValue` dispatcher through Terlan's generic C ABI generator. The
    public LibTorch C++ frontend is not this package's ABI boundary, and PyTorch
    is not required to prove the separate generic C++/`cxx` generator.
  - Requirement: `terlan-pytorch` must not depend on Polars. General tensor
    construction stays package-owned; later `Tensor.from_array` consumes the
    reviewed `terlan-ndarray` DLPack handoff, while DataFrame conversion remains
    a Polars-owned integration surface.
  - Requirement: the first public Terlan namespace should be package-owned,
    currently `pytorch`, with Tensor construction, shape/dtype inspection,
    simple arithmetic, matrix multiplication where available, device
    selection, and stable error conversion.
  - Requirement: the package must support CPU-only validation by default so
    CI and developer machines without CUDA can run the core test suite.
  - Requirement: optional CUDA-enabled validation may exist, but must be
    explicit and skipped with a stable diagnostic when CUDA is unavailable.
  - Requirement: every public `pytorch` Terlan API in the first package surface
    must have executable Terlan tests, generated Rust/C ABI adapter tests, and adversarial
    tests for shape mismatch, dtype mismatch, unsupported device, missing
    LibTorch install, stale handles, and stable diagnostics.
  - Gate: add `make terlan-pytorch-package-check`.
  - Make integration: run `terlan-pytorch-package-check` from `make check`
    only for the CPU/default surface. CUDA-specific checks must be separate
    and opt-in.
  - Remaining package work before LibTorch CUDA execution: preserve a pinned
    external-source fixture and prove a no-sibling, network-free checkout in a
    clean local workspace. Invalid-device placement becomes mandatory when that
    public request operation is introduced; the current observation-only device
    API cannot issue an invalid request without a rejection-only surface.
  - Acceptance: the gate builds the generated C ABI adapter, runs CPU tensor smoke tests,
    runs Terlan package tests, and proves a consumer Terlan project can import
    the package without changing compiler source.
  - Completed 2026-07-30: the pinned LibTorch `2.13.0+cpu` workflow generates
    58 public functions and executes tensor construction, reusable TorchScript
    loading, repeated forward inference, owned result extraction, exact
    disposal, stale-handle rejection, and stable missing/corrupt/forward error
    translation through package and immutable Git consumers.
  - Completed 2026-07-30: generated C ABI helpers now emit mutable resource
    accessors only for declared mutable methods. This keeps immutable-only
    packages warning-clean and is covered by the generic generator suite.

- [x] Slice 2: finalize torch package execution and baseline interop smoke.
  - Requirement: add a minimal in-repo generated fixture workspace that models a
    real `terlan-pytorch` consumer layout and can be run without external
    network access.
  - Requirement: complete multiple tensor-handle dispatcher inputs and execute
    an exact CPU `matmul` result using deterministically shaped tensors, so the
    package validates compiler lowering, multi-resource ownership, and package
    execution wiring before introducing CUDA failures.
  - Requirement: add deterministic negative fixtures for dtype mismatch, invalid
    device request on CPU-only runs, missing native runtime dependency, and stale
    tensor handle access.
  - Requirement: split runtime behavior into explicit policy:
    CPU/default path must pass as baseline, CUDA/native-accelerated checks are
    opt-in and must be tagged as optional.
  - Requirement: prove `terlan-pytorch-package-check` fails with stable
    diagnostics when placeholder native symbols are still being used instead of
    real generated adapters.
  - Requirement: add a minimal stable skip contract for environments that cannot
    execute any native PyTorch surface, ensuring the gate reports skip reasons
    and still validates dependency wiring and manifest integrity.
  - Gate: keep `make terlan-pytorch-package-check` as the executable gate and
    extend it with the fixture set above plus policy coverage.
  - Gate extension acceptance: the gate executes at least one positive CPU-only
    training/inference-like smoke (e.g., matmul or linear-forward style fixture)
    and one negative diagnostic path.
  - Make integration: `terlan-pytorch-package-check` remains after
    `make package-test-exec-check` and before ML experiments gates.
  - Acceptance: package namespace import (`import pytorch.*`) remains available to
    a fresh Terlan project after this package gate, with no compiler source
    changes.
  - Acceptance: skip path must be stable, explicit, and machine-readable for
    CI automation.

- [x] Add executable Terlan machine-learning experiments that combine Polars,
  ndarray, and PyTorch.
  - Requirement: experiments must be written in Terlan source, not only Rust,
    Python, shell, or notebook glue.
  - Requirement: experiments must live outside the golden compiler crate. The
    compiler repo may carry gates and minimal fixtures, but persistent
    experiment source belongs in a package/example workspace that consumes
    `terlan-polars`, `terlan-ndarray`, and the PyTorch package through normal
    package metadata.
  - Requirement: the experiment workspace should be named
    `terlan-ml-experiments` unless a better external package name is chosen
    before implementation starts.
  - Requirement: first experiments must cover:
    - CSV ingestion with Polars, schema inspection, null/error handling, and
      feature/label column selection.
    - Polars-owned `DataFrame.to_array` conversion with explicit column, null,
      dtype, ownership, and `[rows, columns]` shape checks.
    - `Tensor.from_array` through the versioned DLPack handoff, plus the
      Polars-owned `DataFrame.to_torch` convenience built over the same path.
    - A CPU-only linear/logistic regression training smoke with deterministic
      seed, loss calculation, and prediction output.
    - A small inference-only model path that loads fixed weights and scores a
      Polars DataFrame batch.
    - A pipeline example showing Terlan code as the orchestration layer:
      `read_csv |> select_features |> to_array |> Tensor.from_array |> train |>
      evaluate`.
  - Requirement: experiments must be deterministic by default and must not
    require CUDA, network access, or large datasets. CUDA variants may be
    added as opt-in experiments with stable skip diagnostics.
  - Requirement: checked fixtures must include tiny CSV datasets and expected
    output manifests committed with the experiment workspace. Do not download
    datasets during default gates.
  - Requirement: every experiment must have executable Terlan tests plus
    adapter-level Rust/C/C++ tests for the package boundary it exercises.
  - Requirement: adversarial tests must cover malformed CSV, missing columns,
    empty datasets, dtype mismatch, array/tensor shape mismatch, unsupported
    layout or device, consumed DLPack handoffs, missing package/native library,
    stale handles, and stable diagnostics.
  - Requirement: documentation must show the exact Terlan commands to run each
    experiment and must explain which work is owned by Polars, ndarray,
    PyTorch, NativeBoundary, and Terlan VM.
  - Gate: add `make ml-experiments-check`.
  - Make integration: run `ml-experiments-check` after
    `terlan-polars-package-check` and `terlan-pytorch-package-check`; default
    release validation may use CPU-only fixtures while CUDA remains opt-in.
  - Acceptance: the gate builds the dependent packages, runs the Terlan ML
    experiment tests, verifies deterministic outputs against checked fixtures,
    and proves the examples can be copied into a fresh Terlan project without
    modifying compiler source.

- [x] Slice 5: close generated C++ package execution and manifest stability.
  - Requirement: make `make cpp-binding-generator-check` execute a real generated
    package through the same package-test execution path used by
    `package-test-exec-check`; no generated fixture may be validated only by
    Rust unit tests.
  - Requirement: generator execution must include:
    - temporary generated package source checkout in a fresh directory,
    - `terlc test` execution through package mode for Terlan-facing public APIs,
    - adapter-level Rust tests for generated `cxx` crate paths,
    - explicit snapshot of `bindings/skipped-symbols.json`.
  - Requirement: unsupported C++ shapes must fail with machine-readable skips and
    never produce a partial binding.
  - Requirement: generated package execution must be deterministic across runs:
    `skipped-symbols.json`, public declarations, manifest metadata, and docs
    output must be stable modulo sorted ordering.
  - Requirement: adversarial generator fixtures must include templates without
    concrete instantiation, overload ambiguities, ambiguous ownership/lifetime
    models, exceptions, macro-shaped calls, variadic signatures, and raw pointer
    boundary cases; all must produce stable diagnostics and deterministic skip
    entries.
  - Requirement: add a minimal local fixture that mirrors PyTorch generator
    behavior and proves generated package imports remain external (`torch`-like
    namespace) without compiler-local hacks.
  - Gate: extend `make cpp-binding-generator-check` to produce a machine-readable
    execution report (`.gen_report.json`) containing package execution result,
    skipped-symbol hashes, policy reasons, and fixture skip/fail classification.
  - Gate: add `make generated-package-contract-check` that validates manifest and
    report stability; this gate runs after `cpp-binding-generator-check` in
    `make check`.
  - Acceptance: `cpp-binding-generator-check` passes without placeholders and
    without CUDA in default mode.
  - Acceptance: stable skip reasons include all unsupported fixtures and reject
    free-form, unstable skip strings.
  - Acceptance: the generated package can be imported by a package consumer through
    ordinary package metadata after the gate, with no explicit compiler paths or
    `std.native` namespace leakage.

- [x] Slice 4: make ML experiment results reproducible and portable through package CI.
  - Requirement: split ML experiments into three executable groups:
    `baseline` (CPU-only, deterministic),
    `adversarial` (malformed/missing-input failures),
    and `interop` (cross-package boundaries: polars ↔ torch ↔ VM/NativeBoundary).
  - Requirement: each group must have:
    - a runnable Terlan fixture,
    - checked tiny input artifacts under version control,
    - deterministic expected-output manifests,
    - explicit machine-readable diagnostics for skip/failure modes.
  - Requirement: the baseline group must include one fixed feature pipeline and one
    fixed inference pipeline and must assert exact output shape/value checks, not
    only non-error assertions.
  - Requirement: adversarial group must include at least:
    malformed CSV, missing columns, empty dataset, tensor shape mismatch,
    unsupported dtype request, unsupported device request, stale handle/path,
    and missing package/native dependency.
  - Requirement: interop group must prove package imports for both packages are resolved
    through normal dependency manifests and do not rely on compiler-local fixtures.
  - Requirement: experiment artifacts must be built through the new package test execution
    command path used by `package-test-exec-check`; no ad hoc manual commands.
  - Gate: `make ml-experiments-check` is extended to run all three groups with stable
    exit codes for pass/skip/fail.
  - Gate: add `make ml-experiments-check-adversarial` as a focused adversarial
    regression gate for future bisect and CI performance.
  - Make integration: `ml-experiments-check` runs after `terlan-pytorch-package-check`
    and before optional `cuda-package-check`; default release mode runs baseline + interop,
    with adversarial gates explicit and opt-in only when required.
  - Acceptance: the baseline group passes without CUDA unless CUDA is explicitly enabled.
  - Acceptance: adversarial failure fixtures must produce stable, documented diagnostics.
  - Acceptance: experiment group manifests include explicit package graph hashes and
    source-root provenance for reproducibility.

- [x] Implement automatic C++ package binding generation.
  - Requirement: add generator machinery for C++ libraries that produces a
    predictable Terlan package skeleton, Rust `cxx` bridge, native build
    metadata, generated docs, skipped-symbol manifest, and executable package
    tests.
  - Requirement: the generator must consume structured C++ metadata from
    maintained tooling. Do not parse C++ headers with regex or handwritten
    partial parsers.
  - Requirement: the generator output must include explicit skip reasons for
    unsupported C++ shapes such as templates without concrete instantiation,
    overloaded operators, raw pointer ownership ambiguity, reference lifetime
    ambiguity, macros, variadic functions, exceptions, inheritance patterns
    that cannot be represented safely, and unsupported callback shapes.
  - Requirement: generated packages must classify ownership and lifetime
    boundaries explicitly: value, borrowed view, owned handle, mutable handle,
    nullable handle, and thread-safety guarantees.
  - Requirement: OpenCV should be the first real package proving this C++
    generator. PyTorch targets LibTorch's stable C ABI in `terlan-pytorch` and
    must not force C++ frontend concerns into the compiler fixture.
  - Completed slice: `make cpp-binding-generator-check` now runs the
    manifest-backed native binding generator tests plus the native binding
    contract gate. The current fast fixture generates Terlan source,
    NativeBoundary metadata, docs, a Rust adapter crate, a manifest snapshot,
    and an executable generated-adapter Cargo test; it also rejects arbitrary
    C++ template targets with a stable diagnostic.
  - Completed slice: generated packages now include
    `bindings/skipped-symbols.json` rendered through `serde_json`; the generator
    validates stable `native_bindgen.*` skip reasons and has tests proving
    unsupported templates and overloaded operators are recorded while unstable
    free-form reasons are rejected.
  - Gate: `make cpp-binding-generator-check`.
  - Make integration: `cpp-binding-generator-check` runs from `make check`
    before package validation gates.
  - Completed slice: the external `terlan-opencv` package proves maintained
    Clang LibTooling metadata, 443 extracted adapter declarations, 397
    generated functions, package-local and external-consumer AOT execution,
    scoped native helpers, and deterministic generated artifacts without
    compiler-local OpenCV bindings.
  - Completed 2026-07-30: the external package freezes a checked matrix of 21
    admitted generic C++ shapes, each tied to real OpenCV operations and
    permanent native/Terlan evidence. The same gate freezes the ten
    compiler-owned rejection families for pointers, borrowed lifetimes,
    templates, exception crossing, overload ambiguity, callbacks, variadics,
    inheritance, unknown ownership, and unmapped types.
  - Acceptance: adversarial fixtures for unsafe ownership, unsupported
    templates, exceptions, macros, ambiguous overloads, and raw pointers fail
    with stable diagnostics instead of producing partial bindings.

- [x] Add CUDA package support as an explicit optional native capability.
  - Requirement: CUDA must be package/native-boundary support, not a core
    compiler dependency and not a default release requirement.
  - Requirement: support two independent external package paths. A direct
    `terlan-cuda` package may use maintained Rust CUDA bindings such as `cudarc`
    or `cust` through the Rust-package/NativeBoundary path. CUDA through
    LibTorch extends the generated stable C ABI/dispatcher path in
    `terlan-pytorch`. Direct C or C++ CUDA libraries may additionally use the
    generic C/C++ generators where appropriate. None of these paths may add a
    CUDA dependency to the core compiler or create an ad hoc GPU ABI.
  - Requirement: the first CUDA surface must expose device discovery, device
    selection, tensor/device placement metadata, and a small kernel or
    library-backed operation through typed Terlan package APIs.
  - Requirement: CPU-only machines must pass default `make check`; CUDA gates
    must distinguish driver/device, toolkit/compiler, and CUDA-enabled LibTorch
    availability. A prebuilt LibTorch-only path must not require `nvcc` unless
    package-owned CUDA source is compiled. Every unavailable state reports a
    stable skip diagnostic without failing default release validation.
  - Requirement: CUDA-enabled validation must prove native build flags,
    dynamic-library discovery, runtime device detection, error conversion, and
    stale handle cleanup.
  - Requirement: no unsafe GPU memory handle may be exposed directly to Terlan
    user code. Terlan must see typed package values and resource handles only.
  - Completed slice: `make cuda-package-availability-check` is now a
    permanent Rust quality gate with unit coverage for available/unavailable
    probes and core-manifest CUDA dependency rejection. On CPU-only machines it
    reports a stable unavailable status without failing default validation.
  - Gate: `make cuda-package-availability-check`.
  - Gate: `make cuda-package-check`.
  - Make integration: `cuda-package-availability-check` runs from default
    `make check`. Do not run `cuda-package-check` from default `make check`
    unless CUDA is explicitly enabled in the environment.
  - Completed: `make cuda-package-check` stages the external `terlan-cuda`
    package into a fresh workspace, resolves its exactly pinned `cudarc`
    dependency, runs its Rust contract tests, and either executes its typed
    Terlan CUDA smoke or records the stable `cuda-device-unavailable` skip.
  - Acceptance: CPU-only CI reports CUDA unavailable cleanly, while a CUDA
    machine can run `make cuda-package-check` and execute the package smoke
    through Terlan.
  - Ordering: direct Rust CUDA package work may proceed after the shared
    capability model and does not depend on PyTorch. CUDA execution through
    LibTorch opens after `terlan-pytorch` closes multiple tensor inputs, exact
    CPU `matmul`, and its compiler-level package gate; it should then precede
    broad PyTorch operator expansion.
  - Completed: the external `terlan-cuda` package uses dynamically loaded
    `cudarc` driver bindings and checked-in portable PTX, so its real Float64
    vector-add smoke does not require `nvcc`. It exposes typed `Device` and
    `Buffer` resources, placement metadata, explicit disposal, generation-tagged
    stale-handle rejection, and no device pointers. The separate
    `terlan-pytorch-cuda-check` profile uses LibTorch's stable C ABI for
    synchronous CPU → CUDA → CPU placement and the existing `matmul` API; on
    this validation host it reports the typed `libtorch-cuda-unavailable` skip
    independently from the passing direct CUDA backend.
  - Acceptance: adversarial tests prove missing drivers, missing toolkit,
    unsupported device capability, invalid device selection, stale GPU
    handles, and unsafe pointer exposure fail with stable diagnostics.

- [x] Slice 3: close the direct Rust CUDA package execution loop end-to-end.
  - Requirement: add a real external `terlan-cuda` workspace using a maintained
    Rust CUDA binding behind NativeBoundary. `make cuda-package-check` must exercise:
    manifest resolution, dependency wiring, package test execution, and native
    adapter execution through package mode.
  - Requirement: implement a deterministic package-owned operation such as
    vector addition using typed buffers and opaque device resources. Prove
    device discovery, output correctness, synchronization, and deterministic
    cleanup without exposing device pointers to Terlan.
  - Requirement: separately add `terlan-pytorch-cuda-check` for a CUDA-enabled
    LibTorch distribution, CPU → CUDA → CPU tensor placement, and reuse of the
    deterministic CPU `matmul` fixture on CUDA through the same Tensor API.
  - Requirement: opaque stream ownership and non-blocking copies remain later
    slices for both packages, after each synchronous path is green.
  - Requirement: keep the gate CPU-safe by default: when no CUDA environment is
    detected, `make cuda-package-check` must return a typed skip reason and exit
    without non-experimental failures.
  - Requirement: when CUDA is present, `make cuda-package-check` must execute a
    real external package smoke path end-to-end through `terlc` and report stable
    results in a machine-readable report artifact.
  - Requirement: if placeholders, stale runtime symbols, or missing bindings are
    encountered, the gate must fail with explicit diagnostics and must not hide
    fallback behavior.
  - Requirement: add deterministic negative fixtures for missing toolkit/driver,
    unsupported architecture, invalid device selection, stale CUDA handle access,
    and unsupported dtype requests.
  - Gate: extend `make cuda-package-check` with fixture execution, skip/fail
    diagnostics, and smoke result assertions.
  - Make integration: run direct `make cuda-package-check` from
    `make check-experimental` after `make package-test-exec-check`; it does not
    wait for PyTorch. Run `terlan-pytorch-cuda-check` only after
    `terlan-pytorch-package-check` reaches green. Keep both opt-in for default
    release validation.
  - Acceptance: CPU-only CI remains green with stable `cuda-unavailable` status;
    CUDA-capable CI can run the slice and report deterministic success.
  - Acceptance: the gate proves real external fixture execution and stable package
    imports from a fresh sibling package workspace with no manual bootstrap.
  - Completed: the gate stages `terlan-cuda` without its source build artifacts,
    resolves and tests `cudarc = 0.19.8`, loads the CUDA driver dynamically,
    executes package-mode Float64 vector addition on device 0, copies the exact
    result back through typed values, and writes
    `target/quality/cuda-package-execution-status.json`.
  - Completed: direct-package adversarial checks cover missing capability reason
    codes, invalid selection, unsupported dtype encoding, unsupported PTX
    architecture, stale generation reuse, and pointer exposure. The checked-in
    PTX path deliberately records toolkit availability without requiring `nvcc`.
  - Completed: `terlan-pytorch-cuda-check` is a separate opt-in LibTorch stable-C
    profile. It owns synchronous CPU → CUDA → CPU placement, reuses the existing
    Tensor `matmul`, and reports `libtorch-cuda-unavailable` on this host because
    only the pinned CPU LibTorch distribution is installed.

- [x] Slice 4: unify all external package gates under one machine-readable execution matrix.
  - Requirement: add `docs/roadmap/ROADMAP_0_0_7_EXTERNAL_PACKAGE_EXECUTION_MATRIX.json`
    (or equivalent machine-readable artifact) produced by the gates and reviewed by
    CI.
  - Requirement: `make check`/`make check-experimental` must include an
    executable matrix validation step that verifies for each package gate:
    `terlan-polars-package-check`, `terlan-pytorch-package-check`,
    `ml-experiments-check`, `cpp-binding-generator-check`,
    `generated-package-contract-check`, `cuda-package-availability-check`,
    and `cuda-package-check` whether it was:
    - passed,
    - skipped with typed reason, or
    - failed with stable diagnostic classification.
  - Requirement: add a stable artifact for each gate (`.matrix.json` + `.status.json`)
    containing pass/skip/fail state, fixture identifiers, reason codes, and
    dependency provenance (`TERLAN_POLARS_DIR`, `TERLAN_PYTORCH_DIR`,
    generated fixture source, CUDA detection path).
  - Requirement: acceptance matrix checks must assert deterministic JSON ordering,
    identical stable reason codes, and disallow free-form messages in machine
    readable outputs.
  - Requirement: include a single golden source of truth for baseline/experimental
    package gate expectations, with explicit policy that:
    - default package gates may be required,
    - CUDA gates remain opt-in unless explicitly enabled,
    - adversarial ML gates (`ml-experiments-check-adversarial`) are explicit.
  - Requirement: all matrix rows must include package namespace import proof
    (`terl` project import line + manifest path) and whether VM/native mode is
    exercised.
  - Gate: add `make external-package-execution-matrix-check`.
  - Make integration: run matrix generation during `make package-test-exec-check`
    and validate it in both `make check` and `make check-experimental`.
  - Acceptance: CPU-only CI passes default matrix checks with stable skip reasons.
  - Acceptance: CUDA-capable CI can run matrix validation with non-skipped CUDA
    rows and still preserve deterministic artifact schema.
  - Completed: the lexically ordered golden policy records seven package gates,
    their fixture/import/manifest provenance, VM/native execution modes, and
    baseline versus experimental expectations. Its reason-code allowlist
    rejects free-form output and keeps CUDA plus adversarial ML explicitly
    opt-in.
  - Completed: every producer writes canonical per-gate status evidence; matrix
    generation writes canonical per-gate and aggregate matrix artifacts.
    `package-test-exec-check` generates the inventory, while default and
    experimental checks validate their respective profiles.
  - Completed: the CPU baseline passed C++/OpenCV, generated-package, Polars,
    PyTorch, ML, and CUDA-capability evidence. The experimental profile also
    executed direct CUDA vector addition; the independent LibTorch CUDA lane
    emitted the typed `libtorch-cuda-unavailable` skip on the pinned CPU
    distribution.
  - Completed: the Polars matrix fixture is a strict package-mode CSV/head/
    height/disposal execution. The twelve-consumer migration inventory remains
    available as `make terlan-polars-package-full-check` and is not misreported
    as matrix coverage.

- [x] Slice 5: finish and close the C++ binding generator completion gap.
  - Requirement: complete the acceptance items marked in the base generator item by
    proving generator outputs are validated through package-mode execution and not just
    Rust-native fixture tests.
  - Requirement: `make cpp-binding-generator-check` must:
    - generate a real package workspace from each fixture,
    - compile and run package imports via `terlc test` (package mode),
    - assert that unsupported shapes are fully listed in
      `bindings/skipped-symbols.json` with approved machine-readable families.
  - Requirement: replace remaining “acceptance remaining” language by executable
    checks that verify:
    - structured C++ metadata consumption is successful from maintained tooling,
    - a real `cxx` bridge is emitted and compiled,
    - at least one Terlan consumer test is executed for each fixture,
    - every unsupported fixture symbol maps to a stable skip family.
  - Requirement: add a hard-coded allowed-skip-family whitelist and a validation
    test that rejects unknown families and free-form skip messages.
  - Requirement: extend adversarial fixture coverage to include the C++ ownership
    and lifetime boundary set currently only described in prose.
  - Requirement: add deterministic fixture that mirrors a `torch`-style generated
    namespace import path and demonstrates no `std.native.*` leakage in package mode.
  - Requirement: generator outputs (docs, manifests, declarations, `skipped-symbols.json`)
    must be normalized/sorted and hash-stable across runs.
  - Gate: keep `make cpp-binding-generator-check` and move it from “partly complete” to
    executable-complete with these requirements.
  - Gate: run `generated-package-contract-check` as a required follow-up; it fails if
    any generated fixture has unstable symbol skips, unstable output snapshots, or untested
    package import paths.
  - Make integration: keep `make cpp-binding-generator-check` and
    `make generated-package-contract-check` in default `make check`, before `terlan-pytorch-package-check`.
  - Acceptance: CPU-only CI can execute this slice without CUDA/network dependency.
  - Acceptance: generated outputs include stable, sortable IDs for skipped symbols and
    stable diagnostics for unsupported templates, overloads, ownership, and raw pointers.
  - Completed: `cpp-binding-generator-check` generates and compiles the neutral
    fixture, a deterministic `torch.*` namespace variant, and the maintained
    OpenCV 4.13.0 package. Both small fixtures run their generated Rust adapter
    tests and Terlan package tests; the neutral fixture additionally executes
    from an immutable Git package cache after its source repository is removed.
  - Completed: the package consumer proves normal ownership, disposal,
    rebuild-from-cache, and stale-alias rejection. Generator adversarial tests
    require independent producers/disposers, reject unsafe secondary-resource
    aliases, and cover raw pointers, borrowed references, unknown ownership,
    templates, overloads, callbacks, inheritance, exceptions, variadics,
    annotations, and unmapped types.
  - Completed: skipped-symbol output is lexically stable and contains only
    generator-owned IDs, source locations, whitelisted `cpp.*` families, and
    fixed messages. Package-authored free-form detail cannot enter the artifact;
    unknown families and free-form evidence are rejected by Rust and report
    self-tests.
  - Completed: `generated-package-contract-check` validates canonical report
    JSON, exact fixture classifications, package execution, the external
    `torch.NativeBoundary` import, both skipped-symbol hashes, and normalized
    artifact hashes for manifests, docs, declarations, and tests.

### Terlan VM BEAM Test Feature Parity

- [x] Audit and port every relevant Erlang/OTP test suite still present under the external
  `/home/anatoly/Applications/terlan/terlan-vm` checkout.
  - Requirement: inventory all `.erl`, `.hrl`, `.app.src`, Common Test,
    EUnit, shell-script, and make-driven test files under `terlan-vm`, with
    path, owning subsystem, test runner, OTP dependency level, and whether the
    test is still relevant to Terlan VM.
  - Requirement: test feature parity with BEAM is the goal. Any BEAM/OTP test
    that covers behavior Terlan VM can or should own must be ported into a
    Rust VM test, executable Terlan test, or VM-owned integration gate.
  - Requirement: the purpose of this suite migration is reliability
    equivalence, not OTP compatibility as a product promise. Treat the
    Erlang/OTP corpus as accumulated real-world failure feedback. Port the
    observable scheduler, mailbox, timer, process lifecycle, registry,
    link/monitor, TCP/socket, filesystem, serialization, and stdlib edge cases
    that teach Terlan VM how reliable systems fail under pressure.
  - Requirement: each ported area must record behavior-level equivalence
    evidence, not just file-level classification. The status inventory must
    say which real-world failure semantics are covered by the replacement
    Rust/Terlan gate and which remain open.
  - Requirement: any BEAM/OTP test that cannot be ported because it covers
    stock OTP implementation details, BEAM bytecode compatibility, Erlang
    compiler internals, ERTS internals, EUnit/Common Test mechanics, OTP app
    loading, unsupported libraries, or other non-product compatibility details
    must be removed from the `terlan-vm` checkout rather than kept as active
    test-suite material. OTP-compatible EPMD discovery is product behavior and
    must be ported into the golden VM rather than classified for removal.
  - Requirement: classify each discovered suite as one of:
    `port-to-rust-vm-test`, `port-to-terlan-test`,
    `delete-after-vm-equivalent`, or `remove-non-portable`.
  - Requirement: no Erlang test may remain unclassified. No suite may be
    classified as passive reference-only material inside `terlan-vm`; reference
    notes belong in documentation after the test files are removed.
  - Requirement: Common Test/EUnit suites must not be treated as release
    validation for Terlan. If a suite describes VM semantics we still need, the
    behavior must move into Rust VM tests, executable Terlan tests, or a
    VM-owned integration gate.
  - Requirement: tests for BEAM bytecode compatibility, ERTS internals,
    Erlang distribution, OTP app loading, compiler app behavior, or stock OTP
    libraries must be removed unless Terlan VM owns the same behavior under a
    different API and the behavior has been ported to a Terlan-owned test.
    EPMD discovery is retained independently from the Terlan distribution wire
    format; only its Erlang/Common Test plumbing is removable after equivalent
    golden-owned EPMD coverage passes.
  - Requirement: the audit output must live in docs or a machine-readable
    inventory file and must be checked by automation. Ad hoc notes are not
    enough.
  - Requirement: P0 areas (`scheduler`, `mailbox`, `timers`,
    `process-registry`, and `links-monitors`) are release-critical reliability
    gates. They cannot remain merely `partial` for 0.0.7 closeout; each must
    become `ported` with a passing replacement gate.
  - Requirement: repository cleanup is not a runtime quality property. The
    audit must not require legacy files to be absent, reject their
    reintroduction, validate deletion tombstones, or couple a `ported` claim to
    file removal. Historical removal manifests may remain as documentation but
    must not participate in any test or quality gate.
  - Gate: add `make terlan-vm-erl-suite-audit-check`.
  - Completed slice: `docs/runtime/TERLAN_VM_BEAM_TEST_SUITE_SUMMARY.tsv`
    now records checked total, classification, and owner counts for the
    external suite audit. The gate fails if the discovered active corpus,
    active file-status ledger, classification totals, or owner totals drift,
    so the migration inventory cannot silently change.
  - Completed gate hardening: `docs/runtime/TERLAN_VM_BEAM_TEST_PORT_PLAN.tsv`
    is now paired with
    `docs/runtime/TERLAN_VM_BEAM_TEST_PORT_PLAN_SUMMARY.tsv`; the gate checks
    the eleven prioritized port/delete areas, their P0/P1/P2 counts, their
    port/delete classifications, and the replacement-gate distribution. The
    final status records every area as `ported` through executable replacement
    evidence.
  - Completed slice: `docs/runtime/TERLAN_VM_BEAM_TEST_PORT_STATUS.tsv`
    now records executable `ported`/`partial`/`not-ported` status for each
    prioritized behavior area, plus a separately checked `rust-runtime`,
    `native-aot`, or `not-proven` execution path and its owning execution gate.
    `make terlan-vm-erl-suite-audit-check` validates one row per area, checks
    each row against the port plan priority/classification/replacement gate,
    rejects unknown status values, and rejects unrecognized native-AOT proof
    gates. Deletion state is deliberately absent from this checked schema.
  - Completed ledger shape: `TERLAN_VM_BEAM_TEST_FILE_STATUS.tsv` contains no
    active external files. `TERLAN_VM_BEAM_TEST_DELETION_MANIFEST.tsv` remains
    historical documentation only; the audit does not read the manifest or
    inspect whether any recorded path exists.
  - Historical migration slice: the OTP `lib/stdlib/test/pool_SUITE.erl` peer/RPC wrapper
    is replaced by a composed golden-VM parity test for deterministic
    least-connections placement, linked worker spawn, request/reply mailbox
    delivery, normal linked-worker teardown, transactional all-unreachable
    membership rejection, and mutation-free failed spawn. Both exact tests pass
    with warnings denied, and `make terlan-vm-erl-suite-audit-check` passes
    after deleting the obsolete suite. File-level progress is now 143 ported,
    1,777 not ported, 166 deleted, and 1,754 not deleted. The canonical `make
    vm-distributed-scheduling-check` remains blocked before its scheduling
    recipe because its transitive `rust-test-suite` bootstrap currently has 123
    unrelated `terlc` failures in the dirty tree; therefore this parent item
    remains open and the pool replacement is not reported as a closed release
    gate.
  - Historical migration slice: the configurable termination-delay helper
    `lib/stdlib/test/supervisor_2.erl` is replaced by deterministic VM
    logical-clock supervision coverage. The focused test proves concurrent
    in-budget and overdue child shutdown, no early timeout, clean deadline
    cancellation, typed timeout exit, restart, and drained timer state. With
    warnings denied, the exact test, all five shutdown tests, and all three
    supervision quality-validator tests pass; the quality CLI also validates
    29 fixtures and 28 exact selectors. All 24 supervision primitive selectors
    passed during the full gate attempt, but `make vm-supervision-restart-check`
    remains blocked before its shutdown recipe by unrelated failures in the
    dirty tree's canonical Rust-suite prerequisite. The independently scoped
    `make terlan-vm-erl-suite-audit-check` passes after deleting the obsolete
    Erlang helper. File-level progress is now 147 ported, 1,773 not ported, 170
    deleted, and 1,750 not deleted; the parent parity item remains open.
  - Historical migration slice: the OTP `lib/stdlib/test/naughty_child.erl` unlinking
    helper is replaced by a VM actor regression proving linked child spawn,
    child-initiated unlink, unlinked termination, surviving parent liveness,
    and immediate parent mailbox progress. The supervision quality gate now
    checks this exact selector and the previously omitted concurrent shutdown
    selector, and the obsolete Erlang helper is deleted. File-level progress is
    now 192 ported, 1,728 not ported, 215 deleted, and 1,705 not deleted; the
    parent parity item remains open. The focused replacement, supervision
    quality validator, and canonical suite-audit gate pass. The canonical
    supervision gate remains blocked in its transitive Rust-suite prerequisite
    by five unrelated dirty-tree failures (three serve/package-artifact
    assertions, changed-generation REPL p95, and persistent AOT REPL service),
    after 5,245 other `terlc` tests passed.
  - Historical migration slice: the OTP `lib/stdlib/test/dummy_via.erl` custom registry
    helper is replaced by one composed VM actor regression covering stable
    name registration and lookup, mutation-free duplicate rejection, named
    mailbox delivery, explicit unregister, typed missing-name failure,
    automatic owner-exit cleanup, and safe name reuse. The exact replacement,
    canonical `make vm-process-model-check`, and
    `make terlan-vm-erl-suite-audit-check` pass after deleting the obsolete
    helper. File-level progress is now 193 ported, 1,727 not ported, 216
    deleted, and 1,704 not deleted; the parent parity item remains open.
  - Historical migration slice: the 2,594-line OTP
    `erts/emulator/test/scheduler_SUITE.erl` Common Test harness is deleted
    after its portable priority-pressure, weighted progress, bounded wait, and
    atomic suspend/resume behavior was verified through the dedicated VM
    scheduler parity tests. The remaining CPU-topology, scheduler-affinity,
    dirty-thread, pollset, reader-group, runtime-flag, and Common Test cases
    are ERTS implementation mechanics rather than Terlan VM behavior. Both
    parity tests pass with warnings denied and all 46 selectors in
    `make vm-scheduler-contract-check` pass. File-level progress is now 193
    ported, 1,727 not ported, 217 deleted, and 1,703 not deleted; the P0
    scheduler area and parent parity item remain open for the remaining
    scheduler-related corpus.
  - Historical migration slice: the OTP
    `erts/emulator/test/message_queue_data_SUITE.erl` queue-representation
    harness is replaced by a focused VM mailbox storage regression covering
    mixed payload identity, FIFO order across batches, stable inspection
    counts, exact logical mailbox charges, complete draining, and strict
    separation from process heap ownership. Terlan intentionally exposes no
    mutable on-heap/off-heap queue mode; the retired process flags, garbage
    collector synchronization, sequential tracing, remote peers, and Common
    Test plumbing were BEAM implementation mechanics. The exact replacement
    passes with warnings denied. File-level progress is now 194 ported, 1,726
    not ported, 218 deleted, and 1,702 not deleted; the P0 mailbox area and
    parent parity item remain open for the remaining mailbox corpus.
  - Historical migration slice: the 1,302-line OTP
    `erts/emulator/test/erl_link_SUITE.erl` harness is replaced by a focused VM
    failure regression covering repeated two-sided link/unlink mutation,
    canonical symmetric snapshots, owner-scoped idempotent demonitor,
    exactly-once DOWN and trapped EXIT delivery, relationship cleanup,
    unrelated-process isolation, and duplicate-exit suppression. Erlang
    distribution peers and ports, ERTS link/monitor tables, scheduler pinning,
    process priorities, raw distribution operations, `erts_debug`, and Common
    Test plumbing are intentionally retired as implementation mechanics. The
    exact replacement passes with warnings denied. File-level progress is now
    195 ported, 1,725 not ported, 219 deleted, and 1,701 not deleted; the P0
    links/monitors area and parent parity item remain open for the remaining
    monitor, signal, and process corpus.
  - Historical migration slice: the 1,405-line OTP
    `erts/emulator/test/monitor_SUITE.erl` harness is replaced by a VM semantic
    fix and focused regression: monitoring a known exited identity now
    allocates a fresh reference, delivers one immediate `noproc` completion,
    wakes the observer, and retains no active relationship. The same contract
    proves live completion reasons, owner-scoped idempotent demonitor,
    selective DOWN flushing, deterministic relationship inspection,
    128-to-1 and 1-to-128 fanout, and complete cleanup. Registered-name and
    port monitors, distribution nodes, aliases, tagged DOWN tuples, time-offset
    monitoring, ERTS monitor-tree/heap placement, scheduler pinning, arbitrary
    Erlang exit terms, and Common Test plumbing are intentionally retired. The
    exact replacement and related actor relationship/native-transition tests
    pass with warnings denied. File-level progress is now 196 ported, 1,724
    not ported, 220 deleted, and 1,700 not deleted; the P0 links/monitors area
    and parent parity item remain open for the signal and process corpus.
  - Historical migration slice: the 2,461-line OTP
    `erts/emulator/test/signal_SUITE.erl` harness is retired after ten exact
    VM-owned signal contract tests passed through
    `vm-failure-primitives-check`. The replacement covers directional priority
    links, monitors, and aliases; explicit and reply alias lifecycles; direct
    normal, trapped, abnormal, and untrappable-kill exit behavior; linked kill
    chains; post-exit rejection; PID/name/alias message ordering before
    monitor `DOWN`; unlink/exit serialization across old and new link
    generations; and 256 rounds of interleaved relationship churn, mailbox
    inspection, FIFO validation, and duplicate-wakeup exclusion. Dirty
    schedulers, ERTS heap/literal relocation, distribution backpressure and
    old-node encoding, tracing and raw signal queues, driver fixtures, and
    Common Test plumbing are intentionally retired as implementation-specific.
    File-level progress is now 197 ported, 1,723 not ported, 221 deleted, and
    1,699 not deleted; the P0 links/monitors area and parent parity item remain
    open only for the process corpus.
  - Historical migration slice: the 5,887-line OTP
    `erts/emulator/test/process_SUITE.erl` harness is retired after a new
    live-only process-info boundary and six exact VM process contracts passed
    through `vm-process-model-check`. The replacement covers live state,
    parent/source metadata, mailbox and name inspection, scheduler reductions,
    postmortem separation, allocation-ordered enumeration, automatic registry
    cleanup, 1,024-generation monotonic PID churn, atomic linked/monitored
    spawn, suspension with ordered timer delivery, normal/trapped/killed and
    duplicate exits, and explicit/reply alias lifecycles. ERTS heap and GC
    flags, process-table layouts and iterators, system tasks, scheduler locks
    and priorities, distribution and remote spawn, raw BIF argument shapes,
    process dictionaries, huge Erlang argument lists, and Common Test plumbing
    are intentionally retired. File-level progress is now 198 ported, 1,722
    not ported, 222 deleted, and 1,698 not deleted. The emulator process corpus
    is closed; the broader parity item remains open for the other audited P0,
    P1, and P2 files.
  - Historical migration slice: the OTP
    `erts/emulator/test/process_max_heap_size_SUITE.erl` harness is retired
    after a VM-owned heap-pressure policy and two exact regressions proved
    immediate uncatchable termination across 16 allocation families with and
    without the historical wrapper paths, exact `killed` monitor completion,
    complete heap and mailbox reclamation, scheduler removal, relationship
    cleanup, and mutation-free hard-limit rejection. BEAM word accounting,
    JIT-known versus unknown binary sizes, writable-binary internals,
    `save_calls`, system flags, and Common Test timetrap plumbing are retired as
    ERTS implementation mechanics. The focused replacements pass with warnings
    denied, while canonical `make vm-memory-heap-pressure-check` remains blocked
    in its transitive Rust-suite prerequisite by three unrelated stale
    serve/AOT packaging assertions after 5,240 other `terlc` tests passed.
    File-level progress is now 199 ported, 1,721 not ported, 223 deleted, and
    1,697 not deleted; the broader parity item remains open.
  - Historical migration slice: the OTP `erts/emulator/test/receive_SUITE.erl` harness is
    retired after two exact actor-runtime regressions proved correlated
    selective receive through a 512-message skipped backlog, priority FIFO and
    blocked-receiver wakeup, exact preservation of ordinary mailbox order,
    scan-reduction charging, and mutation-free recovery from interrupted and
    nested selective receives. BEAM receive save pointers and markers,
    inner/middle/outer signal queues, copy-literal-area yield markers,
    per-process unique-counter overflow fixtures, scheduler pinning, message
    queue storage flags, wall-clock ratio checks, peers, and Common Test
    plumbing are retired as ERTS implementation mechanics. The focused
    replacements and all process/resource prerequisites pass with warnings
    denied. Canonical `make vm-runtime-semantics-check` remains blocked in its
    transitive Rust-suite prerequisite by the same three unrelated stale
    serve/AOT packaging assertions after 5,242 `terlc` tests passed and five
    were ignored. File-level progress is now 200 ported, 1,720 not ported, 224
    deleted, and 1,696 not deleted; the broader parity item remains open.
  - Historical migration slice: the OTP `erts/emulator/test/timer_bif_SUITE.erl` harness
    is retired after two composed actor-timer regressions proved typed delayed
    and correlated sends, huge absolute deadlines beside near deadlines,
    synchronous read and cancel information, 128 equal-deadline timers with
    selective cancellation, stable delivery order, structured payload
    preservation, exactly-once delivery, atomic overflow rejection, and
    complete owner-exit cleanup. Scheduler pinning and online mutation,
    host-clock tolerances, async Erlang BIF reply tuples,
    registered-name-at-expiry lookup, remote nodes, ERTS timer wheels and
    allocator inspection, internal debug controls, and Common Test plumbing
    are retired as implementation mechanics. The focused replacements pass
    with warnings denied through `vm-timer-primitives-check`. File-level
    progress is now 201 ported, 1,719 not ported, 225 deleted, and 1,695 not
    deleted; the broader parity item remains open.
  - Historical migration slice: the OTP `erts/emulator/test/time_SUITE.erl` harness is
    retired after a new VM time-resolution boundary and two exact regressions
    proved signed floor-rounded conversion across seconds, milliseconds,
    microseconds, nanoseconds, and arbitrary positive resolutions; stable
    negative-value behavior; conversion overflow rejection; monotonic logical
    clock acceptance; backward-clock rejection without pending-timer loss; and
    10,000 monotonic unique values. Timezone and DST databases, local and UTC
    calendar conversion, wall-clock timestamps, time offsets and warp modes,
    OS clock comparison, scheduler pinning, remote nodes, and Common Test
    plumbing are retired as host or ERTS implementation mechanics. The focused
    replacements pass with warnings denied through
    `vm-timer-primitives-check`. File-level progress is now 202 ported, 1,718
    not ported, 226 deleted, and 1,694 not deleted; the broader parity item
    remains open.
  - Historical migration slice: the OTP `erts/emulator/test/statistics_SUITE.erl`
    harness is retired after adding a deterministic VM statistics delta
    boundary and two exact actor-runtime regressions. The replacement proves
    reduction and scheduler-slice differences, zero differences for repeated
    immutable snapshots, full-width counters, regression and profile-mismatch
    rejection, runnable queue depth, blocked-process wakeup, immediate queued
    actor exit cleanup, active/exited process gauges, mailbox ownership, and
    timer start/cancel accounting. Host wall-clock and CPU-time tolerances,
    dirty scheduler utilization and online mutation, ERTS garbage-collector
    and allocator counters, host IO byte counters, microstate accounting, raw
    BIF shapes, crypto/file/socket/NIF triggers, and Common Test plumbing are
    retired as host or ERTS implementation mechanics. The focused replacements
    pass with warnings denied and are owned by
    `vm-scheduler-contract-check`. File-level progress is now 203 ported,
    1,717 not ported, 227 deleted, and 1,693 not deleted; the broader parity
    item remains open.
  - Historical migration slice: the OTP `erts/emulator/test/system_info_SUITE.erl`
    harness is retired after adding a typed immutable VM system-information
    snapshot and two exact actor-runtime regressions. The replacement proves
    stable runtime identity and version, target architecture and word size,
    configured process and scheduler capacities, exact live/exited counts and
    runnable-queue cleanup through 1,024-process churn, resource gauges,
    process-limit enforcement, repeated non-mutating inspection, and safe
    inspection while monitor completion signals remain queued. ETS and atom
    globals, dynamic atoms, ERTS heap and garbage-collector settings,
    allocator/crashdump memory comparisons, scheduler topology and pinning,
    build flags and diagnostic binaries, host logger routing, peer nodes, raw
    BIF shapes, and Common Test plumbing are retired as ERTS or host mechanics.
    The focused replacements pass with warnings denied and are owned by
    `vm-process-model-check`. File-level progress is now 204 ported, 1,716 not
    ported, 228 deleted, and 1,692 not deleted; the broader parity item remains
    open.
  - Historical migration slice: the OTP `erts/emulator/test/system_profile_SUITE.erl`
    harness and its native echo-driver fixture are retired after adding an
    on-demand cursor over immutable VM scheduler transitions and two exact
    actor-runtime regressions. The replacement proves typed runnable/inactive
    activity, logical ticks, stable sequences and queue depths, retained
    process locations, observer-free collection, immutable replay, exact
    activity alternation across a ten-actor ten-lap ring, exit events, empty
    incremental reads, and future-cursor rejection. Mutable global profiler
    registration, host timestamp variants, scheduler blocking and online
    mutation, runnable ports and dynamic drivers, ERTS tracing barriers,
    distribution, and Common Test plumbing are retired as host or ERTS
    mechanics. The focused replacements pass with warnings denied and are
    owned by `vm-scheduler-contract-check`. File-level progress is now 206
    ported, 1,714 not ported, 230 deleted, and 1,690 not deleted; the broader
    parity item remains open.
  - Historical migration slice: the 2,284-line OTP
    `erts/emulator/test/trace_SUITE.erl` harness and its NIF/driver fixtures
    are retired after two composed actor-runtime diagnostics regressions
    passed. The replacement proves priority, ordinary, alias, and self-send
    ordering; immediate timeout; message-before-monitor-completion
    correlation; post-exit rejection; retained postmortem source location;
    relationship cleanup; immutable replay and delivered cursors; 100
    suspend-exit-resume races with exact queue cleanup; and 512-message
    pressure inspection and FIFO draining. Mutable global trace sessions and
    tracer processes, propagation flags, Erlang trace match specifications,
    CPU/host timestamps, runnable ports, ERTS system-monitor thresholds,
    distribution tracing, NIF and driver callbacks, raw BIF shapes, and Common
    Test plumbing are retired as implementation mechanics. The focused
    replacements pass with warnings denied and are owned by
    `vm-actor-primitives-check`. File-level progress is now 208 ported, 1,712
    not ported, 232 deleted, and 1,688 not deleted; the broader parity item
    remains open.
  - Historical migration slice: the OTP
    `erts/emulator/test/trace_bif_SUITE.erl` harness is retired after two exact
    VM regressions passed. The replacement proves typed primitive call frames,
    current-first stack snapshots, exact return-to continuation restoration,
    monotonically ordered logical scheduler events, immutable cursor-scoped
    replay, safe inspection of a process-bound retiring code generation,
    ordered retirement after return, and explicit purge without disturbing the
    active replacement. Erlang trace sessions and flags, local/global match
    specifications, host timestamp tuple variants, trace-message shapes, raw
    Erlang BIF namespaces and arguments, BEAM old-code trace_info behavior,
    dynamic Erlang compilation, and Common Test plumbing are retired as ERTS
    mechanics. The focused replacements pass with warnings denied and are
    owned by `vm-actor-primitives-check`. File-level progress is now 209
    ported, 1,711 not ported, 233 deleted, and 1,687 not deleted; the broader
    parity item remains open.
  - Historical migration slice: the OTP
    `erts/emulator/test/trace_call_count_SUITE.erl` harness is retired after
    adding a typed VM-owned module/function/arity call-count registry and two
    exact actor-runtime regressions. The replacement proves 1,000-entry
    recursive counts, exact arity separation, count preservation when enabled
    again, pause without reset, restart from zero, disable removal,
    deterministic immutable snapshots, full-width overflow rejection without
    mutation, and coexistence with current-first execution stacks. Erlang
    trace sessions, wildcard patterns, local and meta tracer processes, match
    specifications, trace message and host timestamp shapes, trace_info tuples,
    collection timers, and Common Test plumbing are retired as ERTS mechanics.
    The focused replacements pass with warnings denied and are owned by
    `vm-actor-primitives-check`. File-level progress is now 210 ported, 1,710
    not ported, 234 deleted, and 1,686 not deleted; the broader parity item
    remains open.
  - Historical migration slice: the OTP
    `erts/emulator/test/trace_call_memory_SUITE.erl` harness is retired after
    adding a typed VM-owned module/function/arity and process allocation
    profiler plus two exact actor-runtime regressions. The replacement uses
    logical bytes and proves cumulative call and allocation totals, process
    isolation, late enablement, nested ownership, receive release without
    losing diagnostic history, spawn attribution, parallel process rows,
    retained post-exit history, restart and disable behavior, deterministic
    immutable snapshots, full-width overflow rejection without mutation, and
    independence from call counting. BEAM heap-word layouts, GC internals and
    abandoned heaps, erts_debug, Erlang trace sessions and propagation flags,
    on-load and global trace patterns, host trace-message shapes, OTP
    application loading, raw trace_info tuples, and Common Test plumbing are
    retired as ERTS mechanics. The focused replacements pass with warnings
    denied and are owned by `vm-actor-primitives-check`. File-level progress
    is now 211 ported, 1,709 not ported, 235 deleted, and 1,685 not deleted;
    the broader parity item remains open.
  - Historical migration slice: the OTP
    `erts/emulator/test/trace_call_time_SUITE.erl` harness and its native NIF
    build fixture are retired after adding a typed VM-owned
    module/function/arity and process execution-time profiler plus two exact
    actor-runtime regressions. The replacement uses exclusive logical
    scheduler ticks and proves exact call and tick totals, arity and process
    isolation, enable preservation, pause without mutation, restart and
    disable behavior, nested callee attribution, stable post-return and
    retained post-exit inspection, deterministic immutable snapshots,
    full-width atomic overflow rejection, and independence from call-count
    and call-memory profiles. Host wall-clock tolerances, scheduler fairness
    timing, Erlang trace sessions and propagation flags, local and meta
    tracers, raw trace_info tuples, BIF and NIF timing, dynamic trace barriers,
    and Common Test plumbing are retired as ERTS mechanics. The focused
    replacements pass with warnings denied and are owned by
    `vm-actor-primitives-check`. File-level progress is now 213 ported, 1,707
    not ported, 237 deleted, and 1,683 not deleted; the broader parity item
    remains open.
  - Historical migration slice: the OTP
    `erts/emulator/test/trace_local_SUITE.erl` harness and its dynamically
    compiled dummy module are retired after adding a typed VM-owned exact
    module/function/arity local diagnostic stream plus two exact actor-runtime
    regressions. The replacement proves globally ordered call,
    return-to-caller, and exception events; event-class configuration;
    idempotent enable and exact disable; recursive call events; dynamic
    execution-stack growth and LIFO caller restoration; immutable cursor
    replay; retained post-exit exception stacks; code-server unload, purge,
    and missing-export rejection; full-width event ordering; and deterministic
    enable/disable churn. Tracer processes and mailboxes, local/global/meta
    breakpoint modes, match specifications, host timestamps, trace_info tuple
    shapes, emulator breakpoint memory barriers, raw BIF exception shapes,
    dynamic Erlang compilation, and Common Test plumbing are retired as ERTS
    mechanics. The focused replacements pass with warnings denied and are
    owned by `vm-actor-primitives-check`. File-level progress is now 215
    ported, 1,705 not ported, 239 deleted, and 1,681 not deleted; the broader
    parity item remains open.
  - Historical migration slice: the OTP
    `erts/emulator/test/trace_meta_SUITE.erl` harness is retired after adding a
    typed VM-owned exact-function observer stream plus two exact actor-runtime
    regressions. The replacement proves observer-scoped immutable cursors,
    ordered call and return events, calls-only and return-enabled
    configuration, idempotent observer replacement, in-flight return routing
    pinned to the observer that received the call, observer-exit subscription
    cleanup, exact arity filtering, coexistence with local diagnostics,
    explicit local start and stop triggers that never silence meta events,
    4,096 recursive publications, and deterministic sequence ordering. Tracer
    processes and relay mailboxes, meta breakpoints and match specifications,
    host timestamps, trace_info tuple shapes, silent process flags, raw BIF
    tracing, and Common Test plumbing are retired as ERTS mechanics. The
    focused replacements pass with warnings denied and are owned by
    `vm-actor-primitives-check`. File-level progress is now 216 ported, 1,704
    not ported, 240 deleted, and 1,680 not deleted; the broader parity item
    remains open.
  - Historical migration slice: the OTP
    `erts/emulator/test/trace_nif_SUITE.erl` harness, native build fragment,
    and C NIF fixture are retired after wiring diagnostic ownership through
    the real descriptor-admitted pure-native actor boundary and adding two
    exact regressions. The replacement proves typed exact
    module/function/arity entry, return-to-caller, and failure events;
    independent local and observer-scoped streams; deterministic logical
    ordering; exact arity filtering; immediate local enable and disable;
    observer-exit cleanup; failure-versus-return distinction; and trace-token
    ownership across suspended native continuations. Erlang NIF loading and
    stubs, C ABI callbacks, apply-versus-direct-call syntax, global trace
    sessions and flags, match specifications, tracer mailboxes, host
    timestamps, raw trace tuple shapes, and Common Test plumbing are retired
    as ERTS mechanics. Both exact replacements pass with warnings denied and
    are owned by `vm-actor-primitives-check`. File-level progress is now 218
    ported, 1,702 not ported, 242 deleted, and 1,678 not deleted; the broader
    parity item remains open.
  - Historical migration slice: the OTP
    `erts/emulator/test/trace_port_SUITE.erl` harness, native build fragment,
    and C echo-driver fixture are retired after adding two exact VM-owned
    diagnostic regressions. The replacement proves typed exact call and
    return events, observer-scoped delivery, large structured actor-message
    preservation, deterministic logical-memory release, link and unlink
    inspection, scheduler activity ordering, immutable replay and delivered
    cursors, observer-exit subscription cleanup, retained pre-exit history,
    dropped in-flight observer returns, 256 call/return pairs after observer
    failure, and continued tracee execution. Dynamic Erlang port drivers,
    external-term echo transport, global default tracers, raw trace tuples and
    flags, host timestamps, ERTS scheduling and garbage-collection events,
    driver crashes, and Common Test plumbing are retired as ERTS mechanics.
    Both exact replacements are owned by `vm-actor-primitives-check`.
    File-level progress is now 220 ported, 1,700 not ported, 244 deleted, and
    1,676 not deleted; the broader parity item remains open.
  - Historical migration slice: the OTP
    `erts/emulator/test/trace_session_SUITE.erl` suite and shared
    `trace_sessions.erl` wrapper are retired after adding three exact
    VM-owned diagnostic regressions. The replacement proves independent early
    and late local, observer, and scheduler cursors; exact call and return
    filtering; independent stream disablement; observer teardown cleanup;
    replay after another consumer advances; future-cursor rejection; linked
    and monitored child topology; parent and grandchild survival after unlink
    and child exit; name cleanup; deterministic 80-message pressure, drain,
    and refill inspection; current-first stack returns; exact arity isolation;
    independent call-count, logical-time, and logical-memory lifecycles;
    retained post-exit profiles; and disabled-operation rejection. Named and
    legacy trace sessions, tracer processes and mailboxes, match
    specifications, propagation flags, host timestamps, on-load breakpoints,
    ERTS system-monitor thresholds, runnable ports, BIF traced bits, session
    garbage-collection destructors, raw BIF argument errors, peer nodes, ETS
    session controllers, and Common Test plumbing are retired as ERTS
    mechanics. All three exact replacements pass with warnings denied and are
    owned by `vm-actor-primitives-check`. File-level progress is now 222
    ported, 1,698 not ported, 246 deleted, and 1,674 not deleted; the broader
    parity item remains open.
  - Historical migration slice: the OTP `erts/emulator/test/tracer_SUITE.erl` suite,
    `tracer_test.erl` NIF loader, native Make fragment, and C tracer callback
    fixture are retired after adding three exact VM-owned regressions. The
    replacement proves fifteen enable, disable, unload, and reload
    generations; exact call, return, exception, and arity filtering; explicit
    trace versus discard behavior; observer-scoped delivery and stable event
    sequences; structured message preservation and deterministic logical
    memory release; symmetric link and unlink inspection; linked and monitored
    spawn, registry cleanup, exit, and DOWN delivery; scheduler enqueue,
    dequeue, and exit activity with immutable replay and delivered cursors;
    invalid and exited observer rejection without state mutation; dropped
    in-flight returns after observer failure; retained pre-failure events;
    continued subject execution; and future-cursor rejection. Erlang tracer
    modules, NIF enabled and trace callbacks, callback state maps, raw callback
    event tuples, scheduler identifiers, host timestamps, ERTS
    garbage-collection events, sequential trace tokens, NIF load and purge
    caches, trace sessions, match specifications, and Common Test plumbing are
    retired as ERTS mechanics. All three exact replacements pass with warnings
    denied and are owned by `vm-actor-primitives-check`. File-level progress is
    now 225 ported, 1,695 not ported, 249 deleted, and 1,671 not deleted; the
    broader parity item remains open.
  - Historical migration slice: the OTP `erts/emulator/test/tuple_SUITE.erl` suite is
    retired after consolidating its portable behavior under
    `pattern-matching-support-check`. Three new exact VM-owned regressions plus
    the existing tuple reconstruction, tuple/list conversion, exact-arity,
    nested extraction, nested case, and immutable record-update tests prove
    zero-through-eight runtime arities; a 16,385-element value and
    power-of-two boundary reads; typed conversion; persistent set, insert,
    delete, and append reconstruction; structural equality in conditions;
    distinct tuple case and arity selection; scalar fallback; 1,024 immutable
    record generations; source preservation; and compile-time rejection of
    scalar, short, long, and wrong-element tuple inputs. Dynamically typed
    Erlang tuple BIFs and badarg tuples, one-based mutation APIs, destructive
    heap updates, forced minor garbage collection, host-memory probes and
    maximum-arity allocations, BEAM loader and register-reload instructions,
    SASL and os_mon setup, and Common Test plumbing are retired as ERTS
    mechanics. File-level progress is now 226 ported, 1,694 not ported, 250
    deleted, and 1,670 not deleted; the broader parity item remains open.
  - Historical migration slice: the OTP `erts/emulator/test/z_SUITE.erl` last-suite
    aggregate is retired under `vm-final-health-check`. Its new exact VM-owned
    closeout regression churns 64 actors with registered names, opaque aliases,
    long-horizon timers, and native resources, then proves owner-exit cleanup
    leaves zero live processes, runnable entries, mailboxes, active timers,
    names, aliases, and resources while repeated final inspection stays
    identical. The gate also re-runs the existing weighted scheduler progress,
    suspend/resume atomicity, one-through-sixty-minute logical timer, and
    resource owner-exit contracts. Thread-specific event counts, mutable host
    scheduler flags and identities, ERTS node-container references, pollset and
    file-descriptor internals, inet_gethost probing, lock-checker graph files,
    literal-area collector processes, host CPU tolerances, timetrap plumbing,
    and Common Test ordering are retired as implementation mechanics.
    File-level progress is now 227 ported, 1,693 not ported, 251 deleted, and
    1,669 not deleted; the broader parity item remains open.
  - Historical migration slice: the five tracked
    `erts/lib_src/yielding_c_fun/test` files are retired under
    `vm-yielding-native-parity-check`. The replacement executes safe Rust
    SHA-256 incrementally under the real VM scheduler, preserves the original
    `"hej"` vector and every repeated-`h` input from 2 KiB through 16 MiB,
    bounds each work slice by reductions, retains digest state across yields,
    proves a peer runs between every continuation slice, and re-runs both the
    scheduler state-resume contract and compiled direct-AOT continuation
    integration. Erlang application/load_nif scaffolding and the
    source-to-source C transformer CLI, lexer, generated-header, debug-trap,
    memory-log, and golden-C mechanics are retired because Terlan owns native
    continuations in compiler IR and the VM scheduler instead of rewriting C
    stacks. File-level progress is now 232 ported, 1,688 not ported, 256
    deleted, and 1,664 not deleted; the broader parity item remains open.
  - Historical migration slice: the OTP `erts/emulator/test/a_SUITE.erl` first-suite
    aggregate and its ordering-only `a_SUITE_data/Makefile.src` fixture are
    retired under `vm-final-health-check`. The new exact VM-owned initial
    health regression proves zero processes, runnable entries, mailboxes,
    timers, aliases, names, and native resources before workload startup and
    proves repeated inspection is stable; the same gate retains the already
    exact long-horizon logical-timer and final owner-cleanup contracts. The
    unused ERTS timer-driver C fixture is removed with them. Forced large
    pid/port allocation, inet_gethost and timer-server startup, pollset sizing,
    thread-specific event inspection, registered cross-suite holder processes,
    dynamic driver loading, Common Test ordering, and host-I/O observations are
    retired as ERTS mechanics. File-level progress is now 234 ported, 1,686
    not ported, 258 deleted, and 1,662 not deleted; the broader parity item
    remains open.
  - Historical migration slice: the complete tracked
    `erts/emulator/test/alloc_SUITE.erl` family is retired under
    `vm-memory-heap-pressure-check`. Two exact VM-owned regressions now prove
    byte-pattern preservation across larger value replacement, deterministic
    logical retained-size accounting, atomic hard-pressure rejection without
    mailbox or heap mutation, round-robin process-owner isolation, shared
    allocation retain and reclassification semantics, rejected pressured
    retains, and final-zero release for every owner. The custom C driver,
    allocator bucket and red-black-tree tests, block coalescing, carrier pools
    and migration, mseg cache inspection, realloc pointer mechanics, native
    host threads, mmap/supercarrier flags, allocator option mutation, peer
    nodes, os_mon memory probing, and Common Test plumbing are retired as ERTS
    implementation details. File-level progress is now 246 ported, 1,674 not
    ported, 270 deleted, and 1,650 not deleted; the broader parity item remains
    open.
  - Historical migration slice: the tracked
    `erts/emulator/test/async_ports_SUITE.erl` family is retired under
    `vm-tcp-stream-check`. One exact VM-owned regression now drives two blocked
    writers through 4,096 nonblocking pressure rejections without endpoint
    death or queued-byte mutation, proves duplicate writer parks are
    idempotent, drains delayed input, wakes both writers in FIFO order through
    the scheduler adapter, preserves the complete 10 KiB payload on retry, and
    completes a final liveness probe. Erlang ports, `nosuspend` command flags,
    linked tester processes, raw port messages, host sleeps and timetraps,
    executable discovery, packet-4 stdio framing, and the delayed C echo
    process are retired as ERTS mechanics. File-level progress is now 248
    ported, 1,672 not ported, 272 deleted, and 1,648 not deleted; the broader
    parity item remains open.
  - Historical migration slice: the tracked `erts/emulator/test/atomics_SUITE.erl` is
    retired under `vm-table-primitives-check`. A VM-owned fixed-size atomic
    array and exact regression now cover signed and unsigned zero
    initialization, 64-bit limits and wrapping add/subtract, exchange,
    compare-and-exchange success and observed-mismatch results, mutation-free
    rejection of empty arrays, invalid indexes, mismatched value kinds, and
    out-of-range deltas, plus eight-thread fetch-add and retrying-CAS
    contention with exact final counts. Erlang resource references, ERTS
    allocation-size inspection, host atomic-width discovery, exception stack
    rendering, module export enumeration, and Common Test plumbing are retired
    as OTP mechanics. File-level progress is now 249 ported, 1,671 not ported,
    273 deleted, and 1,647 not deleted; the broader parity item remains open.
  - Historical migration slice: the tracked `erts/emulator/test/beam_SUITE.erl` and its
    raw `beam_init_yregs.S` and `beam_register_cache.S` companions are retired
    under `vm-process-model-check`. Four exact VM-owned regressions now cover
    10,000 bounded tail calls, retained function-value arguments, 261 ordered
    high-cardinality bindings, dense and sparse signed integer dispatch through
    64-bit limits, tuple arity dispatch through 300 fields, non-tuple fallback,
    and fail-closed mixed arithmetic. The parser now collects repeated ordinary
    lets into one ordered node, preventing deep generated binding sequences
    from overflowing syntax serialization. BEAM Y-register initialization,
    ARM JIT register caching, BEAM assembly loading, ERTS heap-size tables,
    code purge, merl generation, and Common Test plumbing are retired as OTP
    mechanics. File-level progress is now 250 ported, 1,670 not ported, 274
    deleted, and 1,646 not deleted; the broader parity item remains open.
  - Historical migration slice: the tracked
    `erts/emulator/test/beam_literals_SUITE.erl` and its generated
    `unoptimized_literal_tests.S` and `literal_case_expression.S` companions
    are retired under `vm-literal-parity-check`. Three exact source-to-CoreIR
    VM regressions now cover signed 64-bit boundary values, Float, atom,
    string, proper-list, and tuple construction; literal matching and fallback;
    runtime type classification; mixed Int/Float promotion; out-of-range
    integer rejection; and invalid arithmetic rejection. Float patterns now
    compare finite numeric values rather than decimal storage text, so
    equivalent spellings such as `7` and `7.0` match reliably. Arbitrary-size
    Erlang integers, improper lists, raw `put_list` register permutations,
    literal-pool relocation, assembled BEAM modules, compiler optimization
    toggles, code loading and purging, Erlang exception tuple shapes, and
    Common Test plumbing are retired as OTP mechanics. File-level progress is
    now 251 ported, 1,669 not ported, 275 deleted, and 1,645 not deleted; the
    broader parity item remains open.
  - Historical migration slice: the tracked `erts/emulator/test/bif_SUITE.erl` is retired
    under `vm-bif-parity-check`. A dedicated typed VM gate now covers Int and
    Float min/max, bounded dynamic integer predicates, 100,000-element proper
    list length, callback invocation and malformed arity rejection, finite
    Unicode atom rendering and comparison, process liveness snapshots, checked
    arithmetic failures, and console output routing. Erlang global atom
    interning and existing-atom lookup, arbitrary term ordering, improper
    lists, ambient OS environment mutation, VM halt and crash-dump controls,
    erl_bif_types and Dialyzer metadata, NIF stub inspection, BEAM stacktrace
    shapes, group leaders, dirty schedulers, signal-queue internals,
    port/reference node lookup, runtime BIF enumeration, and Common Test
    plumbing are retired as OTP mechanics. File-level progress is now 252
    ported, 1,668 not ported, 276 deleted, and 1,644 not deleted; the broader
    parity item remains open.
  - Historical migration slice: the tracked `erts/emulator/test/big_SUITE.erl` and its
    dynamically compiled `big_SUITE_data/literal_test.erl` helper are retired
    under `vm-big-integer-parity-check`. Three exact VM-owned regressions now
    prove signed division/remainder reconstruction across positive and negative
    boundary values, deterministic algebraic identities, bounded greatest
    common divisor and exponentiation, recursive factorial and Fibonacci
    execution, maximum signed literals, compile-time rejection beyond the Int
    range, and stable overflow errors without host panics. Arbitrary-precision
    allocation and representation borders, Karatsuba internals, enormous
    shifts and modular powers, cross-node RPC evaluation, forced garbage
    collection, host-memory/system limits, JIT instruction variants, and
    Common Test mechanics are retired as OTP implementation behavior.
    File-level progress is now 254 ported, 1,666 not ported, 278 deleted, and
    1,642 not deleted; the broader parity item remains open.
  - Historical migration slice: the tracked `erts/emulator/test/binary_SUITE.erl` and its
    local-driver build fixture are retired under
    `vm-binary-suite-parity-check`. Three exact VM-owned regressions now prove
    typed Bytes construction, ordered round trips, concatenation, length, and
    full/inner/empty splitting; mutation-free unsigned-octet rejection; 50,000
    byte immutable storage; unaligned large slicing; exact partial-bit
    reconstruction and canonical trailing-bit masking; deterministic
    Terlan-owned term serialization; declared-atom admission; and truncated,
    trailing, and undeclared-atom rejection. Nested dynamic Erlang iolists,
    ETF compatibility and compression/minor-version options, arbitrary Erlang
    term hashing and ordering, sub-binary heap/refcount and garbage-collector
    internals, dynamic C drivers, host memory/system limits, distribution
    peers, scheduler/trapping ratios, and Common Test mechanics are retired as
    OTP implementation behavior. File-level progress is now 256 ported, 1,664
    not ported, 280 deleted, and 1,640 not deleted; the broader parity item
    remains open for the following `bs_*` family and later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/bs_bincomp_SUITE.erl`,
    `bs_bincomp_no_opt_SUITE.erl`, and
    `bs_bincomp_stripped_types_SUITE.erl` module-renamed duplicates are retired
    under `vm-binary-comprehension-parity-check`. Three exact VM-owned
    regressions now prove source-to-CoreIR-to-VM byte mapping, deterministic
    ordered Cartesian generators, typed Bytes materialization, exact 7-bit
    unaligned packing and decoding, nested fixed-width binary output, and
    mutation-free rejection of an invalid generated octet and fixed-width
    field. Erlang binary-generator syntax, call tracing, random-process
    harnesses, Common Test plumbing, optimizer-disabled execution, and stripped
    BEAM type metadata are retired as OTP implementation behavior. File-level
    progress is now 259 ported, 1,661 not ported, 283 deleted, and 1,637 not
    deleted; the broader parity item remains open for `bs_bit_binaries_SUITE.erl`
    and the later corpus.
  - Historical migration slice: the tracked
    `erts/emulator/test/bs_bit_binaries_SUITE.erl` and its module-renamed
    `bs_bit_binaries_no_opt_SUITE.erl` duplicate are retired under
    `vm-bit-binary-parity-check`. Three exact VM-owned regressions now prove
    exact logical lengths from empty through 1,000,001 bits, canonical unused
    storage bits, asymmetric unaligned slicing and reconstruction, aligned
    prefix/remainder splitting, shifted byte-chunk decoding, 2,048 padding-free
    single-bit appends, and 100 actor mailbox round trips of one immutable
    million-bit unaligned payload. BEAM register/instruction selection,
    dynamic Erlang list/bitstring coercion, `erts_debug` allocation inspection,
    host-memory stress, linked Erlang process mechanics, optimizer-disabled
    execution, and Common Test plumbing are retired as OTP implementation
    behavior. File-level progress is now 261 ported, 1,659 not ported, 285
    deleted, and 1,635 not deleted; the broader parity item remains open for
    `bs_construct_SUITE.erl` and the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/bs_construct_SUITE.erl`,
    `bs_construct_no_opt_SUITE.erl`, and
    `bs_construct_stripped_types_SUITE.erl` module-renamed duplicates are
    retired under `vm-binary-construction-parity-check`. Four exact VM-owned
    regressions now prove typed source-to-CoreIR construction for UInt,
    IntBits, Bytes, Bits, and Rest segments; all bounded 1-through-63-bit
    signed and unsigned values in both endian modes; exact multi-segment
    assembly; zero initialization across hundreds of field sizes; IEEE-754
    byte preservation; 512-segment output; and mutation-free width, range, and
    byte-length rejection. Arbitrary-precision fields, dynamic Erlang term
    coercion, half-float-only syntax, native-host endian promises, ERTS
    allocation/reduction accounting, host-memory limits, writable-binary and
    JIT instruction internals, optimizer-disabled execution, stripped BEAM
    types, and Common Test plumbing are retired as OTP implementation behavior.
    File-level progress is now 264 ported, 1,656 not ported, 288 deleted, and
    1,632 not deleted; the broader parity item remains open for
    `bs_match_int_SUITE.erl` and the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/bs_match_int_SUITE.erl`,
    `bs_match_int_no_opt_SUITE.erl`, and
    `bs_match_int_stripped_types_SUITE.erl` module-renamed duplicates are
    retired under `vm-binary-integer-match-parity-check`. Four exact VM-owned
    regressions now prove typed source-to-CoreIR integer patterns and truncated
    fallback; every bounded width from 1 through 63 for signed and unsigned
    values in both endian modes at bit offsets zero through seven; every
    dynamic two-field partition through 63 bits with exact reconstruction;
    stable invalid-width/range rejection; and 1,638 repeated unaligned 32-bit
    reads. Arbitrary-precision extraction, random harnesses, ERTS match-context
    and register placement, huge heap/allocation probes, host-word-size paths,
    garbage-collector forcing, JIT/SSA variants, optimizer-disabled execution,
    stripped BEAM types, and Common Test plumbing are retired as OTP
    implementation behavior. File-level progress is now 267 ported, 1,653 not
    ported, 291 deleted, and 1,629 not deleted; the broader parity item remains
    open for `bs_match_misc_SUITE.erl` and the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/bs_match_misc_SUITE.erl`,
    `bs_match_misc_no_opt_SUITE.erl`, and
    `bs_match_misc_stripped_types_SUITE.erl` module-renamed duplicates are
    retired under `vm-binary-misc-match-parity-check`. Four exact VM-owned
    regressions now prove source-to-CoreIR bound-field, exact-tail, nibble,
    unaligned-bit, and truncated matching; unaligned IEEE-754 32-bit and 64-bit
    wire-byte preservation in both endian orders; variable-length body and
    suffix extraction; nibble and six-bit grouping; ordered content, MAC,
    padding, and terminal-length extraction; 257 retained-context scans; and
    stable overflow, range, and exact-length rejection. Native-host endian
    promises, half-float-only syntax, ERTS match-context registers and reuse,
    writable-binary relocation, heap growth and garbage-collector forcing,
    huge host allocations, BEAM instruction corruption/loading, optimizer
    modes, stripped BEAM types, and Common Test plumbing are retired as OTP
    implementation behavior. File-level progress is now 270 ported, 1,650 not
    ported, 294 deleted, and 1,626 not deleted; the broader parity item remains
    open for `busy_port_SUITE.erl` and the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/busy_port_SUITE.erl` and
    `busy_port_SUITE_data/Makefile.src` are retired under
    `vm-busy-port-parity-check`, together with the fixture directory's five C
    drivers. Four exact VM TCP and scheduler regressions now prove 4,096 atomic
    nonblocking pressure rejections without endpoint death or buffer mutation;
    five FIFO parked writers with two exited waiters safely skipped and three
    survivors scheduled in order; exact ordered retry delivery for commands 1
    through 50; and complete buffer, reader, writer, listener, and stream
    cleanup with stable post-close and post-cancellation rejection. Erlang raw
    ports, dynamic drivers, system busy-port monitors, host timing ranges,
    force/nosuspend driver flags, linked-port exit tuple shapes, scheduler
    pinning, million-signal floods, wall-clock sleeps, ERTS driver callbacks,
    and Common Test plumbing are retired as OTP implementation behavior.
    File-level progress is now 272 ported, 1,648 not ported, 296 deleted, and
    1,624 not deleted; the broader parity item remains open for
    `call_trace_SUITE.erl` and the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/call_trace_SUITE.erl` and
    `call_trace_SUITE_data/my_upgrade_test.erl` fixture are retired under
    `vm-call-trace-parity-check`. Four exact VM actor and code-server
    regressions now prove exact module/function/arity filtering, idempotent
    enablement, per-process call and return identity, disablement and immutable
    cursors; observer replacement with in-flight returns pinned to their
    original observer; wrong-arity silence; a complete typed exception stack
    across 512 recursive frames; and generation-safe hot reload, retirement,
    unload, ordered lifecycle events, and purge. Erlang all/new/existing trace
    propagation, match specifications and payload rewriting, tracer mailboxes,
    mutable global/local/on-load trace patterns, host and CPU timestamps,
    improper-list stress, Erlang exception tuple shapes, dynamic BEAM loading,
    breakpoints, trace-session wrappers, and Common Test plumbing are retired
    as OTP implementation behavior. File-level progress is now 274 ported,
    1,646 not ported, 298 deleted, and 1,622 not deleted; the broader parity
    item remains open for `code_SUITE.erl` and the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/code_SUITE.erl`,
    `code_SUITE_data/Makefile.src`, and `code_SUITE_data/literals.erl` are
    retired under `vm-code-suite-parity-check`, together with the native
    synchronous tracer C fixture. Four exact VM regressions now prove 256
    publish/purge generations with monotonic identity and bounded retained
    metadata; 64 interleaved owners across two modules with lossless retirement
    and explicitly ordered purge; mutation-free compile, missing-module, and
    artifact-identity rejection without consuming a generation; and 16 KiB
    strings plus 512-element nested literal values retained through unload and
    an altered reload. Existing `vm-code-server-check` coverage continues to
    own late-bound external functions, staged visibility, captured functions,
    false dependencies, export inspection, process-bound generations,
    rollback, UTF-8 paths, and unload. BEAM chunks and MD5, prepared loading,
    the two-code-index limit, ERTS code-permission locks and GC handoff, literal
    areas and heap copying, system process requests, scheduler and dirty
    executor internals, dynamic Erlang compilation, raw BIF errors, native
    tracer callbacks, and Common Test plumbing are retired as OTP
    implementation behavior. File-level progress is now 277 ported, 1,643 not
    ported, 301 deleted, and 1,619 not deleted; the broader parity item remains
    open for `code_parallel_load_SUITE.erl` and the later corpus.
  - Historical migration slice: the tracked
    `erts/emulator/test/code_parallel_load_SUITE.erl` is retired under
    `vm-code-parallel-load-parity-check`. A thread-safe VM code-server boundary
    now serializes publication and coalesces identical active artifacts. Two
    exact regressions prove that 160 simultaneous identical requests create one
    generation with 159 reuses, and that 160 process owners atomically switch
    through six logical versions across four passes while every drained
    generation retires and purges exactly once. The complete run produces 75
    contiguous lifecycle events and finishes with no retained generation.
    Dynamic Erlang form compilation, the BEAM two-code-index restriction and
    `not_purged` result, asynchronous `check_process_code` retries, forced
    process termination, wall-clock sleeps, Common Test timetraps, and ERTS
    loader scheduling are retired as OTP implementation behavior. File-level
    progress is now 278 ported, 1,642 not ported, 302 deleted, and 1,618 not
    deleted; the broader parity item remains open for `counters_SUITE.erl` and
    the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/counters_SUITE.erl` is
    retired under `vm-counters-parity-check`. A VM-owned signed 64-bit counter
    facade now reuses the atomic primitive while exposing atomic and
    write-concurrent intent, deterministic logical size, zero initialization,
    and exact `get`, `add`, `sub`, and `put` behavior. Two regressions cover
    ten-slot basic behavior, signed-limit wraparound, full-width deltas,
    mutation-free validation, 32 isolated writers, and eight-thread
    accumulation of 800,000 mixed positive, negative, and wrapping updates
    across 100 cells. ERTS per-scheduler shards, host allocation-size ranges,
    scheduler pinning and discovery, randomized timing, raw
    `badarg`/`system_limit` stack formatting, module export enumeration, and
    Common Test plumbing are retired as OTP implementation behavior.
    File-level progress is now 279 ported, 1,641 not ported, 303 deleted, and
    1,617 not deleted; the broader parity item remains open for `ddll_SUITE.erl`
    and the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/ddll_SUITE.erl` and
    `ddll_SUITE_data/Makefile.src` are retired under
    `vm-dynamic-module-parity-check`. A VM-owned dynamic-module registry now
    validates module identity and initialization before mutation, counts
    process ownership references, exposes deterministic leases and snapshots,
    delays unload and reload while leases remain, cancels pending unload on a
    new reference, cleans up exiting owners, applies replacement atomically,
    force-drains leases in stable order, and preserves permanent modules. Four
    exact regressions cover the complete portable lifecycle and error surface.
    The ERTS `erl_ddll` API, host shared-library loading, driver locks and
    reference tables, raw ports and monitor tuples, C callback ABI, native
    dummy/echo/failure fixtures, wall-clock polling, and Common Test plumbing
    are retired as OTP implementation behavior. File-level progress is now 281
    ported, 1,639 not ported, 305 deleted, and 1,615 not deleted; the broader
    parity item remains open for `decode_packet_SUITE.erl` and the later corpus.
  - Historical migration slice: the tracked
    `erts/emulator/test/decode_packet_SUITE.erl` is retired under
    `vm-packet-decode-parity-check`. The formerly dormant packet helper is now
    an active typed decoder in the VM TCP framing reader, retaining incomplete
    bytes across scheduler polls and preserving trailing frames. Six exact
    regressions cover raw and fixed-length prefixes, ASN.1, Sun RPC record
    marking, CDR endianness, FastCGI padding, TPKT, TLS and SSLv2 ClientHello
    normalization, incomplete and oversized frames, line chunking, HTTP
    requests, responses, known and unknown headers, folded values, malformed
    lines, absolute and arbitrary-scheme URIs, ports, IPv6 authorities, long
    incremental headers, fragmented delivery, and ordered frame retention.
    Erlang term argument validation, list-versus-binary duplication, atom
    interning, random bit-offset sub-binaries, C-stack corruption probes,
    wall-clock timetraps, and Common Test plumbing are retired as OTP
    implementation behavior. File-level progress is now 282 ported, 1,638 not
    ported, 306 deleted, and 1,614 not deleted; the broader parity item remains
    open for `dgawd_handler.erl` and the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/dgawd_handler.erl` is
    retired under `vm-diagnostic-probe-parity-check`. The VM I/O diagnostics
    log now installs typed probes at an exact append position, matches only an
    exact stable diagnostic code, ignores history and message lookalikes,
    keeps a match sticky, binds queries to one log generation, and closes with
    deterministic cross-log and duplicate-close rejection. Two exact
    regressions cover the complete lifecycle and validation surface. OTP's
    global `error_logger` mutation, `gen_event` callbacks, emulator report
    tuples, ten-minute synchronous query timeout, flattened iolist phrase
    scanning, and handler code-change mechanics are retired as host-runtime
    implementation behavior. File-level progress is now 283 ported, 1,637 not
    ported, 307 deleted, and 1,613 not deleted; the broader parity item remains
    open for `dirty_bif_SUITE.erl` and the later corpus.
  - Historical migration slice: the tracked `erts/emulator/test/dirty_bif_SUITE.erl` is
    retired under `vm-dirty-bif-parity-check`. Native continuation parking now
    retains an independent explicit-suspension lease, so native completion
    cannot wake an actor that was suspended while offloaded work was pending,
    and a plain resume cannot steal continuation authority. Five new exact
    regressions cover scheduler reclassification and repeated re-entry, peer
    progress, nonblocking inspection, registration and tracing of a parked
    actor, immediate termination visibility and stale completion rejection,
    plus both explicit-suspend/native-completion orderings. The composed gate
    also reruns native failure propagation and validation, retained reply
    memory, cancellation cleanup, and concurrent module-generation retirement
    and purge. ERTS dirty CPU/IO scheduler labels, debug BIF stacktrace shapes,
    process main locks, heap and binary reference counts, dynamic Erlang
    compilation, BEAM code-index purge behavior, remote peers, wall-clock
    sleeps, and Common Test plumbing are retired as host-runtime implementation
    behavior. That slice left `dirty_nif_SUITE.erl` as the next ordered corpus
    entry; its completed replacement is recorded below.
  - Historical migration slice: the tracked `erts/emulator/test/dirty_nif_SUITE.erl` and
    `dirty_nif_SUITE_data/Makefile.src` are retired under
    `vm-dirty-nif-parity-check`, together with the nine C fixtures owned by the
    deleted build fragment. Managed native delivery now validates exact
    continuation authority before a registered-name lookup and consumes the
    lease only after successful delivery, so a stale name cannot mutate the
    mailbox, scheduler, or continuation state. Two exact regressions cover a
    1,000-element structured native value round trip with complete memory
    release and a 64-actor registered-name burst with exactly-once routing,
    owner-exit registry cleanup, and retry-safe failed lookup. The composed
    gate also reruns dirty-BIF continuation contracts, all stable native value
    shapes, typed failure and cancellation cleanup, deadline owner-exit worker
    cleanup, literal survival across unload and altered reload, resource owner
    cleanup, and a real direct-AOT `.tvm` consumer with explicit shutdown
    acknowledgement. ERTS dirty scheduler labels, the Erlang NIF and driver
    ABIs, raw heap/process locks and reference counts, Erlang stacktrace term
    shapes, remote peers, wall-clock halt callbacks, dynamic Erlang/C
    compilation, port mechanics, and Common Test plumbing are retired as
    host-runtime implementation behavior. The active ledger now contains 1,605
    not-yet-ported files, while the compact deletion manifest contains 315
    completed removals. That slice left `distribution_SUITE.erl` as the next
    ordered corpus entry; its completed replacement is recorded below.
  - Historical migration slice: the tracked `erts/emulator/test/distribution_SUITE.erl`,
    `distribution_SUITE_data/Makefile.src`, and
    `distribution_SUITE_data/run.erl` are retired under
    `vm-distribution-suite-parity-check`. Message identities are now reserved
    transactionally and committed only after complete TETF encoding and size
    validation, so a rejected oversized frame cannot consume an identity or
    mutate transport state. Two exact regressions cover 256 ordered structured
    one-kilobyte frames, contiguous message identities, acknowledgement and
    duplicate lifecycles, mutation-free oversized rejection, visible and
    hidden membership, partition/heal and timeout, epoch-safe restart, stale
    generation rejection, pruning, disconnect/reconnect, pending-state
    preservation, and wrong-identity rejection. The composed gate also reruns
    distributed transport, process-model, and failure-primitives coverage.
    Erlang distribution/ETF wire tags, atom-cache slots, distribution flags,
    raw fragments and headers, distribution ports/controllers, cookies, Erlang
    node-name modes, ERTS locks/refcounts/busy buffers/scheduler flags, OS
    IOV_MAX behavior, remote peers, dynamic Erlang compilation, wall-clock
    timing, and Common Test plumbing are retired as host-runtime mechanics.
    OTP-compatible EPMD discovery remains an independently ported Terlan VM
    service and is not part of this distribution-wire retirement.
    The active ledger contains 1,605 unresolved files and the deletion manifest
    contains 315 completed removals; the next ordered active suite is
    `dummy.erl`.
  - Historical migration slice: the tracked `erts/emulator/test/driver_SUITE.erl` and
    `driver_SUITE_data/Makefile.src` are retired under
    `vm-driver-suite-parity-check`, together with the 32 C fixtures and shared
    header owned by the deleted build fragment. A VM-owned driver runtime now
    provides transactional instance allocation, validated controller transfer,
    bounded scatter/gather commands and front/back byte queues, exact
    head/dequeue accounting, logical timer replacement and cancellation with
    exactly-once firing, isolated environment state, bounded exactly-once
    callback delivery, and deterministic close and owner-exit cleanup. Two
    exact regressions cover atomic capacity rejection, a 512-callback ordered
    burst, retry identity, controller failure cases, timer boundaries, and
    complete resource release. The composed gate also reruns busy-port, timer,
    resource, scheduler, native-worker, no-default-Tokio, and all 32 I/O-reactor
    selectors. Pollset/check_io internals, raw file descriptors and select
    controls, ERTS driver ABI callbacks and version markers, allocator caches,
    scheduler pinning and exact ERTS timeslices, errno and host FD assumptions,
    dynamic C compilation, remote peers, wall-clock timing, and Common Test
    plumbing are retired as host-runtime mechanics. At that closeout the
    canonical audit classified 1,605 active files and 315 completed removals,
    leaving `dummy.erl` as the next ordered active entry.
  - Historical migration slice: the tracked `erts/emulator/test/dummy.erl` stub is
    retired as `remove-non-portable`. The file contained only
    `-module(dummy).`: it exported and defined no callable function, held no
    assertion or runner entry, and was not referenced by any Erlang suite,
    build fragment, or runner. There is therefore no observable reliability
    behavior to reproduce in a Terlan-owned test; manufacturing a replacement
    API would create false parity. Its exact inventory row preserves the
    architectural decision without making file presence a quality-gate
    concern. At that checkpoint the audit classified 1,604 active
    not-yet-ported files; `dump_SUITE.erl` was the next ordered active entry.
  - Historical migration slice: the tracked `erts/emulator/test/dump_SUITE.erl` crash-dump
    harness is retired after `make vm-dump-suite-parity-check` passed. Terlan
    now publishes one bounded, deterministic, redacted fatal-diagnostic bundle
    containing scheduler state, retained live and exited process state, and
    explicitly observed missing identities. Exact regressions cover enabled and
    disabled policy, typed cause and generation validation, deterministic
    ordering, subject and byte limits, collision-safe complete atomic
    publication, and fail-closed cleanup. Heart processes, peer/RPC orchestration,
    environment-controlled ERTS dump policy, scheduler pinning, encrypted dump
    files, text-format regex assertions, wall-clock sleeps, and Common Test
    plumbing are retired as ERTS mechanics. The canonical corpus now contains
    1,603 active not-yet-ported files and 317 completed removals;
    `efile_SUITE.erl` is the next ordered active entry.
  - Historical migration slice: the tracked `erts/emulator/test/list_bif_SUITE.erl` is
    retired under `vm-list-bif-suite-parity-check`. The direct-AOT compiler now
    specializes concrete generic List inspection results before lowering,
    inventories intermediate `Option[T]` layouts from executable expressions,
    and lowers alias-based `Some` and `None` cases by the concrete scrutinee
    type. A linked native object running against the real actor-owned managed
    runtime proves ordered head and tail behavior, empty and singleton
    boundaries, constant-time length, strict signed decimal and radix parsing,
    overflow and malformed-input rejection, canonical rendering round trips,
    and finite Float parsing. The focused ABI tests additionally enforce a
    128-byte integer parse ceiling, bases two through thirty-six, uppercase
    canonical digits, and exact minimum/maximum i64 rendering, while a
    multilevel 2,050-element persistent list and the formal typechecker prove
    large-list boundaries and static rejection of improper tails. Arbitrary
    Erlang bignums, prefix parsing with remainder values, dynamic badarg and
    system_limit exception shapes, forgeable pid/port/reference strings, ETF
    creation rewriting, remote peers, forced garbage collection, wall-clock
    timetraps, and Common Test plumbing are retired as OTP-specific mechanics.
    The canonical ledger now contains 1,585 active not-yet-ported files and 335
    completed removals.
  - Historical migration slice: the tracked
    `erts/emulator/test/literal_area_collector_test.erl` synchronization helper
    is retired under `vm-literal-area-collector-parity-check`. Terlan replaces
    discovery and polling of BEAM's private collector process with a
    synchronous generation-reachability proof on the execution-shard owner.
    Two exact regressions cover initial quiescence; all ten native frame,
    continuation, transfer, heap, mailbox, timer, resource, async callback,
    debugger, and crash-metadata retention classes; deterministic busy
    diagnostics; mutation-free replacement rejection; immediate release
    visibility; ordinary actor-continuation drain; and successful replacement
    only after complete quiescence. Process-table discovery, process-dictionary
    caching, aliases, status-message exchange, host monotonic time, timetrap
    scaling, polling sleeps, and the ERTS literal-area collector are retired as
    implementation mechanics. The canonical ledger now contains 1,584 active
    not-yet-ported files and 336 completed removals; `lttng_SUITE.erl` is the
    next ordered active entry.
  - Historical migration slice: the tracked `erts/emulator/test/lttng_SUITE.erl` and
    `lttng_SUITE_data/Makefile.src` are retired under
    `vm-lttng-suite-parity-check`, together with the native C caller-driver
    fixture owned by that build fragment. The VM driver boundary now offers an
    opt-in provider-neutral trace stream whose disabled path is one bit-mask
    check. Two exact regressions prove typed and contiguous open, command,
    vectored queue, dequeue, readiness callback, logical timer,
    controller-transfer, flush/close, and cleanup transitions; exact VM owner
    and caller attribution; immutable cursor replay; event-class filtering; a
    4,096-event bound; explicit dropped-event and expired-cursor diagnostics;
    future-cursor rejection; and no trace-sequence consumption on failed
    operations. Existing transactional driver and iovec parity remains composed
    into the gate. LTTng CLI sessions and text scraping, org_erlang_otp
    providers, ERTS allocator carriers, Erlang ports and driver callbacks, raw
    file descriptors, localhost timing, host environment mutation, peer nodes,
    dynamic native loading, and Common Test plumbing are retired as host/ERTS
    mechanics. The canonical ledger now contains 1,582 active not-yet-ported
    files and 338 completed removals; `map_SUITE.erl` is the next ordered active
    entry.
  - Historical migration slice: `erts/emulator/test/map_SUITE.erl`, its malformed legacy
    BEAM fixture, and the generated `map_no_opt_SUITE.erl` clone are retired
    under `vm-map-suite-parity-check`. Terlan now lowers the typed Map and
    Iterator surface directly into the actor-owned managed operation ABI:
    empty and from-entries construction, size and emptiness, structural get and
    membership, persistent put/remove/take/clear, and source-ordered iteration
    all execute through linked native objects. Required-key and guarded map
    patterns use checked managed projections, mutable receiver syntax rebinds
    the persistent map in source order, and expression-owned collection and
    iterator-step layouts are admitted before image generation. Focused
    contracts cover duplicate replacement, missing and present lookup/take,
    preserved originals, 160-entry flat-to-indexed transition, full-hash
    collisions, indexed-to-flat demotion, structural reference keys, precise
    relocation, stale-token protection, atomic wrong-type rejection, and
    deterministic direct-AOT object bytes for the former no-opt clone. BEAM
    flatmap/HAMT representation and hash details, process dictionary and
    ETS/DETS coupling, bytecode and y-register behavior, ETF encoding, tracing,
    host benchmarks, forced GC mechanics, optimizer-option clones, and Common
    Test plumbing are retired. The canonical ledger now contains 1,580 active
    not-yet-ported files and 340 completed removals;
    `match_spec_SUITE.erl` is the next ordered active entry.
  - Historical migration slice: `erts/emulator/test/match_spec_SUITE.erl` is retired
    under `vm-match-spec-suite-parity-check` without introducing an ERTS
    match-spec compatibility interpreter. Terlan compiles application matching
    and selection into the AOT image: linked-object contracts cover
    equality-based repeated binding, ordered typed guards, short-circuit
    suppression of invalid division, bounded unary arithmetic, and stable
    Boolean control-flow targets, while typed collection selection lowers to an
    image-private native helper. The composed map and structured-pattern gates
    own required-key matching and managed projections; VM table contracts own
    stable ordered traversal, mutation, access control, and cleanup. Typed
    exact-function trace subscriptions preserve arity identity, enable/disable
    behavior, call/return/exception order, deep stack capture, generation-safe
    reload, in-flight observer pinning, dead-observer cleanup, immutable
    cursors, and silent unmatched calls. A dynamic `match_spec_run` call is
    rejected before native linking. The ERTS match-spec interpreter and
    instruction encoding, trace control words and sequential-trace mutation,
    arbitrary Erlang-term predicate dispatch, ETS match-spec compilation,
    caller-line file rewriting, process dumps, peer nodes, host sleeps, and
    Common Test plumbing are retired. The canonical ledger now contains 1,579
    active not-yet-ported files and 341 completed removals;
    `module_info_SUITE.erl` is the next ordered active entry.
  - Historical migration slice: `erts/emulator/test/module_info_SUITE.erl` is retired
    under `vm-module-info-suite-parity-check`. Terlan now exposes a typed
    compiler-derived active-module descriptor with stable module identity,
    generation, checksum, source-map identity, sorted public exports, and
    sorted public-plus-private functions. Missing lookup is mutation-free;
    unload and retired-generation purge remove the descriptor. The direct-AOT
    descriptor carries unique nonzero dispatch identities, distinguishes
    public and private functions, synthesizes no BEAM `module_info/0` or
    `module_info/1` exports, and its public entries execute through the linked
    native object. Raw native addresses, NIF inventories, BEAM MD5 and file
    metadata, Erlang compile attributes and options, dynamic Erlang
    compilation, BEAM-specific delete/purge mechanics, and Common Test
    plumbing are retired. The canonical ledger now contains 1,578 active
    not-yet-ported files and 342 completed removals; `mtx_SUITE.erl` is the
    next ordered active entry.
  - Historical migration slice: `erts/emulator/test/mtx_SUITE.erl` and its native build
    fixture are retired under `vm-mtx-suite-parity-check`. Terlan replaces
    shared read/write mutex semantics with one actor owner and bounded
    publication: twenty concurrent producers preserve every per-producer
    command order and publication identity while six writer streams mutate
    state only on the owner. Exact 1,024-entry pressure rejects immediately,
    recovers completely after drain, and consumes no sequence on rejection.
    Nineteen simultaneous mutator contenders fail while one scheduler owns the
    actor; explicit release admits a new monotonically generated owner. The
    composed gate runs eight isolated seeded multicore schedules under a
    deadlock watchdog, lock-free bounded MPSC and no-lost-wakeup checks,
    bounded work-stealing decisions, and owner/access-controlled table
    behavior. ERTS rwmutex modes, blocking and try-lock APIs, frequent-read
    variants, scheduler-pinned native threads, host sleeps and CPU thresholds,
    ETS lock implementation stress, the NIF resource boundary, and Common Test
    plumbing are retired. The canonical ledger now contains 1,576 active
    not-yet-ported files and 344 completed removals; `multi_load_SUITE.erl` is
    the next ordered active entry.
  - Historical migration slice: `erts/emulator/test/multi_load_SUITE.erl` is retired
    under `vm-multi-load-suite-parity-check`. One closed direct-AOT image now
    contains 100 distinct modules with four exports each; all 400 exports have
    unique dispatch identities and execute through the linked object. The
    owner-exclusive VM code registry keeps staged modules invisible until
    batch publication, publishes 100 distinct artifacts, and rejects duplicate
    module identities before native image admission or metadata mutation while
    preserving snapshots and event history. The composed gate also retains
    progress under 160 simultaneous publication contenders. Sequential versus
    parallel BEAM loader timing, infinite CPU burner processes, dynamic Erlang
    form compilation, prepared-code blobs, `finish_loading` argument shapes,
    load-time `on_load` callbacks and inspection, purge BIF mechanics, and
    Common Test plumbing are retired. The canonical ledger now contains 1,575
    active not-yet-ported files and 345 completed removals;
    `native_record_SUITE.erl` is the next ordered active entry.
  - Historical migration slice: `erts/emulator/test/native_record_SUITE.erl` and its
    `ext_records.erl` fixture are retired under
    `vm-native-record-suite-parity-check`. Direct-AOT records now carry
    module-qualified nominal identities, so same-named records in distinct
    modules cannot alias. Linked-object coverage executes typed construction,
    access, persistent update, record patterns, a 64-field layout, and 10,000
    recursive updates. Scalar managed field projection now uses the scalar ABI,
    preserving legitimate zero values instead of treating them as null
    references. The composed gate also proves 1,000 ordered actor messages,
    managed GC and mailbox ownership, canonical nested TETF records, invalid
    metadata rejection, repeated distribution round trips, and reference
    envelope validation. Erlang dynamic record reflection, native-record term
    ordering, ETF tags and reserved bits, atom-cache mechanics, BEAM code
    deletion/purge internals, peer nodes, and Common Test plumbing are retired.
    The canonical ledger now contains 1,573 active not-yet-ported files and 347
    completed removals; `nif_SUITE.erl` is the next ordered active entry.
  - Historical migration slice: `erts/emulator/test/nif_SUITE.erl`,
    `nif_SUITE_data/Makefile.src`, `nif_mod.erl`, and `tester.erl`, together
    with the suite-owned C sources, copied API headers, and build fixtures, are
    retired under `vm-nif-suite-parity-check`. Portable native-call behavior is
    now owned by typed VM values and errors, bounded generation-qualified
    capability requests, exact completion/cancellation/timeout credit
    recovery, process-qualified resources, foreign-owner and stale-handle
    rejection, panic isolation, AOT generation drain, and orderly worker and
    shard shutdown. Unsupported Erlang-NIF compatibility requests fail loudly
    as typed unknown operations. Erlang NIF ABI versions and macros,
    load/reload/on_load callbacks, raw term and atom-table mechanics, pollsets,
    file descriptors, ports, ERTS monitors and threads, dynamic C builds, and
    Common Test plumbing are retired. The canonical ledger now contains 1,569
    active not-yet-ported files and 351 completed removals;
    `node_container_SUITE.erl` is the next ordered active entry.
  - Completed closeout (2026-07-29): the final live OTP-derived corpus
    contained 172 test-source files under `lib/stdlib/test`. The exact
    `TERLAN_VM_OTP_STDLIB_MIGRATION.tsv` ledger maps 5 files to executable
    Terlan ports, 59 files to Rust VM replacements, and 108 files to explicit
    non-product retirement, with `remaining_semantics=none` for every row.
  - Completed closeout: `make otp-stdlib-port-check` passes the compiled AOT
    bytes/list/map replacements; set, string, JSON, path, URI, random, and
    regex runtime contracts; process, registry, failure, supervision, timer,
    distribution, diagnostics, and filesystem worker behavior.
  - Completed closeout: all eleven behavior areas are `ported`; the P0
    scheduler, mailbox, timers, process-registry, and links/monitors areas no
    longer claim `partial` status.
  - Completed closeout: after replacement gates passed, all 1,347 tracked files
    under the external `lib/stdlib/test` tree were retired. The active
    file-status and suite summaries now report zero files. This cleanup is
    recoverable from Git history and is not enforced by an absence, tombstone,
    or reintroduction gate.
  - Completed closeout: `make terlan-vm-erl-suite-audit-check` passes with the
    exact historical ledger intact, zero active external files, and all eleven
    behavior areas proven.
  - Make integration: `terlan-vm-erl-suite-audit-check` remains an explicit
    historical-evidence and replacement-gate review; it does not inspect the
    external checkout and is not an absence or reintroduction gate.
  - Acceptance: the gate fails when an active inventory row uses an unknown
    classification or claims replacement coverage through a non-existent
    Rust/Terlan gate.
  - Acceptance: the gate fails while any `port-to-*` suite lacks a named
    replacement test and passing Terlan-owned gate.
  - Acceptance: the first audit produces a prioritized port/delete list for
    scheduler, mailbox, timers, process registry, links/monitors,
    serialization, distribution framing, HTTP/TCP, filesystem, and std
    behavior that Terlan VM actually intends to own.

### VM HTTP Concurrency Investigation

- [x] Investigate and fix VM HTTP socket concurrency after the Erlang/OTP test
  suite migration is complete.
  - Requirement: do not start this slice until
    `terlan-vm-erl-suite-audit-check` has classified, ported, or removed the
    relevant scheduler, process, mailbox, TCP, HTTP, and timer suites from the
    external `terlan-vm` checkout. The concurrency investigation must build on
    migrated VM semantics, not on parallel benchmark-only machinery.
  - Context: current benchmarks validate the performance improvement over the
    old OTP/SafeNative HTTP handler path, but that legacy lane is only
    historical context after the VM pivot. Future performance comparisons must
    use a maintained Rust HTTP baseline such as Axum on Tokio. The current
    loopback socket lane exposed that Terlan VM fell apart under high
    concurrency when intake and handler execution were serialized. The VM
    handler, in-memory stack, and VM stream lanes are fast; the OS socket
    benchmark now uses load-scaled acceptor and handler worker pools over a
    bounded queue, but production-grade concurrency semantics still require
    migrated VM scheduler/process/TCP suites before closeout.
  - Requirement: keep the existing result as a regression baseline:
    VM handler-only, VM no-transport stack, VM stream HTTP/1, and loopback
    socket HTTP/1 must remain benchmarked separately so handler execution,
    protocol parsing, VM streams, and OS sockets are not collapsed into one
    opaque number.
  - Requirement: keep the historical OTP/SafeNative report available for
    migration notes only. Do not use it as the future performance target once
    this slice starts.
  - Requirement: add executable Rust HTTP benchmark baselines outside the
    golden compiler/runtime path. The baselines must implement equivalent
    static, JSON/text handler, request metadata access, route matching,
    keep-alive, CRUD, payload, and 1/100/1000 concurrency tracks without
    sharing Terlan VM internals.
  - Requirement: production-realistic HTTP benchmarks must cover the runtime
    properties that make Axum/Hyper/Tokio hard to beat in real services, not
    only the thin happy path:
    - async I/O readiness and wakeup behavior,
    - scheduler fairness across many active connections,
    - bounded backpressure and overload behavior,
    - full HTTP protocol parsing and response serialization,
    - connection lifecycle, keep-alive, close, cancellation, and timeout,
    - ecosystem integration boundaries such as TLS, static assets, JSON,
      routing, middleware, and handler metadata,
    - long-running load with warmup, fixed request counts, p50/p95/p99,
      throughput, and stable artifact output.
  - Requirement: the benchmark report must explicitly distinguish "thin path"
    wins from production-realistic wins. A lane cannot be used as evidence that
    Terlan HTTP outperforms Axum/Hyper unless it includes async readiness,
    fairness, backpressure, protocol, lifecycle, and long-running load coverage
    comparable to the Rust baseline.
  - Requirement: add an executable benchmark report that compares current
    VM HTTP against Axum/Tokio and Hyper/Tokio baselines. The report must show handler,
    VM stream, and loopback socket lanes with 1, 100, and 1000
    concurrent/request tracks where the lane supports them.
  - Requirement: VM machinery semantics must still benchmark against actual
    OTP, because OTP is the reference for process spawn, mailbox
    send/receive, selective receive, timers, links/monitors, supervision,
    registry behavior, scheduler fairness, hot reload, and distributed
    process semantics. Do not use Axum/Tokio as a proxy for VM resiliency
    semantics.
  - Requirement: split benchmark families clearly:
    `vm-http-vs-axum-check` for HTTP/framework and low-level Rust baseline
    performance under the current gate name and
    `vm-semantics-vs-otp-check` for VM/process/fault-tolerance semantics.
  - Requirement: benchmark output must call out the performance winner and
    percentage delta for each comparable track. Non-comparable tracks must be
    labeled explicitly, for example "VM stream has no host socket transport"
    or "Rust HTTP baseline includes Tokio scheduler and OS socket transport".
  - Requirement: benchmark output must include an HTTP realism matrix for each
    comparable lane. The matrix must say whether the lane exercises async I/O,
    fairness, backpressure, full protocol parsing, connection lifecycle,
    ecosystem integration, and long-running load. Missing dimensions must make
    the lane advisory, not release-decisive.
  - Requirement: whenever Terlan VM falls behind Axum/Tokio, Hyper/Tokio, or OTP on a
    comparable lane, the benchmark gate must produce or link a performance
    clue report. The report must identify the suspected VM subsystem, relevant
    source files/functions, measured symptom, and next optimization hypothesis.
    A slower result without code-level analysis is not an acceptable gate
    output.
  - Requirement: investigate and document the concurrency bottleneck before
    changing architecture. Candidate causes include single handler worker,
    acceptor/worker serialization, queue fairness, backpressure behavior,
    keep-alive scheduling, per-connection process ownership, and socket
    readiness integration.
  - Requirement: throughput is a first-class Terlan VM HTTP success metric,
    not a secondary number. If Terlan VM wins mean latency but loses
    throughput against Axum/Tokio or Hyper/Tokio on a sustained keep-alive
    lane, the benchmark gate must classify that as an investigation item and
    emit a performance clue instead of treating the lane as an unqualified
    Terlan win.
  - Requirement: make sustained HTTP benchmark rows statistically credible
    before using them for architecture decisions. The comparison harness must
    support repeated runs per lane and report at least median, min, max, and
    variance or spread for mean latency, p99 latency, wall time, and
    throughput. Single-run c100/c1000 results are acceptable only as advisory
    smoke evidence.
  - Current investigation finding: the c1000 sustained `/add` lane showed
    meaningful run-to-run variance for both Terlan VM and Rust baselines.
    Terlan VM repeated c1000 add keep-alive runs varied from roughly 124k rps
    in one full-report sample to roughly 155k-163k rps in focused reruns;
    Axum/Hyper c1000 add keep-alive rows also moved materially between full
    baseline runs. Repeated sampling is now enforced by the benchmark harness:
    `TERLAN_BENCH_HTTP_COMPARISON_RUNS` defaults to 3, the credibility checker
    rejects one-sample sustained baselines, and the gate self-test proves
    three completed samples are required before sustained rows can become
    architecture-decisive.
  - Current investigation finding: per-request latency and throughput can
    disagree because the benchmark starts request latency timing after a
    client connection exists, while throughput wall time includes connection
    intake, acceptor scheduling, worker drain, and client/thread scheduling.
    The next benchmark artifact must split accept/connect/intake wall time
    from handler/request service time.
  - Current investigation finding: tuning worker count alone does not explain
    the c1000 behavior. Acceptor count has a strong effect: too few acceptors
    collapse throughput, while too many can add noise. The gate must record
    acceptor count, worker count, queue pressure, connection count,
    requests-per-connection, and effective concurrency for every comparable
    socket row.
  - Requirement: investigate request/stream-granular scheduling rather than
    only connection-granular worker scheduling. A VM worker that owns and
    drains a whole keep-alive connection before yielding can hide handler
    speed behind connection-level head-of-line blocking. The target model
    should let VM scheduling observe request streams, readiness, cancellation,
    and backpressure at the granularity needed to maximize throughput under
    many active keep-alive connections.
  - Requirement: the target architecture must make each accepted connection or
    request stream owned by a VM process, with scheduler-visible readiness,
    cancellation, timeout, backpressure, and resource cleanup. Do not add a
    hidden host async runtime to mask VM scheduler gaps.
  - Requirement: use maintained protocol parsers/serializers where appropriate
    (`httparse`, `http`, `rustls`, etc.), but stream lifecycle, handler
    scheduling, cancellation, and backpressure remain VM-owned.
  - Requirement: success criteria must include a non-stalling 1000-concurrency
    socket benchmark. If the benchmark is skipped by platform permissions, the
    skip must be stable and the VM stream benchmark must still run.
  - Gate: `make vm-http-concurrency-investigation-check`.
  - Gate: `make vm-http-vs-axum-check`.
  - Gate: `make vm-semantics-vs-otp-check`.
  - Make integration: run `vm-http-concurrency-investigation-check` after
    `terlan-vm-erl-suite-audit-check`, `vm-tcp-stream-check`, and
    `terlan-vm-http-lane-check`.
  - Acceptance: executable benchmarks prove no regression in direct VM handler,
    VM stack, VM stream HTTP/1, and loopback socket HTTP/1 mean/p95 latency
    against the checked 0.0.7 baseline.
  - Acceptance: the 1000-concurrency loopback socket lane completes or reports
    a stable platform skip; it must not hang indefinitely.
  - Acceptance: adversarial tests prove queue saturation, handler crash,
    client disconnect, slow client, oversized payload, malformed HTTP,
    cancelled request, and timed-out handler are handled by VM-owned
    backpressure/cancellation/resource cleanup instead of host runtime leakage.

### VM Command-Line Debugger

- [x] Add a Lisp-style interactive Terlan debugger for command-line VM
  execution.
  - Gate: `make terlc-debugger-check`.
  - Requirement: debugging must be VM-owned. The debugger must inspect Terlan
    VM processes, stacks, values, mailboxes, resources, source spans, and
    runtime failures directly; it must not be a wrapper around generated
    Erlang, BEAM, host logs, or ad hoc print tracing.
  - Requirement: provide an interactive break loop that can be entered on
    breakpoint, explicit `debug()`/debug intrinsic, uncaught error, process
    crash, or configured stop-on-condition event. The stopped VM must remain
    inspectable instead of immediately tearing down the process tree.
  - Requirement: preserve source mapping from VM instructions and lowered
    expressions back to `.terl` file, line, column, module, function, current
    expression, and generated/template source origin where applicable.
  - Requirement: implement command-line breakpoints for `module.function`,
    `file:line`, and conditional breakpoints. Commands must support list,
    remove, enable, disable, run, continue, step, next, finish, pause, and
    abort.
  - Requirement: implement live frame inspection: backtrace, frame selection,
    arguments, locals, current expression, current source span, current VM
    instruction, and process id. Values must pretty-print Terlan structs,
    tuples, lists, maps, binaries, objects, functions, actors, resources, and
    large values with stable truncation/paging.
  - Requirement: implement expression evaluation in the selected frame. The
    evaluator must use normal Terlan semantics, target-profile inference, and
    VM value rendering, and must reject side-effecting or unsupported
    evaluation with stable diagnostics unless explicitly allowed by debugger
    mode.
  - Requirement: implement Lisp-style condition/restart support. A failure may
    publish typed restart choices such as `retry`, `use_value(value)`, `skip`,
    `abort_process`, and `restart_process`. Choosing a restart must resume or
    terminate through VM-owned control flow, not by patching host state.
  - Requirement: implement process-aware debugging: list processes, select a
    process, inspect runnable/sleeping/blocked/exited state, mailbox contents,
    selective receive cursor, links, monitors, supervisor restart history,
    reductions/ticks, timers, and owned resources.
  - Requirement: implement dynamic tracing from the debugger for
    calls/returns, sends/receives, mailbox matches, process lifecycle events,
    supervisor restarts, resource acquisition/release, HTTP handler events,
    and NativeBoundary calls. Tracing must be filterable by process, module,
    function, message shape, and resource kind.
  - Requirement: integrate with the REPL: `terlc repl --debug` and `:debug`
    must enter the same debugger command surface, with command history,
    arrow-key navigation, completion for modules/functions/locals, and
    readable error recovery.
  - Requirement: support non-interactive/scripted debugging for CI and editor
    integration: `terlc debug <project>`, `terlc debug --break
    app.Main.main`, `terlc debug --script file.terldbg`, and JSON output for
    machine-readable events.
  - Requirement: debugger command names must be small and familiar:
    `bt`, `frame`, `locals`, `args`, `print`, `eval`, `processes`, `process`,
    `mailbox`, `resources`, `trace`, `untrace`, `break`, `list`, `remove`,
    `enable`, `disable`, `pause`, `continue`, `step`, `next`, `finish`,
    `restart`, `restarts`, `use`, `abort`, and `quit`.
  - Requirement: positive executable tests must prove breakpoints, stepping,
    backtrace, locals, frame eval, process list, mailbox inspection, resource
    inspection, restart selection, scripted command execution, and JSON event
    output through the real `terlc debug` command.
  - Requirement: adversarial tests must prove invalid breakpoint specs,
    stale source maps, unsupported frame eval, side-effect rejection, missing
    restart arguments, process exit during debug pause, mailbox truncation,
    huge-value pretty printing, malformed debugger scripts, and non-existent
    processes fail with stable diagnostics.
  - Full-gate requirement: expand `make terlc-debugger-check` beyond the
    reserved command surface as each debugger runtime capability lands.
  - Make integration: run `terlc-debugger-check` after
    `terlan-vm-run-command-check`, `terlan-vm-test-command-check`,
    `vm-diagnostics-quality-check`, and before editor debugger surface checks.
  - Acceptance: `terlc debug` can stop in a Terlan VM program, inspect live
    frames and processes, choose a typed restart for a recoverable failure,
    continue execution, and produce stable JSON events for editor tooling.
  - Acceptance: the gate fails if debugger behavior is proven only by marker
    output, declaration checks, host logs, or tests that bypass the VM command
    path.

### Lean Proof Track Completion

- [x] Audit and complete the Lean proof track for the current Terlan language,
  type, and CoreIR contracts.
  - Requirement: inventory every Lean file under `proofs/lean` and every
    compiler-generated proof artifact or proof manifest. Each row must record
    the source contract it proves, the Terlan language/CoreIR version it
    targets, whether it is current, stale, incomplete, generated-only, or
    delete-candidate, and the gate that checks it.
  - Requirement: stale proofs from earlier syntax, CoreIR, type-system,
    target-profile, BEAM-lowering, or CoreV0 assumptions must be updated or
    deleted. They must not remain as apparent proof coverage.
  - Requirement: the proof track must align with the current formal spec:
    EBNF, typed CoreIR, target-profile inference, VM-owned execution subset,
    pattern/operator/language feature matrices, std package coverage, and
    Wasm/native-boundary contracts where they affect CoreIR.
  - Requirement: proofs must cover at least preservation for the supported
    typed CoreIR subset, deterministic lowering facts for supported source
    forms, rejection facts for unsupported target-profile forms, and no-stale
    proof payload/version checks.
  - Requirement: positive executable proof checks must run the current Lean
    artifacts or their accepted generated proof manifests for at least one
    supported typed CoreIR preservation path, one supported lowering path, and
    one supported rejection path.
  - Requirement: any Terlan feature that is intentionally outside Lean proof
    coverage for 0.0.7 must appear in a machine-readable gap manifest with
    reason, owner, and planned gate. Unlisted proof gaps must fail the gate.
  - Requirement: generated proof artifacts must be reproducible from the
    current compiler. Hand-edited generated artifacts or stale checked-in
    summaries must fail validation.
  - Requirement: Aeneas/Rust verification work must be tied back to Lean only
    through explicit bridge documents or proof artifacts; do not imply Rust
    verification proves Terlan semantics unless the connection is formalized.
  - Gate: add `make lean-proof-track-check`.
  - Remaining gaps: extending executable Lean coverage beyond the restored
    CoreIR integer-arithmetic seed, proving the supported typed CoreIR surface,
    and formalizing any Aeneas/Rust verification bridge.
  - Make integration: run `lean-proof-track-check` from `make check` after
    language feature coverage and before VM/default release tests.
  - Acceptance: the gate runs Lean, validates the proof inventory, rejects
    stale proof-version metadata, and reports zero unclassified proof gaps.
  - Acceptance: the gate fails if a source language/CoreIR feature is marked
    supported in the 0.0.7 coverage matrices but has neither a Lean proof row
    nor an explicit accepted proof-gap row.
  - Acceptance: the gate fails if a Lean theorem or generated proof artifact
    still names removed BEAM/CoreV0/Erlang-lowering product contracts as
    release semantics.

- [x] Slice 4: run Lean proof slice-by-slice as a release-critical quality
  matrix.
  - Requirement: define a fixed slice manifest `docs/compiler/LEAN_PROOF_SLICE_MATRIX.tsv`
    with ordered lanes: `parser`, `coreir`, `target_profile`, `vm_runtime`,
    `native_boundary`, `wasm`, `distribution`, `std_packages`.
  - Requirement: each lane lists:
    - proof family set
    - minimal proof prerequisites
    - acceptance gates
    - owner
    - expected closeout dependency (e.g., `lean-proof-track-release-closeout-check`)
  - Requirement: extend `make lean-proof-track-check` to execute lanes in order
    and fail immediately on first lane violation (no silent skips in closeout mode).
  - Requirement: for each lane, emit per-lane stats in `build/artifacts/lean-proof-lanes.json`:
    duration, number_of_families, failed_families, gap_count, nondeterministic_count,
    reproducibility_failures.
  - Requirement: add lane-to-gate mapping so every active 0.0.7 feature slice in the
    same domain must reference an accepted lane and owner.
  - Requirement: add an explicit blocker when a feature-lane is closed with a
    synthetic proof artifact or manually updated gap row only.
  - Gate: extend `make lean-proof-track-release-closeout-check` to verify all
    lanes pass with `hard` severity and no lane-level blocker entries.
  - Gate: fail release preflight if any lane record in artifact output lacks matching
    checksum entries in `lean-proof-gate.json`.
  - Acceptance: lane report shows all lanes green and includes at least one
    family per core lane that transitions from incomplete to executable current.
  - Acceptance: lane execution is deterministic: rerunning lanes with no source
    changes changes neither lane durations beyond tolerance band nor pass/fail state.

- [x] Slice 5: close explicit Lean proof gaps only through executable restoration
  or test-classification updates.
  - Requirement: add a strict `proof-gap` lifecycle in `proofs/lean/gaps/*.toml`:
    `open -> triaged -> blocked -> remediated -> closed`.
  - Requirement: remove `open` gaps only when accompanied by a restored executable
    proof family or a released lane-level exception with expiration date.
  - Requirement: each gap row must include:
    - `proof_gap_category` (e.g., `not_started`, `resource`, `model_gap`,
      `performance`, `toolchain`)
    - `planned_gate`
    - `remediation_owner`
    - `deadline_or_exception`
    - `blocker_hash`
  - Requirement: update `make lean-proof-track-check` to fail if any gap has:
    missing owner, missing blocker hash, stale timestamp, or unresolved `open`
    status when its covered manifest no longer marks the feature as removed.
  - Requirement: add a proof-gate metric in
    `build/artifacts/lean-proof-gate.json` for `gap_staleness_days` and
    `gap_classification_confidence` (0..1).
  - Requirement: every accepted gap closure must append a reversible changelog note
    to `docs/compiler/LEAN_PROOF_TRACK.md` with the restoration artifact hash
    and closure rationale.
  - Requirement: add `make lean-proof-track-gap-hygiene-check` that:
    - rejects duplicate feature coverage between gap rows and current proof rows
    - rejects gaps without executable follow-up plan
    - rejects gaps older than policy TTL without a new blocker_hash.
  - Gate: make `lean-proof-track-release-closeout-check` depend on
    `lean-proof-track-gap-hygiene-check` and enforce `0` unresolved `open` gaps.
  - Acceptance: all active 0.0.7 proof gaps are either closed with executable
    proof coverage or have explicit blockers and owner-approved exception windows.
  - Acceptance: release preflight fails if any high-priority feature slice is
    blocked by an unresolved `open` gap without an exception.

- [x] Slice 9: add semantic-end-to-end Lean proof smoke through VM/runtime and
  std contract surfaces.
  - Requirement: define a minimal executable proof smoke corpus in
    `proofs/lean/smoke/` for the critical chain:
    parser → typecheck/CoreIR → VM execution path → native boundary dispatch.
  - Requirement: each smoke item must be linked to a proof family and a 0.0.7
    feature slice in a machine-readable manifest.
  - Requirement: add `make lean-proof-smoke-check` that runs the smoke corpus
    through both proof and runtime smoke harnesses:
    - theorem/lemma discharge for each smoke property
    - VM execution equivalence check where the target is currently VM-owned
      feature
    - fallback rejection tests for unsupported forms.
  - Requirement: add a proof-to-runtime consistency check:
    - if a smoke proves a property, VM runtime behavior for that path must pass a
      corresponding integration test
    - if runtime diverges from proof assumptions, gate creates a blocker row.
  - Requirement: make `lean-proof-track-check` include a "smoke health" score for
    every lane in `lean-proof-lanes.json` and fail when below policy minimum.
  - Requirement: add compatibility mode that compares smoke behavior against
    previous 0.0.7 smoke signatures and reports drift as warning/failure.
  - Gate: require `lean-proof-smoke-check` before `lean-proof-track-release-closeout-check`
    and before promotion of any `current` feature to "release critical."
  - Gate: fail release preflight if runtime-vs-proof mismatch is not accompanied by
    a synchronized blocker and remediation plan.
  - Acceptance: all smoke families run locally and in CI with identical signatures.
  - Acceptance: proof and runtime mismatch appears only with explicit, reviewed
    exception artifacts and hard blockers.

- [x] Slice 10: generate executable feature-proof evidence matrix and reject any
  unbound 0.0.7 feature slice.
  - Requirement: create `proofs/lean/feature_binding/lean_feature_slice_index.json`
    with one record per 0.0.7 roadmap feature slice containing:
    - `slice_id`
    - `feature_class`
    - `proof_family_ids`
    - `gap_ids`
    - `lane_id`
    - `coverage_status`
    - `owner`
    - `blocker_hash`
  - Requirement: classify each 0.0.7 roadmap feature slice as:
    `proof_current`, `proof_in_progress`, `gap_exception`, or
    `slice_blocked`; unresolved states must include an accepted blocker reason and
    expiry.
  - Requirement: add `proofs/lean/feature_binding/` to lint checks that all
    active 0.0.7 slice IDs are present, no duplicated coverage rows exist, and
    each mapped proof family references at least one executable artifact or an
    explicit gap.
  - Requirement: extend `make lean-proof-track-check` with a matrix-closure phase
    that executes after lane execution and before release-closeout checks:
    - every active 0.0.7 feature slice must be represented in the matrix
    - every `proof_current` feature must have at least one passing proof family
      with successful replay
    - every `proof_in_progress` feature must have a blocker hash and planned
      fix gate in the same cycle
    - every `gap_exception` feature must have a valid owner and planned exception
      expiry
  - Requirement: emit `build/artifacts/lean-proof-coverage-matrix.tsv` and
    compare it against a committed baseline with no silent regression in
    `proof_to_gap_ratio`, `coverage_by_class`, or `lane_block_count`.
  - Requirement: any feature currently listed in `docs/roadmap/ROADMAP_0_0_7.md` but
    missing from matrix evidence must fail the closeout gate immediately.
  - Gate: add `make lean-proof-feature-binding-check` and wire it into
    `make lean-proof-track-release-closeout-check` and `make release-0-0-7-preflight`.
  - Gate: fail when proof artifact and gap manifests disagree on ownership for the
    same slice, or when a slice has both `proof_current` and an active `gap_exception`.
  - Acceptance: closeout emits one deterministic binding matrix row per active
    0.0.7 slice, with a one-to-one mapping to proof family families or accepted
    gaps.
  - Acceptance: feature coverage matrix check fails if any active slice has
    unresolved mapping and passes only when all matrix coverage dependencies can be
    explained by concrete artifacts, owners, and blockers.

- [x] Slice 11: make Lean proof closure auditable by machine-readable release
  artifacts and blocked-change diffs.
  - Requirement: add `proofs/lean/feature_binding/lean_feature_slice_index.json`
    diff output in `make lean-proof-feature-binding-check` that emits:
    `added_rows`, `removed_rows`, `coverage_status_delta`, `owner_delta`,
    `new_blockers`, and `resolved_blockers`.
  - Requirement: add a deterministic canonicalizer so ordering changes in JSON/TSV
    artifacts do not produce false diffs; sorting must be stable on
    `slice_id`, `feature_class`, `owner`, and `coverage_status`.
  - Requirement: persist evidence snapshots under
    `build/artifacts/lean-proof-snapshots/<YYYY-MM-DD>/` and compare against the
    last accepted snapshot when `make lean-proof-track-release-closeout-check` is
    run.
  - Requirement: define a blocking policy in
    `docs/runtime/lean-proof-block-policy.toml` with at least:
    `coverage_regression_tolerance`, `owner_change_window`,
    `max_new_blockers_per_cycle`, `unresolved_warning_ttl_days`.
  - Requirement: treat a snapshot diff as hard failure in closeout when any of the
    following increases without matching exception row:
    - unresolved blocked coverage count
    - proof-critical slice ownerless assignments
    - stale-to-current downgrades for core runtime feature-class slices
  - Requirement: add `make lean-proof-change-impact-report` that prints a short
    owner-attributed impact digest and links to affected roadmap slice IDs.
  - Requirement: integrate a reviewer command path: `make
    lean-proof-feature-binding-review` that is PR-safe and allows only warning
    mode unless a signed exception artifact is supplied.
  - Gate: require `lean-proof-feature-binding-check` output to include a signed
    blocker manifest (`blocker_hash`) whenever policy exceptions are used.
  - Gate: add `make lean-proof-snapshot-consistency-check` and fail release
    preflight when snapshot diffs are non-empty without a corresponding
    `lean-proof-feature-binding-review` run artifact.
  - Acceptance: a clean snapshot review flow can show slice-level impact in less
    than 30 seconds and produces a review artifact link for every owner delta.
  - Acceptance: closeout fails if a core slice transitions to a weaker state
    (`proof_current` to `proof_in_progress`) without an updated blocker hash and
    policy exception in the same run.

- [x] Slice 12: generate synthetic minimal counterexamples from failing proof
  obligations and feed them into VM/runtime regression tests.
  - Requirement: add a proof-to-regression bridge in
    `proofs/lean/counterexamples/` that extracts minimal counterexamples for
    each failed or stale theorem family.
  - Requirement: each generated counterexample file must include:
    `theorem_id`, `feature_class`, `reproduction_steps`, `minimal_terlan_ast`,
    `counterexample_lexeme`, and `expected_runtime_oracle`.
  - Requirement: add a converter that maps Lean counterexamples into VM runtime
    test fixtures and existing `terlc` integration test harnesses.
  - Requirement: add `make lean-proof-counterexample-check` that ensures:
    - every stale/incomplete/stopped proof family has at least one reproducer
      when classification is `soundness_gap` or `tooling_gap`
    - reproducibility evidence exists for each counterexample fixture
    - runtime oracles are failing for the reproducer path in a controlled
      benchmark namespace
  - Requirement: add a triage severity policy file `docs/runtime/lean-proof-counterexample-policy.toml`
    with priorities (`high`, `medium`, `low`), allowed rerun windows, and
    max unresolved counterexample backlog per lane.
  - Requirement: make `lean-proof-counterexample-check` produce
    `build/artifacts/lean-proof-counterexamples.json` with machine-readable
    severities and auto-assigned owner.
  - Requirement: wire `lean-proof-counterexample-check` before
    `lean-proof-track-release-closeout-check`; release closeout fails when any
    high-severity unresolved counterexample exists with no blocker plan.
  - Gate: integrate a hard policy guard so a feature cannot move from `proof_in
    _progress` to `proof_current` while unresolved high-severity counterexamples
    are present.
  - Acceptance: a stale or soundness-gap proof produces at least one runnable
    counterexample fixture before the next release cycle gate pass.
  - Acceptance: closeout includes a counterexample backlog scorecard by feature
    class and an owner-to-backlog delta table.

- [x] Slice 13 (post-AOT): replace the handwritten parser with LALRPOP and
  close the Lean proof gap at the parser/CoreIR boundary.
  - Dependency: do not begin this slice until Slices 100 and 101A through 101I
    are complete. LALRPOP adoption is not an AOT-pivot prerequisite.
  - Requirement: keep `docs/grammar/TERLAN_SYNTAX_SPEC.ebnf` canonical and
    derive or mechanically check the LALRPOP grammar against it; a second
    independently drifting language grammar is not permitted.
  - Requirement: feed the existing lexer token stream into LALRPOP, preserve
    source spans and stable diagnostics, and remove the handwritten parser only
    after same-corpus syntax, formatter, tree-sitter, and editor parity passes.
  - Requirement: minimize semantic actions. The generated parser constructs a
    versioned `SyntaxOutput`; validation and lowering into checked CoreIR remain
    explicit compiler phases rather than grammar-side semantic escape hatches.
  - Requirement: define `proofs/lean/parser_shape` over the stable boundary:
    canonical grammar identity, accepted/rejected syntax classifications,
    precedence and associativity, `SyntaxOutput` well-formedness, and
    preservation into checked CoreIR. Do not claim the generated Rust parser
    implementation itself is formally verified.
  - Requirement: prove shape and contract invariants in the typechecker/CoreIR
    domain, including tuples, records, lists, maps, bitstrings,
    `impl Contract[...]`, and capability-deny lists; parser proofs establish
    only the syntax facts required by those theorems.
  - Gate: add `make lalrpop-parser-parity-check` and
    `make lean-proof-parser-shape-check`, then include both in proof-track
    release closeout.
  - Acceptance: the full existing syntax corpus has stable acceptance,
    rejection, spans, diagnostics, formatting, and editor behavior; the
    handwritten parser is deleted; and executable Lean artifacts cover the
    documented syntax-to-CoreIR boundary.

- [x] Slice 14: close the 0.0.7 Lean proof gap for VM-native boundary
  dispatch and async-safe interop.
  - Requirement: create `proofs/lean/native_boundary` theorem family proving:
    resource-handle linearity/non-aliasing, arity and type consistency at
    boundary callsites, and async-policy compliance for typed boundary exports.
  - Requirement: prove `deny-list` effect soundness for boundary operations that
    involve potentially side-effectful host interactions (file, socket, timer,
    process spawn, process registry, ACME/TLS handoff).
  - Requirement: add rejection theorems for unsafe boundary usage patterns that
    rely on removed BEAM-only assumptions or untyped handles.
  - Requirement: require binding between theorem declarations and generated
    `NativeBoundary` manifests in `build/artifacts/native_boundaries/*.json`, with
    hash-level traceability from theorem family to manifest rows.
  - Requirement: add `make lean-proof-native-boundary-check` that runs the
    theorem bundle, validates manifest binding, and emits proof/runtime-oracle
    signatures.
  - Requirement: integrate this check with `lean-proof-smoke-check` so at least one
    native boundary path is executed both as a theorem property and as a VM runtime
    smoke path.
  - Requirement: gate lane sequencing so native-boundary proofs complete before
    `vm_http_critical` and DB/runtime lanes are promoted.
  - Gate: release preflight fails when a native-boundary proof family is
    `incomplete`/`stale` while the feature is marked release-critical or
    `proof_current`.
  - Acceptance: native boundary dispatch remains executable through VM semantics
    without legacy BEAM/NIF fallback in verified proof coverage.
  - Acceptance: runtime violations in native boundary calls fail in proof mode
    before runtime release, with explicit blockers captured for planned
    exceptions.

- [ ] Slice 15: formalize proof-visible semantics for template contracts and
  route binding under VM ownership.
  - Requirement: create `proofs/lean/templates_routes` theorem family for
    template contracts, route tuple forms, and shape-preserving handler
    dispatch.
  - Requirement: prove that typed route declarations (including path/value
    extraction forms) preserve shape, arity, and return-type contracts through
    VM handler lowering.
  - Requirement: add theorem coverage for template interpolation and template
    pattern typing (including `template` with record/shape/value slots and
    nested typed expressions).
  - Requirement: add explicit rejection theorems for invalid template/routing
    forms that can silently pass parser-level checks but violate VM/runtime
    guarantees.
  - Requirement: bind theorem families to route/template slice IDs in
    `proofs/lean/feature_binding/lean_feature_slice_index.json` and update
    `lean-proof-coverage-matrix.tsv` coverage rows.
  - Requirement: add `make lean-proof-templates-routes-check` that validates:
    theorem success, manifest binding, lane smoke alignment, and runtime
    handler-equivalence on at least one route-template integration fixture.
  - Requirement: enforce cross-lane dependency: template-route proofs must be
    available before feature slices in `web_templates` and `web_routing` move
    from `proof_in_progress` to `proof_current`.
  - Gate: require this check before `lean-proof-track-release-closeout-check` and
    before `terlan-vm-web-stack-check` enters release-critical mode.
  - Acceptance: a complete run produces executable theorem output and updates
    slice mapping for template/routing features with zero unresolved ownerless
    gaps.
  - Acceptance: any route/template regression that violates proof assumptions is
    caught by both proof and runtime lanes and blocked with an explicit blocker
    hash and remediation gate.

- [ ] Slice 16: align Lean proof semantics for concurrency, scheduling, and
  mailbox reliability.
  - Requirement: create `proofs/lean/concurrency` theorem set covering actor
    spawn, message send/receive, selective receive, cancellation, and process
    lifecycle semantics as modeled by VM mailboxes.
  - Requirement: prove scheduler/fairness lemmas needed by VM HTTP and distributed
    message-passing slices: message order under same-priority senders, bounded
    progress on runnable work queues, and termination conditions for selective
    receive with timeout.
  - Requirement: add formal rejection theorems for unsupported legacy
    concurrency assumptions (e.g., implicit OTP-style scheduling guarantees not
    modeled in VM runtime).
  - Requirement: connect theorem family with runtime fixtures under
    `tests/concurrency` and export `concurrency_smoke` artifacts consumed by
    `lean-proof-smoke-check`.
  - Requirement: add `make lean-proof-concurrency-check` that runs theorem
    execution, replay verification, and runtime-oracle cross-check for mailbox
    correctness.
  - Requirement: set explicit closeout policy: any stale/incomplete proof in
    `vm_core` concurrency class blocks lanes touching `vm_http_critical`, `terlan-vm-distributed`,
    or `terlan-vm-reliability`.
  - Gate: add `lean-proof-concurrency-check` to `lean-proof-track-release-closeout-check`
    and `make lean-proof-track-check` dependency chain with deterministic
    stop-on-failure semantics.
  - Acceptance: concurrency/actor/stream core slices move from open gap to
    executable proof or explicit exception with owner and blocker hash.
  - Acceptance: release preflight fails if concurrency theorem regressions appear
    without a matching updated concurrency blocker manifest.

- [ ] Slice 17: close Lean proof gaps for collection semantics (List/Map/Set)
  across mutability and ownership boundaries.
  - Requirement: add `proofs/lean/collections` theorem families for
    List/Map/Set core operations that are in VM ownership path:
    construction, iteration, mutation-by-copy/update, clone/borrow semantics,
    deletion/update boundaries, and hash/equality contracts.
  - Requirement: prove equivalence lemmas between abstract collection contracts and
    VM runtime observable behavior for map/list/set operations under persistent and
    mutable update models currently supported in 0.0.7.
  - Requirement: prove failure-soundness theorems for unsupported or removed
    collection behaviors (legacy BEAM-only semantics and non-deterministic iteration
    assumptions).
  - Requirement: add counterexample or witness artifacts for at least one
    benchmark-sensitive collection edge (e.g., hash collision path, sparse update,
    large-map threshold transitions).
  - Requirement: update 0.0.7 benchmark and std coverage slices with collection
    proof bindings in `lean_feature_slice_index.json` and lane matrices.
  - Requirement: add `make lean-proof-collections-check` and connect it to:
    - collection-focused VM benchmark smoke gates,
    - proof/runtime equivalence smoke in `lean-proof-smoke-check`,
    - collection-specific regression baselines.
  - Requirement: add policy rule in `docs/runtime/lean-proof-block-policy.toml`:
    collection-lane regressions are non-recoverable in closeout unless explicitly
    marked `planned_collection_replacement` with expiry and owner.
  - Gate: release closeout must fail if any `proof-critical` collection slice has
    `proof_in_progress` while benchmark drift evidence exceeds policy thresholds.
  - Acceptance: collection theorem families are executable and no longer require
    legacy `core-native-collections` placeholder entries.
  - Acceptance: collection-heavy benchmark or runtime tests stop producing silent
    proof/runtime divergence via the `lean-proof-collections-check` integration.

- [ ] Slice 18: formalize Wasm/VM boundary theorem obligations and prove portable
  export contracts.
  - Requirement: add `proofs/lean/wasm_bridge` theorem family covering value
    export/import boundaries for Terlan `std.wasm` contracts, ABI-compatible
    types, heap/resource ownership transfer, and call/return consistency.
  - Requirement: prove preservation lemmas for Wasm-exposed function signatures
    inferred from typed Terlan definitions and reject mismatched signatures at
    proof stage.
  - Requirement: require one theorem family proving deterministic ABI lowering for
    Wasm call/return/abort paths used by 0.0.7 runtime features.
  - Requirement: add explicit rejection theorems for host-side side-effectful
    invocations that are not represented as SafeNative contracts.
  - Requirement: update evidence matrix and slice mapping so `wasm` feature slices
    are covered by executable theorem families or explicit accepted gaps.
  - Requirement: add `make lean-proof-wasm-bridge-check` that runs theorem
    execution, validates manifest binding, and emits ABI signature digests.
  - Requirement: connect this to runtime lane smoke by introducing at least one
    end-to-end Wasm export fixture that is checked both by Lean and VM execution.
  - Requirement: extend release policy so any unresolved Wasm bridge proof gap in a
    release-critical lane blocks closeout unless exception artifacts and blockers
    are present.
  - Gate: require `lean-proof-wasm-bridge-check` before promoting Wasm slices from
    `proof_in_progress` to `proof_current` and include it in closeout dependency
    graph.
  - Acceptance: Wasm ABI signatures in proof artifacts and runtime manifests are
    bit-identical for covered features.
  - Acceptance: Wasm runtime regression tests fail fast when a proof-accepted export
    signature is violated.

- [ ] Slice 19: close the 0.0.7 Lean proof gap for database/postgres/sql
  contracts and ACID-style transaction semantics.
  - Requirement: add `proofs/lean/database_sql` theorem family proving typed
    SQL statement lifecycle correctness for PostgreSQL integration paths used in VM
    runtime: connection construction, prepared statement binding shape, parameter
    arity/type match, and result-set shape contracts.
  - Requirement: prove transaction boundary lemmas for begin/commit/rollback and
    statement isolation assumptions that matter at compile time (non-repeatable
    reads assumptions, side-effect boundaries, and retry safety conditions).
  - Requirement: prove effect-guard theorems for DB-bound operations that must be
    denied in pure code regions, and acceptance/rejection theorems for wrong
    side-effect contexts.
  - Requirement: map theorem families to std DB/API slices in
    `lean_feature_slice_index.json` and `lean-proof-coverage-matrix.tsv` and
    remove temporary gap rows currently used to represent DB proof debt.
  - Requirement: add `make lean-proof-db-sql-check` that executes theorem families,
    validates sql-shape manifests, and writes runtime oracle signatures for
    DB-safe calls.
  - Requirement: add a DB smoke fixture bridge for at least one query/mutation path
    in VM runtime that is checked by both theorem discharge and integration-level
    runtime smoke tests.
  - Requirement: add policy in proof block policy file: unresolved DB proof gaps can
    only be in exception mode for a bounded maintenance window.
  - Gate: make `lean-proof-db-sql-check` a hard dependency of `lean-proof-track-release-closeout-check`
    when feature slices `db_runtime`, `terlan-vm-db`, or `native_boundary`
    are marked release critical.
  - Acceptance: DB/sql runtime errors arising from proof-invalid shapes are
    rejected in proof stage before runtime execution.
  - Acceptance: release preflight blocks promotion of DB slices while any
    `proof_in_progress` DB theorem remains unimplemented and unblocked.

- [ ] Slice 20: close Lean proof gaps for std package ecosystem contracts
  (JSON/Serialization/Text/Errors/Logging/Timers).
  - Requirement: add theorem families in `proofs/lean/std_packages` for core std
    contracts that define data-shape guarantees and failure behavior: JSON encode/
    decode, binary/string conversion, assertion/option/result/error typing, and
    timer/event contracts.
  - Requirement: prove that std package public functions used in 0.0.7 slices have
    declared type-level preconditions and postconditions that are preserved by VM
    execution.
  - Requirement: add rejection theorems for invalid std edge cases where previous
    contracts relied on runtime panics instead of typed failure values.
  - Requirement: add `make lean-proof-std-package-check` and connect it to
    `lean-proof-track-check` as a mandatory standard-library proof lane before
    release closeout.
  - Requirement: require each std contract proof family to emit manifest row links to:
    `docs/compiler/COMPILER_STANDARD_LIBRARY.md`, `proofs/lean/inventory.tsv`, and
    the relevant 0.0.7 feature slice IDs.
  - Requirement: add std-package regression fixtures under `tests/std` that use
    theorem-driven witnesses for at least one valid and one invalid path.
  - Requirement: add policy rule: unresolved std-contract gaps in proof-critical
    slices cannot be marked `proof_current` without blocker and owner.
  - Gate: run `lean-proof-std-package-check` before `terlan-vm-http-lane-check` and
    `lean-proof-track-release-closeout-check` when any standard-package feature is
    release critical.
  - Acceptance: std package slices lose placeholder artifacts and have
    executable theorem-backed coverage rows.
  - Acceptance: runtime behavior for std contracts matches proof assumptions in
    both local and CI regression artifacts.

- [ ] Slice 22: add end-to-end proof traces from `terlc` compile pipeline
  through Lean coverage for all release-critical 0.0.7 slices.
  - Requirement: add tracing hooks in `terlc` test/compile entrypoints to emit
    `terlanc` → CoreIR → proof artifact candidate IDs for every parsed module.
  - Requirement: build a deterministic mapping from compile artifacts to
    `proofs/lean/inventory.tsv` rows (one compile event ↔ one proof scope) so
    proof drift can be reproduced from compile logs.
  - Requirement: add `make lean-proof-compile-trace-check` that validates trace
    records include proof scope coverage for every 0.0.7 release-critical slice and
    fail when coverage is missing.
  - Requirement: extend smoke and lane reports with per-slice “compiled-without-
    proof” warnings that become blockers in closeout mode.
  - Requirement: persist compiled trace artifacts under
    `build/artifacts/compile-proof-trace/<YYYY-MM-DD>.json` and keep a rolling
    30-day history for regression comparisons.
  - Requirement: add regression rule: two consecutive runs with identical compiled
    shape must not reduce associated proof coverage status.
  - Gate: include compile-trace evidence in `lean-proof-track-check`; if a
    release-critical compile unit has no executable proof linkage, closeout fails.
  - Acceptance: local and CI compile commands generate identical proof-scope
    traces for unchanged inputs.
  - Acceptance: release preflight fails if any release-critical compile unit is
    missing traceable proof coverage.

- [ ] Slice 23: harden proof-trace artifacts with deterministic schema and
  baseline diffing.
  - Requirement: add a stable JSON schema for compile-trace records with
    explicit versioning and field-level compatibility checks (`format_version`,
    `slice_id`, `module_id`, `proof_ids`, `compiler_stage`, `input_hash`,
    `timestamp`) so trace consumers can hard fail malformed artifacts.
  - Requirement: canonicalize artifact ordering by `module_id`, then `slice_id`,
    then `compiler_stage` before diffing and hashing, and record SHA-256 digests
    in a manifest.
  - Requirement: add mutation tests that intentionally corrupt trace fields (e.g.
    missing `proof_ids`, stale `input_hash`, duplicated events) and require the
    gate to reject the build.
  - Requirement: archive a `build/artifacts/compile-proof-trace/Baseline.json`
    against clean input and make regression checks compare each run against it
    with allowed tolerance for timestamp-only fields.
  - Requirement: add `make lean-proof-compile-trace-baseline-check` that verifies
    manifest completeness, schema compatibility, deterministic ordering, and
    non-decreasing trace coverage under stable inputs.
  - Requirement: wire the baseline diff check into closeout mode to preserve a
    historical, non-adversarial proof-trace chain for every 0.0.7 candidate.
  - Gate: include `lean-proof-compile-trace-baseline-check` in the final
    release preflight when `lean-proof-compile-trace-check` is active.
  - Acceptance: changing unrelated code requires trace delta changes only inside
    explicitly classified slice-owned fields, and any unclassified trace drift is
    blocked.
  - Acceptance: all baseline regressions include a generated diff summary with
    trace IDs and failing slice ids, and no release can pass with baseline
    violations.

- [ ] Slice 24: correlate compile proof traces with VM/runtime and std-test gates.
  - Requirement: enrich each `build/artifacts/compile-proof-trace/*.json` record
    with the active runtime lane (`vm`, `vm-core`, `vm-http`) and active test lane
    (`unit`, `integration`, `stdlib-release`) so proof linkage can be cross-checked
    against runtime and std-test execution.
  - Requirement: add `make lean-proof-trace-runtime-correlation-check` that fails
    when any release-critical slice has proof trace records but no matching
    runtime-lane or std-test-lane execution record in the same run.
  - Requirement: add a single source-of-truth `proof-trace-index.tsv` consumed by
    `all-terlan-tests-vm-check`, `stdlib-release-tests-vm-default-check`, and
    `vm-coverage-100-check` so each gate can assert expected proof trace alignment.
  - Requirement: add adversarial cases where a slice has proof trace coverage but
    stale/nonmatching lane execution, and ensure the correlation gate blocks closeout.
  - Requirement: add reporting in smoke output summarizing uncorrelated slices:
    `slice_id`, `missing_runtime_lane`, `missing_std_lane`, `expected_proof_ids`.
  - Requirement: require deterministic correlation under rerun: same source + same
    feature flags must produce identical correlated slice mapping (ignoring timing
    and output-path fields).
  - Gate: require `lean-proof-trace-runtime-correlation-check` during release
    closeout and make it a hard dependency of `release-0-0-7-preflight`.
  - Acceptance: release preflight cannot pass if any release-critical slice lacks
    lane-correlation to both VM runtime execution and std release tests, unless
    explicitly exempted in legacy migration allowlists.
  - Acceptance: correlation reports are machine-readable and can be used to identify
    which execution lane is responsible for any proof-trace gap.

- [ ] Slice 25: make proof traces executable in a replayable, minimal regression
  corpus.
  - Requirement: define a minimal curated trace corpus under
    `build/artifacts/compile-proof-trace/replay/` with one canonical minimal
    project per release-critical slice, plus one intentionally failing adversarial
    fixture per slice.
  - Requirement: add `make lean-proof-trace-replay-check` that re-runs only the
    minimal corpus and validates trace IDs, proof IDs, and lane correlation against
    authoritative index artifacts.
  - Requirement: ensure replay mode is deterministic across machines by
    pinning compiler hash, feature set, feature flags, and `target` in trace
    metadata; reruns with identical settings must produce byte-identical ordered
    trace artifacts after timestamp normalization.
  - Requirement: add a mutation-mode test where a replay fixture is modified to
    miss a required proof ID or map to an excluded lane and require the replay
    gate to fail.
  - Requirement: make replay failures produce concise machine-readable
    diagnostics: `slice_id`, `expected_trace_id`, `observed_trace_id`,
    `replay_cause`.
  - Requirement: wire replay checks into existing closeout gates so each slice
    proves reproducibility in addition to baseline and correlation coverage.
  - Gate: add replay gating to `release-0-0-7-preflight` as a hard prerequisite
    once replay corpus is populated, and make `make all-terlan-tests-vm-check`
    include replay-trace generation.
  - Acceptance: a release-critical slice cannot pass closeout if its replay fixture
    cannot be re-executed to the same proof-trace identity.
  - Acceptance: replay artifacts include per-slice elapsed, hash, and proof-id
    summaries suitable for trend and cacheability checks.

- [ ] Slice 26: make proof trace and coverage artifacts first-class local
  release evidence with traceability and deterministic cleanup policy.
  - Progress: `vm-coverage-classification-check` now requires every VM coverage
    debt row to name executable evidence (`make ...` or `cargo test ...`) and
    classifies the current VM runtime inventory: 25 promoted files and 26
    explicitly classified unpromoted files.
  - Requirement: generate a local release evidence bundle that includes:
    `proof-trace-index.tsv`, `proof-trace-baseline.json`,
    `proof-trace-replay-summary.json`, and one manifest for each compiled slice.
  - Requirement: add machine-validated provenance fields (`git_sha`, `rustc_version`,
    `compiler_target`, `target_profile`, `feature_set`, `build_id`) to every proof
    and coverage artifact used in closeout.
  - Requirement: define deterministic local cleanup and versioning policy in
    docs and tooling. Closeout must not depend on an upload, tag, remote branch,
    hosted retention period, or external account.
  - Requirement: add `make release-artifacts-closeout-check` that fails when
    required artifacts are missing, malformed, stale, or missing provenance fields.
  - Requirement: add integrity checks for artifact drift where only tolerated drift
    is `elapsed` and non-semantic path fields; all semantic drift requires explicit
    blocker with changed slice ownership.
  - Requirement: expose a one-command audit:
    `make proof-coverage-release-artifacts-smoke` that prints artifact status,
    hash lineage, and unresolved provenance gaps.
  - Requirement: include adversarial tests for dropped provenance fields and
    manipulated manifest linkage to ensure integrity gate blocks release.
  - Gate: add `make release-artifacts-closeout-check` as a hard dependency of
    `release-0-0-7-preflight` after trace/replay/correlation checks.
  - Acceptance: release closeout must generate a complete local evidence bundle
    and provenance metadata; release cannot pass with unresolved artifact-policy
    violations.
  - Acceptance: artifact audit output includes per-slice provenance diffs and slice
    ownership attribution for any unexpected semantic delta.

- [ ] Slice 27: add machine-checkable release readiness dashboards for proof+runtime
  gates.
  - Requirement: add a single `make proof-readiness-matrix` command that composes
    `lean-proof-track`, trace, runtime, coverage, and std-test gate outcomes into one
    tabular artifact with strict schema (`slice_id`, `status`, `blocker`, `last_seen`,
    `owner`, `proof_link`, `runtime_link`, `test_link`).
  - Requirement: require that every release-critical slice has at least one evidence
    row in the matrix even when a dependent gate is skipped, and mark explicit
    `SKIP_WITH_JUSTIFICATION` only when listed in release waiver inventory.
  - Requirement: add `make proof-readiness-matrix-check` that ensures matrix rows are
    complete, deterministic, non-empty, and free of placeholder statuses.
  - Requirement: add adversarial tests where matrix is malformed (`missing owner`,
    duplicated `slice_id`, inconsistent statuses) and confirm the check rejects it.
  - Requirement: integrate a nightly trend artifact under
    `build/artifacts/proof-readiness-trend.jsonl` and fail closeout on sustained
    regressions across consecutive runs.
  - Requirement: emit dashboard summary from `make roadmap-visibility-check` with:
    percent complete, blockers by family, unresolved legacy references, and top 5
    largest unresolved proof deltas.
  - Requirement: require that `make proof-readiness-matrix-check` runs from both
    `make check` and `make release-0-0-7-preflight`.
  - Gate: include matrix checks in closeout ordering after proof-artifact gates and
    before final release artifact freeze.
  - Acceptance: release preflight cannot pass without a green matrix row for each
    release-critical slice or a documented, bounded waiver.
  - Acceptance: matrix artifacts must be machine-readable and include provenance from
    Slice 26 in-band.

- [ ] Slice 28: close the 0.0.7 proof readiness loop with release mode parity and
  deterministic replay scheduling.
  - Requirement: add a `--release-readiness-mode` option to readiness matrix generation
    that runs the full gate chain in deterministic order and records scheduling hashes
    for each step.
  - Requirement: enforce that release mode includes a locked feature matrix snapshot and
    rejects command-line feature drift (`--features` not in lockfile) during proof/trace
    and coverage checks.
  - Requirement: add `make proof-readiness-release-mode-check` that verifies readiness
    artifacts were produced under release-readiness-mode and that both local and CI
    command signatures match.
  - Requirement: add deterministic replay sequencing for closeout gates using a fixed
    stage order, explicit dependencies, and machine-auditable timing windows to avoid
    flaky acceptance from scheduling differences.
  - Requirement: add an adversarial fixture that reorders gate execution and confirm
    the check fails with a clear replay-order drift report.
  - Requirement: include scheduler artifacts (`gate_graph.dot`, `gate_execution.log`,
    `scheduler-fingerprint.json`) for forensic comparison in release tickets.
  - Gate: require `make proof-readiness-release-mode-check` in both `make check` and
    `make release-0-0-7-preflight` when any proof/trace/cov gate is executable.
  - Acceptance: closeout fails if gate order, feature set, or scheduling fingerprint
    differs between local and CI artifact runs without waiver.
  - Acceptance: release mode outputs one canonical fingerprint per run to compare against
    the previous release-candidate artifact lineage.

### VM HTTP Production And Benchmarking

- [ ] Slice 29: standardize HTTP benchmark comparability and fix concurrency
  non-linearity effects before handler claims.
  - Requirement: add `benches/http/PROFILE.toml` with fixed request schedules
    (route mix, payload sizes, keep-alive policy, warmup, concurrency ramp,
    malformed/large-header cases, and cancellation/backpressure scenarios) as a
    canonical shared profile between VM, Hyper, and Axum baselines.
  - Requirement: extend all HTTP benchmark commands to emit the same metric surface:
    p50/p95/p99 latency, mean/median latency, throughput req/sec,
    error rate, and memory overhead per request, with explicit warmup and fixed
    total request counts.
  - Requirement: require same parser and TLS code path for VM and comparison stacks
    so handler execution is measured as the shared control plane plus transport
    overhead, not parser mismatch.
  - Requirement: add a replay fixture harness that replays the exact request sequence
    and validates statistical stability across three runs before merge.
  - Requirement: add adversarial checks for malformed headers, slow-clients,
    and route-variant bursts that must not cause benchmark crashes or invalid
    throughput inflation.
  - Requirement: create `vm-http-comparability-report.json` artifacts with
    machine-verifiable deltas against baseline and require explicit justification
    for any Axum/Hyper/Hyper-plain regressions exceeding threshold.
  - Gate: add `make vm-http-benchmark-comparability-check` and run it from `make check`
    and `make release-0-0-7-preflight` after existing `vm-http-concurrency-investigation-check`
    when the VM HTTP stack is present.
  - Acceptance: benchmark comparison fails if p99 latency or throughput regresses by more
    than 15% without waiver, when concurrency scaling is non-monotonic against
    expected profile, or when profile/schema mismatch occurs.
  - Acceptance: reports must include reproducibility confidence interval and
    fixed-load schedule fingerprint so comparisons are not affected by execution noise.

- [ ] Slice 32: close VM-owned ACME/TLS production readiness for `terlc serve`.
  - Requirement: move ACME issuance, renewal scheduling, certificate cache reads/writes,
    and TLS handoff behind a VM-owned NativeBoundary worker contract with typed
    lifecycle states (`pending`, `issued`, `cached`, `renewing`, `failed`, `expired`).
  - Requirement: use maintained Rust crates for ACME and TLS (`instant-acme`,
    `rustls`, and certificate parsing helpers) and forbid hand-rolled certificate,
    challenge, or TLS protocol parsing.
  - Requirement: support deterministic local ACME validation fixtures for CI and an
    opt-in live staging profile for manual release validation; default gates must not
    depend on external network access.
  - Requirement: support HTTP-01 challenge routing through the VM HTTP router without
    bypassing route/middleware visibility, request logging, backpressure, or shutdown
    accounting.
  - Requirement: persist certificate-cache metadata with domain, issuer, not-before,
    not-after, renewal deadline, key algorithm, cache path, and provenance hash.
  - Requirement: add adversarial tests for expired certificates, corrupt cache files,
    mismatched domains, failed challenge response, renewal race, missing permissions,
    and shutdown during issuance.
  - Requirement: add typed diagnostics for ACME/TLS failures so users never see raw
    crate errors or transport panics from production `terlc serve`.
  - Gate: add `make vm-http-acme-tls-production-check` and run it after
    `vm-http-soak-stability-check` in `make check`; the live staging profile remains
    opt-in but must be documented by the release preflight artifact.
  - Acceptance: production TLS startup must either serve with a valid cached/issued
    certificate or fail with a typed diagnostic naming the domain, cache state, and
    challenge state.
  - Acceptance: the gate fails if ACME/TLS paths use host-runtime async assumptions,
    bypass VM HTTP routing, skip cache provenance validation, or leave certificate
    resources live after shutdown.

- [ ] Slice 33: add HTTP/2 and HTTP/3 protocol readiness without surrendering VM
  stream ownership.
  - Requirement: define the protocol layering contract for HTTP/1.1, HTTP/2, and
    HTTP/3: maintained Rust crates own wire parsing/framing, while the Terlan VM owns
    stream lifecycle, handler scheduling, cancellation, backpressure, flow-control
    accounting, timeout policy, and resource cleanup.
  - Requirement: evaluate and pin maintained crates for HTTP/2 and HTTP/3 candidates
    (`h2`/Hyper ecosystem for HTTP/2; `h3`, `quinn`, or equivalent maintained QUIC
    stack for HTTP/3) with a skip manifest for unsupported protocol features.
  - Requirement: add ALPN negotiation coverage through the rustls-backed VM TLS path
    so `h2` and `http/1.1` selection is observable, typed, and reproducible.
  - Requirement: add a VM stream adapter prototype for multiplexed request handling
    that maps protocol stream ids to VM-owned request processes without sharing
    mutable handler state.
  - Requirement: add adversarial protocol fixtures for stream reset, flow-control
    exhaustion, header-list limits, slow body frames, duplicate pseudo-headers,
    oversized trailers, cancellation during response write, and connection shutdown
    while streams are active.
  - Requirement: add benchmark rows for HTTP/1.1 keep-alive, HTTP/2 multiplexed
    streams, and HTTP/3 datagram/stream startup overhead using the same synthetic
    handlers from Slice 30.
  - Requirement: persist `vm-http-protocol-readiness-report.json` with per-protocol
    support status, skipped feature rationale, parser crate/version, ALPN result,
    stream count, failure mode, and VM cleanup evidence.
  - Gate: add `make vm-http-protocol-readiness-check` and run it after
    `vm-http-acme-tls-production-check`; release closeout may allow HTTP/3 as
    `experimental` only with explicit skip-manifest rows.
  - Acceptance: the gate fails if protocol handling bypasses VM scheduling,
    flow-control/backpressure is untracked, ALPN is ambiguous, or unsupported
    protocol features are silently accepted.
  - Acceptance: HTTP/2/HTTP/3 readiness claims require typed negative tests and
    cleanup proof for every active stream after cancellation or connection close.

- [ ] Slice 34: make `terlc serve` production configuration typed, validated, and
  reproducible.
  - Requirement: define a typed serve configuration schema covering bind address, port,
    protocol set, TLS mode, ACME mode, certificate cache, request/body/header limits,
    timeouts, backpressure thresholds, handler pool sizing, static asset roots,
    logging, telemetry, and shutdown policy.
  - Requirement: support deterministic precedence between package config, CLI flags,
    environment variables, and profile defaults; emit the final effective config as
    `build/artifacts/serve-effective-config.json`.
  - Requirement: reject invalid combinations before opening sockets, including TLS
    without certificates/ACME, HTTP/3 without QUIC support, negative or zero limits,
    conflicting bind addresses, unsafe public dev defaults, and missing asset roots.
  - Requirement: add typed diagnostics for every rejected configuration with stable
    diagnostic IDs, source spans when available, and remediation text.
  - Requirement: add adversarial fixtures for malformed TOML/YAML, duplicated keys,
    unknown fields, conflicting env overrides, invalid ports, unreadable cache paths,
    unsupported protocol combinations, and oversized limits.
  - Requirement: ensure serve config participates in the benchmark, soak, ACME/TLS,
    and protocol-readiness gates so every runtime artifact records the exact config
    fingerprint used for execution.
  - Gate: add `make vm-http-serve-config-check` and run it before
    `vm-http-soak-stability-check`, `vm-http-acme-tls-production-check`, and
    `vm-http-protocol-readiness-check`.
  - Acceptance: production serve cannot start with ambiguous, partially parsed, or
    silently ignored configuration.
  - Acceptance: the gate fails if two equivalent config inputs produce different
    effective-config fingerprints, or if invalid configs reach the socket/listener
    startup path.

- [ ] Slice 35: add VM-owned production observability for HTTP and runtime services.
  - Requirement: define a stable observability event schema for VM process lifecycle,
    socket lifecycle, request lifecycle, handler execution, scheduler decisions,
    backpressure, cancellation, NativeBoundary calls, TLS/ACME states, and shutdown.
  - Requirement: emit structured logs with stable event ids, severity, process id,
    request id, connection id, route id, config fingerprint, and source span when
    available; forbid ad hoc production `println`/debug output on the serve path.
  - Requirement: expose metrics counters and histograms for request count, latency,
    queue depth, scheduler wakeups, handler failures, TLS handshakes, ACME renewals,
    protocol selection, resource leaks, and backpressure/cancellation outcomes.
  - Requirement: add trace/span propagation from VM accept through route match,
    middleware, handler, response write, and resource cleanup without depending on a
    host runtime tracing model.
  - Requirement: provide machine-readable artifacts:
    `vm-runtime-observability-events.jsonl`,
    `vm-runtime-observability-metrics.json`, and
    `vm-runtime-observability-traces.json`, all tied to the effective serve config.
  - Requirement: add adversarial tests for malformed trace contexts, log burst limits,
    metric overflow, dropped events under backpressure, shutdown during trace flush,
    and NativeBoundary failures with partial telemetry.
  - Requirement: make benchmark, soak, ACME/TLS, protocol-readiness, and debugger gates
    consume the same observability schema rather than private per-gate formats.
  - Gate: add `make vm-runtime-observability-check` and run it after
    `vm-http-serve-config-check` and before HTTP soak/TLS/protocol gates.
  - Acceptance: production serve paths must produce structured observability data for
    every accepted request and every typed failure without leaking raw crate errors.
  - Acceptance: the gate fails if required events are missing, event ids drift without
    migration metadata, metrics are inconsistent with trace counts, or production
    paths rely on ad hoc text output.

- [ ] Slice 36: add a VM runtime inspector TUI backed by production observability.
  - Requirement: add `terlc inspect` as a reserved command that attaches to a running
    VM or replays observability artifacts without requiring debugger breakpoints.
  - Requirement: build the terminal UI with maintained Rust terminal libraries
    (`ratatui`/`crossterm` or equivalent) and avoid custom terminal rendering.
  - Requirement: provide inspector panels for process list, scheduler queues,
    mailbox sizes, HTTP connections, active requests, route latency, resource handles,
    NativeBoundary calls, TLS/ACME state, and recent typed failures.
  - Requirement: support read-only navigation, filtering, sorting, pause/resume,
    snapshot export, and deterministic replay from
    `vm-runtime-observability-events.jsonl`.
  - Requirement: keep inspector data flow VM-owned: the TUI consumes structured
    observability snapshots and must not read private runtime internals directly.
  - Requirement: add adversarial tests for large process lists, rapidly changing
    metrics, terminal resize, closed stdout/stderr, malformed replay artifacts,
    missing observability fields, and attach target disappearance.
  - Requirement: ensure inspector output is accessible in non-interactive CI by
    supporting `--snapshot json` and `--snapshot text` modes.
  - Gate: add `make vm-runtime-inspector-check` and run it after
    `vm-runtime-observability-check` and before `terlc-debugger-check`.
  - Acceptance: inspector views must be derived entirely from the stable observability
    schema and fail with typed diagnostics when schema versions drift.
  - Acceptance: the gate fails if interactive behavior is the only tested path, if
    terminal rendering panics on resize/malformed input, or if inspector commands
    depend on legacy runtime concepts.

### VM Runtime Primitives

- [ ] Slice 36: implement OTP-style service abstractions in Terlan over VM
  primitives.
  - Requirement: high-level OTP-inspired abstractions such as GenServer,
    Supervisor policy wrappers, Task orchestration, Agent-like state cells,
    persistent actors, and typed service loops must be implemented as Terlan
    stdlib modules wherever possible, not as bespoke VMIR opcodes or direct
    Rust-only framework implementations.
  - Requirement: the Rust VM owns only hard runtime primitives: process spawn,
    process identity, mailbox send/receive, selective receive, timers,
    cancellation, links, monitors, supervision mechanics, resource ownership,
    scheduler accounting, and NativeBoundary parking/resume.
  - Requirement: `std.vm.GenServer` must move from a native/magic operation
    surface to a Terlan implementation compiled to VMIR on top of lower-level
    `std.vm.Process`, `std.vm.Message`, `std.vm.Timeout`, link/monitor, and
    supervision primitives. Any remaining `vm.gen_server.*` intrinsic must be
    justified as a thin primitive wrapper, not as the GenServer framework
    itself.
  - Requirement: port relevant OTP `gen_server` behavioral cases into
    Terlan-owned tests: init success/failure, synchronous call/reply,
    asynchronous cast, state transition ordering, stop/terminate, timeout,
    crash propagation, monitor/link interaction, stale reply handling, mailbox
    pressure, and supervised restart behavior.
  - Requirement: add a quality gate that rejects new std.vm framework modules
    whose public behavior is implemented only through `native` or VMIR
    framework intrinsics when the behavior can be written in Terlan over
    lower-level VM primitives.
  - Gate: `make vm-otp-abstractions-terlan-stdlib-check` and run it before
    `terlan-vm-erl-suite-audit-check`, `vm-supervision-restart-check`, and
    final release readiness.
  - Acceptance: release cannot pass if GenServer remains a compiler-admitted
    source contract without executable Terlan stdlib semantics and VM-owned
    primitive support underneath it.
  - Acceptance: the gate fails if OTP-style framework reliability is validated
    only by Rust unit tests, VMIR intrinsic admission, or target-profile
    allowlists without executable Terlan stdlib behavior.

- [ ] Slice 37: implement VM-owned supervision trees and restart semantics.
  - Requirement: define typed supervision specs for worker processes, HTTP listener
    processes, handler pools, NativeBoundary workers, stream workers, and runtime
    service processes.
  - Requirement: support restart strategies (`one_for_one`, `one_for_all`,
    `rest_for_one`, `temporary`, `transient`, `permanent`) with VM-native naming if
    final syntax differs, but preserve equivalent semantics.
  - Requirement: define restart intensity, backoff, escalation, terminal failure,
    shutdown ordering, child replacement, and supervisor death behavior without
    relying on host runtime supervision.
  - Requirement: integrate links, monitors, process exits, debugger restart choices,
    observability events, runtime inspector views, and HTTP serve lifecycle with the
    same supervision state machine.
  - Requirement: add adversarial tests for crash loops, child start failure, supervisor
    failure, shutdown timeout, NativeBoundary worker crash, handler pool exhaustion,
    cascading failure, and restart during in-flight request cancellation.
  - Requirement: persist `vm-supervision-report.json` with supervision graph,
    restart history, failure reasons, escalation decisions, and final process state.
  - Gate: `make vm-supervision-restart-check` and run it before
    `vm-http-soak-stability-check`, `vm-runtime-inspector-check`, and
    `terlc-debugger-check`.
  - Gate: `make vm-otp-abstractions-terlan-stdlib-check` must pass and must fail if
    direct runtime magic for OTP-style framework abstractions is added instead of
    keeping those abstractions in Terlan stdlib over VM primitives.
  - Acceptance: every supervised failure must produce a typed restart or terminal
    failure outcome with observable graph state and no leaked process/resource handles.
  - Acceptance: the gate fails if restart behavior is nondeterministic, if restart
    intensity is not enforced, or if process exits bypass supervisor accounting.

- [ ] Slice 38: add supervised process state checkpoint and recovery contracts.
  - Requirement: define opt-in process state checkpoint hooks for supervised actors,
    HTTP stateful handlers, NativeBoundary workers, and long-running runtime services.
  - Requirement: separate recoverable state from non-recoverable runtime resources:
    sockets, TLS sessions, file handles, NativeBoundary handles, and in-flight request
    bodies must be represented by typed recovery outcomes rather than serialized raw
    handles.
  - Requirement: support deterministic checkpoint metadata with process id, supervisor
    id, state schema version, sequence number, timestamp, config fingerprint, checksum,
    and owning package/module.
  - Requirement: define restore behavior for restart strategies from Slice 37:
    fresh restart, restore from latest checkpoint, restore with migration, reject stale
    checkpoint, or terminal failure.
  - Requirement: add checkpoint migration hooks with typed version negotiation and
    stable diagnostics for missing, incompatible, or failed migrations.
  - Requirement: add adversarial tests for corrupt checkpoint payloads, checksum
    mismatch, stale sequence, wrong owner, missing schema, failed migration, resource
    handle restore attempts, and crash during checkpoint write.
  - Requirement: persist `vm-process-state-recovery-report.json` with checkpoint
    lifecycle events, restore decisions, rejected resources, migration result, and
    final supervised process state.
  - Gate: add `make vm-process-state-recovery-check` and run it after
    `vm-supervision-restart-check` and before distributed state/replication gates.
  - Acceptance: supervised recovery must never silently reuse stale state or raw
    runtime handles; every rejected checkpoint must produce a typed diagnostic.
  - Acceptance: the gate fails if checkpoint/restore is nondeterministic, if migration
    errors are swallowed, or if restored state violates the process type contract.

- [ ] Slice 40: implement VM scheduler fairness, reductions, and preemption
  accounting.
  - Focused execution plan: follow
    [`ROADMAP_0_0_7_MULTICORE_VM.md`](ROADMAP_0_0_7_MULTICORE_VM.md) for the
    ordered multicore decomposition only after the complete AOT roadmap and its
    closeout pass. That mini-roadmap is inactive before AOT completion, does not
    replace this item, and does not permit benchmark-only host threads to count
    as parallel actor execution.
  - Requirement: define a VM-owned reduction/tick budget for Terlan function calls,
    message receives, pattern matching, collection operations, HTTP handlers,
    NativeBoundary parking, timer delivery, and response writes.
  - Requirement: add preemption points for long-running pure functions, recursive
    functions, comprehensions, stream processing, route handlers, and collection
    traversals without changing Terlan semantics.
  - Requirement: define scheduler queues, priority/normal/background classes,
    starvation bounds, wakeup ordering, parked-process handling, and run-queue
    telemetry in a deterministic replayable form.
  - Requirement: integrate scheduler accounting with HTTP benchmark attribution,
    timer storms, supervision restarts, debugger stepping, runtime inspector views,
    and observability metrics.
  - Requirement: add adversarial tests for CPU-bound actors, recursive loops, mailbox
    floods, timer floods, slow NativeBoundary completion, many parked processes,
    HTTP handler fanout, and cancellation during a preemption point.
  - Requirement: persist `vm-scheduler-fairness-report.json` with per-process
    reductions, preemption count, wait time, runnable duration, starvation warnings,
    queue transitions, and benchmark correlation ids.
  - Gate: add `make vm-scheduler-fairness-check` and run it before
    `vm-http-concurrency-investigation-check`, `vm-timer-deadline-check`, and
    `vm-supervision-restart-check`.
  - Acceptance: no runnable process may starve beyond the configured bound under
    adversarial load unless explicitly parked or cancelled with a typed reason.
  - Acceptance: the gate fails if reduction accounting is missing for a VM execution
    path, if preemption changes result semantics, or if replay produces different
    scheduler ordering under the same seed/input.

### VM NativeBoundary And Database Runtime

- [ ] Slice 44: add typed SQL macro validation and row-shape contracts.
  - Requirement: implement `sql!` macro validation through maintained Rust SQL
    parsing/validation crates plus live Postgres Docker validation where semantic
    database checks are required; do not hand-roll SQL parsing.
  - Requirement: infer query kind (`select`, `insert`, `update`, `delete`, DDL),
    parameter count/types, row shape, nullability, cardinality (`one`, `many`,
    `execute`), and transaction requirements from the validated SQL and Terlan type
    context.
  - Requirement: bind SQL macro outputs to typed Terlan row descriptors, structs,
    tuples, `Option`, `Result`, and collection shapes without requiring noisy
    user-side `query_one` or manual pool arguments when those are inferable.
  - Requirement: validate migration/schema compatibility for SQL macros by comparing
    query expectations against the current migration snapshot and Docker-backed
    Postgres schema fixture.
  - Requirement: emit stable diagnostics for malformed SQL, missing parameters,
    ambiguous column names, unsafe dynamic SQL, unknown tables/columns, nullability
    mismatch, cardinality mismatch, and row-decode mismatch.
  - Requirement: add adversarial tests for SQL injection-shaped interpolation,
    parameter reordering, duplicate aliases, unsupported Postgres features, stale
    migration snapshot, schema drift, LIMIT inference edge cases, and invalid
    transaction nesting.
  - Requirement: persist `vm-sql-macro-validation-report.json` with parser crate,
    schema fingerprint, migration snapshot id, inferred cardinality, inferred row
    shape, validation mode, and diagnostic coverage.
  - Gate: add `make vm-sql-macro-validation-check` and run it before
    `vm-postgres-runtime-check` and `lean-proof-db-sql-check`.
  - Acceptance: `sql!` must either lower to a typed database operation with a proven
    row/parameter contract or fail before runtime with a stable diagnostic.
  - Acceptance: the gate fails if SQL parsing is hand-rolled, if schema validation is
    skipped for live-query fixtures, or if row decoding can fail without a typed
    compile-time/runtime contract boundary.

- [ ] Slice 45: add VM-aware database migration commands and schema snapshots.
  - Progress (2026-07-18): implemented deterministic `terlc db snapshot` export and
    `--check` drift validation through the VM-owned libpq command client, including
    migration/schema SHA-256 fingerprints, catalog-owned relation/column/constraint/
    index/enum metadata, internal migration-table exclusion, corruption tests, and a
    Docker migration/snapshot lifecycle test. The live Postgres lifecycle passes
    migration rebuild/replay, deterministic snapshot export, unchanged `--check`, and
    deliberate schema-drift rejection. Keep this slice open for the remaining
    lock/report/safety requirements below.
  - Progress (2026-07-18): migration commands now own one transaction-scoped,
    nonblocking advisory lock for the entire command, reload and revalidate applied
    history after acquiring that lock, and commit every pending migration together
    with its history row as one atomic unit. Stable lock-conflict, lock-protocol, and
    concurrent-history-divergence diagnostics prevent stale plans or indefinite lock
    waits. Adversarial coverage proves exact-history skipping, divergent-history
    rejection, lock contention, rollback of partially executed multi-statement
    migrations, and lock release after failure. `make db-command-check` passes 132
    non-live tests across the CLI, VM Postgres adapter, and source evaluator; its
    three Docker-only lifecycle tests remain intentionally ignored in the default
    gate, and direct Docker execution is unavailable in this sandbox because access
    to the Docker socket is denied. Report persistence and the remaining protected
    rebuild integrations keep the parent slice open.
  - Progress (2026-07-18): added the canonical
    `make vm-db-migration-command-check` composition ahead of SQL macro validation.
    It runs the maintained DB/VM/source-Postgres gate, five warning-denied report
    contract and adversarial tests, and atomically writes the versioned, deterministic,
    redacted `target/quality/vm-db-migration-report.json`, registered as
    `terlan.vm-db-migration-command.v1` with the release report-schema validator. The
    report records three canonical migration fixture IDs with real SHA-256 checksums,
    input digests, lock
    behavior, success/conflict/divergence/rollback outcomes, rebuild safety evidence,
    diagnostic coverage, SQL-macro ordering, generation policy, and a release-blocking
    decision without credentials or SQL text. The composed gate passes 132 runtime
    tests plus 5 report-contract tests. The static report marks `schema_fingerprint`
    as `docker-live-gate-required` rather than fabricating database evidence, so live
    schema identity keeps this slice open.
  - Progress (2026-07-18): completed the destructive rebuild/reset safety boundary.
    Both commands now require independent `--dev` and `--confirm` flags, admit only
    explicit loopback hosts, never infer safety from `dev`/`test`/`local` database
    names, and reject strict TLS or certificate URL options before migration discovery
    or socket work. Adversarial coverage includes missing confirmation, duplicate and
    misplaced confirmation flags, remote development-named targets, ten protected
    transport option forms, and the accepted loopback path. The canonical
    `make vm-db-migration-command-check` passes 98 DB tests, 29 VM Postgres tests,
    9 source-Postgres tests, and 6 report-contract tests; its deterministic report now
    records 7 diagnostics plus explicit confirmation/local-target/protected-transport
    safety decisions. Docker-only tests remain intentionally ignored here.
  - Progress (2026-07-19): completed out-of-order migration-history detection.
    `terlc db status` now distinguishes an ordinary pending tail from a local gap
    before a later compatible applied migration, renders that gap as `out-of-order`,
    and counts it separately. Initial execution planning and history revalidation
    after the transaction-scoped advisory lock both fail with the shared stable
    `error[db.migration.out_of_order]` diagnostic, so a `1, 3` applied history can
    never cause local migration `2` to run after `3`. Positive and adversarial tests
    cover status classification, execution admission, concurrent post-lock history
    changes, summary accounting, and report-anchor removal. The canonical
    `make vm-db-migration-command-check` passes 101 DB tests, 29 VM Postgres tests,
    9 source-Postgres tests, and 7 report-contract tests; the deterministic report
    records 8 diagnostic families and the out-of-order command outcome. The parent
    remains open for its Docker-backed schema identity and remaining integration
    requirements.
  - Progress (2026-07-19): completed typed initial migration-history identity
    diagnostics. Status and execution planning now distinguish a history row whose
    local file is absent (`missing` / `error[db.migration.file_missing]`), whose
    checksum changed (`checksum-mismatch` /
    `error[db.migration.checksum_mismatch]`), or whose recorded name changed
    (`name-mismatch` / `error[db.migration.name_mismatch]`). The generic
    `error[db.migration.history_divergent]` remains reserved for compatible history
    that changes after initial planning while the command acquires the database
    lock. Positive tests cover every state, label, planner rejection, and summary
    bucket; the report gate includes an adversarial missing-anchor test. The
    warning-denied canonical `make vm-db-migration-command-check` passes 103 DB
    tests, 29 VM Postgres tests, 9 source-Postgres tests, and 8 report-contract
    tests, with three Docker-only tests ignored. Its deterministic report records
    11 diagnostic families and contract fingerprint
    `acccc2e183519a00b88a62f85b513c6ba94a523bed7edc94063ecb9204992039`.
    The parent remains open for Docker-backed schema identity and remaining
    integration requirements.
  - Progress (2026-07-19): completed the typed duplicate migration-id
    diagnostic. Pure filename inventory and filesystem discovery now share one
    production-owned formatter and report the conflicting timestamp through
    `error[db.migration.duplicate_id]`; no untyped duplicate-timestamp fallback
    remains. Existing adversarial duplicate-file tests cover both paths, and the
    release report gate now fails when the diagnostic contract is removed. The
    warning-denied canonical `make vm-db-migration-command-check` passes 103 DB
    tests, 29 VM Postgres tests, 9 source-Postgres tests, and 9 report-contract
    tests, with three Docker-only tests ignored. Its deterministic report records
    12 diagnostic families and contract fingerprint
    `afa90ea13cc65655d467a7c970f9ac7360277fdf2ac3bbda6bc1116159ba2217`.
    The parent remains open for Docker-backed schema identity and remaining
    integration requirements.
  - Progress (2026-07-19): completed the typed failed-migration execution
    diagnostic. Both migration-body execution and parameterized history insertion
    now fail through `error[db.migration.failed]`, identify the migration ID, and
    preserve the already-redacted VM Postgres diagnostic without including source
    SQL, checksums, database URLs, or credentials. Command setup, lock, and
    connection failures retain their narrower existing diagnostics. Unit coverage
    proves migration identity and request-data redaction, while the report gate
    fails when the typed failure anchor is removed. The warning-denied canonical
    `make vm-db-migration-command-check` passes 104 DB tests, 29 VM Postgres tests,
    9 source-Postgres tests, and 10 report-contract tests, with three Docker-only
    tests ignored. Its deterministic report records 13 diagnostic families and
    contract fingerprint
    `4fc5c250accaec53224faef44ded24cf30c2e1891cd23ff62075c505d4643084`.
    The parent remains open for Docker-backed schema identity and remaining
    integration requirements.
  - Progress (2026-07-19): completed typed dirty-schema detection without
    conflating it with stale snapshot metadata. `terlc db snapshot --check` now
    emits `error[db.schema.dirty]` when the migration snapshot identity is
    unchanged but the live schema fingerprint differs, and emits the distinct
    `error[db.snapshot.drift]` when migration identity changed. Tests exercise
    both branches, fingerprint sensitivity, corruption rejection, and SQL
    redaction; the report gate fails when the dirty-schema contract is removed.
    The warning-denied canonical `make vm-db-migration-command-check` passes 105
    DB tests, 29 VM Postgres tests, 9 source-Postgres tests, and 11
    report-contract tests, with three Docker-only tests ignored. Its deterministic
    report records 14 diagnostic families and contract fingerprint
    `4dda07b061c7bdba19de1c73792db6bc4e5f10aae62020c13fb026e1726bf660`.
    The parent remains open for Docker-backed schema identity and remaining
    integration requirements.
  - Progress (2026-07-19): completed the corrupted schema-snapshot adversarial
    contract. Malformed snapshot JSON and forged schema fingerprints now share
    `error[db.snapshot.corrupt]` while retaining the concrete integrity reason;
    they cannot be treated as ordinary drift or accepted as a new `--check`
    baseline. Unsupported snapshot versions and filesystem read failures remain
    separate failure classes. Tests cover malformed content and fingerprint
    forgery, and the report gate fails when the corruption diagnostic anchor is
    removed. The warning-denied canonical `make vm-db-migration-command-check`
    passes 105 DB tests, 29 VM Postgres tests, 9 source-Postgres tests, and 12
    report-contract tests, with three Docker-only tests ignored. Its deterministic
    report records 15 diagnostic families and contract fingerprint
    `114791c2c25cef0f7db79ae548cc287e795a03ee1dc76634e6040a2557474110`.
    The parent remains open for Docker-backed schema identity and remaining
    integration requirements.
  - Progress (2026-07-19): completed schema-snapshot contract admission.
    Snapshot checks now require the exact supported schema version and PostgreSQL
    database-product identity before fingerprint or drift evaluation, with both
    rejected through `error[db.snapshot.unsupported_contract]`. Adversarial tests
    cover unsupported schema versions and database products, and the report gate
    fails when the diagnostic anchor is removed. The warning-denied canonical
    `make vm-db-migration-command-check` passes 106 DB tests, 29 VM Postgres
    tests, 9 source-Postgres tests, and 13 report-contract tests, with three
    Docker-only tests ignored. Its deterministic report records 16 diagnostic
    families and contract fingerprint
    `4e8d54feeeb1db7b1014af97d3a19fefd8c29303690c86e9df7ad7348aa8b55b`.
    The parent remains open for Docker-backed schema identity and remaining
    integration requirements.
  - Progress (2026-07-19): completed applied-migration timestamp ownership.
    History reads now select `applied_at` from PostgreSQL as canonical RFC 3339
    UTC with microsecond precision, validate the value at the VM decode boundary,
    retain it in typed applied-history metadata, and expose it in status rows while
    pending rows use `-`. Non-UTC offsets, malformed timestamps, and the
    noncanonical `+00:00` spelling are rejected; migration identity, checksums, and
    schema snapshots deliberately exclude wall-clock metadata. The deterministic
    report records this storage/read/status policy without recording timestamp
    values and fails when UTC normalization is removed. The canonical
    `make vm-db-migration-command-check` passes 108 DB tests, 29 VM Postgres tests,
    9 source-Postgres tests, and 14 report-contract tests, with three Docker-only
    tests ignored by the default gate. The exact Docker migration/snapshot lifecycle
    additionally passes against live PostgreSQL, including the new timestamp decode
    path. The report contract fingerprint is
    `c7620715dfd7b1888b83549957ae2205459b80b3d4241b064ba28ed65ea635cf`.
    The parent remains open for dependency orchestration, SQL validation, and the
    remaining integration requirements.
  - Requirement: implement `terlc db migrate`, `terlc db status`, and
    `terlc db rebuild --dev --confirm` using maintained Rust migration machinery and
    the VM Postgres runtime contract; do not implement a migration engine from scratch.
  - Requirement: require migration files to support consolidated multi-statement
    migrations with deterministic ordering, stable ids, checksums, applied-at
    metadata, and schema snapshot export.
  - Requirement: make `rebuild --dev --confirm` refuse non-local database URLs,
    active production certificates, or unresolved migration locks; database naming
    must never weaken target admission.
  - Requirement: integrate migration state with Docker dependency readiness,
    SQL macro validation, Postgres runtime resources, serve config, observability,
    debugger diagnostics, and release preflight artifacts.
  - Requirement: add typed diagnostics for dirty schema, failed migration, checksum
    mismatch, duplicate migration id, missing migration file, out-of-order migration,
    lock conflict, destructive rebuild rejection, and schema snapshot drift.
  - Requirement: add adversarial tests for partially applied migration, crash during
    migration, concurrent migrate/status, rebuild against protected config, corrupted
    schema snapshot, missing Docker dependency, and stale SQL macro schema cache.
  - Requirement: persist `vm-db-migration-report.json` with migration ids, checksums,
    schema fingerprint, lock behavior, command outcome, rebuild safety decision, and
    SQL macro snapshot compatibility.
  - Gate: add `make vm-db-migration-command-check` and run it before
    `vm-sql-macro-validation-check` and `vm-postgres-runtime-check`.
  - Acceptance: every migration command must be replayable against a Docker-managed
    local Postgres fixture and produce the same schema fingerprint.
  - Acceptance: the gate fails if `rebuild --dev --confirm` can touch non-local or
    protected config, if
    migration state can drift silently, or if SQL macro validation consumes stale
    schema metadata.

- [ ] Slice 46: add Docker-aware dev dependency orchestration for VM commands.
  - Progress (2026-07-19): moved maintained Docker Compose parsing, strict
    Postgres service validation, project discovery, and healthcheck startup out
    of `serve` into the shared `commands::dev_dependencies` boundary. `terlc
    serve` and loopback `db migrate`, `db status`, `db snapshot`, `db rebuild
    --dev --confirm`, and `db reset --dev --confirm` now use the same typed
    `docker-compose-types` plus Serde YAML path. Remote database commands never
    start local dependencies. Typed missing-Docker, startup-failure, and
    readiness-failure diagnostics are covered alongside malformed Compose,
    unsafe port, environment, and healthcheck adversarial cases. Failed
    readiness now collects the last 200 Compose service-log lines, removes
    sensitive and control-bearing lines, caps the rendered excerpt at 4,096
    characters, and preserves the primary failure when log collection is
    unavailable. Dependency sessions probe before startup, preserve external
    containers, and stop/remove only containers created for the command scope;
    database and serve callers retain that session for their full runtime use.
    `make vm-dev-dependency-orchestration-check` passes 24 warning-denied runtime
    tests and 6 deterministic report-contract tests, writes the redacted
    versioned `vm-dev-dependency-report.json`, and validates five typed
    diagnostics with contract fingerprint
    `8d64e81f9e389065f6aa774d5e2de530cd99f9d78446d2d48c224d345b6ce519`.
    The gate remains an enforced prerequisite of the DB migration gate. The
    slice remains open for multi-service graphs and the remaining command
    integrations.
  - Requirement: teach `terlc` to discover declared local dependencies for
    `serve`, `test`, `db migrate`, `db status`, `db rebuild --dev`, SQL validation,
    and package integration tests before executing runtime commands.
  - Requirement: parse Docker Compose/YAML through maintained Rust or cloud-native
    typed parsers and validate against a strict Terlan dependency schema; do not
    hand-roll YAML or Compose parsing.
  - Requirement: support dependency lifecycle operations: plan, start, health-check,
    wait-ready, reuse-running, collect logs, stop-owned, and preserve-external.
  - Requirement: emit typed diagnostics for missing Docker, unsupported Compose
    fields, invalid ports, image pull failure, service health timeout, port conflict,
    missing volume, missing environment variable, and readiness probe mismatch.
  - Requirement: integrate dependency orchestration with serve config, Postgres
    runtime, database migrations, SQL macro validation, benchmark fixtures, and
    release preflight artifacts.
  - Requirement: add adversarial tests for invalid compose files, duplicate service
    names, dependency cycles, failing health checks, slow startup, port collisions,
    unowned running containers, stale volumes, and missing Docker daemon.
  - Requirement: persist `vm-dev-dependency-report.json` with discovered services,
    compose fingerprint, ownership mode, readiness events, started/stopped resources,
    and command-to-dependency mapping.
  - Gate: add `make vm-dev-dependency-orchestration-check` and run it before
    `vm-db-migration-command-check`, `vm-sql-macro-validation-check`, and
    `vm-postgres-runtime-check`.
  - Acceptance: VM commands that require local services must either start validated
    dependencies deterministically or fail before runtime with typed diagnostics.
  - Acceptance: the gate fails if dependency parsing is ad hoc, if unmanaged services
    are stopped, if readiness is inferred from sleeps, or if command artifacts omit the
    dependency fingerprint.

### Release Artifact And Local Validation Hardening

- [ ] Slice 51: validate installed example projects against the release
  artifact.
  - Requirement: define a curated example matrix that runs only from an
    installed release candidate: hello world, collections/map usage,
    pattern-matching constructors, table/property tests, VM HTTP handler,
    typed templates, Angular facade smoke, WASM export smoke, CLI debugger
    smoke, and package import smoke.
  - Requirement: every example must be generated or copied into a clean
    temporary workspace, must use the installed `terlc` on PATH, and must avoid
    source-checkout imports, absolute workspace paths, or hidden build
    artifacts.
  - Requirement: validate init templates produce `.gitignore`, package config,
    source/test layout, formatter-compatible code, runnable main/test commands,
    and docs links that match the release documentation artifact.
  - Requirement: include negative examples for missing imports, wrong module
    path, unsupported syntax, missing package, stale std symbol, and VM runtime
    capability errors so release diagnostics stay readable for new users.
  - Requirement: add adversarial tests for examples that pass only because of
    workspace leakage, examples that rely on stale generated files, examples
    that skip VM execution, docs snippets that do not compile, and templates
    that omit release-required files.
  - Requirement: persist `release-example-projects-report.json` with example
    names, generated paths, commands run, installed compiler path, stdout/stderr
    summaries, diagnostics snapshots, and cleanup status.
  - Gate: add `make release-example-projects-check` and run it after
    `release-promotion-pipeline-check` and before final release readiness.
  - Acceptance: release cannot pass if any first-party example, README snippet,
    init template, or documented quickstart succeeds only from the source
    checkout or fails under the installed release candidate.
  - Acceptance: the gate fails if any example bypasses the VM default runtime,
    if diagnostics regress to internal implementation errors, or if generated
    projects are not reproducible across two clean workspaces.

- [ ] Slice 52: generate a release diagnostic catalog with stable text and JSON
  contracts.
  - Requirement: collect every public compiler, formatter, lint, package, VM,
    HTTP, database, template, debugger, release, and installer diagnostic into a
    generated catalog with stable IDs, severity, category, short text, long help,
    remediation, JSON schema, and docs URL.
  - Requirement: prove `--diagnostic-format text` and
    `--diagnostic-format json` expose the same diagnostic identity, source span,
    related notes, remediation text, and docs link for every cataloged
    diagnostic.
  - Requirement: connect negative examples, README snippets, std tests, package
    tests, editor diagnostics, LSP diagnostics, and release validation failures
    to catalog IDs so user-facing failures never fall back to internal runtime
    errors or uncategorized strings.
  - Requirement: reserve ID ranges for language syntax/type errors, stdlib
    contract failures, VM runtime failures, NativeBoundary failures, web/runtime
    failures, package/release failures, lint findings, and migration/deprecation
    messages.
  - Requirement: add adversarial tests for duplicate IDs, undocumented IDs,
    mismatched text/JSON fields, missing docs links, diagnostics without source
    spans where spans are available, raw internal errors, unstable wording in
    snapshot fixtures, and editor/CLI catalog drift.
  - Requirement: persist `release-diagnostic-catalog-report.json` with diagnostic
    counts by category, undocumented ID list, text/JSON parity status,
    source-span coverage, editor/LSP parity coverage, and snapshot hash.
  - Gate: add `make release-diagnostic-catalog-check` and run it after
    `release-example-projects-check` and before final release readiness.
  - Acceptance: release cannot pass if any public failure path emits an
    uncataloged diagnostic, if text and JSON disagree, or if docs links point to
    missing generated pages.
  - Acceptance: the gate fails if a new diagnostic appears without an explicit
    stability policy, reserved ID range, adversarial fixture, and generated docs
    entry.

- [ ] Slice 53: enforce public compatibility baselines for release surfaces.
  - Requirement: generate a versioned public surface manifest for language
    syntax, std modules, std functions, types, constructors, shapes, formatter
    output classes, lint rule IDs, CLI commands, diagnostic IDs, package
    manifests, editor command IDs, LSP capabilities, VM commands, and release
    artifact layout.
  - Requirement: compare the current release candidate against the previous
    versioned baseline and classify every difference as additive, compatible
    behavioral tightening, documented breaking change, deprecated surface, or
    private implementation detail.
  - Requirement: require a migration note, diagnostic/codemod plan, docs link,
    and example update for every accepted breaking change, including syntax
    removals, std API removals, command flag changes, package layout changes,
    and runtime behavior changes.
  - Requirement: prove removed or deprecated surfaces fail with cataloged
    diagnostics that point to replacement forms, not parser ambiguity, internal
    VM errors, or missing-symbol crashes.
  - Requirement: add adversarial tests for unclassified public-surface drift,
    accidental std export removal, formatter output drift without migration
    approval, CLI flag rename without docs, stale editor commands, missing
    deprecation diagnostics, and private items leaking into the public manifest.
  - Requirement: persist `release-compatibility-baseline-report.json` with
    manifest hashes, diff summary, classification counts, accepted breaking
    changes, migration coverage, and rejected unclassified changes.
  - Gate: add `make release-compatibility-baseline-check` and run it after
    `release-diagnostic-catalog-check` and before final release readiness.
  - Acceptance: release cannot pass if any public surface changes without an
    explicit compatibility classification and generated release-note entry.
  - Acceptance: the gate fails if a breaking change lacks replacement
    diagnostics, migration documentation, example updates, and editor/LSP
    coverage where the old surface was user-visible.

- [ ] Slice 54: validate release supply-chain provenance and dependency policy.
  - Requirement: generate a release SBOM for the compiler, VM runtime, stdlib,
    editor packages, generated docs, native packages, benchmark harnesses, and
    installer artifacts with crate/package versions, source revisions, licenses,
    checksums, and build inputs.
  - Requirement: validate the dependency policy for maintained Rust crates,
    approved native libraries, generated bindings, no hand-rolled protocol
    parsers where maintained crates are required, and no unreviewed network,
    crypto, database, HTTP, TLS, or parser implementation entering the release.
  - Requirement: classify every `unsafe` Rust block, FFI boundary, generated C++
    binding, CUDA/native package hook, and NativeBoundary resource handle with
    owner, justification, tests, and release risk status.
  - Requirement: prove release artifacts are reproducible from the sealed
    candidate manifest inputs and that checksum/signature files match the
    staged artifact set.
  - Requirement: add adversarial tests for undeclared dependencies, license drift,
    checksum mismatch, stale lockfile, unclassified unsafe code, native library
    path leakage, generated binding drift, vendored code without provenance, and
    release artifacts that include files outside the manifest.
  - Requirement: persist `release-supply-chain-provenance-report.json` with SBOM
    path, dependency counts, license summary, unsafe inventory, generated binding
    hashes, native artifact hashes, and policy violations.
  - Gate: add `make release-supply-chain-provenance-check` and run it after
    `release-compatibility-baseline-check` and before final release readiness.
  - Acceptance: release cannot pass if any shipped artifact lacks provenance,
    license classification, checksum coverage, or dependency-policy approval.
  - Acceptance: the gate fails if release validation discovers unclassified
    unsafe code, unmanaged native artifacts, undeclared generated bindings, or
    hand-rolled infrastructure where the roadmap requires maintained tooling.

- [ ] Slice 55: enforce release security hardening for VM, web, packages, and
  NativeBoundary.
  - Requirement: define the 0.0.7 security model for VM process isolation,
    resource handles, NativeBoundary capability checks, filesystem access,
    environment access, network listeners, package execution, generated
    bindings, TLS/ACME storage, and release installer behavior.
  - Requirement: add default-deny tests for unrequested filesystem paths,
    undeclared environment variables, unauthorized network binds, invalid native
    resource handles, cross-process handle reuse, stale package capabilities,
    and package code attempting privileged release actions.
  - Requirement: harden web/runtime inputs with limits and diagnostics for large
    headers, malformed headers, oversized bodies, slow clients, path traversal,
    invalid TLS state, unsafe redirect targets, and untrusted static asset
    paths.
  - Requirement: connect security failures to the release diagnostic catalog with
    stable IDs, remediation, source/config spans where available, and no raw
    internal errors.
  - Requirement: add adversarial tests for bypassing capability checks through
    aliases, generated bindings, package imports, hot reload, debugger commands,
    template asset paths, HTTP routing, database handles, and installer upgrade
    paths.
  - Requirement: persist `release-security-hardening-report.json` with security
    control coverage, denied-operation counts, web input fuzz summary,
    NativeBoundary capability coverage, and unresolved risk entries.
  - Gate: add `make release-security-hardening-check` and run it after
    `release-supply-chain-provenance-check` and before final release readiness.
  - Acceptance: release cannot pass if any security-sensitive operation succeeds
    without an explicit capability, declared resource, validated input, or
    cataloged failure mode.
  - Acceptance: the gate fails if privileged behavior depends on ambient process
    state, source checkout paths, host environment leakage, or undocumented
    runtime defaults.

- [ ] Slice 56: generate reproducible release support bundles for failures.
  - Requirement: add a `terlc support bundle` flow that captures compiler
    version, VM version, release-candidate provenance, target platform, package
    manifest, stdlib hash, editor/LSP version, diagnostic catalog version, and
    relevant command invocations without exposing secrets by default.
  - Requirement: support bundles must include structured diagnostics, minimized
    source snippets where permitted, VM runtime snapshot metadata, build/test
    timing summaries, package resolution traces, HTTP/runtime configuration
    summaries, and release artifact checksums.
  - Requirement: redact environment variables, credentials, TLS keys, database
    URLs, tokens, home-directory paths where configured, and user-marked secret
    fields while preserving enough shape to debug the failure.
  - Requirement: allow CI gates and local commands to attach a support bundle on
    failure with deterministic filenames and without requiring network access.
  - Requirement: add adversarial tests for secret leakage, missing provenance,
    nondeterministic bundle output, source-checkout path leakage, unsupported
    platform metadata, partial command failure, malformed diagnostics, and
    bundles generated from installed artifacts instead of workspace binaries.
  - Requirement: persist `release-support-bundle-report.json` with fixture
    bundle paths, redaction coverage, provenance coverage, deterministic hash
    checks, and failure-mode coverage.
  - Gate: add `make release-support-bundle-check` and run it after
    `release-security-hardening-check` and before final release readiness.
  - Acceptance: release cannot pass if support bundles omit release provenance,
    leak secrets in adversarial fixtures, or require source checkout paths to
    explain installed-artifact failures.
  - Acceptance: the gate fails if any major release gate cannot emit a
    deterministic support bundle on failure.

- [ ] Slice 57: enforce release performance baselines and regression reports.
  - Requirement: define a release benchmark manifest for compiler startup,
    parse/typecheck/lower/build paths, VM cold start, VM warm execution,
    collections/maps, scheduler/mailbox primitives, HTTP socket handlers,
    template rendering, package resolution, and installed example projects.
  - Requirement: benchmark runs must record fixed total request/work counts,
    warmup phase, p50/p95/p99 latency, throughput, memory high-water marks,
    allocation counters where available, CPU time, VM tick/reduction counters,
    and host metadata.
  - Requirement: compare current release-candidate results against checked-in
    0.0.7 baselines with configurable thresholds, explicit allowed-regression
    files, and human-readable explanations for accepted regressions.
  - Requirement: benchmark artifacts must distinguish synthetic microbenchmarks,
    realistic end-to-end benchmarks, external framework comparisons, and
    non-comparable diagnostic probes so marketing-style claims cannot be inferred
    from the wrong rows.
  - Requirement: add adversarial tests for benchmark rows without correctness
    assertions, missing warmup, variable request counts, hidden source-checkout
    binaries, unstable host metadata, non-comparable rows labeled as comparable,
    and accepted regressions without owner/explanation.
  - Requirement: persist `release-performance-baseline-report.json` with
    benchmark manifest, host metadata, baseline hashes, regression summary,
    accepted-regression list, and links to raw JSON/CSV benchmark output.
  - Gate: add `make release-performance-baseline-check` and run it after
    `release-support-bundle-check` and before final release readiness.
  - Acceptance: release cannot pass if core compiler, VM, HTTP, collection, or
    package paths regress beyond policy without an explicit accepted-regression
    record.
  - Acceptance: the gate fails if benchmark outputs are not reproducible enough
    to classify, if correctness assertions are missing, or if comparable and
    non-comparable baselines are mixed.

- [ ] Slice 58: produce a release-readiness attestation for the sealed
  candidate.
  - Requirement: generate one `release-readiness-attestation.json` that names
    the sealed release candidate, release version, git revision, target triples,
    artifact manifest, docs manifest, editor manifest, stdlib hash, VM hash,
    package hashes, benchmark baseline, and every required gate report.
  - Requirement: the attestation must prove all release gates consumed the same
    candidate manifest and did not rebuild, regenerate, or mutate artifacts after
    the candidate was sealed.
  - Requirement: include machine-readable pass/fail state for release artifacts,
    docs, editor packages, examples, diagnostics, compatibility, supply chain,
    security, support bundles, performance, coverage, lint, formatter, tests,
    and VM runtime checks.
  - Requirement: support local dry-run and CI modes with identical attestation
    schema, deterministic ordering, stable timestamps or normalized timestamp
    fields, and explicit skipped-host entries.
  - Requirement: add adversarial tests for missing reports, mismatched candidate
    hashes, stale gate output, pass status with failed subreports, non-deterministic
    attestation output, post-seal artifact mutation, and release notes that do not
    reference the attested candidate.
  - Requirement: persist `release-readiness-attestation-report.json` with
    attestation path, report coverage, candidate hash, skipped-host matrix,
    failure injection coverage, and deterministic hash comparison.
  - Gate: add `make release-readiness-attestation-check` and run it after
    `release-performance-baseline-check` and before final local readiness.
  - Acceptance: release cannot pass if there is no single attestation tying all
    release gates and artifacts to the same sealed candidate.
  - Acceptance: the gate fails if a report is missing, stale, generated from a
    different candidate, or claims success while a required subgate failed.

- [ ] Slice 59: verify staged release surfaces and rollback behavior.
  - Requirement: define a staged-distribution verification plan that checks the
    installer script, release archives, checksums, static docs, editor packages,
    package indexes, release notes, and attestation record all match the sealed
    release candidate.
  - Requirement: operate entirely against an offline local mirror without
    uploads, hosted services, external accounts, or publication credentials.
  - Requirement: verify a clean install from the staged surface reports the
    correct `terlc --version`, uses the VM default runtime, resolves stdlib/docs
    from the installed layout, runs the release example matrix, and rejects stale
    or mismatched artifacts.
  - Requirement: define rollback steps for a partial staging operation, bad checksum,
    stale installer metadata, broken docs deployment, missing editor package,
    package-index drift, and staged artifact mismatch.
  - Requirement: add adversarial tests for partial mirror contents, stale
    installer pointing at an older release, checksum mismatch, missing
    attestation, docs/version mismatch, package index mismatch, and rollback
    leaving mixed-version metadata.
  - Requirement: persist `release-staged-distribution-verification-report.json`
    with checked mirror paths, artifact hashes, install smoke results,
    rollback dry-run results, and mismatch diagnostics.
  - Gate: add `make release-staged-distribution-verification-check` and run it
    after `release-readiness-attestation-check`.
  - Acceptance: release cannot be considered complete until staged artifacts
    match the attested candidate and a clean install succeeds from the staged
    surface.
  - Acceptance: the gate fails if rollback cannot be dry-run safely, if
    staged verification relies on source checkout paths, or if the staged
    install path can resolve mixed-version artifacts.

- [ ] Slice 60: generate factual release notes and migration guide from gate
  reports.
  - Requirement: generate 0.0.7 release notes from compatibility baselines,
    diagnostic catalog changes, std/API manifests, release examples, benchmark
    reports, security/provenance reports, known limitations, and the sealed
    release attestation.
  - Requirement: every release-note claim must link to an installed example,
    generated docs page, public API manifest entry, benchmark report row,
    diagnostic catalog entry, or accepted compatibility-change record.
  - Requirement: generate a migration guide for breaking or deprecated surfaces
    with old form, new form, compiler diagnostic ID, codemod/lint availability,
    examples, and editor/LSP behavior.
  - Requirement: clearly separate shipped features, experimental features,
    planned-but-unavailable features, removed legacy surfaces, known limitations,
    and performance claims.
  - Requirement: add adversarial tests for unsupported claims, missing migration
    records, stale examples, benchmark claims without comparable rows, docs links
    that do not exist, diagnostics missing from the catalog, and release notes
    generated from a different candidate.
  - Requirement: persist `release-notes-accuracy-report.json` with claim counts,
    evidence links, unsupported-claim list, migration coverage, known-limitation
    coverage, and release-candidate hash.
  - Gate: add `make release-notes-accuracy-check` and run it after
    `release-staged-distribution-verification-check` and before final release
    readiness.
  - Acceptance: release cannot pass if release notes claim support for anything
    not proven by a gate, installed example, generated API reference, or
    attested artifact.
  - Acceptance: the gate fails if any breaking change lacks migration guidance
    or if performance, security, package, editor, docs, or VM claims lack
    machine-readable evidence.

### Release Validation And Adversarial Corpus

- [ ] Slice 65: validate project upgrade and migration behavior across release
  baselines.
  - Requirement: maintain a fixture matrix of representative projects from prior
    Terlan baselines, current templates, web projects, package projects,
    std-heavy projects, editor-generated projects, and intentionally stale
    projects.
  - Requirement: run each fixture with the installed 0.0.7 candidate and classify
    the outcome as direct success, success after formatter/lint migration,
    success after explicit codemod, expected diagnostic with migration guidance,
    or intentionally unsupported legacy behavior.
  - Requirement: prove project migration commands never mutate user files without
    an explicit write flag, produce previews/diffs for proposed changes, and keep
    backups or recovery instructions when writes are enabled.
  - Requirement: validate package manifests, module layout, test naming,
    generated `.gitignore`, Makefile policy, VM default runtime, editor metadata,
    and std imports across upgraded projects.
  - Requirement: add adversarial tests for stale package manifests, removed std
    symbols, old target profile fields, stale generated files, bad module paths,
    hidden source-checkout imports, migration without backup, and diagnostics
    that do not point to a supported replacement.
  - Requirement: persist `release-project-upgrade-matrix-report.json` with
    fixture names, baseline version, commands run, migration classification,
    changed-file previews, diagnostics, and installed compiler path.
  - Gate: add `make release-project-upgrade-matrix-check` and run it after
    `release-ci-local-parity-check` and before final release readiness.
  - Acceptance: release cannot pass if representative existing projects fail
    without a cataloged diagnostic and migration path, or if migrations silently
    mutate user code.
  - Acceptance: the gate fails if upgraded projects only work from source
    checkout paths, bypass the VM default runtime, or rely on stale generated
    artifacts.

- [ ] Slice 66: validate a nontrivial reference application suite on the VM
  default runtime.
  - Requirement: define a release reference-app suite separate from tiny examples
    and template smoke tests: battleship, a multi-module CLI app, a VM HTTP app,
    a typed-template app, a package-import app, a std collections/map-heavy app,
    and a data-style app that exercises external package boundaries when
    available.
  - Requirement: reference apps must live outside release-critical generated
    output, run from clean temporary workspaces or explicitly configured
    external paths, and never require repository pollution to validate release
    behavior.
  - Requirement: each reference app must run build, test, format-check,
    lint-check, package resolution, VM execution, diagnostics snapshot, support
    bundle generation on failure, and installed-artifact provenance checks.
  - Requirement: include both success-path and adversarial scenarios for each
    reference app: bad import, stale package, missing asset, wrong module path,
    invalid runtime capability, failed test, and formatter/lint drift.
  - Requirement: persist `release-reference-app-suite-report.json` with app
    names, source origins, commands run, VM runtime result, diagnostics,
    support-bundle paths, installed compiler path, and cleanup status.
  - Gate: add `make release-reference-app-suite-check` and run it after
    `release-project-upgrade-matrix-check` and before final release readiness.
  - Acceptance: release cannot pass if Terlan only works for minimal examples
    while representative multi-module applications fail under the installed VM
    runtime.
  - Acceptance: the gate fails if any reference app requires source checkout
    imports, hidden generated state, repository-local binaries, or a non-default
    runtime profile to pass.

- [ ] Slice 67: maintain a release adversarial corpus with replay and
  minimization.
  - Requirement: collect adversarial inputs from parser, formatter, typechecker,
    CoreIR, VM runtime, stdlib, packages, HTTP, templates, editor/LSP,
    diagnostics, release tooling, and reference apps into a versioned corpus
    with owner, source, feature area, expected outcome, and minimization status.
  - Requirement: provide deterministic corpus replay for installed release
    candidates, including expected success, expected diagnostic, expected
    timeout, expected resource rejection, and expected unsupported-feature
    outcomes.
  - Requirement: add minimization tooling or documented reduction workflow so
    failing corpus entries can be shrunk without losing the original failure
    class, diagnostic ID, or runtime invariant.
  - Requirement: classify corpus entries by risk: syntax ambiguity, type-system
    soundness, VM safety, resource exhaustion, security boundary, package
    boundary, editor drift, release drift, and performance cliff.
  - Requirement: add adversarial tests for corpus entries with missing expected
    outcomes, stale diagnostics, nondeterministic failures, hidden source
    checkout dependencies, duplicate entries, unowned entries, and minimized
    cases that no longer reproduce.
  - Requirement: persist `release-adversarial-corpus-report.json` with corpus
    counts, replay results, minimization status, stale-entry list, risk
    categories, and installed compiler path.
  - Gate: add `make release-adversarial-corpus-check` and run it after
    `release-reference-app-suite-check` and before final release readiness.
  - Acceptance: release cannot pass if high-risk adversarial fixtures are not
    replayed against the installed VM candidate or if expected diagnostics drift
    without catalog updates.
  - Acceptance: the gate fails if new feature slices add adversarial tests that
    are not registered in the corpus or if corpus entries require workspace-only
    state.

- [ ] Slice 68: run focused mutation checks on critical release behavior.
  - Requirement: define a small, deterministic mutation suite for release-critical
    behavior: parser acceptance/rejection, type errors, formatter stability,
    diagnostic IDs, VM scheduling/resource checks, HTTP routing, package
    resolution, release manifest hashing, generated-artifact freshness, and
    installer/version validation.
  - Requirement: mutations must be source-controlled or generated from a stable
    manifest, scoped to fast critical paths, and expected to be killed by
    existing tests, adversarial corpus replay, or release gates.
  - Requirement: classify surviving mutations as accepted equivalent behavior,
    missing test coverage, unreachable/private behavior, or deferred follow-up
    with owner and milestone.
  - Requirement: mutation checks must run against installed release artifacts
    where applicable and must not rewrite user source, generated release files,
    or package fixtures outside temporary workspaces.
  - Requirement: add adversarial tests for mutation manifests with no owner,
    mutations that silently skip, surviving mutants without classification,
    nondeterministic mutant output, mutated diagnostics not detected by JSON/text
    parity, and release gates that pass with corrupted manifests.
  - Requirement: persist `release-mutation-check-report.json` with mutant count,
    killed/survived/equivalent classifications, responsible gate, runtime, and
    coverage gaps.
  - Gate: add `make release-mutation-check` and run it after
    `release-adversarial-corpus-check` and before final release readiness.
  - Acceptance: release cannot pass if critical behavior can be mutated without
    any test, corpus entry, or release gate detecting the change.
  - Acceptance: the gate fails if surviving mutants are unclassified or if the
    mutation harness mutates checked-in files outside its temporary workspace.

- [ ] Slice 69: run release fault-injection scenarios for runtime and tooling
  resilience.
  - Requirement: define deterministic fault scenarios for VM process failure,
    supervisor restart, timer timeout, NativeBoundary worker failure, resource
    handle invalidation, HTTP client disconnect, slow client, malformed request,
    package resolution failure, database unavailable, and partial release mirror.
  - Requirement: every fault scenario must define expected recovery, expected
    diagnostic, expected support-bundle contents, resource cleanup, and whether
    the failure should be retryable, terminal, or escalated.
  - Requirement: fault injection must run against installed release artifacts and
    clean temporary workspaces without requiring host-specific services unless
    the scenario has an explicit skip diagnostic.
  - Requirement: include long-running stability variants for selected faults so
    restart loops, leaked handles, leaked sockets, leaked temporary files, and
    stuck timers are observable.
  - Requirement: add adversarial tests for injected faults that do not trigger,
    faults that trigger the wrong diagnostic, retries without backoff policy,
    cleanup that leaves resources live, support bundles missing fault context,
    and skipped scenarios without stable reasons.
  - Requirement: persist `release-fault-injection-report.json` with scenario
    names, injected fault, observed outcome, diagnostics, retry/terminal
    classification, cleanup proof, support-bundle path, and skip reasons.
  - Gate: add `make release-fault-injection-check` and run it after
    `release-mutation-check` and before final release readiness.
  - Acceptance: release cannot pass if known runtime/tooling faults crash with
    raw internal errors, leak resources, or leave user projects in ambiguous
    state.
  - Acceptance: the gate fails if fault handling succeeds only by hiding the
    failure, skipping without a diagnostic, or depending on source checkout
    state.

### Incremental Compilation And Release Gate Operations

- [ ] Slice 90: contain Rust build artifacts and compile shared implementation once.
  - Baseline: the current working checkout uses approximately 68 GiB under
    `target/`: 34 GiB of debug incremental state, 25 GiB of debug dependency
    artifacts, 4.9 GiB of release artifacts, 3.5 GiB of coverage artifacts,
    and 1.2 GiB of build-script outputs. This is build cache, not release size;
    the current release `terlc` and `terlan-vm` executables are approximately
    29 MiB and 13 MiB respectively.
  - Sequencing: start this slice only after active VM/runtime parity work has
    stable module boundaries and the Rustdoc/test gates are green. Complete it
    before release artifact freeze, installer validation, and final 0.0.7
    closeout. Final closeout may verify and ratchet the artifact budget, but
    must not introduce the shared-library or generated-artifact architecture.
  - Requirement: add a package library target that owns shared compiler,
    runtime, validation, HTML, formal-pipeline, and support implementation.
    Convert `terlc`, `terlan-vm`, `terlan-lsp`, quality, benchmark, proof, and
    release helper binaries into thin entrypoints that import the package
    library instead of recompiling shared module trees through `#[path]` or
    duplicated root-module declarations.
  - Requirement: move shared Rust tests onto the library test harness. Keep a
    binary test only when it validates behavior owned exclusively by that
    entrypoint, and retain exact selectors through the canonical single-process
    test orchestrator.
  - Requirement: define explicit development, test, release, and coverage
    profile policies. Preserve debugger-usable source locations, retain
    incremental compilation only where measured warm-build latency justifies
    it, and disable incremental state for release and coverage runs when it
    creates no reusable local result.
  - Requirement: add deterministic artifact accounting by profile and class:
    incremental state, dependency/codegen artifacts, build-script outputs,
    coverage output, release output, executable size, and total `target/`
    footprint. Record clean-build and warm-build wall time alongside disk use.
  - Requirement: benchmark equivalent project graphs against the installed Go
    toolchain. Measure cold builds, no-op rebuilds, one-module implementation
    edits that preserve public interfaces, public-interface edits that
    invalidate dependents, and clean optimized release builds as separate
    lanes; never hide release, native, or proof work inside VM edit-run latency.
  - Requirement: enforce Go-class VM development feedback with checked median
    and p95 latency baselines, cache hit/miss counts, invalidated module counts,
    and compiler process starts. Baselines ratchet downward after sustained
    improvements and fail on statistically meaningful regressions rather than
    single-run noise.
  - Requirement: distinguish operational cleanup from architectural progress.
    `cargo clean` may reclaim local space, but cannot satisfy this slice without
    a clean canonical rebuild proving that duplicate artifacts stay bounded.
  - Requirement: persist `build-artifact-budget-report.json` with compiler and
    Cargo versions, enabled features, target triple, profile fingerprints,
    per-class byte counts, clean/warm timings, Go comparison timings, latency
    distributions, invalidation counts, executable sizes, and the exact
    canonical commands used to reproduce the measurement.
  - Requirement: add adversarial checks for binaries reintroducing shared
    module trees, profile or feature drift creating unaccounted target roots,
    reports measured before the canonical build finishes, missing artifact
    classes, stale paths, and cleanup-only reports that contain no rebuild
    evidence.
  - Gate: add `make build-artifact-budget-check` and run it from `make check`
    and `release-0-0-7-preflight` after the canonical Rust test orchestration
    stage, without invoking another build or test pass.
  - Acceptance: one clean canonical 0.0.7 release validation run must leave no
    more than 34 GiB of total build artifacts, at least 50% below the measured
    68 GiB baseline. Once a lower canonical result is recorded, the checked
    baseline ratchets down and future growth is an error unless an explicit
    reviewed artifact class explains it.
  - Acceptance: all release binaries, VM tests, editor/LSP tests, coverage
    gates, and exact-selector reproduction commands must pass through the
    shared-library architecture with no behavior or debugger regression.

- [ ] Slice 91: deliver failed-edit-safe direct-AOT developer hot reload.
  - Requirement: one `terlc` development session watches Terlan source,
    templates, styles, package inputs, and generated binding metadata, coalesces
    related filesystem events, and rebuilds only changed modules plus proven
    dependents through the persistent incremental compiler service.
  - Requirement: stage every successful rebuild as a versioned native
    generation and validate its image descriptor, exports, types, process-state
    shape, capabilities, and native-resource contracts before atomically
    publishing it to the runtime code registry.
  - Requirement: parse, typecheck, code-generation, link, load, validation, and
    test failures leave the last admitted generation running. A failed edit
    must never terminate the working application, expose a partial generation,
    or silently reset process state.
  - Requirement: in-flight calls and continuations retain their admitted
    generation while new calls use the replacement. Compatible processes retain
    state, mailboxes, links, monitors, timers, supervision identity, and owned
    resources; incompatible state or resource changes are rejected with stable
    diagnostics unless the developer explicitly requests a restart.
  - Requirement: coordinate server-handler replacement, template/style/browser
    refresh, debugger source maps, VS Code diagnostics, and VM TUI status through
    one structured reload event stream rather than independent polling loops.
  - Requirement: preserve direct-call optimization for non-reloadable release
    code. Development reload indirection must be explicit in the image and
    dispatch contracts and must not require a JIT, interpreter, generated
    application Rust, or runtime CoreIR/VMIR.
  - Requirement: persist `watch-mode-hot-reload-report.json` with source-event
    batches, invalidated modules, cache reuse, compilation and activation
    timings, generation identities, compatibility decisions, retained runtime
    state, browser refresh events, diagnostics, and failed-build continuity.
  - Gate: add `make aot-developer-hot-reload-check` after the historical
    `make watch-mode-hot-reload-check`; the new gate must execute the direct-AOT
    development path rather than treating the completed pre-AOT gate as current
    evidence.
  - Acceptance: a full development session must prove a compatible handler and
    template edit without restart, an incompatible state edit rejected without
    state loss, an intentionally broken edit with uninterrupted service from the
    previous generation, and a corrected edit that subsequently activates.
  - Inventory this slice after its gate passes and move durable reload image,
    runtime, editor, and browser contracts into their owning documentation.

### Superseded VMIR Baseline And AOT-Native TVM Pivot

The direct-AOT implementation is native-only, but its performance closeout is
reopened. Its former focused mini-roadmap remains retired; the architectural
correction and strict regression evidence are owned by Slice 101F, the native
data ABI specification, and the executable Make gates.

The completed Slice 101 through 105 records below describe the transitional
serialized-VMIR implementation. Their checkmarks prove that work existed and
was tested; they no longer mean that the 0.0.7 runtime architecture is closed.
The following replacement slices are release blockers and take precedence over
the historical requirements where they conflict.

The obsolete CoreIR-to-VMIR, VMIR verifier/artifact, VMIR execution-trace, VMIR
optimization, VMIR native-extraction, JSON artifact freeze/round-trip, JSON VM
execution, and interpreter unsupported-feature quality commands and Make
targets have been deleted. The historical records below are provenance only;
their former gate names are not release commands and must not be recreated.

- [x] Slice 100: replace secondary-language application compilation with direct
    AOT native-object emission.
  - Contract: `terlan/docs/runtime/TVM_NATIVE_DATA_ABI_SPEC.md` is normative for
    compiled values, ownership, calls, actors, continuations, generics, runtime
    transitions, external adapters, hot reload, and rejection behavior.
  - Hard decision: `terlc` remains implemented in Rust, but ordinary Terlan
    application functions compile from checked Terlan IR directly to native
    objects. Generated Rust plus `rustc` is not the product backend.
  - Hard decision: Cranelift is the only 0.0.7 application code-generation
    backend. `terlc` lowers CoreIR through Terlan-owned NativeIR into Cranelift
    IR and uses `cranelift-object` in-process. LLVM and dual-backend work are
    explicitly out of scope.
  - Requirement: define the machine-level value ABI, function ABI, calls,
    constants, control flow, stack/frame layout, error returns, safepoints, and
    object-symbol policy required for the first direct backend.
  - Requirement: implement actor-local bump allocation, precise compiler stack
    maps, relocatable managed references, independently collectible actor heaps,
    and shared immutable bulk storage without universal atomic reference
    counting or a public fixed heap header.
  - Requirement: stress collection during mailbox graph transfer and
    continuation resume, actor exit during transfer, forced OOM/work-budget
    exhaustion, and actor/mailbox churn. Failure must roll back atomically with
    no leaked root, heap object, or cross-owner reference.
  - Requirement: maintain a versioned coverage matrix for every executable
    CoreIR expression, pattern, call, effect, and intrinsic node. Each node is
    native-lowered, compiler-only, or rejected before linking with a stable
    diagnostic; an unclassified new node blocks the AOT build.
  - Requirement: ordinary spawn, local send/receive, yield, reductions, timers,
    links, monitors, and exit use same-shard native runtime fast paths. They do
    not serialize through TVM transport or supervisor IPC.
  - Immediate architecture correction: the supervisor uses a coarse protocol
    only for shard admission, lifecycle, inspection, cross-shard routing, and
    recovery. The execution-shard OS process owns its scheduler, actor heaps,
    continuations, admitted AOT image, and direct runtime ABI calls. Separate
    unsafe Rust/C/C++/CUDA workers receive asynchronous capability RPC only;
    they do not own Terlan application dispatch, actor heaps, continuations, or
    scheduler leases. Further managed-value transport through the current
    application worker is forbidden.
  - Requirement: freeze multicore-ready AOT interfaces without implementing the
    multicore scheduler in this slice. Actor heap ownership is exclusive but
    movable through an owner/epoch handoff; native code is reentrant; code and
    descriptors are immutable; mutable state is explicit; continuation resume
    is single-consumer; mailbox publication has defined memory ordering;
    actor-local safepoints are bounded; and ordinary transitions do not depend
    on thread-local state or one process-global mutex.
  - Requirement: keep the private AOT/native runtime ABI distinct from the
    public C ABI and generated C++ interface. External adapters expose only
    versioned opaque handles, explicit ownership/execution context, bounded
    buffers, status/error values, resource lifetimes, and declared callback
    reentrancy. They must not expose `TvmRef`, actor heaps, Cranelift signatures,
    continuation layouts, native stack addresses, or shard/thread identity;
    asynchronous completion is single-shot, and adapter ABI/target identity is
    included in image admission and cache fingerprints.
  - Requirement: compile one typed `add(Int, Int) -> Int` Terlan consumer into a
    native object, link it once with the precompiled Rust TVM runtime, and invoke
    it through NativeBoundary without serialized VMIR or generated Rust.
  - Tests: positively inspect the native object and run the Terlan consumer end
    to end; adversarially reject unsupported types, invalid ABI records, missing
    symbols, raw pointers, toolchain fallback, and any attempted application
    `rustc`/C/C++ compiler process, LLVM library/backend, LLVM IR/bitcode,
    `opt`, `llc`, ORC, or backend-native stack-map leakage.
  - Gate: add `make tvm-direct-aot-backend-check`.
  - Gate: add `make tvm-managed-memory-check`.
  - Gate: add `make tvm-aot-lowering-coverage-check`.
  - Gate: add `make tvm-aot-shard-ownership-check` to reject application
    dispatch, Terlan managed heaps, or continuation ownership in the unsafe
    native-adapter worker and to reject ordinary actor calls in the supervisor
    protocol.
  - Gate: add `make tvm-aot-supervisor-lifecycle-check` for the versioned
    admission/ready/drain/crash/restart state machine, shard epochs, stale-event
    rejection, restart budgets, quarantine, and non-replay of unsafe effects.
  - Gate: add `make tvm-aot-capability-worker-check` to prove the retained
    native worker is asynchronous, bounded, sandboxed, capability-only, and
    unable to load application images or own actor heaps and continuations.
  - Gate: add `make tvm-aot-image-lifetime-check` for immutable admission,
    generation pins, hot-reload/crash races, stale-generation rejection, and
    quiescent unload across frames, continuations, heaps, mailboxes, resources,
    callbacks, debugger state, and crash metadata.
  - Gate: run `make tvm-aot-runtime-transition-check` for VM-owned native
    transition handling and `make runtime-aot-only-check` for hard-cutover
    enforcement.
  - Gate: add `make tvm-aot-multicore-readiness-check` with deterministic
    schedule exploration for double resume, lost wakeup, ownership handoff,
    mailbox publication, actor-exit races, and global-lock contention.
  - Gate: add `make tvm-aot-c-abi-boundary-check` to reject private AOT layout
    leakage without reactivating the broader C++ package lane.
  - Gate-policy update 2026-07-21: the evaluator/parity-era comparative,
    semantic-preservation, and JavaScript non-regression commands were removed
    from the primary AOT graph during the hard cutover. They must not be
    recreated as evaluator compatibility gates. Native AOT benchmarks and
    executable native-image fixtures must own any reusable assertions needed
    for AOT closeout.
  - Acceptance: the fixture emits and executes native machine code while a
    process trace proves that no secondary compiler and no per-module linker was
    started for Terlan application code. Multicore-readiness evidence proves
    future actor migration will not require changing the native ABI, but does
    not claim parallel actor execution or complete any multicore-roadmap item.
  - Reconciled evidence 2026-07-22: AOT-1 through AOT-4 close the direct
    Cranelift backend, managed-memory and relocation model, complete reachable
    application lowering, and same-shard actor transition ownership. The
    capability worker is asynchronous and external-only; the supervisor owns
    coarse lifecycle rather than application dispatch. The lowering coverage,
    managed-memory, shard lifecycle, image lifetime, multicore-readiness, and
    private/public ABI gates all pass without serialized VMIR, generated
    application Rust, or per-image application workers.

- [x] Slice 101A: audit and freeze the transitional execution baseline.
  - Requirement: inventory every `.tvm.json` producer/consumer, serialized-VMIR
    evaluator, direct CoreIR evaluator, REPL fallback, test fallback, HTTP
    handler path, debugger path, hot-reload path, and quality gate.
  - Requirement: classify each path as reusable runtime semantics,
    compiler-internal IR, temporary migration support, or deletion debt.
  - Requirement: prohibit new runtime features from landing only in the
    transitional evaluator.
  - Tests: use a positive complete inventory fixture plus adversarial fixtures
    with an omitted consumer, an unowned fallback, and a gate that falsely
    claims transitional execution is release-complete.
  - Historical gate: `tvm-aot-pivot-inventory-check` established the complete
    cutover ledger. After the evaluator and interpreter were hard-removed, the
    standalone gate was retired; the maintained inventory, native consumer
    rejection tests, and `runtime-aot-only-check` now enforce the live boundary.
  - Acceptance: the inventory has no unowned `.tvm.json` or VMIR execution path
    and no gate may present transitional execution as release closure.

- [x] Slice 101B: freeze the TVM executable-image descriptor and native format.
  - Contract: `terlan/docs/runtime/TVM_EXECUTABLE_IMAGE_SPEC.md` is normative.
  - Requirement: define the canonical descriptor bytes, stable export and type
    identifiers, runtime ABI range, NativeBoundary protocol range, target and
    calling convention, capabilities, NativeResource ownership, dependency
    fingerprints, digests, signatures, and ELF/Mach-O/PE section mappings.
  - Gate: make the repurposed `make terlan-vm-artifact-format-check` validate
    the executable-image specification.
  - Gate: add `make tvm-native-image-format-check` for real platform images.
  - Implemented evidence: format 1 uses canonical `TVMDSC01` little-endian
    records with a digest footer and target-specific ELF, Mach-O, and PE/COFF
    descriptor sections; the loader statically inspects the native container
    before execution.
  - Implemented evidence: format tests reject JSON/compiler payloads, wrong
    targets, incompatible runtime and NativeBoundary ranges, noncanonical or
    duplicate tables, undeclared resource ownership/cleanup links, duplicate
    dependency IDs, invalid signatures, and code or descriptor digest damage.
  - Acceptance: static inspection rejects JSON, compiler IR, wrong targets,
    unknown ABI versions, malformed tables, bad digests, and renamed
    `.tvm.json` files without executing the image.

- [x] Slice 101C: emit and execute the first real `.tvm` image.
  - Requirement: AOT-compile a tiny Terlan consumer into one target-native image
    with one typed pure export and no serialized Terlan instruction body.
  - Requirement: the VM must validate the descriptor, supervise the worker,
    complete a binary-protocol handshake, invoke the export, receive a typed
    result, and turn crashes or malformed responses into typed VM failures.
  - Gate: add `make tvm-native-image-loader-check`.
  - Gate: add `make tvm-aot-consumer-check`.
  - Implemented evidence: the scalar consumer now derives export IDs, arities,
    parameter types, and result types from the admitted embedded descriptor;
    the worker uses the frozen format-1 dispatch symbol and receives no symbol
    name from JSON or command-line metadata.
  - Implemented evidence: the direct consumer gate deletes `.tvm.json` before
    exercising `terlan-vm load`, `terlan-vm run`, the descriptor-bound binary
    handshake, and the typed `main/0` result.
  - Implemented evidence: the worker enforces strictly increasing non-zero
    request IDs, rejects replayed or out-of-order calls, and completes an
    explicit `Shutdown`/`ShutdownAck` exchange before the VM verifies clean
    process exit.
  - Implemented evidence: bounded control-frame decoding rejects invalid magic,
    malformed payloads, and oversized frames; end-to-end consumer fixtures
    replace the worker with malformed-output and early-exit processes and prove
    both failures remain typed VM diagnostics outside the supervisor process.
  - Acceptance: the end-to-end fixture runs without `.tvm.json`, VMIR
    interpretation, generated-source inspection, or mocked native calls.

- [x] Slice 101D: AOT-compile full Terlan control flow over VM transitions.
  - Requirement: compile pure, effectful, and actor functions to native code.
    Native code yields typed send, receive, spawn, timer, link, monitor,
    resource, cancellation, failure, and scheduling transitions to the VM.
  - Requirement: continuations use stable numeric identities and typed owned
    values, never native stack or raw pointer identities.
  - Tests: execute positive actor yield/resume fixtures and adversarial stale,
    duplicate, wrong-type, wrong-owner, and invalid-continuation cases.
  - Gate: add `make tvm-aot-runtime-transition-check`.
  - Implemented evidence: the pure NativeIR subset now represents sequential
    scalar `let` locals and ordered scalar `if` control flow; Cranelift emits
    native branch/merge blocks and propagates checked arithmetic and
    no-matching-branch failures without VMIR interpretation. Boolean `not`,
    `and`, and `or` are native too; conjunction and disjunction lower through
    ordered branch regions so short-circuiting suppresses an unselected
    division-by-zero path rather than eagerly evaluating it.
  - Implemented evidence: the first scalar `std.vm.Process.yield_now/0`
    vertical splits AOT code into stable numeric entry/resume identities,
    publishes typed continuations in descriptor record 9, and carries bounded
    owned `Int`/`Bool` captures through a capacity-checked caller-owned buffer
    and `Transition(Yield)` / `Resume` worker frames. A fixture resumes
    `yielded_add(41)` as native code and returns `42`; pure scalar `let` prefixes
    now use backward free-variable selection, emit required prefix calculations,
    and transport only locals live in the resume body. The zero-arity
    `yielded_local` Terlan consumer executes through `terlan-vm`, while
    `yielded_local_from(21)` proves one live derived local crosses as `42`, its
    unused source local and original parameter stay out of the continuation, and
    native resume returns `43`. The VM and worker validate descriptor identity,
    arity, scalar types, and Bool domains. Protocol, descriptor, and
    separate-worker fixtures reject unknown operations, reserved bits,
    duplicate IDs, export/continuation collisions, undeclared resource types,
    stale continuation IDs, and wrong-type Bool resumes. Normal source may now
    import `std.vm.Process` and invoke `Process.yield_now()`; CoreIR canonicalizes
    only resolved value-level module aliases, while an unimported `Process`
    remains an ordinary remote call. Linear functions may contain multiple
    suspension points, including consecutive yields: each point receives a
    stable ordinal-derived ID and independently minimized owned captures, the
    worker advances the matching pending continuation, and `terlan-vm` drives
    the declared acyclic transition chain to completion. Fixtures cover repeated
    zero-capture, `Int`, computed-local, and `Bool` capture paths. Native `if`
    branch bodies may now be pure or suspending independently, including nested
    branches, yields in both arms, a resume body that branches and yields again,
    and a short-circuit RHS that yields only when selected. Transition capacity
    is the maximum branch capture width; fixtures prove zero-, one-, and
    two-value branch signatures, stable arm-specific continuation IDs, pure
    sibling completion, and no-match failure propagation. Worker fixtures also
    reject request-owner mismatch, duplicate resume, wrong capture arity,
    calls, or shutdown while a continuation is pending. Suspending native
    functions may now be called in tail position: NativeIR and Cranelift forward
    the callee status, continuation ID, and owned capture buffer without a caller
    stack or a new VM-visible identity. Fixed-point analysis propagates
    suspension and maximum capacity through direct, transitive, and
    branch-selected tail calls. The internal ABI writes the selected
    continuation's exact capture length through a caller-owned output, so a
    zero-capture branch remains distinct from a sibling requiring captures.
    Fixtures prove immediate and yielding paths, stable callee continuation
    identity, zero/`Int`/`Bool` captures, transitive forwarding, differing branch
    capacities, wrong request and Bool rejection, duplicate resume rejection,
    and tail forwarding without caller state. Callees with one to eight
    proven-linear suspension stages and no suspending callee may now be used in
    bounded non-tail scalar contexts. Admission requires one known initial
    continuation and one guaranteed next continuation per intermediate stage.
    Immediate completion continues in the current entry; each yield verifies
    callee identity and capture count, concatenates the stage's exact callee
    captures with caller-live scalars, and returns a distinct stable caller
    wrapper. Intermediate wrappers rewrite the next suspension, preserve caller
    captures, and rebase callee temporary indices around appended parameters.
    The terminal wrapper inlines the final callee continuation before the saved
    caller expression, with no native stack or code pointer. Fixtures cover
    unary, binary, nested, any single pure-call argument, any sequential `let`,
    condition, and selected-branch contexts; checked arguments; zero/`Int`/`Bool` and combined
    captures across repeated resumes; computed locals; tail forwarding; distinct
    wrapper identities; wrong request, invalid Bool, stale prior-stage, and
    duplicate resume rejection. The boundary fixture executes eight distinct
    wrappers and rejects a ninth stage. Ambiguous callee branch graphs and
    chains deeper than eight remain excluded. Linear scalar
    branch conditions compose their resume body
    with the enclosing
    ordered clauses instead of exposing an intermediate Boolean to the VM.
    Fixtures cover first and later
    yielding clauses, true/false fallthrough, three-value capture, repeated
    condition yields with distinct IDs, a selected body that yields again,
    nested conditions, dead-prefix-local elimination, checked prefix failures,
    resumed no-match propagation, and suspending `and`/`or` left operands with
    both suppressed and selected division-by-zero paths. Condition continuations
    reject wrong request ownership, capture arity, Bool domains, and duplicate
    resumes. Non-linear scalar conditions now compose suspension through unary
    and comparison wrappers on the first-evaluated operand spine and through a
    selected lazy `and`/`or` right operand. Fixtures prove unselected operands
    emit no transition, checked earlier calculations fail before suspension,
    true/false ordered fallthrough, resumed no-match, a selected body that
    yields again, nested lazy conditions, exact Bool captures, and stable wrong
    request, invalid-Bool, and duplicate-resume rejection. Admission is bounded
    to eight nested lazy right-hand decisions and rejects deeper alternating
    compositions. Eager binary conditions now preserve source evaluation order
    when the right operand yields: checked scalar left work runs before the
    transition, its exact result is carried as a typed continuation capture,
    and resume completes without recomputation. Fixtures prove pre-transition
    division failure, exact `Int` capture order, and both comparison outcomes.
    Proven-linear suspending callees now compose in the eager right operand too:
    scalar prefix bindings execute before the call, immediate completion uses
    the materialized value, and a yield appends it to the caller-owned wrapper
    signature. Fixtures prove pre-call division failure, immediate and yielding
    callee paths, exact capture values, and recomputation-free resume. Non-linear
    lazy `or` conditions may now select a suspending body either before the
    right condition runs or after it resumes. Fixtures prove a suppressed right
    operand with one body transition, a false resumed condition with immediate
    fallthrough, and a true resumed condition followed by a distinct second
    transition. Pure native calls may now contain the one suspending callee in
    any argument position: earlier arguments materialize and fail before the
    transition, later arguments remain in the native resume, and exact earlier
    values cross in the wrapper signature. A suspending call may likewise occur
    in any sequential `let` binding or after a pure binding prefix; checked
    prefix locals execute once and only live values extend the wrapper capture
    list. Scalar expressions may now sequence up to eight proven-linear
    suspending calls: `CallThen` completion can enter the next suspending native
    region, while initial and resumed paths share stable nested continuation
    layouts. Fixtures cover all-immediate, either-call-yields, two yielded calls,
    a full eight-transition chain, generated-prefix collision avoidance, and
    rejection of a ninth call. Managed-value capture, non-Yield scheduler
    dispatch, same-shard runtime calls, and all non-yield actor/effect
    transitions remain open.
    Direct `yield_now` composition now applies to ordinary unary and eager
    scalar value expressions, any scalar-call argument, and conditions.
    Earlier arguments are materialized before suspension and later arguments
    remain in the resume. Fixtures prove exact live capture ordering,
    pre-transition checked failure, post-resume checked failure, and native
    final-value resume without an intermediate VMIR scalar.
    Direct yields and proven-linear suspending calls now compose in either order
    within the bounded scalar profile. Fixtures cover call-then-yield and
    yield-then-call immediate/yielding paths, two-transition continuation
    handoff, exact mixed capture layouts, and final native results.
    The format-1 binary control protocol now assigns closed tags to yield,
    send, receive, spawn, timer, link, monitor, resource, cancellation,
    failure, and scheduling transitions, round-trips every operation, and
    rejects unknown tags. The scalar-only driver explicitly rejects non-yield
    operations until its scheduler owner is attached.
    Call, transition, and resume frames now carry an independent nonzero owner
    identity. The worker binds pending state to the exact request/owner/
    continuation triple, preserves the owner across repeated yields, and
    distinguishes a foreign-owner resume from stale request or continuation
    failures. Adversarial coverage resumes a valid pending continuation with a
    different owner and proves fail-closed rejection.
    Success and failure replies now retain the same owner correlation, and the
    consumer validates it before accepting either terminal result. Immediate,
    checked-failure, yielding, repeated-resume, and Unit-effect fixtures now
    exercise one owner-preserving lifecycle envelope.
    Transition frames now encode operation arguments separately from owned
    continuation captures, with independent bounded counts and malformed-length
    rejection. Every declared operation round-trips both vectors; the active
    Yield path enforces zero operation arguments while preserving its exact
    descriptor-typed capture vector.
    NativeIR, descriptor emission, Cranelift, worker validation, and the pure
    consumer now share a canonical `Unit = 0` boundary representation. A
    direct exported `yield_now/0` fixture parks with no captures and resumes to
    native `Unit`, removing the need to wrap every effect in an Int/Bool result
    expression.
    `Unit` also participates in native export signatures and owned continuation
    capture minimization. Fixtures prove immediate Unit identity, one live Unit
    across yield/resume, exact descriptor-driven capture shape, and fail-closed
    rejection of a noncanonical Unit resume word.
    Unit-returning suspending functions now compose in tail position and as
    bounded non-tail effect steps. Fixtures prove zero-capture tail forwarding
    and a non-tail Unit call that retains only the caller Int live afterward,
    then completes the final native result without VMIR execution.
    Sequential Unit effect calls now prove distinct native continuation handoff
    with minimized captures. The boundary fixture executes eight Unit-shaped
    effect transitions while retaining one live Int at every stage and rejects
    a ninth effect from native admission.
    The VM actor runtime now resolves the transport owner to a nonzero process
    identity and parks that process behind one exact request/continuation pair.
    Its dual indexes reject duplicate ownership, stale continuations, and a
    foreign process attempting resume without waking the owner; exact resume
    requeues through the scheduler, and actor exit releases the continuation
    lease.
    The direct `.tvm` runner now creates a VM actor for its selected entrypoint,
    sends that process ID as the control-frame owner, and routes every validated
    Yield through separate begin and resume steps. Begin returns an owned
    suspension while the actor remains parked; resume proves the exact
    scheduler lease, requeues the actor, and only then sends the worker Resume.
    A repeated Yield parks again behind its next stable continuation. Existing
    end-to-end fixtures exercise zero-, one-, and repeated-yield image
    entrypoints through this scheduler-visible path. The driver lives in a
    focused native-image runner module instead of expanding the oversized CLI
    entry file.
    `Send` is now the first non-Yield transition serviced by that driver. Its
    operation vector carries one positive VM process identity and one typed Int
    mailbox payload separately from descriptor-owned continuation captures.
    The actor runtime validates the exact parked owner/request/continuation
    lease before mailbox mutation, accounts and delivers the message through
    the VM scheduler, and only then consumes the lease and resumes native code.
    Foreign owners, stale continuations, zero or missing recipients, malformed
    argument shapes, and still-unimplemented operations fail closed without
    mailbox mutation or wakeup.
    The compiler now emits that Send operation for the bounded scalar
    `Process.send_int/2` bootstrap. CoreIR gives it a closed effectful identity;
    NativeIR preserves the operation and its two typed arguments independently
    from live continuation captures; Cranelift returns a distinct transition
    status; and the worker splits the two vectors before descriptor validation.
    A real `.tvm` fixture proves `[recipient, Int payload]` plus one live Int
    capture at the raw control frame, resumes to the expected native result,
    and separately self-sends through the VM-owned mailbox consumer. Lowering
    opaque `Process[T]` and `Message[T]` through this native boundary remains
    open.
    `Receive` now uses the bounded scalar `Process.receive_int/0` bootstrap.
    Its zero-argument transition publishes only live continuation captures;
    the descriptor reserves a leading typed Int resume parameter that the VM
    supplies from the owner mailbox after exact lease validation. Empty or
    nonmatching mailboxes retain both queued values and the parked lease, while
    a matching Int is memory-accounted, consumed, prepended to the capture
    suffix, and resumed through the stable continuation identity. Raw `.tvm`
    fixtures prove direct and non-tail Receive composition with one capture,
    and a VM-owned self-send/receive fixture proves Send-to-Receive handoff
    without VMIR interpretation.
    `Spawn` now uses the bounded scalar `Process.spawn_int/1` bootstrap. Its
    operation argument is one positive stable native entry identity, while its
    VM-injected Int resume value is the new child process identity. The actor
    runtime validates the parent's exact continuation lease before creating a
    parent-owned, runnable, scheduler-visible child and resumes the parent only
    after successful creation. Foreign ownership and zero entry identities
    preserve both the lease and process table. Raw `.tvm` fixtures prove direct
    and non-tail Spawn composition with independent argument/capture vectors;
    each positive entry tag resolves exactly one zero-arity `spawn_<tag>` export
    before actor-table mutation. The child executes in an isolated native worker
    under its VM process identity, may service its own transitions, and records
    normal or error exit without corrupting or failing the parent. A VM-owned
    fixture runs `spawn_1`, sends an Int from child to parent, exits the child,
    resumes the parent, and receives the message without VMIR interpretation;
    another proves checked child failure remains isolated, while a missing tag
    creates no child.
    `Timer` now uses the bounded scalar `Process.sleep_ticks/1` bootstrap. Its
    operation vector carries one positive VM clock delay independently from
    owned continuation captures. After exact lease validation, the actor
    runtime creates a VM-owned one-shot timer while the native owner remains
    suspended, proves the timer cannot fire one tick early, advances the
    monotonic VM clock to the exact deadline, and only then consumes the lease
    and requeues the owner. Foreign ownership, zero or negative delays, and
    deadline overflow fail before wakeup; raw `.tvm` fixtures prove direct and
    non-tail Timer composition with one live Int capture, while the VM runner
    exercises positive and malformed delays without VMIR interpretation.
    `Link` now uses the bounded scalar `Process.link_int/1` bootstrap. The
    transition carries one positive peer process identity separately from owned
    continuation captures. The actor runtime validates the exact parked owner
    before creating the symmetric VM failure relationship, accounts the link,
    and resumes native code only afterward. Foreign owners, self-links, and
    missing peers retain the parked lease without relationship mutation. Actor
    coverage proves an abnormal peer exit propagates through the new native
    link, while raw `.tvm` fixtures prove direct and non-tail composition with
    one live Int capture. A real-image fixture spawns a native child, links that
    child to its live parent, completes the child normally, and resumes the
    parent without VMIR interpretation; malformed real-image peers fail closed.
    `Monitor` now uses the bounded scalar `Process.monitor_int/1` bootstrap. Its
    positive target identity is an operation argument, while the VM-allocated
    numeric monitor reference is injected as the leading typed Int continuation
    value. Exact owner and live-target validation precede reference allocation;
    foreign owners and missing or exited targets retain the lease, create no
    relationship, and do not consume an identity. Actor coverage proves target
    exit delivers one VM-owned DOWN message carrying the returned reference.
    Raw `.tvm` fixtures prove direct and non-tail Monitor composition with an
    independent target/capture layout, and a real-image fixture monitors the
    current native actor without VMIR interpretation while a missing target
    fails closed.
    `Resource` now uses the bounded scalar
    `Process.acquire_resource_int/1` bootstrap. Its positive kind tag remains an
    operation argument, while the VM injects a newly allocated resource identity
    as the leading typed Int continuation value. The actor facade now owns the
    existing resource table: acquisition creates an owner-only `native.scalar`
    descriptor plus process cleanup handle before resume, and actor exit removes
    the table row while returning that cleanup handle. Foreign continuation
    owners and invalid tags retain the lease, allocate no row, and leave identity
    `1` available for the first valid acquisition. Raw `.tvm` fixtures prove
    direct and non-tail Resource composition with independent argument/capture
    vectors, and the real-image runner exercises positive and malformed
    acquisition without VMIR interpretation.
    `Cancellation` now uses the bounded scalar `Process.cancel_int/1`
    bootstrap. The VM validates the exact request/continuation owner before
    recording a scheduler cancellation flag on one positive live target, then
    consumes the lease; foreign owners and missing or malformed targets retain
    the lease and leave the target unchanged. Self-cancellation and cancellation
    delivered by a spawned child are applied at the native boundary before any
    resume frame can execute, using the existing killed-exit cleanup path. Raw
    `.tvm` fixtures prove direct and non-tail Cancellation composition with
    independent argument/capture vectors, while real-image fixtures prove
    isolated child cancellation, self-cancellation, and missing-target failure
    without VMIR interpretation.
    `Failure` now uses the terminal scalar `Process.fail_int/1` bootstrap. The
    VM validates the exact continuation owner and positive application failure
    code before converting it to `VmExitReason::Error`, then routes the exit
    through the existing failure layer instead of resuming native code. That
    path removes the continuation, cleans VM-owned resources, recursively
    propagates abnormal exits through untrapped links, and emits typed monitor
    `DOWN` messages. Foreign owners and malformed codes retain the lease and do
    not mutate process state. Raw `.tvm` fixtures prove direct and non-tail
    Failure composition with independent argument/capture vectors; real-image
    fixtures prove isolated child failure, self-failure, linked propagation,
    and malformed-code rejection without VMIR interpretation.
    `Scheduling` now uses the bounded scalar
    `Process.schedule_class_int/1` bootstrap, with closed tags for priority,
    normal, and background classes. Exact continuation authority is checked
    before the suspended owner is reclassified; the VM scheduler then resumes
    it into the selected weighted queue and charges the scheduler operation.
    Foreign owners retain the lease without queue mutation, and missing, extra,
    or out-of-range class tags fail before parking. Actor fixtures prove
    priority-before-normal and normal-before-background selection. Raw `.tvm`
    fixtures prove direct and non-tail Scheduling composition with independent
    argument/capture vectors, while the real-image runner exercises all three
    classes and malformed tags without VMIR interpretation.
  - Acceptance: actors park and resume through native continuation entry points
    while the VM retains scheduler, mailbox, timer, failure, capability, and
    resource ownership without interpreting an instruction stream.

- [x] Slice 101E: make AOT compilation incremental and bounded.
  - Hard decision: target Go-class compilation speed with a compiler-owned
    direct native-object backend. Generated Rust plus `rustc` remains valid only
    as bootstrap/conformance evidence and for separately cached native adapters.
  - Requirement: generated source, objects, descriptors, and fingerprints live
    in a content-addressed internal cache; deployable output contains one
    `.tvm` per application and target.
  - Requirement: unchanged dependencies load checked interface summaries rather
    than source, independent packages compile in parallel, and generic lowering
    has a documented bounded-specialization strategy instead of uncontrolled
    monomorphization.
  - Requirement: use bounded compilation units and one final link, reuse
    unchanged objects, skip code generation and linking on a no-op build, and
    keep development optimization separate from release/LTO policy.
  - Requirement: the AOT-only REPL uses a persistent compiler service and
    incremental object reuse without a JIT or interpreter fallback.
  - Tests: benchmark positive cold and incremental builds; adversarially poison
    cache keys, remove objects, change the target ABI, and verify stale outputs
    are rejected rather than reused.
  - Tests: compile equivalent Terlan and Go small-command and multi-package
    fixtures on the same reference machine, including cold, one-package edit,
    no-op, first REPL, and unchanged REPL measurements. Record medians, 95th
    percentiles, toolchains, hardware, cache state, and Terlan-to-Go ratios.
  - Gate: add `make tvm-aot-compilation-time-check`.
  - Gate: add `make tvm-single-image-artifact-check`.
  - Implemented evidence: Cranelift application objects and descriptor objects
    now live under the content-addressed compiler cache
    `.terlan/native-aot/<input-sha256>/`; deployable `vm/` output no longer
    contains or names those link intermediates, and a no-op incremental build
    reuses them without starting the linker.
  - Implemented evidence: same-key cache construction now has one filesystem
    owner and revalidates after ownership is acquired. A concurrent cold-build
    fixture runs two compiler processes against one shared cache, proves both
    outputs match the verified entry, and observes exactly one linker
    invocation with no residual lock ownership.
  - Implemented evidence: cache objects, descriptor objects, sealed images,
    and manifests now publish through same-directory temporary files with the
    manifest last. The linker also targets a temporary image, so an unsealed
    executable is never exposed at the content-addressed path; concurrency
    coverage rejects residual temporary files.
  - Implemented evidence: cache ownership now uses the standard OS file-lock
    lifetime instead of pathname creation. Killing a compiler while its linker
    is active releases ownership automatically; a second compiler acquires the
    same cold entry, rejects the incomplete generation, and publishes a valid
    image without waiting for the lock timeout.
  - Implemented evidence: VM-target builds now emit dedicated `vm.compile` and
    `vm.aot-and-artifact` phase timings under `--timings`, for both standalone
    and application builds. `make tvm-aot-compilation-time-check` owns the
    executable timing-contract fixture in addition to the single-image/cache
    gate; comparative samples and enforced percentile ratios remain open.
  - Implemented evidence: dependency-free incremental no-op builds now bypass
    parsing and typechecking only after revalidating exact source, artifact
    checksum/schema/compiler/target identity, deployed image digest and static
    descriptor, embedded cache key, and every cache-manifest file. Seven
    process-level warm samples enforce the roadmap's sub-second p95 budget and
    use an invalid linker to prove the native link path remains skipped.
  - Implemented evidence: export IDs are derived from the full
    module/function/arity identity using the frozen format-1 SHA-256 scheme,
    are independent of declaration order, and fail compilation on collision;
    package-wide dispatch therefore keeps same-named functions in different
    modules collision-safe.
  - Implemented evidence: package builds aggregate every supported scalar-pure
    module into one Cranelift object, one descriptor, one final link, and one
    deployable `.tvm`. Module JSON envelopes reference that shared image;
    descriptor-qualified calls execute after those envelopes are deleted, and
    ambiguous short names fail closed.
  - Implemented evidence: the sealed application image is cached beside its
    content-addressed objects and descriptor. Returning to an earlier source
    fingerprint restores the matching cached image without invoking the linker,
    rather than trusting a same-named deployable output from another build.
  - Implemented evidence: each complete native cache entry now has a
    deterministic non-JSON `manifest.v1` binding the cache-input digest, full
    target, backend, and exact names, lengths, and SHA-256 digests of its native
    object, descriptor object, and sealed image. The manifest is committed last;
    reuse also requires static target/image validation and an embedded
    descriptor build identity equal to the cache key. Missing, partial,
    corrupted, or valid-but-mis-keyed entries regenerate instead of executing.
  - Implemented evidence: the cache key now covers compiler version, an explicit
    native-codegen schema, backend, image format, full target triple,
    architecture, operating system, calling convention, and deterministic
    NativeIR. The single-image gate proves a clean no-op build skips an invalid
    linker, and proves recovery from a poisoned object, missing descriptor
    object, poisoned manifest, poisoned cached image, poisoned deployable image,
    and a self-consistent manifest containing a valid image from the wrong
    source key. Comparative cold/edit/no-op/REPL timing and Go ratios remain open.
  - Implemented evidence: scalar REPL wrappers declared as `Dynamic` now narrow
    to a proved native ABI result from typed CoreIR, so integer, boolean, and
    unit prompts produce real AOT images instead of missing the native lane.
    REPL generation identities are content-stable, and one session-owned
    compiler/runtime service publishes the already checked CoreIR generation,
    retains its native worker, and directly re-executes an unchanged prompt.
    The compilation-time gate proves two identical prompts invoke the linker
    exactly once and enforces a seven-sample unchanged-generation p95 below one
    second. It also enforces changed-generation p95 below one second after
    parsing embedded standard-library interfaces once per compiler process;
    comparative Go ratios remain open.
  - Reconciled evidence 2026-07-22: AOT-7A through AOT-7G supersede the earlier
    partial notes above. The committed seven-sample policy now measures and
    enforces cold and incremental Terlan-to-Go median and p95 ratios at or below
    5.0x, keeps governed warm operations below one second, and rejects weakened
    policy, forged ratios, incomplete samples, and poisoned cache generations.
  - Acceptance: cold development, one-function incremental, no-op, cold
    release, package relink, first REPL, and unchanged REPL reuse baselines are
    measured and release-bounded. On recorded reference hardware, the warm
    one-function build, no-op build, changed REPL declaration-to-generation
    loop, and unchanged REPL reuse loop must each remain below one second at the
    95th percentile; cold startup and release builds are tracked separately.
    Slice closure must set and enforce explicit permitted Terlan-to-Go cold and
    incremental ratios rather than claiming Go-like speed from qualitative
    architecture alone.

- [x] Slice 101F: remove transitional execution.
  - Requirement: migrate build, run, test, REPL, HTTP, debugger, hot reload,
    package validation, support bundles, and release installers to `.tvm`.
  - Requirement: delete the `.tvm.json` runtime loader, serialized-VMIR
    interpreter, source-bearing runtime artifact, generated pure-worker
    sidecars, and obsolete gates/reports after their reusable assertions move
    to native-image equivalents.
  - Requirement: compare the completed native AOT HTTP surface with the
    preserved checked-CoreIR runtime baseline on the same recorded hardware.
    Enforce throughput, p50/p95/p99 latency, allocation rate, backpressure,
    WebSocket/SSE longevity, and overlapping hot-reload-generation behavior.
  - Requirement: execute packaged and installed images on the current native
    host and validate the portable target contract for every supported
    architecture, operating system, object format, and calling convention.
    Exercise debug/stack metadata, crash reports, generation unloading, and
    incompatible-image rejection on the executable host; strict target-schema
    and aggregate self-tests reject incomplete portable coverage without making
    remote execution or artifact publication a completion requirement.
  - Requirement: from a clean environment with `RUSTFLAGS` unset, require
    `cargo check --locked -p terlan` and the complete AOT gate set to pass.
    Dead-code and unused-import suppression cannot conceal hard-removal debris.
  - Tests: execute positive build/run/test/REPL consumers through `.tvm` and
    adversarially reject renamed JSON, stale sidecars, fallback flags, and
    serialized instruction payloads.
  - Gate: add `make no-tvm-json-runtime-check`.
  - Gate: add `make no-vmir-interpreter-check`.
  - Gate: add `make tvm-aot-http-performance-check`.
  - Gate: add `make tvm-aot-platform-matrix-check`.
  - Gate: run `make runtime-aot-only-check` to reject reintroduction of runtime
    CoreIR/VMIR execution or evaluator compatibility layers.
  - Implemented evidence: the public standalone `terlan-vm run` and `load`
    consumers no longer admit `.tvm.json`; help exposes only source and native
    `.tvm` inputs. Process-level adversarial coverage rejects serialized
    instruction JSON under its original suffix and proves the same bytes
    renamed to `.tvm` fail native-image admission without VMIR interpretation.
    Runtime `.tvm.json` loaders, serialized-VMIR interpretation, direct CoreIR
    evaluation, REPL/test evaluation fallbacks, evaluator variants, fallback
    flags, and evaluator/parity-era gates have since been hard-removed. Missing
    AOT coverage now fails loudly rather than selecting a compatibility path.
  - Implemented evidence 2026-07-22: REPL and test execution are native-only;
    the live inventory contains zero `deletion-debt` and zero
    `temporary-migration-support` rows. Ordinary same-shard execution uses
    direct runtime ABI calls; only explicit unsafe Rust/C/C++ capabilities may
    cross the bounded worker protocol. The retired checked-CoreIR runtime
    remains documented as valuable predecessor semantic and HTTP-performance
    evidence rather than as an error.
  - Implemented evidence 2026-07-24: HTTP regression evidence now uses a
    rotating three-lane native-AOT/Axum/plain-Hyper experiment. Maintained
    `curl`, `wrk`, and optional `wrk2` clients validate the complete route,
    payload, connection, overload, cancellation, and offered-load matrix;
    diagnostic hand-written client rows cannot decide a comparison. Decisive
    runs require at least ten paired 10-second samples, deterministic bootstrap
    intervals, disjoint pinned server/client CPUs, the performance governor,
    single-node placement, and no IRQ overlap. Reports reject unequal build or
    socket policy and attribute idle, warmed, peak, retained RSS/PSS, process
    efficiency, soak growth, TLS, and HTTP/2 evidence explicitly.
  - Reopened evidence 2026-07-22: the previous single-run HTTP comparison was
    not sufficient closeout evidence. The policy now treats every AOT latency,
    RSS, reload, or throughput regression against checked-CoreIR as an error
    with exact 1.00 ratios. The v2 harness performs an unmeasured warm-up and
    retains five raw rounds per track before selecting the intact
    median-throughput round. HTTP generations now own one persistent execution
    shard, requests own actors within it, abandoned continuations cancel their
    actors, and completed actors plus internal epoch operations are reaped.
    A 1,000-call soak retains zero actors and zero operation identities; the
    current five-round native run is stable without monotonic RSS growth. The
    preserved checked-CoreIR v1 report is admitted only through an explicit,
    self-tested adapter that retains every recorded measurement and cannot
    invent v2-only evidence. The comparison applies the same exact 1.00
    regression ratios to those historical measurements; the adapter cannot
    weaken policy or silently rewrite the preserved report.
  - Platform closeout evidence 2026-07-22: the Linux x86-64 native target executes the complete
    compile/package/install/debug/crash/reload/quarantine/rejection cycle, while
    the strict six-target matrix contract self-test covers the portable target
    schema. The local AOT gate set, Rust size/quality gate, roadmap integrity,
    and locked Cargo check pass without requiring a commit, push, upload,
    external account, or retained hosted artifact.
  - Acceptance: a repository and installed-release scan finds no default or
    fallback execution of serialized Terlan instructions.

- [x] Slice 101G: guarantee stack-safe tail recursion through compiler-owned
    lowering.
  - Hard decision: tail recursion is a Terlan language and compiler contract,
    not an optimization delegated to Cranelift, the host ABI, linker behavior,
    or a target runtime. A normal native `call` followed by `return` does not
    satisfy this contract even when one platform happens to reuse the frame.
  - Requirement: perform typed tail-position analysis after CoreIR control-flow
    normalization. Preserve tail position through function bodies, terminal
    `let` bodies, every selected `if` and `case` branch, and other
    result-forwarding control forms without misclassifying argument evaluation,
    operators, constructors, cleanup, or work performed after a call.
  - Requirement: lower direct self tail recursion to an explicit native loop
    with parallel argument replacement and a backedge. Managed arguments must
    transfer ownership exactly once, preserve live roots across safepoints, and
    release superseded values without leaking or exposing a partially updated
    parameter set.
  - Requirement: lower statically resolved mutually recursive call components
    to one bounded dispatcher or trampoline with typed function identities and
    argument layouts. The guarantee must cover recursive components linked
    across admitted application modules and fail before native linking when an
    indirect or dynamically replaceable target cannot satisfy the declared
    constant-stack contract.
  - Requirement: retain the native VM's existing suspension-aware tail-call
    behavior. Native tail calls that park an actor forward the callee transition
    and resume identity without retaining a caller continuation or native
    frame; mixed pure and suspending recursive components use one coherent
    ownership and failure model. This is not a JavaScript actor-runtime
    requirement.
  - Requirement: give every supported executable backend the same observable
    stack-safety contract for the typed Terlan constructs admitted by that
    backend. Cranelift must emit compiler-structured loops or trampolines, and
    the maintained JavaScript backend must trampoline admitted pure recursive
    components rather than depending on engine tail-call support. JavaScript
    operations whose actor, mailbox, suspension, or capability semantics are
    intentionally unsupported must remain loud target errors; this slice must
    not invent a JavaScript actor runtime to claim parity. The generated-Rust
    neutrality probe is not a release backend and cannot count as evidence.
  - Tests: execute at least 1,000,000 direct self tail calls and mutually
    recursive calls in stack-limited child processes; cover terminal calls
    selected through `let`, `if`, and `case`; managed aggregate and collection
    arguments; and checked arithmetic failure across the shared admitted
    corpus. Native-AOT evidence must additionally cover actor suspension and
    resume inside a recursive component, cancellation, and hot-reload
    generation retention. Prove non-tail recursion remains distinguishable and
    cannot be silently rewritten with changed evaluation, destruction, or
    stack-trace behavior.
  - Gate: add `make tail-recursion-lowering-check`.
  - Acceptance: direct-AOT and maintained JavaScript execution complete the
    shared admitted deep-recursion corpus with bounded stack growth and
    identical results, checked failures, and value/identity behavior. Native
    AOT additionally preserves actor ownership, suspension/resume,
    cancellation, and generation-retention behavior without requiring or
    fabricating corresponding JavaScript runtime facilities. Object inspection
    proves self recursion contains a backedge rather than a recursive call,
    recursive components use the compiler-owned trampoline, and disabling any
    tail-position transformation makes the gate fail.
  - Inventory this slice after its gate passes and move the durable
    tail-position, recursive-component, ownership, and backend contracts into
    the compiler and executable-image documentation.

- [x] Slice 101H: prove pure termination where possible and persistent-actor
    productivity where termination is intentionally impossible.
  - Hard decision: termination and stack safety are independent properties.
    Tail recursion may be stack-safe and divergent. Failure to prove
    termination means `unproven`, not `divergent`, and cannot reject an
    otherwise valid runtime function unless its execution context explicitly
    requires total behavior.
  - Requirement: infer termination for a useful typed subset using structural
    descent over recursive values, guarded integer descent, lexicographic
    measures, and size-change analysis over mutually recursive call
    components. Every proof must show a well-founded measure that strictly
    decreases on every recursive edge under the selected branch constraints.
  - Requirement: attach deterministic termination evidence to checked CoreIR,
    keep it independent of backend optimization, and expose a stable reason
    when a function is proven terminating or remains unproven. Compile-time
    evaluation and other contexts that require total behavior must consume
    validated evidence rather than a recursion-depth heuristic.
  - Requirement: do not impose termination on actor execution. Persistent
    actor loops are valid intentional nontermination, while finite worker
    actors remain valid and may terminate normally. The compiler must classify
    actor behavior from typed process construction and runtime operations
    without introducing an `actor` keyword or mandatory source annotation.
  - Requirement: analyze persistent actor loops for productivity instead.
    Every unbounded cycle must contain a compiler safepoint, scheduler
    reduction boundary, receive, yield, timer wait, or bounded asynchronous
    capability transition. Native loop backedges must permit preemption,
    cancellation, supervised shutdown, failure delivery, and runtime
    inspection within a bounded reduction budget.
  - Requirement: keep message handlers independently analyzable. A persistent
    mailbox loop may run forever while each selected handler terminates,
    suspends, or reaches a bounded scheduler handoff; one CPU-bound message
    cannot permanently starve peer actors or suppress cancellation.
  - Requirement: emit proof-visible termination and productivity obligations
    through the formal pipeline. Lean validation may confirm supported
    certificates, but absence of a Lean proof cannot be silently converted
    into a termination claim or prevent an intentionally persistent actor from
    running.
  - Tests: prove structural list recursion, guarded countdown, lexicographic
    descent, and mutual size-change termination; reject forged measures and
    recursive edges that do not decrease. Execute persistent receive loops,
    finite workers, busy tail loops, suspension/resume, mailbox pressure,
    cancellation, supervisor shutdown, and failure recovery under deterministic
    scheduler traces and bounded wall-clock watchdogs.
  - Gate: add `make termination-productivity-analysis-check`.
  - Acceptance: machine-readable evidence distinguishes proven termination,
    unproven termination, intentional persistent execution, and productive
    actor loops. Pure proof fixtures validate independently of Cranelift, while
    actor fixtures demonstrate bounded preemption and shutdown without
    requiring their main loop to terminate.
  - Inventory this slice after its gate passes and move durable termination
    evidence, actor-productivity, safepoint, and scheduler-budget contracts into
    the compiler, formal-pipeline, and VM runtime documentation.

- [x] Slice 101I: reject accidental same-scope binding shadowing while
    preserving explicit nested lexical scopes.
  - Hard decision: Terlan keeps binding, matching, equality, and mutation as
    separate operations. `let pattern = value` introduces immutable names,
    `case` selects patterns, `==` compares values, grouped `<- ... else`
    handles recoverable refutable binding, and indexed `=` remains an explicit
    trait-backed update. Terlan does not adopt Erlang's context-sensitive
    bind-or-match `=` operator.
  - Requirement: reject a name introduced more than once in the same lexical
    binding region. One region includes function-head parameters plus the
    function body's top-level sequential `let` chain, lambda parameters plus
    its top-level binding chain, every individual pattern, and every grouped
    refutable-let success chain. Repeating `let x = ...; let x = ...` is a
    diagnostic, not an equality assertion or an implicit replacement.
  - Requirement: permit intentional shadowing only after entering a genuinely
    nested region such as a selected `case` or `if` branch, nested lambda,
    comprehension body, handler body, or another compiler-defined nested
    lexical region. A nested region receives fresh binding identities and
    cannot mutate or retroactively constrain the outer immutable value.
  - Requirement: reject duplicate names introduced by tuple, list, map,
    struct, constructor, string-capture, binary-layout, shape-expanded,
    function-head, and alias patterns. Repeated names do not acquire Erlang's
    implicit equality meaning; express an identity requirement with a guard or
    explicit `==`.
  - Requirement: detect collisions after hygienic shape and macro expansion but
    before type inference mutates a string-keyed local environment. CoreIR must
    carry stable binding identities so backend lowering, source maps, rename,
    debugger locals, closure capture, and incremental invalidation cannot
    confuse two same-spelled bindings from different nested scopes.
  - Requirement: preserve pattern-failure behavior independently from
    shadowing. Refutable `let` assertions still produce structured
    `MatchError`, grouped `<- ... else` remains transactional, and failed
    matching commits no partial bindings.
  - Tooling: formatter output must never introduce a collision while migrating
    repeated lets. LSP diagnostics, rename, references, semantic tokens, and a
    rename code action must identify the exact binding and suggest a
    non-colliding name without rewriting an outer or sibling scope.
  - Tests: cover parameters followed by colliding lets, long sequential chains,
    duplicate names within every structural pattern family, aliases, shape and
    macro expansion, grouped fallible lets, closure captures, debugger locals,
    formatter migration, and incremental rebuilds. Positive fixtures must
    prove same-spelled names remain valid and distinct across nested branches,
    lambdas, comprehensions, and handlers.
  - Gate: add `make binding-shadowing-safety-check`.
  - Acceptance: parser, typechecker, CoreIR, Cranelift, JavaScript, formatter,
    LSP, debugger, and incremental-cache fixtures agree on exact binding
    identities. Same-region collisions fail with stable source spans, nested
    shadowing preserves the outer value, and no `=` expression changes meaning
    based on whether a name was previously bound.
  - Inventory this slice after its gate passes and move durable binding-region,
    pattern-identity, diagnostic, and tooling contracts into the grammar,
    compiler, and editor documentation.

### Roadmap Gate Integrity

Current validated inventory: 205 planned gates, 43 unchecked slices, and 588 Make targets.

- Gate: `make roadmap-gate-integrity-check`.
- Purpose: keep the active roadmap synchronized with planned gates, unchecked
  slice ownership, the Make graph, and the Quality Enforcement Rule.
- Policy: completed implementation history belongs in the implemented roadmap
  archive, release notes, or gate documentation, not in this active roadmap.

## Planned Gates

These commands define the closeout surface. Newly named gates must be added to
the Make graph before 0.0.7 release.

```bash
make callable-syntax-cleanup-check
make value-lifecycle-contract-check
make repeated-let-syntax-check
make language-feature-coverage-100-check
make operator-coverage-100-check
make core-type-contracts-check
make type-alias-shorthand-check
make compiler-purity-metadata-check
make comprehension-guards-check
make string-pattern-matching-check
make flexible-shape-guards-check
make shape-implications-check
make shape-synonyms-check
make binary-bitstring-processing-check
make function-head-pattern-parameters-check
make pattern-matching-support-check
make lean-proof-track-check
make lean-proof-feature-binding-check
make lean-proof-change-impact-report
make lean-proof-feature-binding-review
make lean-proof-snapshot-consistency-check
make lean-proof-counterexample-check
make lean-proof-feature-cull-check
make lean-proof-track-pr-gate
make lean-proof-track-regression-check
make lean-proof-track-runtime-check
make release-artifacts-closeout-check
make proof-readiness-release-mode-check
make terlan-lint-style-profile-check
make terlan-lint-style-check
make terlan-lint-pipe-canonicalization-check
make std-test-honesty-check
make std-test-table-check
make std-test-property-check
make js-type-emission-contract-check
make std-package-coverage-100-check
make std-range-check
make std-random-check
make std-regex-check
make stdlib-release-tests-vm-default-check
make all-terlan-tests-vm-check
make terlc-build-executable-check
make terlan-vm-run-command-check
make vm-release-install-validation-check
make rust-build-feature-shipping-check
make wasm-coreir-lowering-check
make wasm-runtime-exec-check
make cpp-binding-generator-check
make terlan-polars-package-check
make terlan-pytorch-package-check
make ml-experiments-check
make cuda-package-availability-check
make cuda-package-check
make typed-template-interpolation-check
make angular-ts-terlan-integration-check
make terlan-vm-erl-suite-audit-check
make vm-http-concurrency-investigation-check
make vm-http-vs-axum-check
make vm-http-benchmark-comparability-check
make vm-http-runtime-attribution-check
make vm-http-soak-stability-check
make vm-http-acme-tls-production-check
make vm-http-acme-worker-migration-check
make vm-http-acme-cache-custody-check
make vm-http-acme-renewal-rotation-check
make vm-http-protocol-readiness-check
make vm-http-serve-config-check
make vm-runtime-observability-check
make vm-runtime-inspector-check
make vm-supervision-restart-check
make vm-process-state-recovery-check
make vm-timer-deadline-check
make vm-memory-heap-pressure-check
make vm-native-boundary-contract-check
make vm-postgres-runtime-check
make vm-sql-macro-validation-check
make vm-db-migration-command-check
make vm-dev-dependency-orchestration-check
make vm-release-artifact-matrix-check
make release-promotion-pipeline-check
make release-example-projects-check
make release-diagnostic-catalog-check
make release-compatibility-baseline-check
make release-supply-chain-provenance-check
make release-security-hardening-check
make release-support-bundle-check
make release-performance-baseline-check
make release-readiness-attestation-check
make release-staged-distribution-verification-check
make release-notes-accuracy-check
make release-version-channel-check
make release-generated-artifacts-check
make release-code-hygiene-check
make release-project-upgrade-matrix-check
make release-reference-app-suite-check
make release-adversarial-corpus-check
make release-mutation-check
make release-fault-injection-check
make native-no-std-target-feasibility-check
make device-target-planner-check
make package-resolver-reproducibility-check
make package-capability-contract-check
make package-release-test-matrix-check
make package-api-compatibility-check
make package-cli-workflow-check
make package-editor-integration-check
make package-cache-integrity-check
make package-workspace-graph-check
make package-build-artifact-isolation-check
make source-map-debug-info-check
make compiler-incremental-cache-check
make watch-mode-hot-reload-check
make aot-developer-hot-reload-check
make release-flake-detection-check
make release-gate-shard-resume-check
make release-gate-duration-budget-check
make release-gate-report-schema-check
make release-failure-reproduction-check
make dev-fast-feedback-profile-check
make docs-codeblock-executable-check
make editor-definition-navigation-check
make editor-code-action-auto-import-check
make editor-completion-signature-check
make editor-runnable-debug-launch-check
make editor-semantic-token-icon-check
make editor-diagnostic-parity-check
make editor-extension-install-update-check
make target-inference-default-vm-check
make runtime-aot-only-check
make tvm-direct-aot-backend-check
make tvm-managed-list-profile-benchmark-check
make terlan-vm-artifact-format-check
make tvm-native-image-format-check
make tvm-native-image-loader-check
make tvm-aot-consumer-check
make tvm-aot-shard-ownership-check
make tvm-aot-supervisor-lifecycle-check
make tvm-aot-capability-worker-check
make tvm-aot-image-lifetime-check
make tvm-aot-lowering-coverage-check
make tail-recursion-lowering-check
make termination-productivity-analysis-check
make binding-shadowing-safety-check
make tvm-aot-http-performance-check
make tvm-aot-platform-matrix-check
make vm-multicore-invariant-inventory-check
make vm-actor-mutator-ownership-check
make vm-multicore-mailbox-publication-check
make vm-multicore-fixed-placement-check
make tvm-aot-multicore-migration-check
make vm-multicore-work-stealing-check
make vm-multicore-runtime-cleanup-check
make vm-multicore-runtime-integration-check
make vm-epmd-discovery-check
make vm-multicore-replay-observability-check
make vm-multicore-performance-check
make vm-multicore-memory-model-check
make vm-multicore-thread-sanitizer-check
make vm-multicore-mc9-evidence-check
make vm-scheduler-fairness-check
make tvm-aot-runtime-transition-check
make tvm-managed-memory-check
make rust-quality-check
make roadmap-gate-integrity-check
make check
make vm-multicore-release-check
make tvm-aot-multicore-readiness-check
make tvm-aot-roadmap-reconciliation-check
make tvm-aot-c-abi-boundary-check
make tvm-aot-compilation-time-check
make tvm-single-image-artifact-check
make no-tvm-json-runtime-check
make no-vmir-interpreter-check
make vm-native-worker-runtime-check
make vm-io-reactor-runtime-check
make vm-http-handler-dispatch-check
make vm-http-handler-scheduler-fairness-check
make vm-http-stateful-actor-session-check
make vm-live-template-stream-check
make vm-live-template-client-protocol-check
make typed-template-render-mode-check
make web-asset-pipeline-check
make vm-web-security-policy-check
make vm-web-config-secret-boundary-check
make vm-web-observability-check
make vm-web-lifecycle-health-check
make vm-web-deployment-profile-check
make vm-web-route-schema-client-check
make vm-model-sync-store-check
make vm-persistent-actor-store-check
make vm-persistent-actor-schema-check
make vm-persistent-actor-compaction-check
make vm-persistent-actor-restore-check
make vm-persistent-actor-adapter-conformance-check
make vm-persistent-actor-performance-budget-check
make vm-persistent-actor-telemetry-check
make vm-persistent-actor-policy-check
make vm-semantics-vs-otp-check
make terlc-debugger-check
make achamp-adversarial-coverage-check
make roadmap-legacy-runtime-cleanup-check
make dormant-runtime-code-check
make no-default-tokio-runtime-check
make vm-tcp-stream-check
make terlan-vm-http-lane-check
```
