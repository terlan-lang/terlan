# LLM native compiler completion evidence

The first two roadmap tasks passed on the same compiler and gate source
closure on 2026-11-01. The retained run directories are:

- [LNC-001 contract evidence](LNC-001/fed5ac49e535/report.json): 43 schema
  acceptance/rejection cases and 10 MCP message/model fixtures checked by
  independent `jsonschema` 4.25.1 and MCP SDK 1.26.0 peers.
- [LNC-002 harness evidence](LNC-002/fed5ac49e535/report.json): 9 authored
  Terlan tests passed, zero failed or unexecuted, with exact test membership.

The gate orchestration and evidence admission are Terlan code. Python is an
independent test peer. No Rust implementation was added. These checks do not
establish a live MCP server, semantic query API, or transactional source editor.
M1's live interoperability remains a separate gate. M0's subsequent completion
is recorded below.

A later [live query/MCP checkpoint](M1/query-mcp-a05ae1adafcb/README.md) records
17 independent SDK checks through the Terlan stdio server and CLI parity,
with frozen component inputs and raw query/protocol/harness evidence. It is
partial LNC-005 evidence; M1 remains open.

The subsequent [semantic origins checkpoint](M1/semantic-origins-8bd5a70c7c23/README.md)
adds 48 passing authored Terlan cases and a 21-case live MCP run over 1,514
entities, including real resource continuation after reconnect. It retains the
official protocol schema, corrected error handling and measured startup debt.

## Exact candidate identity

Each run retains `inputs.json`, raw peer/compiler reports, logs, timings, and
SHA-256 manifests. Inputs include compiler binary, standard-library tree,
native toolchain identity, execution policy, and gate/contract source digests.
The gate checks the source/compiler/stdlib identities again before success.

Gate source digest:
`fed5ac49e535f49b73756a6e26070dfc84286150d469870740f9bb0e6bd5596c`.

The [frozen source archive](sources-fed5ac49e535f49b73756a6e26070dfc84286150d469870740f9bb0e6bd5596c.tar.gz)
contains the exact contract, schemas, fixtures, and gate implementation for
these runs. Its SHA-256 is
`8c22ff460c84e1b5b4ce96ae6ec1774297411cb55986ee916f0d6b92a5c1f89e`.
It captures working-tree inputs on base revision `366feeb72516`, without
claiming that unrelated staged or unstaged repository changes were checked.
Earlier exploratory runs at the task directory roots retain their own hashes;
the completion records refer specifically to `fed5ac49e535/`.

## Reproduction

From the repository root, install the pinned independent test peers as described
in [runner setup](../../../../scripts/llm_native/README.md), then run:

```sh
export TERLAN_LLM_NATIVE_PEER_PYTHON=/tmp/terlan-llm-native-tools/bin/python
target/release/terlc run scripts/llm_native/scripts/Gate.terls -- \
  "$PWD/target/release/terlc" LNC-001 "$PWD/target/quality/llm-native-contract-new-run"
target/release/terlc run scripts/llm_native/scripts/Gate.terls -- \
  "$PWD/target/release/terlc" LNC-002 "$PWD/target/quality/llm-native-harness-new-run"
```

These commands use the currently checked-out sources and reject existing output
directories. For the historical candidate, use the frozen source inputs and
matching compiler/stdlib identities; a different toolchain creates new evidence.
Schema cases use synthetic document digests. SDK model checks validate the wire
forms, not live client/server behavior. Harness tests execute authored validation
code and bounded test peers, not generated candidate programs.

## M0 baseline completion

The [combined M0 report](M0/cdee894bf92d/report.json) passes the contract suite,
10 SDK wire/model cases, the expanded 12-test Terlan harness, and the compiler
baseline. Its SHA-256 is
`76d40a6f46b561eb78489061e6a1ad5f9ea03817c9aba2a4e58816daa4a0fe59`.
The [baseline report](M0/cdee894bf92d/baseline.json) retains all raw timings,
commands, source identities, diagnostics, and limitations.

| Leaf modules | Functions per leaf | Cold check seconds | Median of three warm checks seconds | Independent fresh check seconds |
| --- | --- | ---: | ---: | ---: |
| 10 | 4 | 1.190 | 0.238 | 1.176 |
| 128 | 64 | 13.707 | 0.864 | 13.635 |
| 1024 | 4 | 122.950 | 2.556 | 121.354 |

All workloads use the existing record/generic/shared-call compilation corpus.
Independent fresh checks produce matching public contract fingerprints. Warm
checks produce matching diagnostics; the current compiler emits no phase files
for cached checks, which is recorded as unavailable phase evidence. The small
workload's intentionally invalid interface yields byte-identical cached/fresh
diagnostics and checks successfully after repair. Existing LSP binding rename
passes in approximately 8 ms with 121 source-context bytes.

These are exploratory development-host observations, with three warm samples
and one requested build job. They establish neither p95 latency nor a release
regression budget. Current APIs do not expose phase work counts or peak RSS;
the report records these gaps. The ten required property case IDs are registered
for future milestones, without claiming those future cases have passed.

