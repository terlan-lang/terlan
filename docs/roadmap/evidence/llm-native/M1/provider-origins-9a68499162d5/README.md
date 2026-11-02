# Bound providers and shape expansion origins checkpoint

This advances LNC-005 without completing LNC-005 or M1. Four of 27 roadmap tasks
remain complete.

The Terlan query service resolves checked trait bounds to local or selectively
imported provider declarations. Canonical resolver bindings select the provider;
a same-named trait in another source cannot replace it. A provider outside the
snapshot retains its binding and an explicit unavailable entity link.

The actual compiler shape expander now retains canonical declaration references
on expanded pattern nodes, including nested shapes and imported aliases. Terlan
resolves those references to exact shape entities in the immutable snapshot.
Ordinary patterns cannot borrow provenance by resembling an expansion. The
optional metadata is omitted for unexpanded patterns, defaults when decoding
older data, and is excluded from semantic pattern equality. Required constructor
initializers for the added metadata field do not change pattern meaning.

Verified on this candidate:

- 12 authored relation tests, including decoy providers, nested local/imported
  shape origins, unavailable bound providers, exact entity/digest links and
  work-limited continuation without losing a link.
- 22 existing query tests, 13 harness tests and the generated-default origin test.
- The full snapshot gate: 33 tests across six suites, plus independent live
  schema and exact-source evidence. This includes stable repeated capture,
  canonical source order, dependency changes, overlays, stale handles and tamper
  rejection.
- 28 independent MCP checks, including bound/provider and shape-expansion links,
  declared schema validation, CLI parity, raw CRLF errors and clean EOF.
- 15 focused Rust export tests and 312 shape-related regressions. Filtered Rust
  suite counts overlap and are not assumed additive. The latter includes the
  metadata/equality and legacy-decoding check.
- 58 semantic schema cases, 10 SDK model cases and four official MCP schema cases.

Every authored suite passed with exact test membership and current compiled
source fingerprints. The archive binds 134 declared component inputs.
The compiler SHA-256 is
`acd377ac80a0dc3927b73731601d34b6ad7aec87802f52326c879aa9da5da487`.
The VM SHA-256 is
`2ee6c10712f4c50a80893ae02daee31b79134cae3df19df032b217007447c870`.
The gate source digest is
`9a68499162d5fe95889f044dd75e9203752b00370c325d6512b117005367e1ab`.
The checkpoint SHA-256 is
`a1a7eab0544cb9a145d5a92a1b67698f0e092648d3e395a42a4ccfbca590596e`.
The source archive SHA-256 is
`331e656e445443137cd35bdceca3dc6e82ca6203e4d42425f9cfeb89deb688fc`.

`checksums.json` covers 95 retained raw files and the source archive. This
explanatory README is outside that manifest. Disposable build caches are not
retained. The archive is the declared component closure, not a complete
reproducible compiler build; compiler and VM remain trusted binary anchors.

## Retained failures and limits

The initial cold MCP launch exceeded the SDK's 120-second initialization deadline
before any case completed. A subsequent attempt using the existing cache passed
all 28 cases with the same deadline. Its first initialization took about 75
seconds; the complete peer run took about 150 seconds. These are development
results, not cold-start or release latency qualification.

The first pagination test incorrectly expected a work-limit error when one of
two origin facts could be returned. The corrected test requires a partial first
page, a continuation, and exactly the remaining provider link on resume.

The first snapshot gate correctly rejected its stale inventory: all five
DeclarationEntities tests passed, but only four were registered. The omitted
duplicate-declaration case is now registered, and the reported test count is
computed from the registered suites. The historical 32-test LNC-004 completion
report remains unchanged; this candidate passes the current 33-test inventory.

Expansion origins are separate query facts; catalog `expanded_from` IDs are not
fabricated. Definitions, bound records and pattern origins have explicit scopes.
Provider traversal is charged to the work budget. These facts do not enumerate
all satisfying values or establish call-site applicability. Remaining inferred
facts, origin cases and the combined LNC-005 gate stay open. No source
transactions, candidate execution or later roadmap milestone is claimed.

This MCP fixture has one resource page. The earlier semantic-origins checkpoint
retains historical multi-page/reconnect evidence; it is not a fresh claim here.

## Local reproduction

Use a separate checkout with the archived component inputs and a compiler/VM
containing the bootstrap changes. The commands below reference the frozen local
binaries used in this run. Different binaries or inputs produce new identities.
Run from the repository root using fresh output/cache paths.

```sh
export TERLC=/tmp/terlan-provider-origins-tools-1/terlc
export TERLAN_SEMANTIC_TEST_COMPILER="$TERLC"
export TERLAN_LLM_NATIVE_TEST_COMPILER="$TERLC"
export TERLAN_LLM_NATIVE_PEER_PYTHON=/tmp/terlan-llm-native-tools/bin/python
"$TERLC" test compiler/semantic/src/semantic/RelationsQueryTest.terl \
  --cache-dir /tmp/terlan-provider-replay-relations-cache \
  --emit-test-result-manifest /tmp/terlan-provider-replay-relations.json
"$TERLC" test compiler/semantic/src/semantic/QueryTest.terl \
  --cache-dir /tmp/terlan-provider-replay-query-cache \
  --emit-test-result-manifest /tmp/terlan-provider-replay-query.json
"$TERLC" test scripts/llm_native/src/llm_native/GateTest.terl \
  --cache-dir /tmp/terlan-provider-replay-harness-cache \
  --emit-test-result-manifest /tmp/terlan-provider-replay-harness.json
"$TERLC" test compiler/semantic/src/semantic/GeneratedOriginsTest.terl \
  --cache-dir /tmp/terlan-provider-replay-generated-cache \
  --emit-test-result-manifest /tmp/terlan-provider-replay-generated.json
"$TERLC" run scripts/llm_native/scripts/Gate.terls \
  --cache-dir /tmp/terlan-provider-replay-gate-cache -- \
  "$TERLC" LNC-001 target/quality/llm-native-provider-replay-contract
"$TERLC" run scripts/llm_native/scripts/Gate.terls \
  --cache-dir /tmp/terlan-provider-replay-gate-cache -- \
  "$TERLC" LNC-004 target/quality/llm-native-provider-replay-snapshots
cargo test -p terlan --lib commands::emit_core::tests
cargo test -p terlan --lib shape
```

Copy the retained store before querying because continuations can create cursor
files. Read the snapshot ID from `snapshot.json`; all store contents are bound
by its identity.

```sh
replay_store=$(mktemp -d /tmp/terlan-provider-replay-store.XXXXXX)
cp -a docs/roadmap/evidence/llm-native/M1/provider-origins-9a68499162d5/snapshot-store/. "$replay_store/"
snapshot_id=$("$TERLAN_LLM_NATIVE_PEER_PYTHON" -c \
  'import json; print(json.load(open("docs/roadmap/evidence/llm-native/M1/provider-origins-9a68499162d5/snapshot.json"))["snapshot_id"])')
"$TERLAN_LLM_NATIVE_PEER_PYTHON" scripts/llm_native/peers/mcp_peer.py \
  --root "$PWD" --compiler "$TERLC" --store "$replay_store" \
  --snapshot "$snapshot_id" --cache /tmp/terlan-provider-replay-mcp-cache \
  --output /tmp/terlan-provider-replay-mcp.json \
  --stderr /tmp/terlan-provider-replay-mcp.stderr
```
