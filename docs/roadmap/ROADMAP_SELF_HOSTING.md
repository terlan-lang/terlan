# Terlan Self-Hosting Roadmap

Status: implementation in progress on main
Created: 2026-08-29
Latest implementation checkpoint: 2026-10-22

## Implementation Status

The implementation now extends beyond the original source-interface slice. The
Terlan package contains canonical lexing, projected concrete and expression
parsing, callable and binding resolution, local type-constraint propagation,
semantic IR planning, ABI-1 backend emission, interface-catalog production,
and a runnable compiler driver. Guarded `case` clauses now survive both parser
paths and lower through the general root control-flow planner.

The current ABI-1 execution checkpoint is `image39`. Its two compiler modules
changed after closure 22 were rebuilt by the ABI-1-hosted `image38`, and those
artifacts and interfaces are byte-identical to the recovery-compiler outputs.
The promoted slice passes guarded-case backend emission and public-interface
filtering. This is a fixed point for the changed slice, not a full 200-unit
bootstrap-convergence claim.

Rust remains authoritative until every phase gate passes and a complete clean
stage-1/stage-2 build converges. The current implementation must not be
described as parser, type-system, compiler, or authority replacement yet.

Phase 0 bootstrap artifact locations and canonical comparison layers are now
declared in `compiler/self_host/bootstrap/BOOTSTRAP.toml`. Cross-language
schemas are owned by `compiler/self_host/SCHEMAS.md`, and the maintained corpus
has an explicit acceptance/rejection inventory in
`compiler/self_host/corpus/CORPUS.tsv`.

### Current ABI-1 checkpoint

- Recovery compiler: `target/quality/self-host-bootstrap-guard-recovery-36`.
- Last complete recovery-built closure: `target/quality/self-host-current-production-closure-22-recovery`.
- ABI-1 fixed-point compiler: `target/quality/self-host-bootstrap-current-39`.
- Self-produced changed-unit overlay: `target/quality/self-host-current-production-overlay-24`.
- Changed units at this checkpoint: `ProjectedBindingTypeMerge` and
  `ProjectedTypeConstraintResolution`.
- Fixed-point evidence: 2/2 backend artifacts and 2/2 interfaces are
  byte-identical across the recovery-built and ABI-1-built changed slice;
  diagnostics are empty.
- Promotion smoke evidence: guarded root `case` lowering contains
  `root_case_guard_1`, and the visibility fixture exports only `value/0`.

Target-directory paths are working evidence, not release provenance. A release
promotion record must replace them with source revision, source-closure digest,
artifact digest, compiler ancestry, target identity, and recovery identity.

## Objective

Move language semantics, compiler policy, and developer tooling from Rust into
Terlan while retaining Rust as the small trusted substrate for execution,
unsafe platform integration, and initial bootstrap.

Self-hosting is complete when a compiler written primarily in Terlan can build
its own source and the resulting compiler can repeat that build with equivalent
canonical outputs. Completion also requires the self-hosted compiler to meet
the compilation-performance gates below; correctness convergence is not enough
if normal development builds remain operationally impractical.

Self-hosting does not require rewriting the VM, Cranelift, operating-system
adapters, or NativeBoundary in Terlan.

## Design Boundary

### Terlan-owned target

- Compiler driver and build policy
- Package manifest interpretation
- Module discovery and dependency graphs
- Source loading policy
- Lexer and parser
- Canonical syntax tree
- Name and module resolution
- Type declarations and inference
- Trait, shape, and implication checking
- Guard, pattern, and exhaustiveness checking
- Desugaring and typed Core IR construction
- Target-independent optimization passes
- Diagnostics and source rendering
- Formatter and linter policy
- Documentation generation
- Test discovery and orchestration
- Web compiler and bundler policy
- Incremental dependency and invalidation policy

### Rust-owned substrate

- Stage-0 bootstrap compiler
- VM scheduler and actor runtime
- Managed heap and collection implementation
- Native resource ownership and generation enforcement
- NativeBoundary and capability workers
- Operating-system and low-level I/O integration
- Cranelift integration
- Object-file production and native linking
- C, C++, and Rust binding generation substrate
- Cryptographic and package transport primitives requiring native libraries

Rust components may implement mechanisms. They must not remain the sole owner
of language semantics after the corresponding Terlan phase becomes
authoritative.

