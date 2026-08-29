# Terlan 0.0.7 Code Quality Mini Roadmap

This is the focused structural-stabilization plan for the Rust implementation
before the 0.0.7 release. It supplements
[`ROADMAP_0_0_7.md`](ROADMAP_0_0_7.md); it does not replace the product,
runtime, compiler, or release requirements in the main roadmap.

The immediate decision is a bounded **architectural expansion freeze**:

- finish, split, or cleanly park work already in flight;
- continue correctness, security, compatibility, test, and release-critical
  fixes;
- do not add another subsystem, binary, broad dependency, numbered Rust
  fragment, or cross-cutting feature until CQ-0 and CQ-1 close;
- after CQ-1, admit feature work only when it does not grow a metric frozen by
  this roadmap.

This was not an indefinite feature freeze. CQ-6 passed on 2026-07-29, ending
the freeze while retaining its permanent release-blocking regression gates.

## Why This Work Is Release-Critical

The code is generally careful at the function level, but the repository has
accumulated structural patterns that make ownership and change impact hard to
see:

- a very large primary crate owns compiler, runtime, server, LSP, benchmark,
  quality, and CLI responsibilities;
- binary targets reuse implementation through cross-tree `#[path = "..."]`
  declarations, which obscures the build graph and may compile shared source
  more than once;
- handwritten modules and tests are split through numbered
  `*_part_NNN.rs` files and textual `include!` rather than normal Rust modules;
- the 1,000-line implementation limit prevents unbounded file growth but can
  encourage mechanical splitting at the threshold;
- broad lint allowances and widespread `Result<_, String>` interfaces weaken
  compiler-assisted maintenance;
- mandatory template-shaped Rustdoc on obvious private helpers and tests can
  lower documentation signal while increasing source volume.

The objective is not a preferred crate count or a smaller line count by
itself. The objective is a build graph and module graph in which a maintainer
can identify responsibility, dependencies, invariants, tests, and failure
types without following textual fragments.

## Planning Baseline

The 2026-07-28 working-tree review found approximately:

| Measure | Observed snapshot |
| --- | ---: |
| Rust source files under `crates/terlan/src` and `crates/terlan/tests` | 2,099 |
| Rust source lines in those paths | 679,411 |
| Numbered `*_part_NNN.rs` files under `crates/terlan/src` | 212 |
| `include!` sites under `crates/terlan/src` | 244 |
| Non-test-looking implementation files at 900 lines or more | 76 |
| `#[allow(clippy::too_many_arguments)]` sites | 88 |
| Rough `Result<_, String>` signature matches | 2,773 |
| Test attributes | 6,810 |

These values are a planning snapshot, not yet a canonical baseline. The
working tree contained substantial in-flight changes. CQ-0 must replace the
approximate counts with a reproducible machine-readable report after current
work is landed or parked. The report must classify syntax rather than assume
that every filename or textual match is production debt.

The initial validation snapshot also had these outcomes:

- `cargo check --locked -p terlan --bins` passed;
- the current Rust file-size/inline-test quality gate passed;
- `cargo fmt --all -- --check` found formatting drift in a modified file;
- Clippy stopped on generated libpq warnings with default features and reported
  253 errors in the main crate when that dependency was excluded;
- the Rustdoc baseline gate reported stale and newly undocumented entries.

CQ-1 owns these failures. Baseline files may be corrected only after the
underlying source and classification are reviewed; rewriting a baseline is not
itself a fix.

## Execution Rules

1. Work on the first unchecked CQ item. A later item may be investigated, but
   implementation must not use that investigation to bypass an earlier gate.
2. Every CQ item includes its measurement, implementation, tests, gate, and
   documentation. Partial work remains unchecked.
3. Preserve observable Terlan behavior. Structural changes must use the
   existing compiler, runtime, JavaScript, native-image, package, and editor
   tests as regression evidence.
4. Do not combine a semantic feature with a module move unless separating them
   would make the change less safe. When they must be combined, record the
   reason and provide before/after behavior evidence.
5. Do not improve metrics by renaming fragments, hiding code in macros,
   disabling targets, moving implementation into tests, or excluding source
   from the inventory.
6. Generated code must live under an explicitly generated path, identify its
   generator and regeneration command, and be reproducible. Handwritten source
   cannot claim the generated-code exception.
7. Crates are architectural and build-cache boundaries, not substitutes for
   modules. No crate may be introduced without an acyclic dependency graph,
   an owner, a narrow API, and measured build results.
8. Workspace crates must inherit one root package, dependency, formatting, and
   lint policy. A crate split must not copy code or duplicate configuration.
9. Within a crate, each domain concept has exactly one canonical type
   definition. Features import or re-export that type; they must not create
   parallel structs, enums, aliases, option records, or error types for the
   same concept.
10. The `terlan-vm` binary remains part of the Terlan distribution. An internal
   library-crate extraction must not create a second public VM product,
   artifact, version line, or installation contract.
