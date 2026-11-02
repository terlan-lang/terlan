# Terlan semantic compiler service contract

Status: contract v1; runtime service implementation is tracked separately
Roadmap task: LNC-001
MCP compatibility baseline: 2025-11-25
JSON Schema dialect: 2020-12

## Protocol and implementation ownership

The agent interface uses MCP tools and resources. JSON-RPC framing, lifecycle,
capability exchange, tool discovery, calls, results, progress, cancellation,
and error handling follow the pinned MCP revision. Compiler payloads follow
the [service inventory](../../compiler/semantic/contract.json) and the
self-contained schemas in `compiler/semantic/schemas/`.

The initial revision is deliberately pinned to MCP 2025-11-25, which the
independent Python SDK 1.26.0 test peer supports. This is a compatibility
baseline, not a claim to implement newer revisions. The initial server offers
standard stdio. Initialization selects only implemented revisions; a client
that cannot accept the selected revision disconnects according to MCP's
[lifecycle rules](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle).
The connection requires `initialize` and `notifications/initialized` before
ordinary tool calls. Neither the newer per-request metadata model nor older
protocol versions are silently accepted as this revision.

Errors preserve a readable request ID. If malformed input makes the ID
unavailable, the response omits it, as specified by the pinned MCP error schema.
The official schema is retained with revision and license provenance for
independent checks where the pinned SDK's error model is narrower.

Implement protocol and service policy in Terlan. `std/mcp/` owns reusable MCP
support; `compiler/semantic/` owns semantic schemas, queries, proposals, and
transactions. Existing native JSON parsing, hashing, filesystem/process I/O,
and authoritative bootstrap compiler analysis remain mechanisms behind typed
boundaries. A Rust MCP SDK is not required. Independent Python/other SDK peers
are test dependencies, not production semantic implementations.

Initial capabilities are `tools` and `resources`, without optional list-change,
subscription, prompts, sampling, task, or elicitation claims. Advertise only
implemented tools. The operation inventory includes planned operations and is
not an MCP `tools/list` response. Optional capabilities and remote transports
need their own interoperability evidence before advertisement.

## Standard tools and resources

Tool names are the following compiler-defined names carried by standard
`tools/list` and `tools/call`. They are not new JSON-RPC method names.

| Tool | Arguments | Result |
| --- | --- | --- |
| `terlan_snapshot` | Authorized workspace identity and optional versioned document overlays | Snapshot document and immutable resource links |
| `terlan_query` | Query document | Query result document, or a typed query-error with `isError: true` |
| `terlan_prepare` | Proposal document | Prepared transaction or semantic diagnostics |
| `terlan_verify` | Transaction ID and expected candidate digest | Verification document and updated transaction |
| `terlan_apply` | Transaction ID, candidate digest, and verification digest | Applied transaction, canonical patch, and provenance links |
| `terlan_abort` | Transaction ID and expected candidate digest | Aborted or already-terminal transaction |
| `terlan_transaction_status` | Transaction ID | Current transaction document |

Use `inputSchema` and `outputSchema` for the actual tool payloads and
`structuredContent` for structured results, with the serialized JSON text
representation recommended by the pinned [tools specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).
Inline local schema definitions into advertised schemas so clients never need
to fetch compiler schema references over the network. A proposal's `operation`
selects a compiler transformation within `terlan_prepare`; MCP handles tool
discovery and invocation around it.

The current development server admits one explicitly selected immutable snapshot
and advertises only `terlan_query`; the other tools above remain planned. Its
standard resources expose the manifest and bounded entity pages at
`terlan://snapshots/<snapshot-id>/entities/<page-index>`. Each entity includes its
own identity, digest and origin. Query failures have their own self-contained
`query-error` schema; the advertised output schema accepts both success and error
documents without introducing another RPC envelope.

Snapshot artifacts use immutable resource identities such as
`terlan://snapshots/<snapshot-id>/entities/<entity-id>` and
`terlan://transactions/<transaction-id>/evidence/<digest>`.
Use standard resource reads/templates and result resource links. Resolve only
resources the server owns and the caller may access. URI possession does not
grant filesystem or execution permissions. Pagination cursors bind the
snapshot and query, have explicit lifetime, and cannot select another snapshot.