## Bootstrap Architecture

The bootstrap chain has three explicit stages:

| Stage | Implementation | Built by | Purpose |
|---|---|---|---|
| `terlc0` | Current Rust compiler | Rust toolchain | Stable bootstrap root and recovery compiler |
| `terlc1` | Self-hosted Terlan compiler | `terlc0` | First compiler produced from Terlan sources |
| `terlc2` | Same Terlan compiler sources | `terlc1` | Bootstrap convergence evidence |

The repository must always identify which compiler owns each build step. A
release gate must reject implicit use of a compiler found elsewhere on `PATH`.

`terlc0` remains available after self-hosting for clean bootstrap, bisecting,
recovery, and verification. It does not remain the authoritative implementation
of migrated language semantics.

## Compilation Performance Requirements

Compilation performance is part of authority transfer, not deferred cleanup.
The self-hosted compiler must provide reproducible benchmark evidence for:

- Cold compiler startup and one-module compilation.
- Warm unchanged-package compilation with no repeated parsing, semantic
  analysis, or backend emission.
- Single-module edits that invalidate only the affected dependency closure.
- Deterministic parallel compilation of independent package modules.
- Full bootstrap and parity evidence without launching one fresh VM process per
  module and evidence layer.
- Peak memory and wall-clock regression budgets on the maintained bootstrap
  corpus.

The benchmark manifest must record source-closure identity, compiler ancestry,
VM identity, host target, worker count, elapsed time, and peak resident memory.
Release gates compare against an explicit checked-in baseline and fail on an
unapproved regression. Performance work must not bypass canonical evidence,
reduce corpus coverage, or weaken bootstrap fixed-point comparison.

## Backend Contract

The self-hosted compiler will not bind directly to Cranelift's Rust API. It will
emit a versioned, deterministic backend request containing:

- Target and calling-convention identity
- Typed Core IR
- Function, callable, and continuation identities
- Managed aggregate and collection layouts
- Stack-map and root information
- Native dependency and capability requirements
- Source maps and debug metadata
- Optimization profile

A Rust backend adapter will:

1. Validate the complete request before code generation.
2. Reject unsupported or unbounded constructs.
3. Lower accepted Core IR through Cranelift.
4. Produce the object and native-image descriptor.
5. Return structured diagnostics without exposing Rust or Cranelift types.

The request encoding must be versioned independently from ABI 1. Compiler
backend protocol changes must not silently alter the runtime ABI contract.

## Migration Policy

Every migrated subsystem passes through four authority states:

1. `rust-authoritative`: only the Rust implementation decides behavior.
2. `differential`: Rust and Terlan run on the same corpus and differences fail.
3. `terlan-authoritative`: Terlan decides behavior; Rust remains a checked fallback.
4. `bootstrap-only`: Rust remains only in `terlc0` and no longer participates in normal builds.

No subsystem may skip the differential state. No Rust implementation may be
removed before clean-bootstrap and failure-recovery procedures exist.

Each differential comparison must use canonical semantic outputs rather than
unstable debug text or native object bytes.

## Phase 0: Boundary And Evidence Foundations

### Deliverables

- Record the Rust/Terlan ownership boundary in compiler architecture docs.
- Define canonical source-span, token, syntax, diagnostic, and Core IR schemas.
- Add a self-hosted compiler package with no dependency on private Rust compiler types.
- Add explicit `terlc0`, `terlc1`, and `terlc2` artifact locations.
- Define deterministic corpus and report formats for differential execution.
- Define normalized artifact comparison rules.
- Add bootstrap provenance to compiler artifacts.

### Exit gates

- `self-host-boundary-check`
- `self-host-schema-version-check`
- `self-host-bootstrap-provenance-check`
- `self-host-corpus-integrity-check`

### Completion condition

The self-hosted package can be built by `terlc0`, and every cross-language
boundary is represented by a versioned data contract rather than direct access
to Rust compiler structures.

## Phase 1: Source Interface Vertical Slice

Implement a small end-to-end Terlan frontend that:

1. Reads one source module.
2. Tokenizes source without losing byte spans.
3. Recognizes module declarations and imports.
4. Recognizes public declarations and signatures.
5. Emits a canonical module-interface document.
6. Compares that document with the Rust frontend.

### Required corpus

