# Checked structural relations component checkpoint

This advances LNC-005 without closing LNC-005 or M1. Four of 27 roadmap tasks
remain complete.

The Terlan service now exposes checked trait definitions, parsed shape patterns
and guards, the typed bounds used by callable and constrained-alias checking,
and explicit positive and negative trait facts. Negative evidence is distinct
from a missing implementation. Shape entities retain exact checked declaration
origins and stable identity across unrelated insertion. Changed shape guards
invalidate identity.

The Rust bootstrap reuses the canonical shape/pattern parsers, serializes the
existing checked interface, and observes actual declaration-checking schemes.
It does not independently infer satisfaction or implement MCP. Terlan owns
entity selection, relation projection, query budgets, discovery and transport routing.

Verified on this candidate:

- Six new authored Terlan relation tests, including nested implication fields,
  constrained aliases, overload origins, work limits and shape identity.
- All 22 existing query tests and 13 gate-harness tests.
- 26 independent live MCP checks, including positive/negative trait evidence,
  shape definitions, typed implications, declared schema validation, CLI parity,
  raw CRLF error handling and clean EOF.
- 13 focused Rust export tests, 99 implication regressions and two requirement
  capture tests. Filtered suite counts are not assumed additive.
- 52 semantic schema cases, 10 MCP SDK model cases and four official schema
  cases. The six new semantic cases constrain relation scopes, implication
  shapes, negative polarity and non-exhaustive candidate sets.

Compiled Terlan source fingerprints and exact authored test membership were
verified before retention. The archive binds 120 declared component inputs.
The compiler SHA-256 is
`2be8a855b292cee8d4c32c80a578dffd1c36cdd7bd041eb39c1b7f525934557e`.
The gate source digest is
`d2b2787573b0cf3764efca7dc0e9a14187435d955ac680b325cf9396b31b3000`.
The checkpoint SHA-256 is
`c64d2dfa706c6468fd715f3995277bd6ced7ce6636fdd312f433873685c6e0cd`.
The source archive SHA-256 is
`a92227ad67ba0ae1ea4db95f4b74a51abbd5b06ac7a8c09ee2a2107cb5b08a62`.

`checksums.json` covers 35 retained raw files and the archive; this explanatory
README is outside that manifest. The archive records declared component inputs,
not a complete reproducible compiler build. Compiler and VM identities remain
trusted binary anchors. Intermediate compile/parse failures are retained apart
from passing-candidate evidence.

## Scope and remaining work

Definitions and bounds belong to the selected declaration; explicit conformance
inventories are scoped to the containing module. Resolver bindings are exposed
without inventing provider entities. Generic variable IDs are local to each
checked requirement record. Capture is limited to 4,096 records and reports
truncation. Query work limits charge record traversal and cannot silently return
a truncated answer as complete.

Implications are attached to generic declarations, not invented standalone
symbols. Shape patterns and guard trees describe the canonical parsed definition;
they are not complete shape-expansion or use-site satisfaction proofs. Nested
expression spans are declaration-relative metadata. Candidate sets remain
non-exhaustive and call-site applicability requires verification.

Imported bound/provider entity links, shape-expansion origin links, broader
inferred facts, remaining origin cases, and the full LNC-005 gate remain open.
No transactions, candidate program execution, or release latency qualification
are claimed. This MCP fixture uses one resource page; the prior semantic-origins
checkpoint retains historical multi-page/reconnect evidence.

## Local reproduction

Use a separate checkout with the archived component inputs and a compiler/VM
containing the bootstrap changes. The commands below reference the frozen local
binaries used here. Another binary or input set produces another evidence
identity. Run from the repository root with fresh output/cache paths.

```sh
export TERLC=/tmp/terlan-semantic-relations-tools-1/terlc
export TERLAN_SEMANTIC_TEST_COMPILER="$TERLC"
export TERLAN_LLM_NATIVE_TEST_COMPILER="$TERLC"
export TERLAN_LLM_NATIVE_PEER_PYTHON=/tmp/terlan-llm-native-tools/bin/python
"$TERLC" test compiler/semantic/src/semantic/RelationsQueryTest.terl \
  --cache-dir /tmp/terlan-relations-replay-relations-cache \
  --emit-test-result-manifest /tmp/terlan-relations-replay-relations.json
"$TERLC" test compiler/semantic/src/semantic/QueryTest.terl \
  --cache-dir /tmp/terlan-relations-replay-query-cache \
  --emit-test-result-manifest /tmp/terlan-relations-replay-query.json
"$TERLC" test scripts/llm_native/src/llm_native/GateTest.terl \
  --cache-dir /tmp/terlan-relations-replay-harness-cache \
  --emit-test-result-manifest /tmp/terlan-relations-replay-harness.json
"$TERLC" run scripts/llm_native/scripts/Gate.terls \
  --cache-dir /tmp/terlan-relations-replay-gate-cache -- \
  "$TERLC" LNC-001 target/quality/llm-native-relations-replay
cargo test -p terlan --lib commands::emit_core::tests
cargo test -p terlan --lib implication
cargo test -p terlan --lib requirement_observations
```

Copy the retained store before querying; continuations may write cursor files.
The exact snapshot ID is also recorded in `snapshot.json`.

```sh
replay_store=$(mktemp -d /tmp/terlan-relations-replay-store.XXXXXX)
cp -a docs/roadmap/evidence/llm-native/M1/relations-d2b2787573b0/snapshot-store/. "$replay_store/"
"$TERLAN_LLM_NATIVE_PEER_PYTHON" scripts/llm_native/peers/mcp_peer.py \
  --root "$PWD" --compiler "$TERLC" --store "$replay_store" \
  --snapshot s:a913ac8cd6187f39f891b660d913cdb69fbb7c5470a3379427051ca7819ca291 \
  --cache /tmp/terlan-relations-replay-mcp-cache \
  --output /tmp/terlan-relations-replay-mcp.json \
  --stderr /tmp/terlan-relations-replay-mcp.stderr
```
