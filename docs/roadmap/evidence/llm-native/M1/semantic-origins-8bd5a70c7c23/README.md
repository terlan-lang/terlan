# Semantic origins and resource continuation checkpoint

This is partial LNC-005 evidence, not LNC-005 or M1 completion.
[Checkpoint report](checkpoint.json) SHA-256:
`1151115d1a9777d16cb18b386d6ce9897bee9dc3b8e5c2ad167d51838b450bd9`.

The Terlan service now selects pattern nodes with the catalog's exact identity
factory and a bounded syntax walk. Pattern Core views identify their enclosing
function through checked compiler origins. Type views expose checked Core type
declarations, copied local field metadata and exact included-parent entities.
Imported include aliases use resolver-exported bindings and provider units in
the same immutable snapshot. Missing provider source retains alias evidence and
an explicit partial result. Generated receiver-default adapters retain the
original source module, function, arity and declaration span.

Retained authored evidence, all passing with compiled-source fingerprints checked:

- [Query tests](query-tests.json): 16 cases, including nested patterns, stale
  handles, work/byte limits, overloads, alias types and local/imported includes.
- [Catalog tests](catalog-tests.json): 6 cases for ownership, origin/identity
  stability and invalid source.
- [Declaration tests](declaration-tests.json): 5 cases, including explicit
  rejection of duplicate checked declaration rows.
- [Generated origins](generated-tests.json): one default-adapter case.
- [Protocol tests](protocol-tests.json): 7 lifecycle, identity and error cases.
- [Harness tests](harness-tests.json): 13 evidence/failure-injection cases.
- [Bootstrap export tests](export-tests.log): 6 focused Rust cases.

The [independent live MCP report](mcp-peer.json) passes 21 cases on the retained
[fixture](fixture.terl), which adds 1,500 constant functions. The service exposes
1,514 entities through 21 bounded resources, discovered over two pages. The
continuation survives a new server/client connection and returns the same next
page. The client also verifies query schemas, real types/origins, explicit
unknowns, rejection paths, and CLI/MCP parity. Raw CRLF stdio probes send 13
requests/notifications/responses and receive exactly nine replies, then clean
EOF. Full-width IDs remain exact; notifications and anonymous peer errors do
not create response loops.

Anonymous malformed-input errors omit an unreadable ID, while errors preserve a
readable ID. These replies are validated against the unmodified official MCP
2025-11-25 schema; SDK 1.26.0 requires an ID in its error model. The schema,
license and revision provenance are included in the frozen source archive.
The [contract gate](contract-gate/report.json) passes 46 semantic schema cases,
10 SDK model cases and four official-schema cases.

The large fixture exposed a quadratic duplicate scan and unnecessary pattern
owner hashing. A set replaces duplicate rescans, and owner identities are now
computed only when patterns exist. VM continuation limits and SDK request
timeouts were retained. The failed resume-budget, startup-timeout and anonymous
error runs are preserved alongside the passing run. The [progress log](mcp-peer.progress.jsonl)
retains elapsed observations: the complete live case run takes about 323 seconds
and initial development startup takes about 115 seconds. These are correctness
observations with a development launcher/compiler cache, not release latency
qualification. Incremental reuse and scheduling remain later roadmap work.

[Sources](sources.json) identify 104 declared component/gate inputs with digest
`8bd5a70c7c2338f265f8f596151b7865bf4e46aaba4082ce1701a802cfd5de3a`.
[Archive](sources.tar.gz) SHA-256:
`968f2b313738487f4627cfebc65ff5c434a4aac4b13ecce5b8defa8d8a454d92`.
Compiler binary SHA-256:
`aba5d5189b0e9b64ce8b9c3cad334b0a0c9c752f8949199854fb3a931ba493c2`.
The archive is not the complete compiler build closure. [Checksums](checksums.json)
bind raw artifacts, the snapshot store and frozen sources; this explanatory file
is outside that manifest.

To repeat the large-client checks with the matching development toolchain and
the pinned independent peers, use fresh output paths:

```sh
repeat_root=$(mktemp -d)
cp -a docs/roadmap/evidence/llm-native/M1/semantic-origins-8bd5a70c7c23/snapshot-store "$repeat_root/store"
/tmp/terlan-llm-native-tools/bin/python scripts/llm_native/peers/mcp_peer.py \
  --root "$PWD" --compiler "$PWD/target/debug/terlc" \
  --store "$repeat_root/store" \
  --snapshot s:633c2517a4cd64ef88f480bd0a9bdfd2edd86dbddafba2a0d2dfc5bc98915438 \
  --source-fixture "$PWD/docs/roadmap/evidence/llm-native/M1/semantic-origins-8bd5a70c7c23/fixture.terl" \
  --require-resource-pagination --cache "$repeat_root/cache" \
  --output "$repeat_root/report.json" --stderr "$repeat_root/report.stderr"
```

Compiler, source or service changes produce different evidence identities.
Pattern exhaustiveness, fuller effect facts, shape/implication relation queries,
additional alias/origin cases and the combined M1 acceptance gate remain open.