- Empty and comment-only inputs
- Unicode identifiers and strings
- Every declaration family
- Every import form
- Public and private declarations
- Function clauses and argument patterns
- Shapes, traits, and implications
- Guards and refutable bindings
- Malformed and truncated source
- Stable diagnostic span cases

### Exit gates

- `self-host-token-differential-check`
- `self-host-interface-differential-check`
- `self-host-source-span-check`
- `self-host-malformed-source-check`

### Completion condition

Terlan and Rust produce equivalent tokens, public interface identity, and
diagnostic locations for the maintained corpus.

## Phase 2: Complete Lexer And Parser

### Deliverables

- Complete Terlan lexer.
- Complete parser for the released language grammar.
- Canonical typed syntax tree represented with sum types.
- Error recovery with bounded progress and stable diagnostic identifiers.
- Parser corpus generated from accepted and rejected language tests.
- Differential reducer that minimizes parser disagreements.

The parser implementation may consume generated parse tables, but table
generation and runtime interpretation must have deterministic schemas. A parser
generator must not become an undocumented second language specification.

### Exit gates

- `self-host-lexer-complete-check`
- `self-host-parser-differential-check`
- `self-host-parser-recovery-check`
- `self-host-parser-fuzz-check`
- `self-host-syntax-roundtrip-check`

### Completion condition

The Terlan parser becomes authoritative for normal compiler builds. The Rust
parser remains available through `terlc0` and differential release testing.

## Phase 3: Modules, Names, And Packages

### Deliverables

- Module graph construction.
- Import and visibility resolution.
- Package manifest interpretation.
- Dependency graph validation.
- Duplicate, ambiguous, cyclic, and inaccessible-name diagnostics.
- Canonical public interface hashing.
- Incremental invalidation based on interface identity.

### Exit gates

- `self-host-module-graph-differential-check`
- `self-host-name-resolution-differential-check`
- `self-host-package-graph-check`
- `self-host-interface-hash-stability-check`

### Completion condition

Terlan owns module discovery, package graph semantics, imports, visibility, and
public interface identity.

## Phase 4: Type System And Pattern Semantics

### Deliverables

- Type declaration and constructor checking.
- Function-clause and argument-pattern checking.
- Local and exported type inference.
- Trait declaration and implementation checking.
- Shape checking and shape-pattern reuse.
- Structural implication checking.
- Guard type checking and narrowing.
- Exhaustiveness and unreachable-clause analysis.
- Ownership and effect metadata required by later lowering.

The implementation must use explicit typed representations. `Dynamic` must not
be introduced as an internal shortcut around incomplete compiler modeling.

### Exit gates

- `self-host-type-differential-check`
- `self-host-trait-conformance-check`
- `self-host-shape-conformance-check`
- `self-host-implication-conformance-check`
- `self-host-pattern-exhaustiveness-check`
- `self-host-guard-narrowing-check`

### Completion condition

Terlan owns all user-visible acceptance, rejection, and diagnostic semantics
through the typed syntax stage.

## Phase 5: Typed Core IR And Lowering

### Deliverables

- Versioned typed Core IR schema.
- Desugaring from typed syntax into Core IR.
- Pattern decision trees.
- Closure and capture representation.
- Actor send, receive, suspension, and continuation representation.
- Native capability and resource operations.
- Managed layout requirements.
- Target-independent validation and optimization passes.
- Canonical Core IR serializer and semantic hash.

### Exit gates

- `self-host-core-ir-differential-check`
- `self-host-pattern-lowering-check`
- `self-host-closure-lowering-check`
- `self-host-actor-lowering-check`
- `self-host-native-operation-lowering-check`
- `self-host-core-ir-determinism-check`

### Completion condition

Equivalent Rust and Terlan inputs produce equivalent validated Core IR and
managed-layout requirements.

## Phase 6: Rust Backend Adapter

### Deliverables

- Versioned Core IR backend request codec.
- Complete request validation before Cranelift invocation.
- Rust adapter from validated Core IR to existing native IR and Cranelift.
- Structured backend diagnostics.
- Target matrix evidence.
- Adversarial malformed-request and resource-bound tests.

### Exit gates

- `self-host-backend-request-check`
- `self-host-backend-admission-check`
- `self-host-cranelift-equivalence-check`
- `self-host-backend-resource-bound-check`
- `self-host-backend-cross-target-check`

### Completion condition

