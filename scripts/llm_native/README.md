# LLM native compiler gates

The gate driver and evidence admission are implemented in Terlan. Pinned Python
packages provide independent schema/MCP test peers; they are not production
compiler dependencies. The MCP compatibility baseline is 2025-11-25.

The official protocol schema and license are pinned under
`tests/fixtures/llm_native/mcp/spec.*`, with revision and SHA-256 provenance.
The schema oracle checks anonymous errors directly against that official schema;
SDK 1.30.0's error model requires an ID. The Terlan server omits unreadable IDs,
preserves readable IDs in malformed requests, and never replies to peer errors.

Create a test environment once:

```sh
python3 -m venv /tmp/terlan-llm-native-tools
/tmp/terlan-llm-native-tools/bin/pip install -r scripts/llm_native/peers/requirements.txt
export TERLAN_LLM_NATIVE_PEER_PYTHON=/tmp/terlan-llm-native-tools/bin/python
```

From the repository root, with an already built compiler and a new output path:

```sh
target/release/terlc run scripts/llm_native/scripts/Gate.terls -- \
  "$PWD/target/release/terlc" LNC-001 "$PWD/target/quality/llm-native-contract-run-1"

target/release/terlc run scripts/llm_native/scripts/Gate.terls -- \
  "$PWD/target/release/terlc" LNC-002 "$PWD/target/quality/llm-native-harness-run-1"
```

`LNC-001` checks all registered schema examples against the independent peer,
including exact case membership and input hashes. `LNC-002` runs the exact
Terlan failure-injection test inventory and checks the compiler's result
manifest. Nested harness checks use the selected compiler through an explicit
environment overlay.

`LNC-003` captures compiler baselines at 10, 128, and 1,024 leaf modules, using
the existing record/generic compilation corpus. It compares independent fresh
public contracts, warm diagnostics, a small invalid-interface edit/repair cycle,
and existing LSP binding rename. Generated applications are never executed.
The combined `M0` selector runs all three tasks. Large cold/reference compiler
checks have a 420-second process limit; other baseline commands have a
180-second limit. Process timeouts remain failures and retain their typed cause.

`LNC-004` runs 33 semantic component tests with exact membership and current
compiled-source hashes, using separate caches for each suite. It captures a live
two-source snapshot and catalog, then checks public schemas, source digests,
document versions, entity kinds, ownership, and origin bounds with the pinned
independent `jsonschema` peer. Set `TERLAN_SEMANTIC_TEST_COMPILER` to a built
compiler containing `emit-core` (default: `target/debug/terlc`). The host compiler
argument still selects the compiler/runtime executing authored Terlan tests.

```sh
export TERLAN_SEMANTIC_TEST_COMPILER="$PWD/target/debug/terlc"
target/release/terlc run scripts/llm_native/scripts/Gate.terls -- \
  "$PWD/target/release/terlc" LNC-004 "$PWD/target/quality/llm-native-snapshots-new-run"
```

The gate retains test manifests, logs, bootstrap binary identity, live artifacts,
independent evidence, and hashes. Source sets explicitly describe their analysis
limits; this task does not claim M1, live MCP, or transaction completion.

LNC-005 is the combined semantic-query acceptance selector. It runs current
schema/MCP contract checks and the failure-injection harness, then the exact
registered query inventories in tests/fixtures/llm_native/semantic/query-gate.json.
This currently includes 60 authored cases across QueryTest, RelationsQueryTest,
BindingTypesTest, CallReferencesTest, GeneratedOriginsTest, QueryGateTest and the
standard MCP ProtocolTest suite.
Every authored manifest must contain precisely those tests and current hashes
for all compiled source files, including the test source itself.

The same command reruns the LNC-004 snapshot prerequisite using the selected
compiler as both host and bootstrap. It prepares the MCP and query hosts
serially in one cache, captures small and large immutable fixtures, and invokes
the pinned independent SDK peer. The large fixture adds 1,500 deterministic
constant declarations that are indexed but never invoked. Acceptance requires
all 34 baseline SDK cases and all 35 large-fixture cases, including resource
discovery continuation after a client reconnect. No source edit tool is exposed.

    target/debug/terlc run scripts/llm_native/scripts/Gate.terls -- "$PWD/target/debug/terlc" LNC-005 "$PWD/target/quality/llm-native-queries-new-run"

Query-suite build/test and host-preparation processes have explicit 600-second
bounds, separate from protocol interaction. SDK processes have a 300-second
overall bound. The independent peer retains its existing 120-second RPC and CLI
deadlines and 180-second raw-wire deadline. Existing LNC-004 per-suite limits
remain unchanged. These compilation bounds account for observed development
build costs; they do not qualify release latency or remove LNC-017 performance
debt. Timings, process failures, raw peer output, test manifests, progress and
artifact digests are retained in semantic-queries.json and query-progress.json.
Invalid inventories, incomplete/stale evidence and missing resource-reconnect
cases fail the gate. LNC-005 remains incomplete until a retained full run passes;
M1 also requires LNC-006 diagnostics.

Milestone selectors `M1` through `M8` and `all` are registered. Unfinished suites
exit unsuccessfully with `not_implemented` evidence. `all`
expands the full milestone inventory and cannot silently pass a supported
subset. Reports separate schema validation, authored harness tests, and future
runtime/compiler evidence.

The runner refuses existing output directories. Reports bind the compiler
binary, gate implementation, and independent schema input closure. Raw peer
and compiler output is retained for failed runs. Required cases cannot be
empty, missing, duplicated, skipped, or failed. Dependencies are pinned in
`peers/requirements.txt`; local package installation is never triggered by a
gate run, and the installed peers can run without network access.
