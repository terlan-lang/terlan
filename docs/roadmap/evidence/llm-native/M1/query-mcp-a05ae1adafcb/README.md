# Query and live MCP component checkpoint

This checkpoint makes concrete progress on LNC-005. It does **not** close
LNC-005 or M1. The [checkpoint report](checkpoint.json) records its limits and
verified case membership. Its SHA-256 is
`aceb0daf25dea9c094cd1d65ffd52a5e32282d236dce18d63abda593f06a5dbf`.

The [independent MCP SDK report](mcp-peer.json) records 17 passing live checks
against `terlc semantic mcp`, implemented by Terlan `std/mcp` and the semantic
package. The client negotiates MCP 2025-11-25, discovers `terlan_query`, validates
its self-contained input/output schemas, reads bounded snapshot/entity resources,
queries actual checked types and Boolean inhabitants, distinguishes same-arity
overloads, observes explicit unknown ownership, and rejects stale handles,
out-of-scope snapshots, malformed budgets, unknown tools, forged continuations
and filesystem resource URIs. `terlc semantic query` returns the same document.

Additional retained evidence:

- [Query tests](query-tests.json): 9 passing authored Terlan tests, including
  shadowing, exact work/byte accounting, reference continuation, cursor scope and
  storage corruption. All 19 compiled source fingerprints match retained inputs
  or unchanged repository dependencies at capture.
- [Protocol tests](protocol-tests.json): 6 passing Terlan tests covering lifecycle,
  version negotiation, errors, notifications, duplicate IDs and full-width IDs.
- [Harness tests](harness-tests.json): 13 passing Terlan tests with the required
  pinned peer environment.
- [Contract gate](contract-gate/report.json): 46 schema cases and 10 independent
  SDK wire/model cases pass after adding expected-value queries and typed query
  errors. Model fixtures remain distinct from live interoperability checks.
- Focused Rust logs: 5 bounded-console/frame tests, 1 tail-transition regression,
  9 formatter tests and 21 existing launcher regressions pass. These support
  generic I/O and compiler fixes; no Rust MCP implementation was introduced.
- [Interactive console probe](console-probe.json): two responses arrive while
  stdin remains open, including CRLF input, followed by clean EOF.

The compiler binary digest for the live MCP/contract run is
`a2b9ac29430f68e5a73cc65d54a9e2aa2d07adb3d982f55d3357cf2a58d41ce2`.
The [declared source inventory](sources.json) binds 95 component/gate input files
to `a05ae1adafcb13df58edfcaa2466cfb3ac0b129221f62e8edf2842c2f5575f52`.
The [frozen archive](sources.tar.gz) SHA-256 is
`09e6ac6e387fc78b98926e1b396fe27e74b6e8db3c676b2b87dde7c758b858e7`.
This archive is not the complete source closure for rebuilding the compiler.
[Checksums](checksums.json) bind raw reports, logs, the immutable snapshot store
and archived inputs. This explanatory file is outside that frozen manifest.

The server currently admits one explicit snapshot, materializes a bounded
catalog before serving, and processes requests serially. The live fixture has
one entity page: its successful resource reads and forged-cursor rejection do
not prove multi-page resource continuation. Query reference pagination has
separate authored evidence. Unsupported semantic facts remain explicit, and
M1's imported/included/generated origin requirements remain incomplete.

Exploratory failures are retained. The first client run exposed a formatter
rewrite that changed overlapping branch priority; a later run exposed an
incorrect expected error code in the peer. The initial harness invocation
omitted its required peer environment and failed two tests. Corrected reruns
pass, with new compiler/formatter regressions retained.

To exercise the current implementation with the pinned test-only peers:

```sh
cargo build --locked -p terlan --bin terlc --bin terlan-vm
target/debug/terlc run compiler/semantic/scripts/Snapshot.terls -- \
  "$PWD/target/debug/terlc" "$PWD/tests/fixtures/llm_native/mcp/query.terl" \
  query.terl /tmp/terlan-mcp-new-store
# Use the returned snapshot ID below, with fresh output paths.
/tmp/terlan-llm-native-tools/bin/python scripts/llm_native/peers/mcp_peer.py \
  --root "$PWD" --compiler "$PWD/target/debug/terlc" \
  --store /tmp/terlan-mcp-new-store --snapshot s:RETURNED_DIGEST \
  --cache /tmp/terlan-mcp-new-cache --output /tmp/terlan-mcp-new-report.json \
  --stderr /tmp/terlan-mcp-new-report.stderr
```

Different compiler/source bytes produce new snapshot and evidence identities.
The saved snapshot store and source inventory identify this historical run.