11. A check that merely counts files or strings is supporting evidence. Closure
   also requires compilation, focused tests, and a representative end-to-end
   path.
12. Any accepted exception must name its owner, rationale, measurable scope,
    and expiry release. “Existing code” is not a rationale.

## Workspace Uniformity Policy

Splitting the implementation into crates must centralize policy rather than
replicate it. Every in-repository Rust crate, including generated native
support crates, is governed by the workspace root.

The required shape is:

- one `rust-toolchain.toml` at the workspace root;
- one root `rustfmt.toml`;
- one root `clippy.toml` when Clippy configuration beyond lint levels is
  required;
- `[workspace.package]` ownership of shared edition, Rust version, version,
  license, authorship, and repository metadata;
- `[workspace.lints.rust]` and `[workspace.lints.clippy]` ownership of lint
  levels, with every member using `[lints] workspace = true`;
- `[workspace.dependencies]` ownership of versions and common feature policy
  for dependencies used by more than one member;
- one Make target for workspace formatting and one for workspace Clippy, both
  covering all members and targets required by the release feature matrix.

Member crates must not contain copied lint tables, private `rustfmt.toml` or
`clippy.toml` files, independently selected editions/toolchains, or duplicate
versions of shared dependencies. Item-level lint allowances remain possible
only at the smallest affected item with a concrete rationale.

Generated Rust must be emitted in root-rustfmt form and pass the workspace lint
policy. A genuine generated ABI exception belongs as a narrow emitted
attribute controlled and tested by the generator; it does not justify a second
configuration policy.

Shared implementation has exactly one owning crate. Other crates depend on and
call that owner through an ordinary API. They must not copy the implementation,
textually include its source, compile it again through `#[path]`, or maintain a
parallel “small” version. Shared test builders follow the same rule through a
named test-support module or dev dependency.

The workspace policy gate must reject:

- additional formatting, Clippy, toolchain, or lint-policy files below the
  root;
- a member manifest that does not inherit workspace lints and package metadata;
- repeated dependency versions or feature sets that should be inherited;
- cross-crate `include!` or `#[path]` source reuse;
- exact or normalized duplicate function/type bodies across workspace members;
- a generated crate whose checked-in output differs after regeneration,
  workspace formatting, or workspace Clippy.

## Responsibility And Feature Hierarchy

The source tree uses two deliberate organizing levels:

1. **Crate by responsibility.** A crate owns one coarse architectural
   responsibility, dependency direction, and change reason.
2. **Module by feature.** Within that responsibility, named feature modules own
   the behavior, model, diagnostics, and tests for one recognizable capability.

A crate must not exist merely because a directory is large, and a feature must
not become a crate merely because it has many files. Conversely, one crate must
not remain responsible for unrelated compiler, runtime, editor, benchmarking,
quality, and command concerns solely to avoid workspace members.

The initial responsibility candidates are:

| Responsibility | Typical owned behavior |
| --- | --- |
| Compiler | source model, checking, checked IR, diagnostics, lowering contracts |
| Native backend | NativeIR optimization, Cranelift emission, object metadata |
| Runtime protocol | stable image, transition, NativeBoundary, and control-plane types |
| Runtime | actors, scheduling, mailboxes, timers, heaps, resources, supervision |
| CLI and server | command orchestration, project workflows, HTTP serving |
| LSP/editor | documents, navigation, completion, diagnostics publication |
| Quality tooling | repository policy, inventories, reports, release gates |
| Benchmarks | measurement harnesses and benchmark-only models |

CQ-3 may merge or split these candidates when the dependency graph and build
measurements require it. Every resulting crate still needs one sentence that
completes: “This crate changes when ___ changes.” If the answer contains
unrelated reasons joined by “or,” the boundary needs review.

Within each responsibility, feature modules are named after capabilities, not
file size or implementation chronology. Examples include:

```text
compiler/
  features/
    binding_identity/
      model.rs
      analysis.rs
      diagnostics.rs
    tail_recursion/
      call_graph.rs
      validation.rs
      lowering.rs
    templates/
      typing.rs
      lowering.rs

runtime/
  features/
    actors/
      lifecycle.rs
      mailbox.rs
      cancellation.rs
    timers/
      deadlines.rs
      dispatch.rs
      telemetry.rs
    persistent_actors/
      schema.rs
      restore.rs
      compaction.rs

lsp/
  features/
    binding_navigation/
      index.rs
      references.rs
      rename.rs
    completion/
      imports.rs
      templates.rs
```

The paths are illustrative; the responsibility and feature rule is normative.
A feature may use responsibility-level infrastructure such as shared IR,
parsing primitives, scheduler primitives, protocol codecs, diagnostics, or
test support. Such infrastructure must be named for the concept it provides
and remain dependency-inward. It must not become a generic `misc`, `common`,
`helpers`, or `utils` bucket.

Feature organization does not authorize duplicate phase implementations. For
example, compiler features may contain feature-specific checking and lowering,
while the shared type engine, visitor framework, and IR definitions remain
single responsibility-level facilities. A feature depends on those facilities;
it does not copy them.

