# Terlan LLM native compiler executable roadmap

Status: M0 and LNC-004 complete; M1 compiler queries and MCP support in progress
Created: 2026-11-01
Tracking authority: the task checkboxes in this file
First delivery: a verified semantic rename transaction

## Outcome

A language platform is LLM-native when a probabilistic model can participate
through structured intent and semantic operations while the platform
deterministically verifies, constrains, explains, and applies its work.

Terlan will provide this development loop:

```text
intent
  -> bounded semantic context
  -> structured proposal
  -> deterministic verification
  -> actionable diagnostics
  -> transactional repair
  -> canonical artifact
```

LLM-first describes the development priority: design compiler interfaces for
this loop from the beginning. LLM-native describes the capability delivered
when the gates below pass. An MCP server or a source-generation assistant
alone does not satisfy the roadmap.

The compiler owns meaning and evidence. Models propose changes; they cannot
declare successful type checking, test execution, ABI compatibility, ownership
safety, or proof completion. The deterministic loop must work without a model
provider or network connection.

## Two first class programming interfaces

Terlan serves humans writing source and agents manipulating typed program
semantics. Both interfaces use the same name resolution, type system,
constraints, ownership rules, and artifact validation. Semantic program
transformation is part of the language platform contract.

Agents propose intent and implementation choices. The compiler resolves the
target, returns supported transformations and their obligations, prepares the
selected transformation, checks it, and publishes the verified result. Agents
can supply source bodies where needed; they do not need to reconstruct types
or invent textual patches for compiler-owned operations.

Source remains a readable, editable representation. Structured edits must
produce reviewable source and canonical semantic artifacts; handwritten source
must remain fully supported without an agent session or provenance history.

| Programmer question | Compiler response | Owning tasks |
| --- | --- | --- |
| What values are legal here? | Expected type, available constructors and bindings, and supported constraints; distinguish examples from a complete characterization of legal values | LNC-005, 011 |
| Which patterns make this match exhaustive? | Uncovered cases, guard assumptions, and checked pattern alternatives; branch behavior remains an implementation choice | LNC-005, 015 |
| What satisfies this shape or implication? | Resolved structural requirements, known satisfying candidates, and missing obligations | LNC-005, 014 |
| Which actor protocols can receive this message? | Compatible resolved protocols, state restrictions, and unresolved routing or protocol assumptions | LNC-022, 023 |
| What transformation preserves the public ABI? | Supported transformation plans with explicit ABI preconditions and target-specific compatibility evidence | LNC-008, 013, 023 |
| What changes would this edit propagate? | Affected declarations, interfaces, callers, artifacts, and tests, including conservative or unknown dependency edges | LNC-010, 016 |
| Can this patch be applied atomically and proven valid? | Transaction admission, checked invariants, outstanding obligations, and evidence with its proof or test scope | LNC-007, 009, 019–024 |

A legal transformation has explicit preconditions and verification obligations.
Listing it does not establish that every supplied implementation will pass.
Intent that has no supported operation returns a structured unsupported result
or bounded context for a source proposal. Successful verification establishes
the declared properties within the recorded scope; it does not prove arbitrary
program correctness or user intent.

## MCP standardization and Terlan implementation

Use MCP as the standard agent interface from the first query and rename
milestones. Follow its published wire contracts and JSON Schema conventions;
Terlan defines the compiler-specific operations, payloads, invariants, and
evidence carried through them. A new general-purpose agent protocol is outside
this roadmap.

Implement as much as possible in Terlan, including MCP message handling,
tool/resource registration, schema handling, request orchestration, and the
semantic transaction service. A first-party implementation of MCP in Terlan
is explicitly allowed. Standards reuse does not require using a Rust MCP SDK
or delegating compiler policy to Rust libraries.

Place reusable protocol support under `std/mcp/` and compiler-specific services
under `compiler/semantic/`. Reuse Terlan standard-library facilities and existing
native mechanisms for I/O, processes, sockets, and other platform boundaries.
Add Rust only for a concrete missing substrate mechanism that cannot reasonably
be implemented in Terlan. Record that boundary and keep protocol policy,
compiler orchestration, and new semantic rules in Terlan. Existing authoritative
Rust analyses can be called through a narrow versioned bootstrap boundary until
their self-hosted replacement passes its authority-transfer gates.

| Concern | Standard mechanism | Terlan-specific responsibility |
| --- | --- | --- |
| Tool discovery and calls | MCP `tools/list` and `tools/call` | Compiler tool names, arguments, legal transformations, and obligations |
| Request and result structure | MCP `inputSchema`, `outputSchema`, and `structuredContent` | Versioned semantic document schemas and invariant validation |
| Bounded context and artifacts | MCP resources, resource templates, and resource links | Immutable snapshot/context/evidence identities, contents, and access policy |
| Transport and compatibility | Standard MCP stdio initially; Streamable HTTP when needed | Select supported revisions, implement their required metadata/lifecycle, and publish compatibility |
| Progress, cancellation, and failures | MCP conventions for the supported revision | Stop compiler work, preserve transaction state, and return actionable semantic diagnostics |
| Transaction lifetime and publication | Compiler tools invoked through standard MCP calls | Candidate handles, freshness checks, idempotent recovery, verification, atomic publication, and rollback |

