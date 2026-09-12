# Terlan Self-Hosting Roadmap

Status: staged plan; implementation remains in the separate self-hosting checkout
Created: 2026-08-29

This roadmap was carried into the release worktree on 2026-09-07. Following the
2026-09-10 scope decision, the active [0.0.10 roadmap](ROADMAP_0_0_10.md)
references this maintained plan. The status
below describes that separate checkout; it does not claim its implementation
has been merged or its differential gates have passed here.

## Implementation Status

Phase 0 and Phase 1 have started in `compiler/self_host` with Terlan-owned
source spans, token and lexical-error types, deterministic UTF-8 tokenization,
module/import/public-interface extraction, canonical JSON evidence, and a
maintained positive and negative corpus. The package also provides a runnable
file-to-evidence driver that binds source SHA-256, token spans, extracted public
interface, and typed diagnostics into `terlan.self-host.source-evidence.v1`.
The standalone implementation gate is `make self-host-source-slice-check`.

Rust remains authoritative until canonical Rust frontend evidence is connected
and the differential gates pass. The current slice does not claim parser or
compiler replacement.

Phase 0 bootstrap artifact locations and canonical comparison layers are now
declared in `compiler/self_host/bootstrap/BOOTSTRAP.toml`. Cross-language
schemas are owned by `compiler/self_host/SCHEMAS.md`, and the maintained corpus
has an explicit acceptance/rejection inventory in
`compiler/self_host/corpus/CORPUS.tsv`.

## Objective

Move language semantics, compiler policy, and developer tooling from Rust into
Terlan while retaining Rust as the small trusted substrate for execution,
unsafe platform integration, and initial bootstrap.

Self-hosting is complete when a compiler written primarily in Terlan can build
its own source and the resulting compiler can repeat that build with equivalent
canonical outputs.

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
   descriptors, and runtime behavior.
5. Compare normalized build artifacts after removing only documented variable
   metadata.

### Exit gates

- `self-host-stage1-build-check`
- `self-host-stage2-build-check`
- `self-host-bootstrap-convergence-check`
- `self-host-clean-machine-bootstrap-check`
- `self-host-recovery-bootstrap-check`
- `self-host-release-corpus-check`

### Completion condition

`terlc1` and `terlc2` are semantically equivalent, the clean bootstrap is
reproducible, and no normal compiler path depends on Rust-owned language
semantics.

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
- Differential and recovery gates remain part of every release candidate.
- Rust-owned code no longer defines user-visible language semantics outside the
  documented bootstrap implementation.