### Canonical Type Ownership

Type ownership is a release-blocking architectural invariant, not a style
preference. Within each crate, a domain concept has one canonical definition
in the lowest-level module that owns its invariants. All feature modules use
that definition through ordinary imports. A feature may publicly re-export the
canonical type when that improves its API, but a re-export must not be replaced
with a second declaration.

The rule covers:

- structs, enums, unions, type aliases, option/configuration records, error
  types, identifiers, state markers, and protocol models;
- types copied under another module or renamed with suffixes such as `Data`,
  `Info`, `Model`, `Dto`, `Request`, `Options`, `Config`, or `Internal` while
  retaining the same concept and invariants;
- partial replicas that copy a common field or variant set and then drift;
- handwritten mirrors of generated types when the generated definition can be
  wrapped, imported, or adapted at one boundary;
- test-only replicas of production models when builders or canonical
  constructors would express the scenario.

Equal shape alone is evidence for review, not proof that two types represent
the same concept. Semantically distinct values such as source and destination
positions, validated and unvalidated input, or wire and domain representations
may use separate newtypes or models when they enforce different invariants.
Their names must state that distinction, conversion must occur in one named
boundary module, and the type-ownership inventory must record why unification
would be incorrect. Two types that differ only in derives, visibility,
serialization annotations, or convenience methods are duplicates and must be
consolidated.

Responsibility-level concepts belong outside individual feature modules. For
example, a compiler feature may own a feature-specific analysis result, but it
must reuse the crate's canonical source span, symbol identity, diagnostic,
checked-type, and IR definitions. Feature-local `Context`, `Options`, or
`Error` types are allowed only when their state and invariants are genuinely
feature-specific; generic names do not create separate ownership.

Cross-crate boundaries follow the same ownership principle. A type shared by
responsibilities is defined by exactly one owning crate and consumed through
that crate's API. Crates must not maintain mirrored copies merely to avoid a
dependency. If depending on the current owner would violate dependency
direction, CQ-3 must move the canonical type to the correct lower-level
protocol or model crate rather than duplicate it.

The canonical-type gate must:

- parse every handwritten type declaration and record its crate, owning
  responsibility, feature, public path, normalized fields or variants, and
  invariants;
- reject exact normalized duplicate declarations within a crate;
- report structurally equivalent and similarly named declarations as
  mandatory-review candidates;
- maintain a small reviewed registry for semantically distinct equal-shape
  types and for high-value canonical concepts whose definitions must remain at
  one path;
- reject a new candidate unless it resolves to the existing canonical type or
  adds a reviewed invariant-based distinction;
- reject stale exceptions so the registry cannot become a permanent duplicate
  type baseline.

Each migrated feature must have:

- a named owner module and short responsibility comment;
- an explicit public or crate-private API;
- feature-local positive, adversarial, and regression tests;
- shared fixtures only through named responsibility-level test support;
- imports of canonical responsibility-level types rather than feature-local
  replicas;
- no dependency on sibling private implementation;
- no numeric, chronological, or arbitrary-size child modules.

## Numbered Fragment Policy

Handwritten `*_part_NNN.rs` files are presumed to be unresolved structural
debt. Their numeric names communicate ordering but not responsibility, and
`include!` erases the module boundary that would otherwise control visibility
and dependencies.

For each numbered-fragment family, CQ-0 must record:

- wrapper and fragment paths;
- implementation, test, generated, or fixture classification;
- logical responsibilities contained in each fragment;
- imports and private names shared only because of textual inclusion;
- recent change frequency and merge-conflict history when available;
- destination modules and owner;
- whether the family can be collapsed after removing duplicated scaffolding.

An accepted migration must result in one of:

1. a cohesive file below its applicable limit;
2. named Rust submodules organized by responsibility;
3. behavior-oriented test modules with shared fixtures in a named support
   module; or
4. reproducible generated output under a generated path.

The following do not count:

- renaming `part_001.rs` to `helpers_a.rs`;
- replacing one `include!` list with another;
- creating a directory whose modules still depend on a shared unstructured
  parent namespace;
- splitting at an arbitrary line count;
- copying shared definitions into each new module.

The 0.0.7 target is zero handwritten numbered Rust fragments and zero textual
`include!` of handwritten implementation. Intentional `include!` for generated
tables or templates must be explicitly inventoried and gated.

## Proposed Dependency Direction

CQ-3 must validate or revise this conceptual direction before crate extraction:

```text
syntax/source model
        |
        v
HIR and type checking
        |
        v
checked CoreIR and NativeIR
        |
        +-------------------+
        v                   v
native object backend   other maintained backends
        |
        v
runtime ABI/protocol types
        |
        v
VM/runtime implementation

CLI/server  -> compiler APIs + runtime APIs
LSP         -> source model + compiler analysis APIs
quality     -> repository contracts; compiler parsing only through a narrow API
benchmarks  -> published internal benchmark APIs, never copied source trees
```