Protocol errors use the pinned MCP/JSON-RPC error rules. Tool execution failures
use `isError: true` and structured semantic diagnostics; a verification tool can
successfully return a `failed` verification result, which does not authorize
application. Schema acceptance establishes document shape only. It is never
compiler evidence of source correctness or permission to execute code.

## Semantic documents

Every document has an exact `schema` discriminator such as
`terlan.semantic.proposal.v1`. Required fields cannot be omitted; unexpected
fields fail validation. The JSON examples in the fixture inventory are schema
examples with synthetic digests, not trusted compiler outputs.

| Document | Meaning and runtime invariants |
| --- | --- |
| Snapshot | Immutable source closure, dependency interfaces, compiler ancestry/binary, syntax, target, flags, and policy. Complete analysis has no omitted facts; partial/invalid analysis explains limitations. |
| Entity | One resolved identity and kind within a snapshot, with owning declaration, digest, UTF-8 byte range, and expansion origins. Unrelated insertion preserves identity where the binding contract promises it; moves/renames require a mapping or invalidation. |
| Query | Snapshot, exact target identity/kind/digest, requested fact, bounded work/bytes, and optional cursor. No name-only or ambiguous target selection. |
| Query result | Facts with origin identities, measured work/bytes, explicit completeness, limitations, and cursor. A truncated result cannot report complete; unavailable results contain no invented facts. |
| Proposal | Intent digest, base snapshot, operation, exact target, operation arguments, expected source/entity/dependency/policy digests, compatibility policy, and obligations. |
| Diagnostic | Stable invariant code, severity, entity or explicit absence, ranges, expected/actual observations, dependency chain, limitations, and repair candidates. No safe repair requires an explanation. |
| Verification | Exact candidate/input/compiler/target/policy/gate identities, required cases, actual checks, evidence kinds, exit status, diagnostics, and artifacts. Success requires every required case exactly once and passed, with matching evidence. |
| Transaction | Explicit handle, base/candidate identities, lifecycle state, verification link, publication snapshot, recovery journal, and identity mapping. Verified/applied states require verification; only applied has a published snapshot. |
| Provenance | Content-addressed intent/context/proposal, before/after snapshots, candidate/verification/patch, artifact identities, and optional attribution. These links must resolve to matching verified bytes. |

Ranges are half-open UTF-8 byte intervals with `0 <= start_byte <= end_byte`
inside the identified source. Paths are normalized workspace-relative paths;
absolute paths, parent traversal, and symlink escapes are rejected by resource
admission. These contextual checks are additional to JSON Schema validation.

IDs have kind prefixes (`s:`, `e:`, `c:`, `t:`) and 64 lowercase hexadecimal
characters. Raw digests are SHA-256 hexadecimal. Snapshot/candidate identities
bind canonical input manifests using domain-separated framed hashing, with
the exact algorithm version recorded by the implementation before ID creation.
Never derive identity from human display names, timestamps, or host paths.
Canonical JSON sorts object keys and preserves meaningful array order; initial
identity manifests use strings, booleans, nulls, and integers, with no floating
point identity fields. Preserve source bytes and explicit document versions.

Transaction handles also require server-owned state and authorization checks;
a well-shaped or guessed handle is not authority. Verification documents from
clients are untrusted and cannot replace server-generated evidence.

## Transformation availability and preconditions

| Operation | Target kinds | Arguments and additional obligations |
| --- | --- | --- |
| `rename_symbol` | binding, function, type | `new_name`; valid identifier, scope/capture checks, complete supported references, public-label/API/ABI compatibility |
| `add_function` | module | `declaration_source`; parse and check complete declaration, visibility, signature, effects, and duplicate names |
| `replace_body` | function | `body_source`; preserve the selected signature and declared contracts unless a separate explicit change authorizes them |
| `implement_trait` | trait, type | Resolved trait/type IDs and method bodies; coherence, receiver, signature, and constraint checking |
| `resolve_exhaustiveness` | pattern | Proposed branches and explicit coverage policy; compiler coverage checking without inventing branch behavior |
| `add_actor_message` | actor, protocol | Message declaration; compatibility, sender/receiver, state, and exhaustiveness obligations |
| `edit_actor_transition` | actor, protocol | Resolved state/message IDs and transition body; explicit protocol/effect/ownership invariants |

