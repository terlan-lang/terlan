# Checked inferred binding types component checkpoint

This is evidence for progress on **LNC-005**, not completion of that task or M1.
The roadmap remains at four completed tasks out of 27.

The compiler observes its own checked local environments, reuses its lexical
binding analyzer on the same prepared syntax nodes, and applies the final clause
substitution before serializing each type. Pointer keys live only during that
borrowed compiler operation and never become artifact identities. The Terlan
service joins observations by module and the full Core binding record. A matching
name or numeric ID alone cannot substitute for that join.

Types use `checked-binding-type.v1` and `terlan.typecheck.type.v1`; generic IDs
are canonical within one observation and have no implied cross-observation or
declaration correlation. Direct parameter queries keep their existing Core type
representation. Conflicting visits and incomplete bounded captures return unknown.
These facts describe the checked definition, without claiming arbitrary use-site
expectations, ownership, or a complete effect set.

Passing evidence:

- 23 authored query tests, including concrete locals and nested shadowing.
- Six authored observation-selection tests, including adjacent unsigned 64-bit
  IDs, full-record joins, conflicting visits, incomplete capture and work limits.
- 12 relation queries and 13 gate harness tests.
- All 33 current snapshot cases in six suites, plus independent live schema and
  source inventory evidence.
- 29 independent MCP SDK/raw-wire checks, including a local inferred type query
  and CLI parity; no custom client transport.
- 64 semantic contract cases, ten SDK model cases and four official schema cases.
- 20 exporter, 21 binding identity and four capture/canonicalization Rust tests.
  These filtered Rust inventories overlap and are not additive.
- Seven actual checked structured local types independently validated against
  the recursive public schema and exact Core bindings: tuple, list, map, named,
  array, generic and function.

The source inventory is 142 declared component inputs. `sources.tar.gz` records
that closure; it is not a verified complete reproducible compiler build closure.
The compiler and runtime digests in `checkpoint.json` are trusted binary anchors.
Final observer unit tests and Rust formatting followed the producer build.
Every retained authored Terlan test manifest is checked against its exact current
source membership and compiled-source SHA-256 values. The raw reports, fixture,
structured-type oracle and snapshot store are retained. `checksums.json` covers
raw files and the checkpoint; this README is outside that inventory.

Exploratory failures are retained separately: the first added handler fixture
used invalid try/catch syntax; the corrected fixture passes. The first harness
invocation omitted the pinned Python environment, causing its two subprocess
checks to return false. The corrected invocation uses the required environment
and passes all 13 cases. Neither failure is counted as passing evidence.

Use a trusted compiler/runtime matching the checkpoint digests, from the matching
source archive or a checkout with identical declared input hashes. For example,
with fresh output/cache paths:

```sh
export TERLAN_SEMANTIC_TEST_COMPILER=/path/to/terlc
export TERLAN_LLM_NATIVE_TEST_COMPILER="$TERLAN_SEMANTIC_TEST_COMPILER"
export TERLAN_LLM_NATIVE_PEER_PYTHON=/path/to/pinned-peer-environment/bin/python
"$TERLAN_SEMANTIC_TEST_COMPILER" test compiler/semantic/src/semantic/QueryTest.terl --cache-dir /tmp/replay-query-cache --emit-test-result-manifest /tmp/replay-query.json
"$TERLAN_SEMANTIC_TEST_COMPILER" test compiler/semantic/src/semantic/BindingTypesTest.terl --cache-dir /tmp/replay-selection-cache --emit-test-result-manifest /tmp/replay-selection.json
"$TERLAN_SEMANTIC_TEST_COMPILER" test compiler/semantic/src/semantic/RelationsQueryTest.terl --cache-dir /tmp/replay-relations-cache --emit-test-result-manifest /tmp/replay-relations.json
"$TERLAN_SEMANTIC_TEST_COMPILER" test scripts/llm_native/src/llm_native/GateTest.terl --cache-dir /tmp/replay-harness-cache --emit-test-result-manifest /tmp/replay-harness.json
"$TERLAN_SEMANTIC_TEST_COMPILER" run scripts/llm_native/scripts/Gate.terls -- "$TERLAN_SEMANTIC_TEST_COMPILER" LNC-001 /tmp/replay-contract-gate
"$TERLAN_SEMANTIC_TEST_COMPILER" run scripts/llm_native/scripts/Gate.terls -- "$TERLAN_SEMANTIC_TEST_COMPILER" LNC-004 /tmp/replay-snapshot-gate
```

For MCP, capture `tests/fixtures/llm_native/mcp/query.terl` with the snapshot script,
then pass its returned snapshot ID and store to
`scripts/llm_native/peers/mcp_peer.py --root ... --compiler ... --store ...
--snapshot ... --cache ... --output ... --stderr ...`. The recorded run used the
unchanged 120-second SDK deadline. Its startup and CLI compilation cost remains
performance debt, without release latency qualification. The resource fixture
is single-page; earlier pagination/reconnect evidence remains historical.

The structured-type oracle can independently recheck the retained export using
`structured-type-oracle.py` with `--artifact structured-type-export.json`,
`--source structured-type-fixture.terl`, `--schema` pointing to the recorded
query-result schema, `--compiler` matching the trusted binary, and a new `--output`.
A fresh export can be generated with `terlc emit-core structured-type-fixture.terl`.
Remaining symbol-reference and origin cases, the full LNC-005 acceptance gate,
transactions and subsequent milestones remain open.