The graph is intentionally about dependency direction rather than final crate
names. The experiment may retain a substantial `terlan` facade crate. It must
first remove cross-target source inclusion and prove which boundaries improve
normal developer builds.

## CQ-0: Freeze And Reproducible Structural Census

- [x] Establish the code-quality baseline and stop structural debt growth.
  - Add a repository structure report covering Rust files, logical and physical
    lines, numbered fragments, handwritten `include!`, cross-tree `#[path]`,
    oversized/near-limit files, inline tests, lint allowances, unsafe blocks
    and safety comments, `Result<_, String>` boundaries, type declarations,
    normalized duplicate-type candidates, canonical type ownership, direct
    dependencies, crate targets, and test targets.
  - Classify every `include!` and `#[path]` use as handwritten composition,
    generated content, fixture content, or an approved platform boundary.
  - Record counts separately for implementation, tests, generated source, and
    fixtures. File-name heuristics alone are insufficient.
  - Add a changed-files mode that rejects new numbered fragments, new
    handwritten `include!`, new cross-tree source inclusion, new crate-root lint
    allowances, new unreviewed duplicate-type candidates, and growth above the
    recorded debt baseline.
  - Record representative clean, parser-edit, type-model-edit, runtime-leaf,
    LSP-edit, focused-test, and full-test compile timings with Cargo timing
    artifacts and a separate target directory.
  - Gate: add `make rust-structure-census-check`.
  - Acceptance: the report is deterministic, has no unclassified entry, and
    fails when adversarial fixtures introduce each forbidden pattern.
  - Completion evidence: `docs/quality/RUST_STRUCTURE_CENSUS.json` records
    2,068 tracked or untracked non-ignored Rust files, 668,847 physical lines,
    446,941 logical lines, 208 numbered fragments, 225 handwritten `include!`
    edges, 83 cross-tree `#[path]` edges, 71 crate/module-root allowances,
    3,248 `Result<_, String>` boundaries, 2,452 type declarations, 115
    equal-shape implementation type candidates, the canonical ownership
    inventory, direct dependencies, and 31 Cargo targets. The report separates
    implementation, test, generated, and fixture metrics and reproduced
    byte-for-byte across consecutive scans.
  - Completion evidence: the changed-files validator freezes the identities
    and aggregate ceilings for numbered fragments, handwritten composition,
    cross-tree source reuse, root allowances, duplicate type candidates,
    lint allowances, string errors, unsafe blocks without safety comments, and
    file-size debt. Five source/validator adversarial tests cover every
    forbidden pattern and reject non-literal unclassified composition.
  - Completion evidence:
    `docs/quality/RUST_STRUCTURE_TIMINGS.json` records locked Cargo HTML timing
    evidence for clean, parser, type-model, runtime-leaf, prewarmed LSP,
    focused-test, and full-test compilation using the isolated
    `target/cq0-timings` directory. `make rust-structure-census-check` passes
    the census, timing policy, adversarial tests, and changed-files no-growth
    enforcement.

## CQ-1: Restore Trustworthy Rust Gates