All operations are currently planned. Detailed typed argument schemas for
later operations must be added before those tools/operations are enabled.
The v1 schema already constrains rename, function addition, and body replacement
arguments. Generic argument objects on unavailable later operations are reserved
payloads, not acceptance of arbitrary executable transformations.

Discovery lists implemented tools. `legal_transformations` returns contextual
candidates and unmet obligations for an exact snapshot/entity; it does not
mutate tool schemas or claim that every supplied body passes. Unknown operation
names fail the proposal schema. A known but unavailable operation yields
`semantic.operation.unsupported`. Target-kind mismatch yields
`semantic.operation.target_kind`. Public compatibility defaults to preservation;
breaking requests require explicit policy and separate authorization where
applicable. No operation can grant its own execution capabilities.

## Verification and transaction authority

```text
prepare -> prepared -> verify -> verified -> apply -> applied
                 |              |
                 +---- abort ---+-----------> aborted
                 +---- stale/conflict ------> conflicted
```

Failed or incomplete verification leaves a prepared candidate without publication
authority. Repeated verification is safe for identical inputs. Apply compares
the verified candidate and all base inputs immediately before publication under
the workspace publication lock. A changed source, dependency, target, compiler,
policy, or gate invalidates evidence and requires a fresh candidate/check.

Repeated apply for the same verified transaction returns its original result;
it cannot replay a patch over later user work. Abort never rewrites applied
source. Transaction status remains inspectable after disconnect. Cancellation
uses MCP signalling and independently disables publication; disconnect does not
mean a multi-file write was rolled back. Recovery follows the journal and
rejects overwriting subsequent user edits. External filesystem readers retain
the visibility limitation documented in LNC-007.

The initial service is static-only. Test, proof, bounded-model, and compiler
checks have separate evidence kinds and scopes. Running generated programs
requires M6's execution admission; no natural-language request, tool annotation,
or capability advertisement can bypass it. At-most-once actor completion and
ABI preservation require explicit checks in their declared models/targets.

## Diagnostics and schema compatibility

Schema validation failures use `semantic.schema.<keyword>`, where `<keyword>`
is the standard JSON Schema keyword (`required`, `type`, `const`, `enum`,
`additionalProperties`, `pattern`, `minimum`, `minItems`, `maxItems`, `anyOf`,
and so on). Return escaped JSON Pointer locations for instance and schema,
plus a human explanation. Sort diagnostics by instance pointer, schema pointer,
then code. Preserve all independent errors; a failure need not have one cause.
Contextual validation uses distinct codes such as `semantic.snapshot.stale`,
`semantic.entity.unknown`, `semantic.operation.target_kind`,
`semantic.verification.incomplete`, and `semantic.transaction.conflict`.

Document schema IDs, MCP protocol revision, and compiler semantic contract
version are independent. A change to field meaning, required shape, operation
semantics, or evidence authority requires a new semantic contract version.
Do not change a discriminator's meaning in place. Support for another MCP
revision requires protocol conformance evidence, not a renamed version field.

## Executable contract evidence

The [fixture inventory](../../tests/fixtures/llm_native/contracts/cases.json)
contains valid and rejected examples for every document, including unsupported
versions, unknown operations, ambiguous targets, missing preconditions,
false completeness, unverified publication, and false successful verification.
The independent schema peer uses pinned upstream `jsonschema` and MCP SDK
packages to validate documents and advertised tool schema forms. It is a test
oracle; Terlan owns the production implementation and gate orchestration.
The MCP wire fixture inventory additionally checks initialization, tool calls,
descriptors, structured results, resources, and malformed messages against
the pinned SDK's models. Model validation is distinct from live server/client
interoperability, which remains an M1 gate.

The schema gate establishes the LNC-001 document contract. It does not close
M0's baseline work or any runtime semantic, transaction, or MCP interoperability
milestone. Those require their own same-candidate evidence.