The Terlan frontend and middle-end can produce complete executable artifacts
without invoking Rust frontend or type-system code.

## Phase 7: Self-Hosted Driver And Tooling

### Deliverables

- Build and test command orchestration in Terlan.
- Incremental compilation ownership in Terlan.
- Formatter, linter, documentation, and package workflows in Terlan.
- Compiler diagnostics rendered by Terlan.
- Build graph parallelism using bounded module workers.
- Support-bundle and compiler trace production.

Compiler transformations remain ordinary deterministic functions. Actors are
used for module ownership, bounded parallelism, cancellation, and isolation,
not as a mandatory abstraction for every compiler pass.

### Exit gates

- `self-host-driver-parity-check`
- `self-host-incremental-equivalence-check`
- `self-host-tooling-parity-check`
- `self-host-cancellation-check`
- `self-host-build-reproducibility-check`

### Completion condition

Normal development uses the Terlan compiler driver. Rust remains responsible
only for bootstrap and substrate components.

## Phase 8: Bootstrap Convergence

### Required sequence

1. Build `terlc1` from self-hosted sources using `terlc0`.
2. Build `terlc2` from the same sources using `terlc1`.
3. Compile the maintained language, standard-library, package, and web corpora
   with both compilers.
4. Compare canonical tokens, syntax, interfaces, diagnostics, typed Core IR,
   actor continuation IR, descriptors, and runtime behavior.
5. Compare normalized build artifacts and actor-aware native objects after
   removing only documented variable metadata.

### Exit gates

- `self-host-stage1-build-check`
- `self-host-stage2-build-check`
- `self-host-bootstrap-convergence-check`
- `self-host-actor-bootstrap-convergence-check`
- `self-host-actor-profile-catalog-check`
- `self-host-actor-native-object-check`
- `self-host-clean-machine-bootstrap-check`
- `self-host-recovery-bootstrap-check`
- `self-host-release-corpus-check`

### Completion condition

`terlc1` and `terlc2` are semantically equivalent across Core IR and actor
continuation IR, the clean bootstrap is reproducible, and no normal compiler
path depends on Rust-owned language semantics.

## Phase 9: Authority Transfer And Architecture Hardening

Bootstrap convergence is necessary but is not sufficient for authority
transfer. Source rewrites introduced only to fit an incomplete bootstrap
backend are transitional scaffolding, not the preferred architecture of the
self-hosted compiler.

### Deliverables

- Complete semantic-IR and backend lowering for nested `if` and `case`
  expressions, wide clause tables, branch-local bindings, and nested
  expression control flow.
- Complete lowering for tuple, list, list-cons, constructor, literal,
  wildcard, and binding patterns wherever the released grammar permits them.
- Pattern decision trees that preserve guards, clause order, exhaustiveness,
  source spans, and stable diagnostic identity without requiring manual helper
  decomposition.
- Removal of bootstrap-only source-shape restrictions from the backend
  admission contract.
- Refactoring of helper-expanded bootstrap modules back to idiomatic Terlan
  after the corresponding backend constructs are supported.
- Canonical before-and-after evidence proving that architectural cleanup does
  not change tokens, syntax, interfaces, diagnostics, typed Core IR, backend
  requests, or runtime behavior.
- Complexity and resource bounds for self-analysis, including bounded compile
  time, memory use, recursion, diagnostic growth, and incremental invalidation
  for every compiler unit.
- A maintained compiler-source regression corpus containing the nested control
  flow and structural patterns that previously required bootstrap workarounds.
- A documented minimal trusted base. Every compiler module outside that base
  must produce an interface and backend artifact without an ad hoc exclusion.
- Removal of Rust frontend and type-system participation from normal build,
  test, package, web, and release paths.
- Promotion metadata that names the authoritative Terlan compiler image, its
  source closure, canonical artifact digest, bootstrap ancestry, and recovery
  compiler identity.
- Self-hosted authority for actor effect propagation, continuation ordinals,
  CFG splitting, capture liveness, actor sidecar emission, and actor-aware
  native object tooling, with stage-1/stage-2 actor artifact convergence.
- Actor IR v2 profile catalogs derived from outward continuations, consumed as
  dependency interfaces, and converged independently across bootstrap stages.
- Bounded in-compiler actor profile iteration must replace provisional local
  seeds with liveness-derived outward-entry sets before sidecar publication;
  imported and runtime-owned native profiles remain explicit ABI evidence.
