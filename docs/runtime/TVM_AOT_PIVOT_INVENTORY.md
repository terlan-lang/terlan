# TVM AOT Pivot Inventory

Status: interpreter-retirement record and active AOT cutover inventory for Terlan 0.0.7.

The retired checked-CoreIR runtime was an important preliminary implementation:
it replaced OTP-facing behavior incrementally, made language semantics
executable, and produced the first meaningful HTTP-handler performance and
attribution baselines. Those results remain valid predecessor evidence. The
runtime implementation itself is now retired so new execution behavior lands
in reusable VM services or AOT-native TVM code.

Classifications are closed:

- `reusable-runtime-semantics`: behavior retained behind the native image
  boundary.
- `compiler-internal-ir`: compiler-only representation that never becomes a
  runtime artifact.
- `temporary-migration-support`: a currently used bridge that must be replaced
  before transitional execution is removed.
- `deletion-debt`: tests, documentation, or tooling that exists only for the
  transitional format and must be deleted or rewritten.

No row in the last two classifications is release completion evidence.

Last named-consumer fallback audit: 2026-07-22. A consumer row may be
classified as `reusable-runtime-semantics` only after its executable evaluator,
serialized-instruction, sidecar, and runtime-selection paths have been deleted
and its negative proof is part of `make runtime-aot-only-check`.

