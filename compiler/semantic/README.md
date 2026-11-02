# Semantic compiler service

This package implements immutable scoped snapshots, entity indexes, bounded
semantic queries, and a Terlan MCP stdio service. See the
[roadmap](../../docs/roadmap/ROADMAP_LLM_NATIVE_COMPILER.md) for acceptance status.
LNC-005 remains in progress; source transactions are not implemented yet.

The service, admission checks, identities, storage, and overlay handling are
Terlan code. The narrow bootstrap command `terlc emit-core` serializes results
from the existing authoritative compiler. It does not introduce another type
checker or a Rust MCP implementation.

## Run

Build the bootstrap exporter and its runtime once:

```sh
cargo build --locked -p terlan --bin terlc --bin terlan-vm
target/release/terlc check compiler/semantic/src
target/release/terlc run compiler/semantic/scripts/Snapshot.terls -- \
  "$PWD/target/debug/terlc" /path/to/sample.terl src/sample.terl /tmp/semantic-store
target/release/terlc run compiler/semantic/scripts/Entities.terls -- \
  /tmp/semantic-store s:THE_RETURNED_64_HEX_DIGEST
target/release/terlc run compiler/semantic/scripts/Entities.terls -- \
  /tmp/semantic-store s:THE_RETURNED_64_HEX_DIGEST declarations
target/release/terlc run compiler/semantic/scripts/SourceSet.terls -- \
  "$PWD/target/debug/terlc" tests/fixtures/llm_native/semantic/sources.json /tmp/semantic-source-set
target/release/terlc run compiler/semantic/scripts/Entities.terls -- \
  /tmp/semantic-source-set s:THE_RETURNED_64_HEX_DIGEST catalog
```

After capturing a snapshot, run the shared query service or connect a standard
MCP stdio client:

```sh
target/debug/terlc semantic query /tmp/semantic-store /path/to/query.json
target/debug/terlc semantic mcp /tmp/semantic-store s:<snapshot-sha256>
```

Run these commands from this repository, or set `TERLAN_SEMANTIC_HOME` to the
absolute `compiler/semantic` package directory. This development launcher builds
the Terlan host, sends build diagnostics to stderr, then gives the application
stdin/stdout directly to the VM. Installed service packaging is tracked by M8.
`--cache-dir` selects a separate compiler cache when needed.

The server admits one explicit snapshot and advertises only `terlan_query` and
read-only resources. It uses MCP 2025-11-25 initialization, JSON-RPC errors,
standard `tools/list`, `tools/call`, and resource methods. Tool discovery embeds
the input schema and a self-contained output union: successful query documents
or typed `query-error` documents accompanied by `isError: true`. Serialized JSON
also appears in the standard text content block. No source edit tools are
advertised. No client-supplied host paths or resource URI become filesystem reads.

Resources expose the snapshot manifest and entity pages, including exact
snapshot/entity/digest handles for queries. Each resource body is at most 64 KiB;
entity page payloads are capped at 32 KiB, and resource discovery pages contain
at most 16 descriptors. Opaque content-addressed continuations bind the snapshot,
question and projection, survive reconnects, and reject altered storage or scope.
Admission caps the materialized catalog at 65,536 entities, 16 MiB and 1,024
entity pages; an oversized item fails explicitly. Admission currently materializes
the catalog before serving, so this is not an incremental workspace index.
Every domain request rechecks the admitted snapshot's integrity.

Query limits apply to logical compiler record visits and canonical result bytes.
Snapshot authentication and projection hashing are separate costs. Reference
pages retain shadowing identity and never label truncation as completion. Typed
Core signatures distinguish overloads using declaration origins; unsupported
ownership and impact facts are explicit. Boolean
expected-value candidates describe type inhabitants and retain unresolved
program-location constraints. Transformation candidates are planned and unverified.
MCP framing is implemented in `std/mcp/Stdio.terl` over generic bounded console I/O;
all lifecycle and domain decisions are Terlan code. The initial stdio host serves
serially; scheduling and cancellation are tracked by LNC-018.

