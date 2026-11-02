# Checked purity and finite coverage component checkpoint

This checkpoint advances LNC-005; it does not complete LNC-005 or M1.
Four of 27 roadmap tasks remain complete.

The Terlan service selects checked purity contracts by exact Core signature and
coverage observations by compiler-owned function origins. The Rust bootstrap
serializes existing interface classifications and observes the existing finite
checker; protocol routing, selection, budgeting and query responses are Terlan.
Diagnostics and coverage observations consume the same analysis result.
Guarded clauses contribute no coverage. Conflicting inference visits return
unknown verdicts, capture is scoped and restored on unwind, and the 4,096-record
capture cap reports incompleteness.

Verified on this candidate:

- 22 authored Terlan query tests, including six new purity/coverage cases.
- 13 authored Terlan gate-harness tests.
- 22 independent MCP SDK checks, including purity, coverage, CLI parity,
  official-schema error validation, CRLF framing and clean EOF.
- 10 focused Rust bootstrap export tests and 78 targeted coverage regressions.
  The filtered suites overlap; their counts are not additive. The latter includes
  guard handling, nested finite domains, ambiguity, bounded capture and unwind.
- 46 semantic schema cases, 10 SDK model cases and four official MCP schema cases.

Compiled Terlan source fingerprints and exact authored test membership were
checked before retention. The checkpoint binds 112 declared input files.
The compiler identity is
`6034b4a586ca240df239adad9aa15a24598961e805e315c92fd89b254b7fa613`.
The gate source digest is
`397ce26f09392fd5191ad505490b206e256c48d73a16c14c2963014438da6656`.
The checkpoint SHA-256 is
`c5bf284c269e9071912c6abaf32dfa71836d9aaa71c7d68503a33fae5958ad18`.
The source archive SHA-256 is
`0a92836f7a02e5bf93e32311195e49762ec5a83a1978e36f5d3503d4cd1e8498`.

The source archive is the declared component closure, not a complete reproducible
compiler build closure. The compiler and VM remain trusted binary anchors.
`checksums.json` covers retained raw evidence and the source archive; this
explanatory README is outside that manifest. Initial invalid annotation fixtures,
a mistaken function-head fixture, and intermediate compilation/parse failures
are retained separately from passing-candidate evidence.

The live MCP fixture here uses one entity page. The preceding
[semantic origins checkpoint](../semantic-origins-8bd5a70c7c23/README.md) retains
multi-page resource discovery and reconnect evidence; those historical checks
are not claimed as a fresh run on this candidate.

Purity does not enumerate individual or transitive effects. Coverage observations
refer to the enclosing checked function's prepared syntax and are not proof of
all control-flow paths, generated branch behavior or an applicable repair.
Unknown domains stay explicit. Structural relations, remaining origin cases,
the full LNC-005 acceptance gate, transactional edits and release latency
qualification remain open.

## Local reproduction

Use a separate checkout with the archived component inputs and a compiler/VM
containing these bootstrap changes. The commands below refer to the frozen local
binaries used for this run; another compiler produces another evidence identity.
Run from the repository root with fresh output and cache directories.

```sh
export TERLC=/tmp/terlan-semantic-coverage-tools-1/terlc
export TERLAN_LLM_NATIVE_PEER_PYTHON=/tmp/terlan-llm-native-tools/bin/python
export TERLAN_SEMANTIC_TEST_COMPILER="$TERLC"
"$TERLC" test compiler/semantic/src/semantic/QueryTest.terl \
  --cache-dir /tmp/terlan-coverage-replay-query-cache \
  --emit-test-result-manifest /tmp/terlan-coverage-replay-query.json
"$TERLC" test scripts/llm_native/src/llm_native/GateTest.terl \
  --cache-dir /tmp/terlan-coverage-replay-harness-cache \
  --emit-test-result-manifest /tmp/terlan-coverage-replay-harness.json
"$TERLC" run scripts/llm_native/scripts/Gate.terls \
  --cache-dir /tmp/terlan-coverage-replay-gate-cache -- \
  "$TERLC" LNC-001 target/quality/llm-native-coverage-replay
cargo test -p terlan --lib commands::emit_core::tests
cargo test -p terlan --lib coverage
```

Copy the retained store before querying it because continuations may write cursor
files. The snapshot ID is recorded in `snapshot.json`.

```sh
replay_store=$(mktemp -d /tmp/terlan-coverage-replay-store.XXXXXX)
cp -a docs/roadmap/evidence/llm-native/M1/effects-coverage-397ce26f0939/snapshot-store/. "$replay_store/"
"$TERLAN_LLM_NATIVE_PEER_PYTHON" scripts/llm_native/peers/mcp_peer.py \
  --root "$PWD" --compiler "$TERLC" --store "$replay_store" \
  --snapshot s:89a5100509cefa461293b4d8b214352f360f0c0c374e55e1f170a9aa810ad3c7 \
  --cache /tmp/terlan-coverage-replay-mcp-cache \
  --output /tmp/terlan-coverage-replay-mcp.json \
  --stderr /tmp/terlan-coverage-replay-mcp.stderr
```
