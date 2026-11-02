# Checked call references and provider origins component checkpoint

This records progress on **LNC-005**, not completion of that task or M1.
The roadmap remains at four completed tasks out of 27.

The authoritative checker returns the selected scheme or interface signature from
its existing overload decision. Compatibility wrappers retain their original type
result for candidate probes. Selected-import trials keep candidate metadata private
until the checker accepts one provider. Local schemes link to their registered
source declaration spans; call-site paths come from the existing lexical traversal
on the same prepared syntax. No separate overload checker or name/arity fallback
was added to the semantic service.

Terlan `CallOrigins` requires an exact source declaration origin or agreement
between the complete accepted interface signature, the provider's checked contract,
and one exact source signature. `CallReferences` provides function callers within
the frozen snapshot and a module call inventory with target entity links.
Function aliases, module aliases and overloads retain provider identity among
same-named decoys. Unknown indirect/unsupported targets have no claimed target;
conflicting visits remain unknown. A truncated source capture contributes no
claimed references. Providers outside the snapshot retain canonical signatures
without fabricated entity links. Ranges cover enclosing syntax, not identifier
tokens. Reference results stay partial; empty results do not prove absence.

Passing evidence:

- Six authored call-reference tests, including alias/provider decoys, exact local
  overloads, indirect shadowed calls, providers outside the snapshot, pagination
  with every site unique, and incomplete capture.
- 23 existing query tests, 12 relation tests, six binding-type selection tests,
  one generated-origin test and 13 gate harness tests.
- All 33 current snapshot cases across six suites and independent live source
  and schema evidence.
- 31 independent standard MCP SDK/raw-wire checks, including selected-overload
  references, explicit indirect targets, existing inferred types and CLI parity.
- 70 semantic contract cases, ten SDK model cases and four official schema cases.
- Two actual calls sharing one imported name across distinct providers,
  independently checked against the accepted signatures and public schema.
- 24 exporter, four observation-capture, 19 import and 29 call/inference Rust tests.
  These filtered Rust inventories overlap and are not additive.

The source inventory binds 153 declared component inputs. The source archive is
not a verified complete reproducible compiler build closure. Compiler and VM
identities in `checkpoint.json` are trusted binary anchors, including existing
unrelated workspace work. Authored Terlan manifests are checked against exact
current test membership and every compiled-source SHA-256 in their callable
inventories. Raw reports, source archive, snapshot store and failed explorations
are retained. `checksums.json` covers retained raw files and the checkpoint;
this README is outside that inventory.

Explorations distinguish helper import/visibility compile failures, incorrect
import/module-path fixtures, a helper local-name lowering failure, and a compound
condition lowering failure. The test helper now reads its pure fields before its
Boolean condition, without changing test scope or assertions. The first executed
six-case suite passed five cases and exposed a real provider join bug: a JSON
wrapper comparison needed decoded Boolean values. The corrected suite passes all
six cases. A schema case's expected nested diagnostic was corrected from `type`
to `oneOf`; its rejection constraint was unchanged. The initial MCP run passed 29 checks before cold CLI parity compilation exceeded
the unchanged 120-second deadline. Its retry uses the same deadline after explicit
host preparation in the same cache. None of these failed runs counts as passing
evidence.

Reproduce with a trusted matching compiler/runtime and identical declared source
inputs, using fresh cache and output paths:

```sh
export TERLAN_SEMANTIC_TEST_COMPILER=/path/to/terlc
export TERLAN_LLM_NATIVE_TEST_COMPILER="$TERLAN_SEMANTIC_TEST_COMPILER"
export TERLAN_LLM_NATIVE_PEER_PYTHON=/path/to/pinned-peer-environment/bin/python
"$TERLAN_SEMANTIC_TEST_COMPILER" test compiler/semantic/src/semantic/CallReferencesTest.terl --cache-dir /tmp/replay-call-cache --emit-test-result-manifest /tmp/replay-call-tests.json
"$TERLAN_SEMANTIC_TEST_COMPILER" test compiler/semantic/src/semantic/QueryTest.terl --cache-dir /tmp/replay-query-cache --emit-test-result-manifest /tmp/replay-query-tests.json
"$TERLAN_SEMANTIC_TEST_COMPILER" test compiler/semantic/src/semantic/RelationsQueryTest.terl --cache-dir /tmp/replay-relation-cache --emit-test-result-manifest /tmp/replay-relation-tests.json
"$TERLAN_SEMANTIC_TEST_COMPILER" test scripts/llm_native/src/llm_native/GateTest.terl --cache-dir /tmp/replay-harness-cache --emit-test-result-manifest /tmp/replay-harness-tests.json
"$TERLAN_SEMANTIC_TEST_COMPILER" run scripts/llm_native/scripts/Gate.terls -- "$TERLAN_SEMANTIC_TEST_COMPILER" LNC-001 /tmp/replay-contract-gate
"$TERLAN_SEMANTIC_TEST_COMPILER" run scripts/llm_native/scripts/Gate.terls -- "$TERLAN_SEMANTIC_TEST_COMPILER" LNC-004 /tmp/replay-snapshot-gate
```

Capture `tests/fixtures/llm_native/mcp/query.terl` with the snapshot script and pass
its snapshot/store to `scripts/llm_native/peers/mcp_peer.py` with `--root`,
`--compiler`, `--store`, `--snapshot`, `--cache`, `--output` and `--stderr`.
The run retains host build logs separately; the SDK initialization deadline stays
120 seconds. This is not release latency qualification. This MCP resource fixture
is single-page; prior large-fixture pagination/reconnect evidence is historical.
General function-value flow, an incremental call graph, the combined LNC-005
acceptance gate and subsequent roadmap milestones remain open.