- `actor-backend-unit` must compile through the self-hosted projected frontend
  and publish Core, variant, and actor sidecars behind a last-written commit
  marker; Phase 9 tooling authority cannot rely on a Rust-only artifact driver.

### Architectural invariants

- The compiler may use helper functions when they improve its design, but no
  helper decomposition may be required solely because the backend cannot lower
  valid Terlan syntax.
- The self-hosted compiler must exercise the same released syntax and semantic
  contracts available to ordinary Terlan applications.
- Compiler implementation convenience must not create an undocumented subset
  of Terlan accepted only during bootstrap.
- Backend completeness is measured using typed semantic constructs, not by
  matching incidental source text or compiler-module names.
- Restoring idiomatic source must preserve deterministic bootstrap convergence.
- Performance exceptions for self-analysis must be explicit, bounded, and
  covered by release gates; pathological units cannot be silently excluded.

### Current workaround inventory

- `ProjectedBindingTypeMerge` now uses typed `Map` indexes constructed from
  canonical entries; its association-list bootstrap substitution is removed.
- `ProjectedTypeConstraintResolution` now uses
  `Map[Int, Option[String]]` and the mutable-receiver `put` command contract;
  repeated node facts replace the indexed value while fact and diagnostic
  lists retain source order.
- Callable metadata, typed semantic IR, and ABI lowering now preserve mutable
  receiver identity and command-return projection. The restored compiler
  source therefore exercises the same managed `Map` contract as applications.
- Source restoration is implemented, but the idiomatic-source and post-cleanup
  convergence gates remain pending until reference and candidate artifacts
  prove token, syntax, interface, diagnostic, typed Core IR, backend-request,
  native-object, and runtime equivalence.
- The bounded package stage reuses stable interfaces and invalidates backend
  artifacts when source, interface manifests, or the compiler image changes.
  Full convergence still requires measured per-unit time and memory limits plus
  complete stage-1/stage-2 closure evidence before this path can become a
  release gate.

### Immediate implementation order

1. Materialize canonical reference evidence for the last recovery-built
   association-list image and candidate evidence for the restored `Map` image.
2. Compare tokens, syntax, interfaces, diagnostics, typed Core IR, backend
   requests, native objects, and runtime behavior for both restored modules.
3. Record measured compile-time and memory bounds for the restored modules and
   the complete compiler source closure.
4. Produce complete stage-1 and stage-2 closures through the self-hosted driver.
5. Run every Phase 8 and Phase 9 production evidence-plan row, including
   `post-cleanup-converged`, with last-written completion markers.
6. Change authority metadata only after the production evidence pipeline emits
   a ready decision from those persisted artifacts.

### Exit gates

- `self-host-backend-language-completeness-check`
- `self-host-bootstrap-workaround-inventory-check`
- `self-host-idiomatic-source-restoration-check`
- `self-host-pattern-decision-tree-check`
- `self-host-self-analysis-resource-bound-check`
- `self-host-no-ad-hoc-exclusion-check`
- `self-host-post-cleanup-convergence-check`
- `self-host-authority-transfer-check`

### Completion condition

The Terlan compiler is promoted as authoritative only after it accepts its
idiomatic source without bootstrap-only structural rewrites, recompiles that
source with equivalent canonical outputs, satisfies documented self-analysis
resource bounds, and leaves Rust language semantics reachable only through the
identified `terlc0` bootstrap and recovery path.

## Differential Evidence

Each phase emits a report containing:

- Schema version
- Source revision
- `terlc0`, `terlc1`, and when applicable `terlc2` identities
- Corpus digest
- Target identity
- Cases executed
- Accepted and rejected case counts
- Canonical output digests
- Diagnostic identity and span comparisons
- Classified differences
- Failure reproducer paths

Reports from different source revisions cannot be combined into one release
candidate.

## Canonical Comparison Layers

Bootstrap comparisons occur in this order:

1. Tokens and byte spans
2. Syntax tree and declaration identity
3. Public module interfaces
4. Diagnostics and stable diagnostic identifiers
5. Typed syntax and inferred types
6. Core IR and semantic hashes
7. Managed layouts and native-image descriptors
8. Runtime behavior
9. Normalized executable artifacts

A later layer cannot hide disagreement in an earlier layer.