- [x] Make the standard Rust feedback loop clean before structural migration.
  - Make `cargo fmt --all -- --check`, locked binary checking, Clippy, Rustdoc
    policy, and the existing Rust quality gate pass from a clean checkout.
  - Define a generated-code Clippy policy for libpq: fix the generator where
    practical and use narrowly scoped generated-code allowances only when the
    emitted cross-platform ABI genuinely requires them.
  - Move Rust and Clippy lint levels into root `[workspace.lints]`; require
    every current and future member to use `[lints] workspace = true`.
  - Add one root `rustfmt.toml` and, if configuration values are needed, one
    root `clippy.toml`. Reject nested policy files.
  - Move shared package metadata and dependency versions/features to
    `[workspace.package]` and `[workspace.dependencies]`; member manifests
    inherit instead of copying values.
  - Remove crate-root `allow(dead_code, unused_imports)`. Feature- or
    platform-specific exceptions must be placed on the smallest item and state
    why the item is intentionally inactive in that build.
  - Remove duplicate and obsolete allowances. Do not respond to Clippy by
    broadly allowing a lint category.
  - Classify every remaining allowance by its actual constraint. Each
    genuinely necessary exception must identify its exact scope, owner,
    concrete rationale, and expiry milestone; unclassified, stale, duplicate,
    placeholder, or ownerless exceptions fail the preflight.
  - Rework the Rustdoc gate so it protects public APIs, subsystem contracts,
    safety invariants, and non-obvious algorithms without requiring formulaic
    prose for self-explanatory private helpers or every test.
  - Make all five checks part of one fast code-quality preflight with
    individually runnable subcommands.
  - Gate: add `make rust-code-quality-preflight-check`.
  - Acceptance: the preflight passes with default features and the release
    feature set, and adversarial fixtures prove each subcheck can fail.
  - Completion evidence: the root workspace now owns package metadata, shared
    dependency versions, Rust and Clippy lint levels, `rustfmt.toml`, and
    `clippy.toml`; both members inherit that policy. Six workspace-policy
    fixtures reject nested policy files, copied member lint tables, duplicate
    dependency versions, crate-root allowances, and broad Clippy categories.
  - Completion evidence: generated libpq adapters use a documented
    resource-implementation allowance only for target-varying C integer casts.
    The generator owns the attribute, deterministic checked-in regeneration
    passes, workspace Clippy covers the generated member, and all 37 focused C
    ABI generator tests pass.
  - Completion evidence: the Rustdoc gate now protects public APIs, unsafe
    functions, and explicitly marked internal contracts while excluding
    private helpers and test sources. Its nine policy tests pass and the
    reviewed baseline contains 146 remaining public or contract documentation
    entries.
  - Completion evidence: `make rust-code-quality-preflight-check` runs five
    independently callable gates for formatting, locked workspace binaries,
    Clippy, Rustdoc, and structural Rust quality. Locked checking and Clippy
    pass with both default and `--all-features` release configurations.
    Temporary-crate and focused Rust adversarial fixtures prove all five gates
    reject representative defects. The durable command and exception policy
    is recorded in
    [`RUST_CODE_QUALITY_POLICY.md`](../quality/RUST_CODE_QUALITY_POLICY.md).
  - Completion evidence: the refreshed structural census passes with no debt
    growth; crate-root allowances fell from 74 to 71, syntax-aware workspace
    lint allowances fell from 280 to 238, and the repository remains at zero
    oversized Rust files.
  - Completion evidence: `RUST_LINT_ALLOWANCES.tsv` classifies all 261 actual
    repository-wide allowance attributes across ten bounded categories,
    separating 27 currently required boundaries from 234 structural-debt
    allowances. Every row names its CQ owner, expiry, and rationale; eight
    adversarial validator tests cover missing, stale, duplicate, placeholder,
    unknown, misowned, and literal-only entries.

## CQ-2: Eliminate Numbered Handwritten Fragments

- [x] Replace numbered textual composition with cohesive Rust modules.
  - Migrate production families before test-only families, ordered by change
    frequency and fan-in rather than filename.
  - Start with module roots that currently concatenate multiple implementation
    fragments, including the LSP backend, serve command, quality command router,
    VM entrypoint, benchmark entrypoint, and compiler/runtime roots identified
    by the CQ-0 report.
  - Give each destination module a responsibility name and explicit visibility.
    Move shared state into a context type or named support module instead of
    relying on the include wrapper's ambient namespace.
  - Place every destination first under its owning responsibility and then
    under its named feature. Mechanics such as model, validation, lowering,
    protocol, rendering, or telemetry may form submodules inside that feature.
  - Split large tests by behavior or contract, with common builders and fixtures
    in named test support. Do not preserve one 2,000-line conceptual test module
    as several numbered files.
  - Reduce the implementation line limit after migration based on the measured
    distribution. The limit is a backstop, not a target.
  - Gate: add `make rust-module-structure-check`.
  - Acceptance: no handwritten `*_part_NNN.rs` remains, no handwritten
    implementation is textually included, all former fragment groups retain
    focused and end-to-end coverage, and the gate rejects renamed numeric
    fragments.
  - Completion evidence (2026-07-28): all 212 baseline numbered fragments were
    replaced by responsibility-named modules, with production families
    migrated before test families. The refreshed 2,075-file census reports
    zero numbered fragments, zero handwritten includes, and zero oversized
    Rust files.
  - Completion evidence: implementation and test backstops are now 999 and
    2,000 physical lines respectively; the largest implementation is 998 lines
    and the largest test is 1,905 lines. Former mechanical test fragments are
    grouped by behavior with named fixture/support modules.
  - Completion evidence: `make rust-module-structure-check` passes and its ten
    adversarial tests reject numbered parts, renamed chronology tokens, bare
    numeric suffixes, literal/computed handwritten includes, and includes
    hidden after raw-string fixtures while preserving generated-source
    exceptions.
  - Completion evidence: the canonical library test target compiles; 190 actor
    behavior tests, 32 C++ binding-generator tests, 39 TLS/ACME tests, and 11
    value-lifecycle end-to-end consumers pass. `make
    rust-code-quality-preflight-check` passes locked default/all-feature binary
    checks and Clippy, while `make rust-structure-census-check` preserves the
    no-growth structural baseline.

## CQ-3: Make The Build Graph Match Architectural Ownership