The [initial failed run](M0/initial-180s-timeout/report.json) is retained. Its
large cold checks exceeded the initial 180-second process limit. The passing
run uses an explicit 420-second bound for large cold/reference checks and has
a tested typed timeout diagnostic. Smaller and warm checks retain their own
bounds; a timeout still fails the gate.

The [M0 source archive](sources-cdee894bf92d79db9fa04ef608ad9fa619d5bd56ec700b8a0304acf6ac625643.tar.gz)
has SHA-256 `8bf93a64b7e87ea812129427ff601f6376455eb42345054e17c613850e5f22e5`.
The gate source digest is
`cdee894bf92d79db9fa04ef608ad9fa619d5bd56ec700b8a0304acf6ac625643`.
The retained run includes checksum manifests, raw logs, and compiler/stdlib/
toolchain identities. Large disposable caches stay in `target/`; the source
archive includes the exact fixture generator needed for fresh replay.

## M1 component checkpoint

The [initial snapshot checkpoint](M1/initial-snapshots/checkpoint.json) records
18 passing Terlan component tests and three focused bootstrap export tests.
It covers immutable single-file capture, invalid source, tamper rejection,
versioned unsaved overlays, stale selection rejection, lexical identities,
checked declarations, and same-name overloads. Live exports verify that valid,
invalid, and interface-only source each produce a single JSON artifact; checked
interface exports retain their dependency digests.

The checkpoint retains raw test manifests and logs, source/binary identities,
native callable source-identity checks, schema-validated example artifacts,
and a source archive. A failed parallel test invocation is retained alongside
its successful isolated-cache replay. Its project-interface failure does not
establish the safety of concurrent shared-cache preparation.

This is a partial component checkpoint, not completion of LNC-004 or M1.
Pattern and other entity identities, expanded origin links, broader source
closures, semantic query coverage, repair diagnostics, and live MCP stdio
interoperability remain required work. The [service notes](../../../../compiler/semantic/README.md)
describe the exact implemented scope and reproduction commands.

## LNC-004 completion

The [LNC-004 report](LNC-004/54fd37697019/report.json) passes on source closure
`54fd37697019bbba415c68e556b4fbabc5ecf16c33e69051d9711cbd8cb89739`.
Its SHA-256 is `ff1574d50f6470314fa4a588c1b47ea435ef451c5eb5d76abbcb0522174b0261`.
The [semantic report](LNC-004/54fd37697019/semantic-snapshots.json) retains exact
membership for 32 tests across six suites. Every suite passes with current
compiled service-source hashes and its own cache. Cases cover snapshot
immutability, stale and forged evidence, canonical source order, dependency
changes, imported declarations, overlay versions, invalid source, corruption,
module/declaration/overload/type/trait/binding/pattern identity, unrelated
insertion, rename invalidation, and lexical moves.

The live fixture has two sources and 12 entities spanning six required kinds.
An independent `jsonschema` 4.25.1 peer verifies public schemas, the exact source
and version inventory, entity uniqueness and membership, pattern ownership,
and origin bounds. Raw documents, the immutable store, logs, test manifests,
hash inventories, and bootstrap binary identity are retained. Supplementary
reports include 13 passing gate-harness tests and five focused Rust export
tests. The Rust bridge serializes existing compiler facts and shared interface
graphs; service logic and gate orchestration remain Terlan.

The [frozen sources](sources-54fd37697019bbba415c68e556b4fbabc5ecf16c33e69051d9711cbd8cb89739.tar.gz)
have SHA-256 `2f6142f6d7e2926e8a165d1afa37217ae697906ff166fcc28698b3737dcbdd36`.
The archive includes all 56 source inputs; [archive metadata](LNC-004/54fd37697019/source-archive.json)
records their ordering and individual digests. Compiler build ancestry remains
an explicit trusted binary anchor. It is not an independently verified build
chain.

Reproduce from the repository root with a fresh output directory:

```sh
export TERLAN_LLM_NATIVE_PEER_PYTHON=/tmp/terlan-llm-native-tools/bin/python
export TERLAN_SEMANTIC_TEST_COMPILER="$PWD/target/debug/terlc"
target/release/terlc run scripts/llm_native/scripts/Gate.terls -- \
  "$PWD/target/release/terlc" LNC-004 "$PWD/target/quality/llm-native-snapshots-replay"
```

The selected bootstrap compiler must contain `emit-core` source-set export.
Different sources or binaries produce new evidence identities. Snapshot scope
and unavailable facts are explicit; no MCP interoperability, transactions,
whole-program verification, or M1 completion is claimed by this task.

## LNC-005 checked effects and coverage checkpoint

The [effects and coverage checkpoint](M1/effects-coverage-397ce26f0939/README.md)
retains checked purity contracts and finite-pattern observations, 22 passing
Terlan query tests, 13 harness tests and 22 independent live MCP checks. Ten
bootstrap export tests and 78 targeted coverage regressions pass; their Rust
inventories overlap. Source fingerprints and exact authored test membership
were checked before retention. The archive identifies 112 declared inputs;
compiler build ancestry remains a trusted binary anchor.