Local binding type queries consume observations from the authoritative checker,
joined by module and the complete Core lexical binding record, including its
unsigned 64-bit identity, region, path, kind and spans. Names alone never join.
Direct parameters retain their existing Core type encoding; inferred locals use
`checked-binding-type.v1` with `type_encoding: terlan.typecheck.type.v1` and a
structured internal type. Type variable IDs are canonical within each observation;
they do not imply correlations with other observations or declaration generics.
Let/destructuring, case, lambda, comprehension and try/else scopes record their
checker environment after their body constraints. Capture is limited to 65,536
bindings and pending records per clause. Incomplete capture or conflicting visits
returns explicit unknown, and scanning every possible conflicting visit consumes
the query work budget. This describes the checked binding definition, not an
ownership guarantee or an expected type at an arbitrary use site.

Function reference queries scan compiler-observed calls across the immutable
snapshot, and module reference queries expose that module's call inventory.
The checker exports a target only after accepting its local overload or imported
interface signature; candidate probes do not publish targets. Call-site paths
come from the compiler's lexical traversal on the same prepared syntax nodes.
Source targets retain exact declaration spans. Imported targets retain the full
accepted interface signature, including canonical provider, generic bounds,
receiver flags and purity metadata. The Terlan service requires agreement with
that provider's checked contract and a unique exact declaration signature before
linking a snapshot entity. Function aliases and module aliases keep provider
identity even among same-named decoys.

Unknown indirect/unsupported targets and conflicting resolution visits have no
claimed target. Providers outside the snapshot retain the canonical signature
with an unavailable source link. A truncated capture contributes no claimed
references from that source unit. Capture admits at most 65,536 call sites and
visits and 4,096 declaration origins; queries charge source units, observations,
provider contracts, declarations and parameter visits against their work limits.
Call references remain partial: empty results never prove the absence of callers,
and this is not an incremental call graph or an arbitrary function-value flow
analysis. Source ranges cover enclosing compiler syntax, not identifier tokens.

Effects queries select the compiler's checked interface purity contract by the
exact Core signature, including overload parameter types. A false purity flag
means the compiler has not proven purity. Individual and transitive effects are
not enumerated, so these answers remain partial.

Coverage queries expose observations from the compiler's existing finite-pattern
checker for an exactly linked enclosing function. Diagnostics and observations
consume the same analysis result. Observations include expected types, uncovered
alternatives, guard indices, checked syntax identity and coarse source spans.
Guarded clauses contribute no coverage. Open domains and unsupported patterns
have null verdicts; conflicting inference contexts cannot certify exhaustiveness.
Capture is bounded to 4,096 observations and reports truncation explicitly.
Queries charge observation scans against their work budget and retain partial
status: these are checked prepared-syntax facts, not proof of all control-flow
paths, a branch implementation, or an applicable source repair. Rejected programs
retain diagnostic observations in the bootstrap export without successful Core.

Conformance queries return checked trait definitions, shape definitions, typed
declaration bounds and explicit positive or negative trait facts. Definition and
bound records belong to the selected declaration; conformance inventories are
explicitly scoped to the containing module. A negative fact is distinct from
an absent implementation. Definitions retain the compiler resolver's import
bindings, while implication types and variable IDs come from the same checked
schemes used by declaration and constrained-alias checking. Nested required
fields stay structured; the model does not need to parse type display strings.

Shape entities bind the checked raw definition plus its canonical pattern and
guard trees. Their IDs survive unrelated declaration insertion and invalidate
when the definition changes. Nested expression spans are declaration-relative
metadata, not exact source token ranges. Shape definitions are not an expanded
proof for every use site. Standalone implication symbols do not exist in Terlan:
implications are requirements attached to generic declarations.

Relation scans count against query work limits. Requirement capture is bounded
to 4,096 records with an explicit incomplete flag. Results remain partial and
candidate sets are not exhaustive; call-site satisfaction requires verification.
The public query-result schema constrains relation scopes, requirement models,
conformance polarity and structured field requirements. Trait bounds link to
local declarations or selectively imported providers through the checked resolver
map. Same-named declarations in another provider cannot replace that origin.
Providers outside the snapshot retain their canonical binding with an unavailable
entity link. Every bound/import/provider traversal counts against the work limit.