- [x] Remove copied source trees from binary targets and validate internal crate
  boundaries.
  - Replace cross-tree `#[path = "../..."]` reuse in VM, LSP, benchmark, and
    quality binaries with calls through ordinary library APIs.
  - Produce an acyclic module/crate dependency graph and identify compiler,
    runtime protocol, runtime implementation, LSP, quality, benchmark, and CLI
    ownership.
  - Write and gate one responsibility statement for every workspace crate, then
    inventory its top-level feature modules. Reject crates with unrelated
    change reasons and top-level modules organized by chronology or file size.
  - Measure a facade-only design and a small set of internal-crate candidates.
    Compare clean builds and representative incremental edits; do not optimize
    only the clean full-workspace build.
  - Extract a boundary only when it removes source duplication or unrelated
    dependency loading and does not cause unacceptable downstream invalidation.
  - Move each shared implementation to exactly one owning crate. Dependents
    call its API; they may not retain a copied, textually included,
    `#[path]`-compiled, or independently maintained variant.
  - Establish one canonical definition for every responsibility-level type
    within each crate. Replace feature-local mirrors with imports, re-exports,
    newtypes that enforce a documented distinct invariant, or centralized
    boundary conversion.
  - Add an AST-based canonical-type inventory and duplicate-candidate check.
    Exact normalized duplicates fail automatically; equal-shape or similarly
    named candidates require a reviewed invariant-based classification.
  - Move types shared across crate responsibilities to one dependency-inward
    owner rather than generating or maintaining a copy in each consumer.
  - Extend duplicate-helper detection across all workspace members using
    syntax-normalized bodies so trivial renaming cannot hide copied code.
  - Require every extracted crate to inherit the root toolchain, package
    metadata, dependency versions, formatting, Rust lint, and Clippy policy.
  - If a runtime library crate is selected, amend
    `terlan-vm-internal-crate-check` so it continues to forbid a separate public
    VM distribution while allowing the reviewed internal implementation
    boundary.
  - Preserve one version, one installation contract, and the existing public
    command names.
  - Gate: add `make rust-build-graph-boundary-check`.
  - Acceptance: every binary compiles shared implementation through normal
    dependencies, the dependency graph is acyclic, focused builds do not
    compile unrelated LSP/benchmark/quality dependencies, no shared code or
    configuration is duplicated between crates, every domain concept has one
    canonical type definition within its crate, shared cross-crate types have
    one dependency-inward owner, all members inherit workspace policy, every
    crate has one responsibility with feature-oriented internal modules, and
    measured timing decisions are checked into the report.
  - Completion evidence (2026-07-28): `terlc`, `terlan-vm`, `terlan-lsp`,
    `terlan-native-worker`, `terlan-benchmark`, and `terlan-quality` are
    48 lines of binary façade code in total and call canonical library APIs.
    The boundary audit finds no executable cross-tree source reuse, copied
    implementation tree, dependency cycle, or unclassified workspace crate.
  - Completion evidence: LSP, quality, and benchmark ownership is isolated
    behind `editor-lsp`, `quality-tools`, and `benchmark-tools`. Focused product
    and tool builds pass independently, while the selected façade design
    records a 37,513 ms clean compiler build and 4,589/4,637 ms representative
    parser/runtime incremental edits. The independently owned AST audit builds
    in 1,495 ms clean and 139 ms incrementally.
  - Completion evidence: the syntax-aware inventory parses 683 implementation
    files and 2,563 type declarations with zero parse failures and zero exact
    normalized duplicates. Six equal-shape and 23 same-name candidates have
    checked-in invariant classifications; workspace helper and policy gates
    also pass.
  - Completion evidence: all Rust test recipes now select canonical library
    ownership rather than thin binary façades. The permanent selector gate
    resolves 1,429 exact selectors and rejects stale grouped filters; focused
    REPL (30), benchmark protocol (7), native-worker façade (3), LSP completion,
    and HTTP lifecycle tests pass.
  - Completion evidence: `make rust-build-graph-boundary-check`,
    `make rust-module-structure-check`, `make rust-structure-census-check`, and
    `make rust-code-quality-preflight-check` pass with zero oversized files,
    zero numbered fragments, zero handwritten includes, and all 215 remaining
    lint allowances classified.

## CQ-4: Typed Failure And API Complexity Boundaries

- [x] Replace stringly cross-subsystem failures and oversized call signatures.
  - Inventory `Result<_, String>` by public API, cross-subsystem API, command
    boundary, internal helper, test, and quality-gate classification.
  - Introduce typed errors for compiler phase orchestration, NativeIR emission,
    runtime/image admission, NativeBoundary operations, LSP analysis, and
    command execution. Stable user-facing diagnostic codes remain renderable
    data, not the error type itself.
  - Require error sources and structured context at I/O, parsing, process,
    protocol, and code-generation boundaries.
  - Ban new `Result<_, String>` on public and cross-subsystem APIs. Remaining
    internal string errors need a measured inventory and 0.0.9 owner.
  - Replace repeated long argument lists with responsibility-specific context,
    options, or state types, reusing an existing canonical type whenever the
    concept already exists. This work must not create feature-local `Context`,
    `Options`, or error replicas. FFI and code-generation functions may retain
    explicit parameters only with a localized rationale.
  - Remove the blanket `too_many_arguments` debt rather than increasing its
    baseline.
  - Reduce the complete `#[allow(...)]` inventory to zero. Resolve each
    allowance through ownership, API, `cfg`, generated-code, or test-layout
    changes; do not disguise it as `#[expect]`, `cfg_attr`, a renamed lint, or
    a broader root policy. Generated ABI adapters must eventually emit
    warning-free portable conversions instead of retaining CQ-1's transitional
    item-scoped cast allowance.
  - Lower the centralized Clippy shape thresholds as the affected APIs are
    split, and remove each configured override when the workspace reaches
    Clippy's default.
  - Gate: add `make rust-api-boundary-quality-check`.
  - Acceptance: no public or cross-subsystem API returns an untyped string
    error, the workspace contains no lint-allowance attributes, Clippy has no
    unreviewed argument-count exception, and tests match structured error
    fields before checking rendered text.
  - Completion evidence: the AST boundary audit reports zero public or
    cross-subsystem string-error APIs and zero oversized implementation
    signatures; the reviewed internal inventory contains 450 owned rows
    covering 3,020 internal string-error sites.
  - Completion evidence: `make rust-api-boundary-quality-check` passes both
    default and all-feature workspace Clippy with zero lint allowances, and
    `cargo test -p terlan --lib --no-run` compiles the complete library test
    graph after the boundary migrations.