The initial contract pins MCP 2025-11-25, matching the independent SDK 1.30.0
test peer. Its mappings follow the official [tools specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools),
[resource specification](https://modelcontextprotocol.io/specification/2025-11-25/server/resources),
[transport specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
and [cancellation rules](https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/cancellation).
Other revisions require explicit compatibility evidence. Reuse the applicable
standard features before proposing an extension. Optional features are only
advertised when implemented and tested.

Keep domain documents inside standard tool arguments/results and resources;
do not wrap MCP in a second RPC envelope or invent a parallel discovery,
transport, or generic session protocol. Context-specific legal transformations
are compiler query results, distinct from the standard tool catalogue.
MCP capabilities and tool annotations do not establish program-execution
permissions or verified compiler facts. Transaction handles are explicit
compiler data and cannot rely on an implicit connection remaining alive.

Validate the Terlan implementation with protocol fixtures and independent MCP
clients, including clients built with official SDKs. SDKs can serve as test
peers without becoming runtime implementation dependencies. Interoperability
must exercise discovery, schema-valid calls/results, resources, cancellation,
and errors; tests against only our own client do not establish conformance.

## Tracking and completion rules

Each task has a stable ID, prerequisites, a concrete deliverable, and acceptance
conditions. Use its ID in implementation commits and pull requests. Keep a
task unchecked until its acceptance conditions pass; partial work belongs in
the active work table. Milestones close only when all their tasks and their
combined gate pass on the same candidate.

| Active task | Assignee | State | PR or commit | Blocker or next action |
| --- | --- | --- | --- | --- |
| LNC-005 | Codex | in_progress | [Call references and origins checkpoint](evidence/llm-native/M1/call-origins-5680298d78c4/checkpoint.json) | Accepted static calls now retain exact overload and provider origins through Terlan queries and MCP; finish the capability/origin audit and combined executable LNC-005 gate, including current resource continuation evidence |

When starting work, assign the task and set its state to `in_progress`. Other
states are `ready`, `blocked`, and `in_review`. A completed task gets a checked
box and a completion record below. Reopen it when its governing contract or
evidence becomes invalid. Do not rewrite IDs or silently remove requirements.

| Completed task or milestone | Implementation revision | Gate command | Evidence digest and retained location |
| --- | --- | --- | --- |
| LNC-001 | Working tree on `366feeb72516`; frozen source closure `fed5ac49e535…` | `Gate.terls -- "$PWD/target/release/terlc" LNC-001 <new-output>` with pinned peer environment | [Report](evidence/llm-native/LNC-001/fed5ac49e535/report.json), SHA-256 `cbdabbc5e21bafbf5a17692e818e958e634759af15ac2ae1a112152cbce25821` |
| LNC-002 | Same frozen source closure and compiler | `Gate.terls -- "$PWD/target/release/terlc" LNC-002 <new-output>` with pinned peer environment | [Report](evidence/llm-native/LNC-002/fed5ac49e535/report.json), SHA-256 `9b2234fe4fe16932aa49bb93036dabc90d4da17c263c126c9f7e5293fa6fb2d5` |
| LNC-003 and M0 | Frozen source closure `cdee894bf92d…`; includes current LNC-001/002 evidence | `Gate.terls -- "$PWD/target/release/terlc" M0 <new-output>` with pinned peer environment | [Report](evidence/llm-native/M0/cdee894bf92d/report.json), SHA-256 `76d40a6f46b561eb78489061e6a1ad5f9ea03817c9aba2a4e58816daa4a0fe59` |
| LNC-004 | Frozen source closure `54fd37697019…`; explicit source sets, overlays, and entity catalogs | `Gate.terls -- "$PWD/target/release/terlc" LNC-004 <new-output>` with pinned peer environment and selected bootstrap exporter | [Report](evidence/llm-native/LNC-004/54fd37697019/report.json), SHA-256 `ff1574d50f6470314fa4a588c1b47ea435ef451c5eb5d76abbcb0522174b0261` |

The [evidence notes](evidence/llm-native/README.md) give complete commands,
input identities, source archives, scope, and counts. M0 passes 43 schema cases,
10 MCP SDK model cases, 12 Terlan harness tests, compiler baseline parity, and
existing LSP rename checks. Later semantic editing milestones remain open.
LNC-004 adds 32 semantic tests with compiled-source freshness checks and an
independent live snapshot/catalog schema and exact-source oracle. Its snapshots
explicitly scope supplied sources and dependency interfaces; unexported facts
and origin precision remain limitations. M1 is not complete.

The [query/MCP component checkpoint](evidence/llm-native/M1/query-mcp-a05ae1adafcb/README.md)
adds a live Terlan MCP stdio server and `terlc semantic query` over shared query
code. Seventeen independent MCP SDK checks pass, including discovery, embedded
schemas, resource reads, overloads, explicit unknowns, rejection paths, and CLI
parity. Nine query tests, six protocol tests, thirteen harness tests and focused
runtime/compiler regressions pass; the refreshed contract gate accepts 46 schema
cases and 10 SDK model cases. Its frozen archive contains 95 declared inputs.
This is component evidence, not LNC-005 or M1 completion: broader semantic
categories, required origin links, multi-page resource continuation coverage and
the combined acceptance gate remain outstanding. Completion stays at 4/27 tasks.

The later [semantic origins checkpoint](evidence/llm-native/M1/semantic-origins-8bd5a70c7c23/README.md)
adds exact bounded pattern lookup, checked type declarations, local and imported
struct-include origins, and a generated default-adapter origin check. Its 48
authored Terlan cases and six focused bootstrap export cases pass with source
fingerprint checks. A 1,500-function fixture exposes 1,514 entities through 21
resources; 21 independent MCP checks cover discovery continuation, reconnect,
CLI parity, and raw stdio error handling. The official MCP schema corrects
anonymous error handling beyond the pinned SDK's narrower model. The refreshed
contract gate passes 46 semantic cases, 10 SDK model cases and four official
schema cases. Source inputs and raw failures/timings are retained; startup is
still slow and does not meet a release latency qualification. LNC-005 and M1
remain open, with 4/27 tasks complete.

The [effects and coverage checkpoint](evidence/llm-native/M1/effects-coverage-397ce26f0939/README.md)
adds checked interface purity and finite-pattern checker observations. Twenty-two
query tests, thirteen harness tests and twenty-two live MCP checks pass. Ten
bootstrap export tests and seventy-eight targeted Rust coverage regressions pass
with overlapping inventories. Guards contribute no coverage, conflicting inference
visits stay unknown, and bounded capture reports incompleteness. The refreshed
contract gate passes 46 semantic, 10 SDK model and four official schema cases.
Its archive binds 112 declared component inputs. Individual/transitive effects,
structural relations, remaining origins and the combined task acceptance gate
remain open; completion is still 4/27 tasks.

The [structural relations checkpoint](evidence/llm-native/M1/relations-d2b2787573b0/README.md)
adds checked trait/shape definitions, typed callable and alias requirements, and
explicit positive/negative conformance facts. Six new relation tests, all 22
query tests, 13 harness tests and 26 independent MCP checks pass. Thirteen Rust
export checks, 99 implication regressions and two bounded-capture checks pass.
The refreshed contract gate accepts 52 semantic cases plus 10 SDK model and four
official schema cases. Its archive binds 120 declared inputs. These are scoped
facts with explicit verification needs; provider/expansion links and the full
LNC-005 acceptance gate remain open. Completion remains 4/27 tasks.

The [provider origins checkpoint](evidence/llm-native/M1/provider-origins-9a68499162d5/README.md)
adds exact trait-bound provider links and canonical shape expansion provenance,
including nested imported aliases and decoy providers. Twelve relation tests,
22 query tests, 13 harness tests, the generated-origin test and all 33 current
snapshot cases pass with source fingerprints and independent live source/schema
evidence. Twenty-eight MCP checks, 15 Rust export checks, 312 shape-related
regressions and 58 semantic schema cases pass; filtered Rust counts overlap.
The snapshot inventory now includes the previously added duplicate-declaration
case. Initial inventory/pagination expectation failures and a cold MCP startup
timeout are retained; the retry uses the same 120-second deadline and existing
cache, without release latency qualification. The archive binds 134 declared
inputs. Remaining inferred facts, origin cases and the full LNC-005 gate stay
open; completion remains 4/27 tasks.

The [inferred binding types checkpoint](evidence/llm-native/M1/binding-types-3fb8a509fa69/README.md)
adds authoritative checker observations for local scopes, with complete lexical
identity joins in Terlan and an independent live MCP query. Twenty-three query,
six observation-selection, 12 relation and 13 harness cases pass with exact
authored membership and current compiled-source hashes. All 33 snapshot cases
and the independent live source/schema check pass. Twenty-nine MCP checks and
64 semantic schema cases pass; 20 exporter, 21 binding identity and four capture
Rust tests pass with overlapping inventories. Seven actual structured local
types independently satisfy the recursive public schema and exact Core joins.
Incomplete captures and conflicting visits remain unknown; generic variable IDs
are observation-local. The archive binds 142 declared inputs. An invalid added
handler fixture and a harness invocation missing its pinned Python environment
are retained separately from the corrected passing runs. Nonlexical symbol
references, remaining origins and the full LNC-005 gate stay open; completion
remains 4/27 tasks.

The [call references and origins checkpoint](evidence/llm-native/M1/call-origins-5680298d78c4/README.md)
adds checker-accepted call targets, snapshot-scoped function references and module
call inventories. Six new reference cases, 23 query, 12 relation, six binding-type
selection, one generated-origin and 13 harness tests pass with exact membership
and current compiled-source hashes. All 33 snapshot cases plus independent live
source/schema evidence pass. Thirty-one MCP checks and 70 semantic schema cases
pass. Twenty-four exporter, four capture, 19 import and 29 call/inference Rust
tests pass with overlapping inventories. An independent two-provider fixture
confirms rejected trials do not become accepted call targets. The archive binds
153 declared inputs. Indirect/unsupported targets and conflicting visits stay
unknown; truncated captures contribute no claimed references, and empty reference
results never prove absence. An initial cold CLI parity build exceeded its
unchanged 120-second deadline; the prepared-cache retry passes without a latency
qualification. The combined LNC-005 gate, current large-resource continuation
evidence and remaining capability/origin audit stay open; completion remains 4/27.

A completion record must identify the source closure, compiler identity,
target, feature and capability policy, exact command, exit status, test
inventory, and report digest. Working files under `target/` may hold local
results; completion records must link to retained CI artifacts or another
durable evidence location. A source revision alone does not identify uncommitted
candidate inputs. Record their content hashes as well.

Documentation of an intended command does not count as implementation. Empty
test selections, skipped required checks, stale evidence, and reports assembled
from different candidates cannot close a task. Tests, compiler checks, static
proofs, and bounded model checks must retain distinct evidence kinds.

## Current foundations and implementation boundaries

These are integration starting points, not completed roadmap milestones:

| Foundation | Existing entry point | Work required here |
| --- | --- | --- |
| Immutable lexical identities and reference evidence | [binding identity contract](../compiler/TERLAN_BINDING_IDENTITIES.md), `crates/terlan/src/compiler/typeck/binding_identity.rs` | Extend identity and navigation contracts to package symbols and snapshots |
| LSP binding rename | `crates/terlan/src/lsp/mod.rs`, `backend/navigation.rs` | Reuse its binding analysis; add checked workspace transactions and version guards |
| Typed CoreIR and interface loading | `crates/terlan/src/compiler/typeck/`, `compiler/hir/`, `formal_pipeline/` | Expose bounded, versioned semantic views without leaking private structures |
| Phase and artifact evidence | [phase manifest internals](../../crates/terlan/src/validation/phase_manifest/README.md) | Bind query, proposal, verification, and application to one candidate |
| Incremental cache and latency experiments | [incremental contract](../compiler/COMPILER_INCREMENTAL_CACHE.md), `scripts/benchmarks/compilation/` | Add semantic query invalidation, focused verification, and interaction budgets |
| VM ownership and capabilities | [ownership contract](../runtime/TERLAN_VM_OWNERSHIP.md), `crates/terlan/src/runtime/native_boundary/capability_sandbox.rs` | Bound generated execution and expose which guarantees are actually enforced |
| Self-hosted compiler schemas | [schemas](../../compiler/self_host/SCHEMAS.md), [self-hosting roadmap](ROADMAP_SELF_HOSTING.md) | Preserve the same public contract through compiler authority transfer |

`terlc inspect --snapshot` currently describes the runtime and installed release
layout. It is not a semantic query endpoint. Add a separate `semantic` command
family. CLI, LSP, and MCP adapters must call the same compiler service;
none may implement an independent type system or symbol resolver.

Shapes, implications, traits, sum types, patterns, and actor ownership provide
useful semantic inputs. Expose their supported facts explicitly. If a property
is unavailable or an analysis is incomplete, return that state instead of
manufacturing a guarantee.

Compiler semantics follow the authority boundary in the self-hosting roadmap.
Use the currently authoritative compiler first, with versioned public documents;
do not make this roadmap depend on completing self-hosting or create a permanent
second semantic implementation in Rust.

## Milestones and dependency order

| Milestone | Depends on | Delivery | Tasks |
| --- | --- | --- | --- |
| M0 | None | MCP mapping, semantic contracts, and evidence harness | LNC-001–003 |
| M1 | M0 | Terlan MCP support, snapshot queries, and diagnostics | LNC-004–006 |
| M2 | M1 | Verified rename over MCP with transactional application | LNC-007–009 |
| M3 | M2 | Bounded task context and repair proposals | LNC-010–012 |
| M4 | M3 | Checked function, trait, and pattern edits | LNC-013–015 |
| M5 | M2; close before M7 | Incremental feedback budgets | LNC-016–018 |
| M6 | M2; close before generated tests execute | Bounded execution and test evidence | LNC-019–021 |
| M7 | M4, M5, M6 | Actor protocol edits and retry demonstration | LNC-022–024 |
| M8 | M7 | Shared adapters and release qualification | LNC-025–027 |

Start with M0, M1, and M2. M2 is the first usable product milestone. Start
performance measurement in M0; M5 hardens reuse and enforces the complete
budgets. M3 and M5 can progress independently after M2. Operations that run
generated code remain disabled until M6 passes.

### M0 Contracts and executable evidence

- [x] **LNC-001 Define the semantic service contracts.** Prerequisite: none.
  Add `docs/compiler/LLM_NATIVE_COMPILER_CONTRACT.md` and versioned schemas under
  `compiler/semantic/schemas/`. Define snapshot, entity, query, proposal,
  diagnostic, verification, transaction, and provenance documents. Specify
  the MCP revision, standard feature mappings, JSON Schema dialect, compatibility
  rules, each operation's supported entity kinds, and discovery of legal
  transformations with preconditions and outstanding obligations. Identify the
  Terlan modules and any necessary native substrate boundaries. Acceptance:
  valid and invalid examples cover every document; unsupported versions,
  unknown operations, ambiguous targets, and missing preconditions fail with
  stable diagnostic codes. The contract defines compiler payloads inside MCP
  tools/resources and introduces no replacement RPC or discovery protocol.
- [x] **LNC-002 Build the milestone gate runner.** Prerequisite: LNC-001.
  Add a Terlan-owned harness under `scripts/llm_native/`, following the existing
  compilation benchmark layout. Register explicit case inventories and expected
  outcomes under `tests/fixtures/llm_native/`. Acceptance: missing executables,
  empty test selections, missing reports, timeout, stale input hashes, and
  unexpected skips fail the gate. Every milestone has a machine-readable result;
  unimplemented milestones report `not_implemented` and exit unsuccessfully.
  Register MCP wire/schema fixtures and independent-client interoperability
  cases, binding client versions and protocol revisions into the evidence.
- [x] **LNC-003 Establish correctness and latency baselines.** Prerequisite:
  LNC-002. Capture current binding rename, diagnostics, and clean/incremental
  checks on small, medium, and large modular fixtures. Record hardware, build
  mode, worker count, memory, compiler and input identities, context bytes, and
  phase work counts. Acceptance: repeated runs retain raw samples, a clean
  reference result, and explicit unsupported capabilities. Register all ten
  required properties in the coverage matrix below with executable case IDs.

M0 gate: schema validation, harness failure injection, and baseline replay pass.
M0 does not claim a working semantic editing API.

### M1 Semantic snapshots and diagnostics

- [x] **LNC-004 Introduce immutable semantic snapshots.** Prerequisite: M0.
  Build on lexical identity evidence and the compiler's resolved declarations.
  A snapshot binds source bytes, dependency interfaces, compiler and syntax
  identities, target, flags, and capability policy. Define entity IDs for local
  bindings, declarations, overloads, types, traits, and patterns. Acceptance:
  unrelated declaration insertion preserves IDs where promised; rename and
  lexical moves return explicit identity mappings or invalidate the old IDs.
  Stale IDs cannot silently select another entity. Unsaved document overlays
  and invalid source return versioned partial results with analysis status.
- [ ] **LNC-005 Expose compiler semantic queries.** Prerequisite: LNC-004.
  Implement reusable MCP support in Terlan under `std/mcp/` and handlers under
  `compiler/semantic/`, starting with standard stdio. Expose tools and bounded
  context/evidence resources using MCP discovery, calls, and declared schemas.
  Add `terlc semantic query` over the same service for declaration, definition,
  references, inferred type, interface, trait/shape/implication relations, effects, ownership facts,
  pattern coverage, and a bounded typed CoreIR view. Acceptance: resolved
  queries agree with the compiler; imported aliases, overloads, shadowing,
  included struct members, and generated declarations retain origin links.
  Unknown dynamic targets and unsupported ownership facts are explicit.
  Pagination and work limits never present truncated results as complete.
  Expected-value and legal-transformation queries distinguish known candidates
  from exhaustive answers and return their constraints and verification needs.
  An independent MCP client discovers the query tools, validates their schemas,
  and reads their results/resources without a Terlan-specific client protocol.
- [ ] **LNC-006 Define repair-oriented diagnostics.** Prerequisite: LNC-004.
  Adapt compiler diagnostics into stable invariant code, responsible entity,
  source ranges, expected/actual structured values, and dependency explanation.
  Attach repair candidates with preconditions, applicability, and evidence
  requirements. Acceptance: type mismatch, unresolved symbol, trait failure,
  non-exhaustive pattern, and effect/capability violation fixtures expose these
  fields. Inapplicable fields carry a reason; no safe automatic repair produces
  an empty candidate list with an explanation, not an invented fix.

M1 gate: fresh, stale, invalid, imported, and partial-snapshot queries match
clean compiler results; diagnostics are stable across equivalent runs. MCP
stdio interoperability passes discovery, calls, resources, and malformed-input
cases for the declared revision, with protocol handling implemented in Terlan.

### M2 Verified semantic rename transactions

- [ ] **LNC-007 Implement candidate isolation and atomic publication.**
  Prerequisite: M1. Add `prepare`, `verify`, `apply`, `abort`, and `status`
  transaction operations. Prepare changes in an isolated candidate; bind
  verification to its content digest. Apply requires unchanged source,
  document versions, dependencies, and policy, and the exact verified bytes.
  Acceptance: concurrent edits, deleted files, symlink escapes, interrupted
  writes, and candidate tampering preserve user work and fail safely. Compiler
  readers observe one published snapshot. For ordinary multi-file working
  trees, use a workspace lock and recovery journal; document visibility to
  external tools rather than claiming the filesystem offers a multi-file
  atomic write. Crash recovery produces the complete old or new compiler
  snapshot. Rollback refuses to overwrite later user edits.
- [ ] **LNC-008 Deliver `rename_symbol`.** Prerequisite: LNC-007.
  First support exact local bindings, then workspace functions and types with
  resolved references and import updates. Preserve comments, unrelated names,
  literals, and lexical scope. Acceptance: shadowing, capture, collisions,
  aliases, qualified calls, overloads, and included members have positive and
  rejection cases. Public API and named-argument label changes require an
  explicit compatibility policy. Missing external callers, generated owners,
  unsupported macros, or unresolved references cannot yield a claim that every
  use was updated. Recheck the affected semantic closure before publication.
  The prepare/verify/apply/abort/status operations and rename are accessible as
  standard MCP tool calls with structured results; no custom agent transport is
  required to complete the rename transaction.
- [ ] **LNC-009 Publish canonical patches and provenance.** Prerequisite:
  LNC-008. Emit intent, selected entity, proposal, before/after snapshot
  identities, canonical patch, source maps, verification results, and artifact
  digests. Keep human-readable diffs linked to machine-readable operations.
  Acceptance: deterministic replay produces identical semantic outputs and
  patches; formatting is idempotent; canonical import organization preserves
  meaningful order and comments. Stale evidence is rejected. Equivalent syntax
  can normalize; arbitrary behavioral equivalence is not promised. Git commit
  and remote publication are separate actions from applying a transaction.

M2 gate: an independent MCP client replays a recorded intent and renames a symbol
across a multi-module fixture, checks the candidate, publishes it, and replays
its evidence without a model.
Crash and concurrent-edit cases leave recoverable state. No generated program
executes. This milestone must also report the initial interaction budgets.

### M3 Bounded context and repair proposals

- [ ] **LNC-010 Index semantic dependencies and test relationships.**
  Prerequisite: M2. Add callers/callees, implementations, imported contracts,
  relevant tests, effects, and constraint edges to snapshot queries. Separate
  resolved edges from conservative or unknown edges. Acceptance: cross-module
  fixtures verify each edge kind, with recursion, higher-order calls, native
  boundaries, and unavailable dependencies exercising incomplete results.
- [ ] **LNC-011 Construct task-specific context slices.** Prerequisite:
  LNC-010. Accept an operation, target entity, constraints, and byte/work budget;
  return relevant declarations and implementations, callers, tests, effects,
  and invariants with inclusion reasons and snapshot identity. Acceptance: a
  required fact outside the budget yields `needs_expansion` or a rejected
  operation, never silent omission. Completeness is relative to the declared
  analysis and operation; verification still uses the necessary dependency
  closure. No claim of globally smallest context is required.
- [ ] **LNC-012 Connect diagnostics to structured repair.** Prerequisite:
  LNC-006, LNC-011. Convert applicable repairs into ordinary versioned proposals
  that use the same prepare/verify/apply path. Acceptance: a repair has explicit
  alternatives when intent is ambiguous, cannot change evidence or policy to
  make itself pass, and receives fresh diagnostics after application. Repeated
  failures stop at an attempt/resource bound with unresolved obligations.

M3 gate: an offline agent fixture receives only a compiler-produced slice and
submits a structured repair. The compiler checks it; withheld dependencies and
exhausted budgets produce actionable expansion requests.

### M4 Structured function and conformance edits

- [ ] **LNC-013 Add `add_function` and `replace_body`.** Prerequisite: M3.
  Accept typed signatures and parsed bodies or structured syntax. Source text
  is allowed inside a body; it must pass the same parser and semantic checks.
  Acceptance: signature, visibility, purity/effects, argument patterns,
  ownership obligations, and public ABI changes are checked. Comments and
  surrounding declarations survive. Invalid bodies remain isolated candidates.
- [ ] **LNC-014 Add `implement_trait`.** Prerequisite: LNC-013.
  Query required methods and constraints, then propose implementations against
  resolved trait/type identities. Acceptance: generic constraints, receiver
  rules, coherence, missing methods, shape/implication obligations, and wrong
  signatures have focused cases. A generated placeholder is an incomplete
  proposal and cannot count as a verified implementation.
- [ ] **LNC-015 Add `resolve_exhaustiveness`.** Prerequisite: LNC-013.
  Query uncovered cases and propose branches or an explicit policy choice.
  Acceptance: guards, nested sums, refutable argument patterns, and newly added
  variants are handled by compiler coverage analysis. Catch-all insertion
  cannot automatically satisfy a requirement to handle each variant, and
  ambiguous branch behavior remains a user/model decision.

M4 gate: each operation completes the full transaction loop and its negative
cases fail with structured diagnostics. Static verification can run before M6;
focused execution evidence requires M6. Adding an operation requires schema,
query, verification, provenance, and adversarial cases together.

### M5 Incremental feedback and interaction budgets

- [ ] **LNC-016 Reuse semantic query and verification results.** Prerequisite:
  M2. Extend existing cache policy to snapshot queries and candidate overlays.
  Track body, interface, dependency, target, effect, and policy changes
  separately where sound. Acceptance: body edits avoid unrelated analysis;
  public contract changes invalidate dependent queries and checks. Cold and
  warm results have identical canonical diagnostics and artifact identities.
- [ ] **LNC-017 Enforce measured latency and context budgets.** Prerequisite:
  LNC-003, LNC-016. Extend the compilation benchmark harness with query, slice,
  prepare, and verify measurements on the registered workloads. Acceptance:
  report p50/p95, memory, context bytes, cache hits, and phase work counts;
  compare against the budgets below on a declared reference host. Cold startup,
  index creation, focused tests, and release validation remain separate costs.
- [ ] **LNC-018 Make interaction cancellation and scheduling reliable.**
  Prerequisite: LNC-016. Bound concurrent proposals and cache storage; cancel
  obsolete work and clean only owned temporary artifacts. Acceptance: stop,
  timeout, resource exhaustion, and concurrent sessions release resources,
  retain useful diagnostics, and cannot apply cancelled candidates. A focused
  iteration does not silently invoke the full browser or release matrix;
  broader required gates are separately reported and still required for release.

M5 gate: the edit/invalidation matrix passes clean/incremental parity and
interaction budgets; cancellation and corrupted-cache injection fail safely.

### M6 Safe execution and focused test evidence

- [ ] **LNC-019 Define execution admission.** Prerequisite: M2.
  Extend capability and VM ownership boundaries to candidate execution. Declare
  network, filesystem, native/FFI, process, CPU/time, memory, mailbox, and output
  limits, with explicit unsupported target policies. Acceptance: a generated
  request cannot grant itself permissions; policy widening requires a distinct
  authorized action. Unsupported enforcement fails before execution. Renaming
  and static queries require no program-execution permission.
- [ ] **LNC-020 Run focused evidence in isolated candidates.** Prerequisite:
  LNC-019, LNC-010. Use compiler dependency data to select relevant tests and
  report selection reasons, unknown coverage, and required broader checks.
  Acceptance: missing coverage cannot be reported as success; deterministic
  fixture tests bind to exact candidate and runner identities. Timeout,
  nondeterminism, crashes, and capability denial have distinct outcomes. Child
  workers terminate when the transaction is cancelled.
- [ ] **LNC-021 Verify execution containment and evidence integrity.**
  Prerequisite: LNC-020. Add malicious and resource-exhausting generated
  fixtures, forged reports, symlink/file races, and native-boundary escape
  attempts within supported targets. Acceptance: no unauthorized effects or
  accepted forged success; capability and resource failures identify the
  responsible operation. Execution logs cannot replace compiler evidence.

M6 gate: permitted tests run within declared limits; denied, unsupported,
cancelled, and forged-evidence cases fail without modifying the live workspace.

### M7 Actor protocol edits and the retry acceptance test

- [ ] **LNC-022 Represent explicit actor protocol obligations.** Prerequisite:
  M4, M5, M6. Expose message variants, state variants, transitions, ownership,
  effects, and declared invariants. Decide and document the supported protocol
  representation before adding syntax. Acceptance: the checker identifies
  which properties are statically enforced, checked over a finite model, tested,
  or unknown. Actor ownership alone does not establish at-most-once completion.
- [ ] **LNC-023 Add `add_actor_message` and checked transition edits.**
  Prerequisite: LNC-022. Update protocol declarations, senders, receivers, and
  exhaustiveness obligations through ordinary transactions. Acceptance: stale
  senders, invalid transitions, missing handlers, capability changes, and
  message/ABI compatibility failures reject publication with repair candidates.
- [ ] **LNC-024 Deliver the retry demonstration.** Prerequisite: LNC-023.
  Given “Add retry behavior to this actor without allowing duplicate
  completion,” produce bounded context and a structured proposal for a
  maintained actor fixture. Model correlation IDs, attempt IDs, timeout races,
  terminal state, late replies, cancellation, duplicate delivery, and retry
  bounds. Acceptance: terminal completion occurs at most once for each request
  in the declared model; counterexamples include a dependency/transition trace.
  Define the finite state bound and external-effect assumptions. Restart or
  durable completion claims require durable deduplication evidence; otherwise
  report them outside the verified scope. At-most-once completion does not
  imply eventual success or exactly-once external effects.

M7 gate: the demonstration runs through intent, context, proposal, rejected
invalid candidate, actionable repair, bounded evidence, and canonical
publication. Its offline replay never requires the model to scan repository
files, forge a transition rule, or infer missing compiler behavior.

### M8 Shared adapters and release qualification

- [ ] **LNC-025 Route clients through one semantic service.** Prerequisite:
  M7. Expand the MCP integration delivered in M1/M2 and route CLI/LSP operations
  through the same Terlan service. Qualify additional clients and protocol
  revisions; add standard Streamable HTTP only when needed. Acceptance:
  equivalent requests produce the same snapshot identity,
  diagnostics, candidate, and evidence through every supported transport.
  At least two independent MCP clients pass the supported operation matrix.
  Client disconnect and stale editor versions preserve transaction safety.
- [ ] **LNC-026 Qualify deterministic replay and adversarial tasks.**
  Prerequisite: LNC-025. Maintain recorded intents and proposals for all
  operations, including malformed and hostile inputs, schema migrations, and
  compiler upgrades. Acceptance: the deterministic gates run offline. Optional
  live-model evaluations separately report model/version, attempts, context
  consumption, latency, repair success, and cost; model variability cannot
  replace compiler acceptance. Repository text and model output cannot alter
  the service's policy or evidence records.
- [ ] **LNC-027 Publish and maintain the supported capability matrix.**
  Prerequisite: LNC-026. Document operation/entity/target support, limitations,
  schema compatibility, budgets, recovery, and compiler authority. Acceptance:
  clean installed-toolchain replay passes the supported matrix and verifies
  artifact/provenance digests. Self-host authority transfer requires parity
  evidence for the same service contract. All ten properties below have retained
  evidence; unsupported combinations remain explicitly unavailable.

M8 gate: the complete recorded task corpus passes installed-toolchain replay,
cross-adapter parity, adversarial checks, and the interaction budget suite.

## Public operation and evidence contract

LNC-001 must define these compiler payload fields before implementing transport
handlers. MCP supplies the surrounding tool/resource and message contracts;
these documents do not replace them:

| Document | Required contract |
| --- | --- |
| Snapshot | Schema version; source/dependency closure digest; compiler ancestry; syntax, target, flags, and capability policy; document versions; analysis status |
| Query | Snapshot and entity IDs; requested fact; scope; work/byte limits; pagination; completeness or explicit unknown reason; origin/dependency links |
| Proposal | Intent and operation IDs; base snapshot; selected entity; typed arguments; source/body payload where needed; constraints; compatibility policy; expected entity/source digests |
| Diagnostic | Stable invariant code; entity and ranges; expected/actual values; dependency explanation; analysis limits; repair candidates and their preconditions |
| Verification | Candidate digest; compiler/target/policy; checks required/executed/skipped; diagnostics; test inventory/results; proof kind and scope; exit status; artifact digests |
| Transaction | Prepared/verified/applied/aborted/conflicted state; cancellation; candidate and base identities; verified publication preconditions; recovery journal; before/after identity mapping |
| Provenance | Intent, proposal, and context digests; before/after snapshots; verification and patch digests; artifact identities; actor/client attribution where supplied |

Canonical documents exclude wall-clock timings, random IDs, and machine paths
from semantic hashes. Keep operational timing and host metadata in a separate
observation envelope linked to those hashes. Record sensitive context only
under an explicit retention policy; provenance can retain digests and access
references without publishing source, credentials, or prompts.

## Gate commands and evidence layout

The Terlan runner implements task selectors `LNC-001` through `LNC-004` and the
combined `M0` gate; see [runner setup](../../scripts/llm_native/README.md).
Milestones `M1` through `M8` remain registered with `not_implemented` results.
The commands below fail until their acceptance suites are delivered:

```sh
# Use an explicitly selected, already built compiler; output directory is new.
target/release/terlc run scripts/llm_native/scripts/Gate.terls -- \
  "$PWD/target/release/terlc" M2 "$PWD/target/quality/llm-native-m2-run-1"

# Final deterministic qualification, independent of any model provider.
target/release/terlc run scripts/llm_native/scripts/Gate.terls -- \
  "$PWD/target/release/terlc" all "$PWD/target/quality/llm-native-all-run-1"
```

The runner owns its output directory and must not replace an existing run.
Each run writes `report.json`, `inputs.json`, `cases.json`, `diagnostics.json`,
`artifacts.json`, and `timings.json`. Transaction cases additionally retain
their proposal, canonical patch, journal outcome, and provenance record. Large
outputs may be content-addressed references with verified retrieval.

`all` expands to the checked-in milestone inventory. An individual milestone
gate runs its acceptance cases and validates prerequisite evidence bound to
the candidate; cached evidence is reusable only when all relevant inputs and
the gate implementation match. Unknown or unimplemented gates fail. The runner
lists expected case IDs and rejects zero matches or an incomplete result set.
Cheap schema and static cases run before compilation and execution.

Existing baseline commands remain useful before the new harness exists:

```sh
make binding-shadowing-safety-check
target/release/terlc check path/to/fixture --emit-phase-manifest path/to/report.json
```

The first command is an existing broader safety gate. The second is a template
using the existing check interface; supply real fixture and output paths.
Neither closes a new milestone on its own.

## Initial interaction budgets

These are acceptance targets to measure and ratify in LNC-003, not current
performance claims. Declare the reference host and optimized compiler; retain
at least 30 warm samples per operation. Benchmark a 10-module fixture, the
existing 128-module compilation workload, and a generated 1,024-module fixture.
Record function count, source bytes, graph shape, and supported feature mix.

| Operation | Initial p95 budget | Scope |
| --- | ---: | --- |
| Cached declaration/type/reference query | 100 ms | Warm snapshot, bounded result page |
| Task context slice | 250 ms | Warm index, at most 64 KiB returned |
| Local rename preparation and static verification | 1 s | Small fixture, no execution or code generation |
| One private body edit and focused semantic verification | 2 s | Warm 128-module fixture, no public contract change |
| Cancellation acknowledgement | 250 ms | Publication disabled immediately on cancellation |

These targets apply to the declared fixtures and scopes. Large public API
changes, cold indexing, native code generation, and tests report separate
budgets established from M0 measurements. Report context cost in UTF-8 bytes;
token counts are optional and identify the tokenizer. Limit expansions and
total attempts. Cancellation must also bound worker termination, cleanup time,
and temporary storage, with explicit values established in LNC-003.

M2 must measure its query and rename rows even before M5 optimizations. M2 can
close its correctness gate with measured latency debt assigned to LNC-017; M5
cannot close until the ratified budgets pass. Changing a budget requires a
reviewed roadmap update with measurements. Do not reduce fixture coverage,
skip required analysis, or relabel failures to meet a latency target.

## Required property coverage

LNC-003 registers concrete case IDs for these obligations; LNC-027 verifies
that every row has current retained evidence.

| Required property | Owning tasks | Required evidence |
| --- | --- | --- |
| Semantic observability | LNC-004, 005, 010, 022 | Queries agree with resolved compiler facts and expose unknowns |
| Structured actions | LNC-008, 013–015, 023 | Every supported operation completes its checked transaction path |
| Deterministic verification | LNC-002, 006, 016, 026 | Invalid and forged candidates fail; clean/replay results agree |
| Repair-oriented diagnostics | LNC-006, 012, 024 | Invariant, entity, expected/actual, chain, and applicable repair data |
| Bounded context construction | LNC-010, 011, 017 | Dependency inclusion, explicit omissions, and enforced budgets |
| Transactional changes | LNC-007–009, 018 | Conflict, crash, recovery, cancellation, and rollback cases |
| Fast incremental feedback | LNC-003, 016–018 | Latency distributions, work counts, and invalidation parity |
| Canonical output | LNC-009, 026 | Idempotent normalization and deterministic patches/artifacts |
| Safe execution | LNC-019–021 | Capability denial, ownership boundaries, resource limits, containment |
| Provenance | LNC-009, 026, 027 | Verified intent-to-artifact chain with freshness and integrity checks |

## First implementation sequence

1. Assign LNC-001 and define the schemas, operation boundaries, diagnostic
   representation, MCP standard mappings, Terlan implementation ownership, and
   compatibility rules. Keep all other tasks unchecked.
2. Deliver LNC-002 with a genuinely executable gate runner and explicit
   `not_implemented` results for later milestones.
3. Capture LNC-003 baselines, then deliver Terlan MCP support, immutable snapshots,
   and queries in M1 using the compiler's existing identity and phase evidence.
4. Deliver M2 as the first complete feature. Demonstrate one valid rename,
   one collision rejection, one stale-candidate rejection, and one interrupted
   application recovery before expanding the operation set.

The roadmap is complete when the maintained actor retry task and every
supported simpler edit can move from bounded context to canonical artifact
through the same deterministic, inspectable transaction service, within the
declared verification and execution limits.