Expanded pattern nodes retain canonical shape references from the actual compiler
expansion, including nested definitions and imported aliases. Pattern conformance
queries resolve these references to exact shape entities in the same snapshot.
An ordinary pattern cannot acquire a shape origin by resembling its expansion.
Provenance remains separate from pattern semantic equality and is omitted for
unexpanded patterns, preserving legacy decoding and matching behavior. Declaration
anchors and expansion-provider links are separate query facts; the catalog's
`expanded_from` ID list is not populated by a guessed match. Remaining origin
cases and exhaustive satisfaction queries are still required for LNC-005.

Pattern handles now support bounded declaration/definition selection using the
catalog's exact identity factory. The traversal charges syntax nodes, object
keys and array positions, stops on budget exhaustion, and rejects stale handles.
A pattern's Core query is explicitly scoped to its enclosing function and links
that function through the compiler declaration origin, not a name-only guess.

Type-declaration queries return the checked Core type and expanded source
declaration. Local include edges retain exact parent entities and copied field
metadata. Imported include edges use the authoritative resolver's exported
`resolved_imports` map and the same snapshot's provider units, including checked
interfaces. A provider outside the snapshot retains its canonical alias evidence
and an explicit unavailable origin; that result stays partial. Provider lookup
and every declaration scan count against the query work budget. These facts do
not enumerate legal values for every alias or prove full program-location legality.

The compiler executable is trusted host configuration, not an agent-selected
program. Its export runs with a 30-second timeout and an 8-MiB output bound.
Source-set exports use a 420-second timeout and request at most 16 MiB of output.
Both paths consult the runtime's process ceilings and preserve typed failures.
No candidate application is executed. A failed check retains actual compiler
diagnostics and exports no checked Core. Stdout contains exactly one JSON
artifact, including for invalid programs; rendered diagnostics use stderr.

## Snapshot contract

`Snapshot.capture_file` freezes source before compilation. The manifest binds
source bytes, compiler binary, syntax contract, target, dependency interface
digests, flags, and static execution policy. `capture_overlay` additionally
binds a positive editor version and requires the expected disk digest before
and after compilation. It never writes editor contents into the working tree.
Disk captures use document version zero.

The initial compiler ancestry digest explicitly anchors the trusted bootstrap
binary: `sha256_domain_framed("terlan.semantic.bootstrap-root.v1", [binary_sha])`.
It is not a claim that compiler build ancestry has been independently verified.
Dependency interfaces are the actual interfaces supplied by the bootstrap
pipeline, including its required standard interfaces. Interface-only exports
include those dependencies too.

The snapshot ID hashes canonical manifest JSON without `snapshot_id` and the
normalized compiler artifact digest, using the domain
`terlan.semantic.snapshot.v1`. Physical staging paths are normalized only in
known origin fields; source literals are preserved. Store readers require the
publication marker and recheck source, document, export, and identity hashes.
Interrupted publication and artifact corruption cannot serve partial facts.
This store does not claim a durable multi-file source transaction; LNC-007 owns
that separate contract.

`SourceSet.capture` freezes an explicit list of source and overlay contents,
canonically sorts source-root-relative paths, and checks them against the same
source-interface graph as the authoritative project compiler. Each unit retains
its own checked Core, public interface, source declarations, and diagnostics.
Checked `.terli` files expose declarations without pretending to contain an
implementation. Input is limited to 1,024 files and 8 MiB of source; duplicate
paths, traversal, excluded cache paths, and negative versions fail before
compilation. The source-root exporter applies the compiler's existing module
layout validation. Supplied bytes are immutable inputs, not a claim about the
current contents of a separately edited workspace.

Source-set storage uses a versioned publication marker and generated physical
filenames. Every source, document version, unit export, and digest must agree
before reading the snapshot. Catalogs can retain conditional facts from checked
units when another unit fails; the snapshot remains `invalid`, never verified.