## CQ-5: Raise Documentation And Test Signal

- [x] Make documentation and tests easier to scan without weakening coverage.
  - Reserve full Rustdoc for public behavior, internal contracts, invariants,
    safety arguments, lifecycle rules, algorithms, and non-obvious failure
    semantics.
  - Replace repetitive “Inputs / Output / Transformation” prose on obvious
    private helpers and tests with precise names or a short rationale when that
    communicates the same information.
  - Eliminate the remaining inline-test baseline by moving tests to adjacent
    named test modules, except compile-time assertions that cannot be expressed
    externally.
  - Consolidate repeated fixture construction and assertion plumbing without
    hiding scenario intent behind generic test frameworks.
  - Keep positive, adversarial, regression, and end-to-end coverage visible and
    independently selectable.
  - Gate: extend `make rust-module-structure-check` and
    `make rust-code-quality-preflight-check`.
  - Acceptance: Rustdoc policy is clean without a stale baseline, there are no
    unexplained inline test modules, and representative tests are shorter while
    retaining the same behavioral assertions.
  - Completion evidence: the Rustdoc inventory and inline-test inventory both
    contain zero debt rows. Ten inline test modules moved to adjacent named
    test files, repeated persistent-actor replay fixtures now use the canonical
    snapshot conversion, and low-signal private Inputs/Output/Transformation
    prose was condensed without changing behavioral assertions.
  - Completion evidence: `make rust-module-structure-check` adversarially
    rejects reintroduced Rustdoc or inline-test baseline rows, and
    `make rust-code-quality-preflight-check` passes default/all-feature checks,
    Clippy, zero-debt Rustdoc, and structural quality with zero oversized source
    files and zero inline-test files.

## CQ-6: 0.0.7 Structural Closeout