## First Implementation Milestone

Create the self-hosted compiler package with these initial modules:

- Source text and byte-span model
- Token and lexical-error sum types
- Deterministic lexer
- Module-header and import parser
- Public declaration/interface extractor
- Canonical interface serializer
- Differential corpus runner

The first milestone intentionally excludes full expression parsing, type
inference, Core IR, Cranelift interaction, and compiler replacement.

Its release gate is successful only when malformed cases are tested alongside
successful cases and both implementations agree on the complete maintained
corpus.

## Release Policy

Self-hosting phases are assigned to numbered releases only after the preceding
phase's gates exist and pass continuously. Release dates must not force an
authority transfer.

The runtime ABI lifecycle remains independent. Self-hosting ABI 1 consumers does
not freeze ABI 1, and an ABI freeze does not imply compiler bootstrap
convergence.

## Non-Goals

- Reimplementing Cranelift in Terlan
- Eliminating Rust from the trusted runtime substrate
- Exposing Cranelift Rust types through language bindings
- Rewriting every subsystem before any Terlan implementation becomes useful
- Using actors for every AST node or compiler pass
- Treating successful compilation alone as bootstrap equivalence
- Removing `terlc0` before clean recovery bootstrap exists

## Completion Definition

Terlan is self-hosting when all of the following are true:

- The authoritative frontend, type system, lowering, compiler driver, and
  target-independent optimization passes are written in Terlan.
- `terlc0` builds `terlc1` from a clean checkout.
- `terlc1` builds a semantically equivalent `terlc2`.
- The standard library and maintained package corpus build and test with
  `terlc2`.
- Cross-target native images are produced through the validated Rust backend
  adapter.
- Actor-aware native objects are produced from matching self-hosted Core IR and
  actor sidecars, and both artifacts converge independently across stages.
- Differential and recovery gates remain part of every release candidate.
- Rust-owned code no longer defines user-visible language semantics outside the
  documented bootstrap implementation.
- The authoritative compiler source no longer contains helper expansion or
  syntax avoidance required solely by incomplete self-hosted lowering.
- Every compiler module outside the documented minimal trusted base produces
  deterministic interface and backend artifacts without ad hoc exclusions.
- Self-analysis compile time, memory use, recursion, and diagnostic growth stay
  within the Phase 9 release bounds.

### Phase 2 authority checkpoint (2026-10-16)

The lexer/parser authority gates now have concrete targets and complete checked-in
corpora:

- `self-host-lexer-complete-check`
- `self-host-parser-differential-check`
- `self-host-parser-recovery-check`
- `self-host-parser-fuzz-check`
- `self-host-syntax-roundtrip-check`
- aggregate `self-host-phase2-complete-check`

Current component evidence is token parity 4/4, accepted surface parity 4/4,
concrete syntax parity 4/4, exact normalized rejection diagnostics 11/11,
fragment parity 17/17, recoverable token-stream continuation, and formatter
roundtrip/idempotence 4/4. The deterministic rejected corpus includes minimized
unclosed-call, unclosed-list, invalid-case, invalid-generic, and recovery-tail
mutations. Corpus integrity requires every accepted/rejected fixture to be
listed, preventing silent coverage shrinkage.

Projected evidence execution now supports batched error, surface, and mixed
fragment requests. Error and surface evidence are content-addressed, so a cold
self-host compile is paid once per relevant source identity rather than once per
fixture. Rejection parity delegates to the stronger exact parser-error gate.

The expanded corpus exposed and fixed three implementation defects: empty
generic argument lists were accepted by `ProjectedTypeParser`; unbalanced
expression groups were misclassified as generic unterminated declarations; and
the Rust formatter emitted invalid `try` clauses and separated raw-macro names
from their opening brace.

The optimized ABI-1 runner still has a distinct runtime defect: selected
same-module `CallImport` instructions can report `MissingExport` even though
`loaded_program` resolves the same MFA before execution. `CallLocal` is not a
valid workaround because it is a raw jump and does not establish managed call
frames. Until VM import dispatch is repaired, affected authority gates use the
trusted projected executor; token and concrete modes continue to use image 39.
## Phase 2 image authority correction (2026-10-16)