The checkpoint SHA-256 is
`c5bf284c269e9071912c6abaf32dfa71836d9aaa71c7d68503a33fae5958ad18`.
LNC-005 and M1 remain incomplete. Follow the linked README for scope, raw evidence,
source and binary identities, and reproduction commands.

## LNC-005 structural relations checkpoint

The [structural relations checkpoint](M1/relations-d2b2787573b0/README.md)
retains checked trait/shape definitions, typed implication requirements and
explicit positive/negative conformance evidence. Six relation, 22 query and 13
harness tests pass with compiled-source freshness and exact membership checks;
26 independent live MCP checks validate the declared schemas and semantic
results. The refreshed contract gate accepts 52 semantic cases, 10 SDK model
cases and four official schema cases. Thirteen Rust export, 99 implication
regression and two requirement-capture checks pass; filtered counts are not
assumed additive.

The checkpoint SHA-256 is
`c64d2dfa706c6468fd715f3995277bd6ced7ce6636fdd312f433873685c6e0cd`.
The source archive binds 120 declared component inputs. Provider entity links,
shape-expansion origins and the combined LNC-005 acceptance gate remain open;
this checkpoint does not complete LNC-005 or M1.

## LNC-005 bound providers and shape expansion origins

The [provider origins checkpoint](M1/provider-origins-9a68499162d5/README.md)
retains exact trait-bound provider links and canonical nested shape expansion
references. Twelve relation, 22 query, 13 harness and one generated-origin test
pass. The complete snapshot gate passes all 33 current cases plus independent
live source/schema evidence. Twenty-eight MCP checks, 15 focused Rust export
checks, 312 shape-related regressions and 58 semantic schema cases pass; filtered
Rust counts overlap.

The checkpoint SHA-256 is
`a1a7eab0544cb9a145d5a92a1b67698f0e092648d3e395a42a4ccfbca590596e`.
The archive binds 134 declared component inputs. Retained failures include a
cold MCP initialization timeout, a pagination expectation corrected to verify
continuation, and the stale snapshot inventory's rejection of an unregistered
passing duplicate-declaration case. The current snapshot inventory registers all
33 cases. No release latency or LNC-005/M1 completion is claimed.

## LNC-005 inferred local binding types

The [inferred binding types checkpoint](M1/binding-types-3fb8a509fa69/README.md)
retains authoritative checker observations and bounded Terlan selection by module
and complete Core lexical identity. Twenty-three query, six observation-selection,
12 relation and 13 harness tests pass with exact authored membership and current
compiled-source hashes. All 33 snapshot cases plus independent live source/schema
evidence pass. Twenty-nine standard MCP SDK/raw-wire checks include inferred local
types and CLI parity; the contract gate accepts 64 semantic, ten SDK model and
four official schema cases. Twenty exporter, 21 binding identity and four capture
Rust tests pass, with overlapping inventories. Seven actual structured inferred
types pass independent schema and exact-binding validation.

The checkpoint SHA-256 is
`16309af66a4a0ad08fd0c764ecfa22d942203a599dbe786ac83b45f08b0713ca`.
The source archive binds 142 declared component inputs. Incomplete capture and
conflicting checker visits produce unknown; generic IDs are observation-local.
Retained exploratory failures distinguish an invalid handler fixture and an
omitted peer environment from corrected passing runs. Nonlexical references,
remaining origin cases and the full LNC-005 gate remain open. This checkpoint
does not establish release latency or complete LNC-005/M1.

## LNC-005 accepted call references and provider origins

The [call references and origins checkpoint](M1/call-origins-5680298d78c4/README.md)
retains exact accepted local overload origins and canonical imported signatures.
Terlan resolves provider links and serves function references and module call
inventories, keeping indirect or unsupported targets unknown. Six new reference,
23 query, 12 relation, six binding-type selection, one generated-origin and 13
harness cases pass with exact authored membership and current compiled-source
hashes. All 33 snapshot cases plus independent live source/schema evidence pass.
Thirty-one MCP checks and 70 semantic schema cases pass; 24 exporter, four capture,
19 import and 29 call/inference Rust tests pass with overlapping inventories.
An independent two-provider fixture checks that rejected trials do not publish
targets and that accepted signatures satisfy the public schema.

The checkpoint SHA-256 is
`14fdcd51cdb37c839d6814bcacc0e6c9e25bec378173e39eec993ec269d7be78`.
The archive binds 153 declared component inputs. An initial cold CLI parity build
exceeded the unchanged 120-second deadline after 29 passing SDK checks; all 31
checks pass on retry after host preparation in the same cache. Failed compiler,
fixture, lowering and JSON-comparison explorations are retained separately.
These are partial call inventories, with no claim that empty results prove
absence, no release latency qualification, and no LNC-005/M1 completion claim.