- [x] Close the stabilization window with evidence and permanent regression
  gates.
  - Run the standard Rust preflight, module-structure, build-graph, API-boundary,
    canonical-type, test hierarchy, shared-helper, dormant-code, safe-runtime,
    and release-code-hygiene gates.
  - Rerun the CQ-0 compile-timing matrix using the same toolchain, target,
    features, machine policy, and sample procedure.
  - Publish baseline-to-closeout deltas, remaining accepted exceptions, and
    their owners. No exception may expire later than 0.0.9 without an explicit
    main-roadmap decision.
  - Exercise one representative change in parser/type checking, native
    lowering, runtime, LSP, and quality tooling to prove that the new boundaries
    improve or at least preserve the intended feedback loop.
  - Gate: compose the code-quality closeout aggregate for the stabilization
    window.
  - Acceptance: CQ-0 through CQ-5 are checked, every named gate is executable,
    no handwritten numbered fragment or textual implementation include remains,
    every in-crate domain concept has one canonical type definition, shared
    cross-crate types have one owner, no lint allowance remains, standard Rust
    checks are clean, and all product release behavior remains green.
  - Completion evidence: the code-quality closeout aggregate passed on
    2026-07-29. It ran every named structural and release
    gate, the representative parser, type-checking, native-lowering, runtime,
    LSP, and quality-tooling tests, and the seven-scenario timing check.
  - Completion evidence: the closeout census reports zero numbered fragments,
    handwritten implementation includes, cross-tree path edges, lint
    allowances, oversized files, Rustdoc debt rows, inline-test debt rows, and
    exact normalized duplicate types. All seven compile scenarios improved;
    closeout-to-baseline ratios range from 0.3839 to 0.8690.
  - Completion evidence: post-closeout improvement extracted all five reviewed
    duplicate-helper groups and reduced internal string errors to 450 rows
    covering 3,006 sites. The support-error owner reached its scheduled target
    at 8 rows/25 sites and was ratcheted to a further 6/20 target. Seven
    domain-owner budgets prevent growth and require lower targets through their
    0.0.9 expiry.
  - Completion evidence: Direct-AOT integration coverage retains twelve
    isolated integration-test targets and a checked no-growth ceiling. The
    line-table-only test profile reduces debug metadata without conflating
    process or environment ownership across harnesses. The first recurring
    dedicated-host timing run improved every CQ-6 scenario. The subsequent
    three-sample median protocol confirms full-test compilation at 69,150 ms
    against the 82,780 ms CQ-6 reference.
  - Completion evidence: `make rust-dependency-impact-check` validates 25
    source domains, 27 cross-domain reference edges, zero source-domain or
    workspace dependency cycles, maximum transitive blast radius 11, and the
    twelve-target integration-test ceiling. Resolved duplicate-version
    families fell from 44 to 30 and the main package is capped at 56
    non-optional normal dependencies. Rust-quality orchestration is owned by
    `mk/code-quality.mk`.
  - Post-closeout hardening (2026-07-29): stable `BoundaryError` and
    `TvmBoundaryType` contracts moved into the dependency-inward
    `terlan-runtime-abi` crate, while compatibility paths remain re-exports.
    Unused `cxx`, `cxx-build`, and `wat` test dependencies were removed from
    the main crate.
  - Post-closeout hardening: lint-style and formatter selector inventories now
    execute as canonical module batches, and quality/release Make aggregates
    use prerequisite edges so shared gates run once per top-level traversal.
  - Post-closeout hardening: `make rust-file-headroom-check` freezes 82 files
    already at 900 implementation or 1,800 test lines at exact no-growth
    ceilings after the declaration formatter and native-image descriptor were
    split below the warning band. The typed-error budget now includes strictly
    lower 0.0.9 reduction targets rather than only no-growth maxima.
  - Post-closeout hardening: recurring compilation evidence uses at least
    three samples and per-scenario medians with affinity, governor, load, and
    dispersion provenance. Dependency evidence now includes resolved version
    and feature fanout, domain API fan-in, cycle budgets, and transitive change
    blast radius.
  - Post-closeout hardening: the closeout Make DAG runs shared Clippy and
    structure self-tests once per traversal. Recurring timings reject unstable
    affinity/governor, normalized load above 0.50 per CPU, and coefficient of
    variation above 0.10. Isolated CQ-0/CQ-3/sanitizer targets are now removed
    automatically after compact evidence is copied. Shared debug incremental
    state is capped at 64 GiB while compiled dependencies, outputs, and quality
    artifacts are preserved.
  - Post-closeout hardening: dependency-impact schema 4 separates production
    coupling from test-only coupling, classifies all 30 resolved duplicate
    dependency families with owners and expiry, and exposes executable 0.0.9
    targets for dependency count, duplicate families, and production blast
    radius. Direct canonical runtime-ABI imports reduced the maximum production
    blast radius from 11 to 7 and `support` from 10 to 6 dependents.
  - Post-closeout hardening: HIR symbol construction, managed-operation codec
    decoding, and database configuration resolution moved out of three
    997–998-line owners. The warning-band inventory fell from 82 to 79 files,
    and an executable 0.0.9 milestone gate requires 65 or fewer.
  - Post-closeout hardening: database configuration now has a typed subsystem
    error with stable rendering, reducing internal string-error sites from
    3,006 to 3,001 and ratcheting the command-owner ceiling to 901 sites.
    The standard-library test orchestrator is an independent fifth workspace
    crate rather than a product-crate target.
  - Post-closeout hardening: retained shared debug output is measured in
    `target/quality/cargo-artifact-retention.json`, warns above 128 GiB, and has
    an explicit maintenance command. The closeout retained approximately
    98.1 GB without deleting reusable build outputs.
  - Completion evidence: the post-hardening closeout rerun passed with 2,109 Rust files,
    673,203 physical lines, 443,043 logical lines, zero structural debt, 21
    production domains, 16 production coupling edges, 14 test-only coupling
    edges, five workspace packages, and both default and all-feature Clippy.

### Release code hygiene ownership

CQ-6 owns the permanent `make release-code-hygiene-check` umbrella and its
`target/quality/release-code-hygiene-report.json` evidence. Rust warnings,
dead-code classification, file-size headroom, hard file-size, function-size,
and module-size limits are release blockers. Public `panic!`, `unwrap`, and
`expect` sites must remain classified, and duplicate-helper findings retain
explicit owners.

The umbrella runs `dormant-runtime-code-check`, `shared-helper-check`,
`rust-file-headroom-check`,
`terlan-lint-style-profile-check`, and
`terlan-lint-pipe-canonicalization-check` alongside the safe Rust and structural
gates. The report is evidence, not a replacement for any executable subgate.

## Freeze Exit

CQ-6 passed on 2026-07-29, so the architectural expansion freeze is lifted.
The milestone-only closeout aggregate was retired after completion. Its durable
semantic subgates remain release-blocking through `make check` and prevent the
structural debt addressed by CQ-0 through CQ-6 from returning.