- The previously suspected VM import defect was diagnosed as an incomplete bootstrap native surface: `std.data.Json` constructors and stringifiers were absent from the self-host runner.
- The VM runner now supplies the required persistent JSON values and `Result`-returning stringifiers.
- `target/quality/self-host-current-production-overlay-25/artifacts.manifest` replaces the projected type parser and parser-error evidence modules with their current self-hosted artifacts.
- `target/quality/self-host-bootstrap-current-40/bin/terlan-self-host` is the current root image and executes the Phase 2 authorities directly.
- `make --no-print-directory self-host-phase2-complete-check` passes: 15 corpus fixtures, 4 token fixtures, 4 accepted surface fixtures, 4 concrete-syntax fixtures, 11 exact rejection diagnostics, 17 fragments, recoverable token streams, deterministic parser mutations, and 4 formatter round trips.
- Phase 2 is complete. Subsequent bootstrap work must consume image40 or a proven fixed-point successor rather than the projected fallback.

## Phase 3 authority checkpoint (2026-10-17)

- `target/quality/self-host-phase3-image-21/bin/terlan-self-host-semantic-evidence` is the retained Phase 3 image.
- Common semantic evidence is byte-identical to the Rust oracle for all 5 semantic and accepted fixtures; supplemental inferred-type evidence is emitted for each fixture.
- The self-hosted signature path now preserves guards, exact arity, explicit and implicit return types, and Rust-compatible callable offsets.
- Local semantic events cover keyword-valued dynamic callees, record access, comprehensions, conditional clauses, and refutable lets without fixture-specific source parsing.
- All 8 backend fixtures emit canonical ABI-1 JSON accepted by the current VM consumer.
- `make --no-print-directory self-host-phase3-complete-check` is the aggregate Phase 3 authority gate and writes retained evidence under `target/quality/self-host-phase3-evidence-21`.
- Phase 3 is validated. The remaining operational risk is self-hosted `backend-unit` latency for large modules; authority transfer must add batched or memoized unit compilation rather than returning to on-demand full package builds.

## Phase 4 correctness checkpoint (2026-10-20)

- `target/quality/self-host-phase4-image-24/bin/terlan-self-host` is the retained self-hosted compiler image.
- Stage 1 produced 200 token, 200 syntax, 200 common-semantic, and 200 supplemental-semantic artifacts without projected-oracle substitution.
- All 800 Stage 1 artifacts are byte-identical to the Rust-seeded Stage 0 evidence.
- The Stage 0 and Stage 1 bootstrap artifacts are byte-identical with token digest `fnv1a64:52845676e94371f1`, syntax digest `fnv1a64:ae00f4c5f777c686`, semantic digest `fnv1a64:1edcb317e734f67d`, and backend digest `fnv1a64:ab8783086770e0c0`.
- The self-hosted `SelfHostBootstrapTool` fixed-point comparator passes through the retained image.
- Managed-call sparse register restoration now preserves register liveness. On `ProjectedParser.syntax`, the repaired runner reduced elapsed time from 84.77 seconds to 12.06 seconds and peak RSS from 86,612 KiB to 45,172 KiB while retaining byte parity.
- `make --no-print-directory self-host-phase4-correctness-check` is the aggregate correctness and fixed-point gate.
- Final authority transfer remains open until cold, warm incremental, parallel-package, bootstrap-throughput, and checked regression-budget performance gates pass. Correctness convergence does not waive those gates.


### Dynamic-call target authority

Production authority requires finite target evidence for every typed Core
`call-value`. The self-hosted actor compiler derives closure identities from
`make-closure`, propagates them through `move`, `phi`, `select`, and local-call
parameters to a module-wide fixed point, and feeds those targets back into
effect and suspension-profile convergence. Compilation fails closed when a
target remains opaque. Imported higher-order contracts may satisfy that case
only through explicit compile evidence; an unconstrained dynamic dispatch is
not an authority-bearing fallback.

#### Persisted opaque higher-order evidence

The self-hosted `actor-backend-unit` command accepts an
`actor-dynamic-evidence.manifest`. Each non-empty manifest line names a strict
`terlan.self-host.actor-dynamic-evidence/v1` JSON document. A document contains
exact local function identities and, for each overridden `call-value` result,
a non-empty finite list of callable identities. Duplicate functions, result
names, or callable targets are rejected. The backend still resolves those
identities against the converged actor suspension-profile catalog; the artifact
cannot directly claim suspension behavior or continuation metadata.