| Path | Surface | Classification | Disposition |
| --- | --- | --- | --- |
| `Makefile` | quality-gate, native-consumer-test | reusable-runtime-semantics | Keep conformance and release gates on canonical descriptor and native-image execution tests, including the compiler-owned tail-recursion lowering gate. |
| `crates/terlan/src/benchmark/main.rs` | native-image-benchmark | reusable-runtime-semantics | Measure native image publication and descriptor-bound admission without serialized VMIR. |
| `crates/terlan/src/commands/build/README.md` | native-image-documentation | reusable-runtime-semantics | Document one native application image, public descriptor exports, cache ownership, and executable runtime bundling. |
| `crates/terlan/src/commands/build/build_test/tests/annotation_isolation_artifact_test.rs` | native-image-test | reusable-runtime-semantics | Prove annotations cannot rewrite the public descriptor export boundary. |
| `crates/terlan/src/commands/build/build_test/tests/artifact_test.rs` | native-image-test, target-artifact-test | reusable-runtime-semantics | Prove default and explicit VM builds emit descriptor-audited native images while other targets retain their own artifact contracts. |
| `crates/terlan/src/commands/build/build_test/tests/asm_labels_artifact_test.rs` | native-image-test | reusable-runtime-semantics | Prove local-call implementation symbols remain private while the supported public root alone enters the native descriptor. |
| `crates/terlan/src/commands/build/build_test/tests/debug_info_artifact_test.rs` | native-image-test | reusable-runtime-semantics | Prove public/private native code identities and compiler-owned source provenance through the native debug section. |
| `crates/terlan/src/commands/build/build_test/tests/dependency_test.rs` | native-image-test, compiler-frontend-test | reusable-runtime-semantics | Prove dependency closures typecheck while independently native dependency leaves enter the descriptor-audited application image. |
| `crates/terlan/src/commands/build/build_test/tests/deterministic_artifact_test.rs` | native-image-test | reusable-runtime-semantics | Prove native image bytes are reproducible across output roots and unchanged source rewrites. |
| `crates/terlan/src/commands/build/build_test/tests/embedded_line_coverage_artifact_test.rs` | native-image-test | reusable-runtime-semantics | Prove UTF-8-safe executable declaration ranges and line coverage through the native debug section. |
| `crates/terlan/src/commands/build/build_test/tests/executable_vm_artifact_test.rs` | native-package-test, compiler-frontend-test | reusable-runtime-semantics | Prove descriptor-gated native packages run without serialized VMIR while managed library shapes retain compiler-interface evidence. |
| `crates/terlan/src/commands/build/build_test/tests/import_constructor_test.rs` | compiler-frontend-test | reusable-runtime-semantics | Prove imported managed constructors and aliases through compiler-interface evidence without serialized runtime artifacts. |
| `crates/terlan/src/commands/build/build_test/tests/key_compatibility_test.rs` | native-image-test | reusable-runtime-semantics | Prove sealed descriptor integrity is populated while optional signing remains absent. |
| `crates/terlan/src/commands/build/build_test/tests/project_layout_test.rs` | native-image-test, compiler-frontend-test | reusable-runtime-semantics | Prove manifest layouts emit descriptor-audited native scalar leaves while managed and cross-module closures retain compiler-interface evidence. |
| `crates/terlan/src/commands/build/mod.rs` | artifact-producer | reusable-runtime-semantics | Route VM builds to one native application image and retain compiler interfaces for managed-only libraries. |
| `crates/terlan/src/commands/build/args.rs` | native-image-consumer-options | reusable-runtime-semantics | Accept the VM build target and reject removed runtime selectors before artifact production. |
| `crates/terlan/src/commands/build/package_git_test.rs` | native-image-test, compiler-frontend-test | reusable-runtime-semantics | Prove locked Git closures typecheck while independently native dependency leaves enter the descriptor-audited application image. |
| `crates/terlan/src/commands/build/vm_artifact.rs` | artifact-producer | reusable-runtime-semantics | Own native image compilation, deterministic cache publication, embedded descriptors, and native debug metadata without serialized runtime IR. |
| `crates/terlan/src/commands/build/vm_artifact/native_cache.rs` | artifact-producer | reusable-runtime-semantics | Keep atomic publication, locking, complete-entry verification, and stale-image cleanup independent from artifact serialization. |
| `crates/terlan/src/commands/build/vm_artifact/native_cache_test.rs` | artifact-producer-test | reusable-runtime-semantics | Prove complete native cache files replace atomically and publication removes stale `.tvm`, `.tvm.json`, and `.tvm.reuse` artifacts. |
| `crates/terlan/src/commands/build/vm_artifact/native_descriptor.rs` | artifact-producer | reusable-runtime-semantics | Keep canonical native descriptor construction independent from the transitional JSON projection. |
| `crates/terlan/src/commands/build/vm_artifact/native_image.rs` | artifact-producer | reusable-runtime-semantics | Keep the compiler-owned CoreIR-to-native-image, linking, and cache boundary shared by build, package, and REPL. |
| `crates/terlan/src/commands/build/vm_artifact/native_reuse.rs` | artifact-producer | reusable-runtime-semantics | Keep dependency-free source-to-image reuse on verified native descriptors and complete content-addressed cache entries without serialized VMIR. |
| `crates/terlan/src/commands/build/vm_artifact/orchestration.rs` | artifact-producer | reusable-runtime-semantics | Coordinate source closures directly into native application images or compiler-interface-only managed libraries. |
| `crates/terlan/src/commands/run/mod.rs` | native-image-consumer | reusable-runtime-semantics | Build and launch the emitted `.tvm` application image through the packaged native runner; reject unknown runtime selectors before build delegation. |
| `crates/terlan/src/commands/run/run_test.rs` | native-image-consumer-test | reusable-runtime-semantics | Prove run selects the native package launcher and rejects removed target and runtime selectors. |
| `crates/terlan/src/commands/test/mod.rs` | native-image-consumer | reusable-runtime-semantics | Compile each selected test application into one native image and fail when compilation produces no image or native export. |
| `crates/terlan/src/commands/test/vm_runner.rs` | native-image-consumer | reusable-runtime-semantics | Admit one `PureNativeExecutionShard`, validate every selected export, execute Boolean test results, and shut the shard down without an evaluator fallback. |
| `crates/terlan/src/commands/repl/mod.rs` | native-image-consumer | reusable-runtime-semantics | Compile prompt generations into native images, retain one admitted execution shard across prompts, and reject runtime selection or missing native exports. |
| `crates/terlan/src/commands/serve/handler.rs` | artifact-consumer, http-handler-path | reusable-runtime-semantics | Keep Request, Response, Router, middleware, template, WebSocket, and SSE orchestration on canonical managed ABI values and request-owned native entry/resume points. Immediate callbacks cancel and reject suspension rather than injecting a synchronous wake or selecting an interpreter. |
| `crates/terlan/src/commands/serve/handler_cache.rs` | artifact-consumer, http-handler-path | reusable-runtime-semantics | Cache only descriptor-admitted native images, immutable static router plans, and the shared VM session runtime. Source CoreIR remains compiler-local until image emission and is never retained by a cache entry or runtime lease. |
| `crates/terlan/src/commands/serve/args.rs` | native-image-consumer-options, http-handler-path | reusable-runtime-semantics | Reject removed handler-runtime selection before the HTTP server starts. |
| `crates/terlan/src/commands/serve/handler_cache_generation_test.rs` | artifact-consumer-test, http-handler-path | reusable-runtime-semantics | Seed renamed serialized bodies and legacy sidecars, compile a handler, execute its admitted native image, and prove stale artifacts were removed. |
| `crates/terlan/src/commands/emit_js/README.md` | javascript-backend-contract | reusable-runtime-semantics | Require stack-safe lowering for the pure typed subset admitted by JavaScript while retaining loud errors for native-only actor operations. |
| `crates/terlan/src/commands/emit_js/tail_recursion.rs` | javascript-object-backend | compiler-internal-ir | Emit compiler-owned loops and typed component dispatchers without depending on host-engine proper-tail-call support. |
| `crates/terlan/src/commands/emit_js/tail_recursion_test.rs` | javascript-backend-test | reusable-runtime-semantics | Execute one million direct and mutual calls, checked failure, non-tail control, and aggregate/collection identity through emitted JavaScript. |
| `crates/terlan/src/commands/emit_js/binding_identity_emit_test.rs` | javascript-backend-test | reusable-runtime-semantics | Execute nested same-spelled bindings and prove the JavaScript backend preserves both the selected inner identity and the unchanged outer value. |
| `crates/terlan/src/commands/serve/serve_test.rs` | artifact-consumer-test, http-handler-path | reusable-runtime-semantics | Keep native HTTP handler, option rejection, and cache coverage split into focused test modules. |
| `crates/terlan/src/commands/debug/mod.rs` | native-image-consumer, debugger-path | reusable-runtime-semantics | Parse native debugger requests, reject runtime selection, and render metadata from one admitted `.tvm` image. |
| `crates/terlan/src/commands/debug/session.rs` | native-image-consumer, debugger-path | reusable-runtime-semantics | Inspect embedded native debug records and admit the exact image through a `PureNativeExecutionShard`; source files and renamed JSON fail closed. |
| `crates/terlan/src/commands/debug/debug_test.rs` | native-image-consumer-test, debugger-path | reusable-runtime-semantics | Prove native debugger admission, renamed-JSON rejection, and removed runtime-option rejection. |
| `crates/terlan/src/commands/vm.rs` | native-image-consumer, hot-reload-path | reusable-runtime-semantics | Compile source execution into one native image and route reload batches through the compiler-owned native generation service. |
| `crates/terlan/src/commands/vm/native_reload.rs` | native-image-consumer, hot-reload-path | reusable-runtime-semantics | Compile each unique `.terl` watcher batch into one image, admit it before metadata publication, and ignore non-source serialized artifacts. |
| `crates/terlan/src/commands/vm_test.rs` | native-image-consumer-test, hot-reload-path | reusable-runtime-semantics | Prove native source execution and hot reload while stale output sidecars are removed and renamed serialized inputs remain ignored. |
| `crates/terlan/src/quality/vm_diagnostics_quality.rs` | quality-gate | reusable-runtime-semantics | Keep native image admission, descriptor, runtime object, resource, and NativeBoundary diagnostics under exact release selectors. |
| `crates/terlan/src/quality/vm_artifact_format.rs` | quality-gate | reusable-runtime-semantics | Enforce native descriptor bytes and reject serialized runtime fallback claims. |
| `crates/terlan/src/quality/vm_artifact_format_test.rs` | quality-gate-test | reusable-runtime-semantics | Exercise native-image conformance and serialized-runtime rejection language. |
| `crates/terlan/src/compiler/native_ir.rs` | native-ir | compiler-internal-ir | Keep Terlan NativeIR inside the compiler as the stable boundary before backend lowering, including Unit, Int, Float, Bool, and mixed numeric scalar proofs. |
| `crates/terlan/src/compiler/native_ir/cranelift.rs` | native-object-backend | compiler-internal-ir | Emit checked integer and finite Float scalar code directly through the pinned in-process Cranelift backend. |
| `crates/terlan/src/compiler/native_ir/cranelift/function.rs` | native-object-backend | compiler-internal-ir | Emit direct and mutual recursive loop headers, bounded component dispatch, parallel argument backedges, and precise managed tail roots. |
| `crates/terlan/src/compiler/native_ir/cranelift/managed_stack_map_test.rs` | native-object-backend-test | reusable-runtime-semantics | Inspect Cranelift IR to prove a managed tail parameter produces precise stack-map metadata across a safepoint. |
| `crates/terlan/src/compiler/native_ir/cranelift/tail_call.rs` | native-object-backend | compiler-internal-ir | Forward suspending tail transitions through a backedge or terminal native call without retaining a caller frame. |
| `crates/terlan/src/compiler/native_ir/cranelift/test_support.rs` | native-object-backend-test | compiler-internal-ir | Keep complete application-object test convenience outside the size-bounded production backend owner. |
| `crates/terlan/src/compiler/native_ir/tail_position.rs` | native-ir-transformation | compiler-internal-ir | Perform iterative application-global SCC analysis, reject dynamic recursive terminal targets, and classify result-forwarding recursive calls. |
| `crates/terlan/src/compiler/native_ir/tail_position_source_test.rs` | compiler-frontend-test, native-object-backend-test | reusable-runtime-semantics | Compile valid Terlan case recursion through every frontend and native stage, then execute one million edges on a small native stack. |
| `crates/terlan/src/compiler/native_ir/tail_position_test.rs` | native-object-backend-test | reusable-runtime-semantics | Prove direct, mutual, managed, suspending, cancellation, object, SCC stress, and transform-sensitivity tail-recursion invariants. |
| `crates/terlan/src/compiler/native_ir/binding_identity_source_test.rs` | compiler-frontend-test, native-object-backend-test | reusable-runtime-semantics | Compile checked binding identities through native application lowering and execute outer and nested same-spelled values. |
| `crates/terlan/src/compiler/typeck/core_ir.rs` | compiler-ir-definition | compiler-internal-ir | Keep CoreIR inside the compiler and lower it without serializing it as runtime code. |
| `crates/terlan/src/compiler/typeck/binding_identity.rs` | compiler-binding-analysis | compiler-internal-ir | Assign deterministic lexical region and immutable binding identities after expansion, reject same-region collisions, and resolve exact CoreIR references. |
| `crates/terlan/src/compiler/typeck/binding_identity_test.rs` | compiler-binding-test, formatter-binding-test | reusable-runtime-semantics | Prove collision diagnostics, structural patterns, nested scopes, macro hygiene, transactional groups, debugger locals, formatter migration, and incremental identity stability. |
| `crates/terlan/src/compiler/typeck/core_ir/termination.rs` | compiler-proof-analysis | compiler-internal-ir | Infer deterministic termination and actor-productivity evidence from checked CoreIR, reject forged certificates, and keep unproven distinct from divergent. |
| `crates/terlan/src/compiler/typeck/core_ir/termination_test.rs` | compiler-proof-test | reusable-runtime-semantics | Prove structural, guarded integer, lexicographic, mutual size-change, persistent actor, and forged-evidence behavior through source-backed checked CoreIR. |
| `crates/terlan/src/compiler/typeck/core_const_termination.rs` | compile-time-totality-bridge | compiler-internal-ir | Project const-only functions into proof-only Core shapes so compile-time calls consume the shared recomputable termination evidence without becoming runtime callables. |
| `crates/terlan/src/compiler/value_lifecycle.rs` | compile-time-totality-consumer | reusable-compiler-semantics | Admit local const-function calls through validated Core totality evidence and reject recursive imported bodies that lack such evidence instead of treating a depth cutoff as proof. |
| `crates/terlan/src/compiler/value_lifecycle_test.rs` | compile-time-totality-test | reusable-compiler-semantics | Execute proven recursion beyond the former depth cutoff and reject an unproven recursive const function with a stable totality diagnostic. |
| `crates/terlan/src/native_worker/main.rs` | external-capability-worker | reusable-runtime-semantics | Keep only bounded, versioned, capability- and scheduler-class-admitted external adapter RPC. Loading Terlan images, dispatching application exports, and owning actor heaps or continuations are forbidden. |
| `crates/terlan/src/runtime/native_image/control.rs` | native-control-protocol | reusable-runtime-semantics | Keep descriptor-bound native worker calls on the bounded binary TVM control protocol. |
| `crates/terlan/src/runtime/native_image/descriptor.rs` | native-image-descriptor | reusable-runtime-semantics | Keep the canonical format-1 descriptor codec independent from compiler IR and JSON. |
| `crates/terlan/src/runtime/native_image/image.rs` | native-image-admission | reusable-runtime-semantics | Embed, seal, and statically admit ELF, Mach-O, and PE/COFF TVM images before execution. |
| `crates/terlan/src/runtime/vm/pure_native.rs` | execution-shard-application-boundary | reusable-runtime-semantics | Keep all admitted application-image calls and local actor transitions inside the execution shard. This boundary contains no worker-process spawning or application-call framing. |
| `crates/terlan/src/runtime/vm/pure_native/direct_backend.rs` | execution-shard-image-backend | reusable-runtime-semantics | Load admitted Terlan AOT images once in the shard, invoke their fixed dispatch ABI directly, allocate through owner-scoped actor heaps, and retain precise managed continuation roots without application-call IPC. |
| `crates/terlan/tests/direct_aot.rs` | native-consumer-test | reusable-runtime-semantics | Prove CoreIR-to-object emission, linking, isolated loading, caching, integer and Float scalar ABI calls, and Terlan consumer execution. |
| `crates/terlan/tests/direct_aot_cache.rs` | native-consumer-test | reusable-runtime-semantics | Prove deterministic cache hits plus atomic recovery from missing or poisoned native cache members. |
| `crates/terlan/tests/direct_aot_condition.rs` | native-consumer-test | reusable-runtime-semantics | Prove suspending native conditions compose with enclosing control flow and resumable continuations. |
| `crates/terlan/tests/direct_aot_condition_expr.rs` | native-consumer-test | reusable-runtime-semantics | Prove conditional expressions preserve native continuation state across suspending branches. |
| `crates/terlan/tests/direct_aot_multi_stage_call.rs` | native-consumer-test | reusable-runtime-semantics | Prove bounded multi-stage callees preserve distinct continuation state through direct and enclosing native calls. |
| `crates/terlan/tests/direct_aot_non_tail_call.rs` | native-consumer-test | reusable-runtime-semantics | Prove suspending non-tail calls retain and resume their caller continuation. |
| `crates/terlan/tests/direct_aot_package.rs` | native-consumer-test | reusable-runtime-semantics | Prove package-wide native image emission, qualified exports, rebuild replacement, and Terlan consumer execution. |
| `crates/terlan/tests/direct_aot_tail_call.rs` | native-consumer-test | reusable-runtime-semantics | Prove suspending tail calls forward yields and resumptions without retaining a caller continuation. |
| `crates/terlan/tests/tvm_transition_rejection.rs` | native-consumer-test | reusable-runtime-semantics | Prove `.tvm.json`, stale sidecars, serialized instruction bodies, and JSON renamed to `.tvm` cannot enter the public native runtime boundary. |
| `crates/terlan/src/vm/main.rs` | artifact-consumer | reusable-runtime-semantics | Keep native image admission and execution with explicit `.tvm.json` rejection at the public VM CLI. |
| `crates/terlan/src/vm/main_test.rs` | artifact-consumer-test | reusable-runtime-semantics | Keep native descriptor, worker, transition, and benchmark coverage split into focused test modules. |
| `docs/runtime/EDITOR_DEBUGGER_SURFACE.md` | debugger-path | reusable-runtime-semantics | Define source compilation and native-image launch without a JSON artifact fallback. |
| `docs/runtime/TVM_EXECUTABLE_IMAGE_SPEC.md` | native-image-contract | reusable-runtime-semantics | Keep as the normative native image contract. |
| `docs/compiler/TERLAN_TAIL_RECURSION.md` | compiler-contract | reusable-runtime-semantics | Define source semantics, typed tail-position analysis, native and JavaScript lowering, native-only actor obligations, and release evidence. |
| `docs/compiler/TERLAN_TERMINATION_AND_PRODUCTIVITY.md` | compiler-runtime-contract | reusable-runtime-semantics | Define totality evidence, intentional actor persistence, productivity boundaries, and bounded native reduction yields. |
| `docs/compiler/TERLAN_BINDING_IDENTITIES.md` | compiler-binding-contract | reusable-runtime-semantics | Define immutable lexical regions, stable CoreIR identities, post-expansion collision analysis, and backend evidence validation. |
| `docs/editor/TERLAN_BINDING_NAVIGATION.md` | editor-binding-contract | reusable-runtime-semantics | Require exact-identity definitions, references, rename, semantic tokens, debugger locals, and duplicate-binding quick fixes. |
| `crates/terlan/src/lsp/binding_navigation.rs` | editor-binding-index | reusable-runtime-semantics | Project compiler binding evidence onto source tokens so local navigation and edits never merge same-spelled lexical identities. |
| `crates/terlan/src/lsp/binding_navigation_test.rs` | editor-binding-test | reusable-runtime-semantics | Prove nested identity separation and duplicate quick-fix targeting. |
| `crates/terlan/src/runtime/vm.rs` | runtime-services-only | reusable-runtime-semantics | Own actor, scheduler, transport, resource, persistence, and native-transition services; compiler IR execution is absent. |
| `crates/terlan/src/commands/serve/watch.rs` | change-detection, hot-reload-path | reusable-runtime-semantics | Detect filesystem changes and hand source batches to the native generation owner without carrying executable payloads. |
| `crates/terlan/src/runtime/vm/source_reload.rs` | generation-lifecycle, hot-reload-path | reusable-runtime-semantics | Publish admitted native generations transactionally and preserve drain, quarantine, and retirement ownership. |
| `crates/terlan/src/quality/watch_mode_hot_reload.rs` | quality-gate, hot-reload-path | reusable-runtime-semantics | Enforce native-image reload and generation-lifecycle invariants. |

## Frozen Rules

1. Compiler IR remains compiler-owned and cannot be retained or executed by a
   runtime consumer.
2. Every required execution surface must have its named surface tag and an
   allowed classification.
3. `temporary-migration-support` and `deletion-debt` rows cannot claim
   release-complete, final-runtime, or conforming-native status.
4. A missing AOT image, export, or managed ABI is a stable, loud error; it is
   never permission to add a compatibility execution path.
5. Historical HTTP benchmark reports from the retired runtime remain baseline
   evidence for the managed-AOT HTTP implementation to meet or beat.
6. Numeric Rust split fragments named `*_part_<digits>.rs` inherit the exact
   classification of their inventoried `*.rs` include wrapper; child modules
   and every other filename still require an explicit row.
7. Named consumer rows can move to `reusable-runtime-semantics` only after the
   executable fallback is deleted and `make runtime-aot-only-check` proves the
   native owner and negative boundary.