Analysis is explicitly `partial`, or `invalid` after compiler rejection. The
provided source set and loaded interfaces are bound; package configuration,
dependency implementations outside that set, expanded origin chains, and
automatic workspace context discovery are not claimed. Dependency interfaces
alone do not establish the correctness of their implementations.

## Entity identity

`Entities.bindings` projects the compiler's resolved lexical binding evidence.
It preserves full unsigned compiler identities through their JSON spelling.
Handles bind logical source path, module, lexical path, compiler binding ID,
and name. Unrelated declaration insertion preserves IDs; rename invalidates
the old handle. Resolution requires both snapshot ID and expected entity
digest, with no fallback to matching names. Digests conservatively bind the
whole normalized compiler unit, including dependency interfaces, so semantic
context changes require refreshed selections even when IDs remain stable.

`DeclarationEntities.declarations` indexes checked functions, receiver methods,
types, structs, traits, and described shapes. IDs bind complete checked declaration payloads with
syntax `span` metadata removed. This conservative initial scheme preserves IDs
across unrelated declaration insertion and distinguishes same-name overloads;
any semantic payload change invalidates the declaration ID. Duplicate identities
are rejected. These handles are not public ABI fingerprints.

`PatternEntities.index` gives identities to checked syntax pattern nodes,
including constants, wildcards, nested children, and map/record field values.
IDs bind the owning declaration, structural syntax path, and pattern payload.
Semantic changes to the owner conservatively invalidate pattern IDs; unrelated
declaration insertion preserves them. Compiler spans are retained without
inventing coverage or exact token positions. Local type facts use the separate
checker observation channel described above.

`Catalog.index` combines module, declaration, lexical binding, and pattern
entities, including checked interface declarations across a frozen source set.
`Catalog.resolve` requires snapshot, ID, expected kind, and expected content
digest. It rejects absent, stale, and wrong-kind selections without falling
back to names or source positions. A lexical move invalidates the old handle.

Compiler lexical spans currently cover enclosing syntax. The lexical catalog
reports `span_precision: enclosing_syntax`; these ranges must not be used as
exact rename edits. Module origins are explicitly file anchors. Unexported
local type observations, generated origins, other entity categories, and broader
workspace relations remain unavailable until their compiler evidence exists.

## Checks

```sh
target/release/terlc test compiler/semantic/src/semantic/CompilerBridgeTest.terl
target/release/terlc test compiler/semantic/src/semantic/SnapshotTest.terl
target/release/terlc test compiler/semantic/src/semantic/EntitiesTest.terl
target/release/terlc test compiler/semantic/src/semantic/DeclarationEntitiesTest.terl
target/release/terlc test compiler/semantic/src/semantic/CatalogTest.terl
target/release/terlc test compiler/semantic/src/semantic/SourceSetTest.terl
cargo test --locked -p terlan --lib commands::emit_core::tests -- --test-threads=1
```

The Terlan tests use `target/debug/terlc`, or the explicit
`TERLAN_SEMANTIC_TEST_COMPILER` override. Authored tests cover stale source and
forged-success rejection, immutable capture, corruption, invalid programs,
unsaved overlays, stable IDs, stale selection, rename invalidation, overloads,
declaration payload changes, pattern ownership and lexical moves, canonical
source-set order, imported declarations, and dependency freshness. The executable
`LNC-004` selector adds exact test inventories, current compiled-source hashes,
isolated per-suite caches, and an independently validated live source-set
snapshot/catalog. It does not close M1 or establish MCP interoperability.

Run these commands sequentially, or give concurrent invocations distinct
`--cache-dir` values. One parallel checkpoint run failed during project
interface preparation; its isolated replay passed. Shared-cache concurrency
is not established by this component checkpoint.

Catalog admission checks duplicate declaration identities with a set and only
computes pattern-owner identities for declarations that actually contain
patterns. The large-resource interoperability fixture exercises 1,500 additional
constant functions under the existing VM continuation and MCP timeout limits.
Its cases and timings remain component evidence, not release latency targets.
