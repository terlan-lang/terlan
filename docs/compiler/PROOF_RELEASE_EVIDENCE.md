# Proof Release Evidence

Terlan uses one local, content-addressed proof evidence manifest. It
links every proof-current roadmap slice to its Lean families, theorem IDs,
compiler inputs, runtime lane, standard-library test lane, and candidate ID.

`make release-artifacts-closeout-check` reproduces Lean proofs and runtime
oracles, validates adversarial mutations, rebuilds the canonical manifest, and
compares its normalized semantic digest with the checked baseline. Absolute
checkout paths, wall-clock timestamps, and elapsed-time history are not part of
the identity. The repository therefore does not maintain a dashboard or a
rolling history for this evidence.

`make proof-readiness-release-mode-check` consumes that same candidate and
seals the fixed local/CI command signature, locked feature set, and stage order.
Both commands are local: they do not publish, upload, tag, push, or require an
external account.

The minimal replay corpus is the family list in
`proofs/lean/release_evidence/release-mode.json`. Existing per-family replay
metadata remains the source of truth; the release evidence layer does not copy
or reinterpret Lean semantics.

Generated reports live under ignored `target/quality/proof-artifacts`. Historical
reports checked into `build/artifacts` are neither inputs to current validation
nor outputs of preparation. The accepted policy baseline remains a reviewed
source file at `proofs/lean/release_evidence/baseline.json`.

Propose an intentional baseline change without modifying that source file:

```bash
make terlan-self-validation-bootstrap
TERLAN_PROOF_RELEASE_ROOT="$PWD" \
  target/debug/terlan-vm run \
  target/self-validation/proof-release-evidence/vm/scripts_ProofReleaseEvidence.tvm \
  --script-eval -- propose-baseline
```

Review `target/quality/proof-artifacts/proof-release-baseline-proposal.json`
against the accepted baseline and inspect the slice changes reported in
`proof-release-evidence-diff.json`. Apply only explained changes after the
affected proofs and runtime oracles pass, then rerun closeout. The explicit
`record-baseline` maintenance command remains available for an already reviewed
change; it must not be part of automatic preparation or publication.

The normal closeout gate never updates the baseline. Missing proofs, stale
inputs, duplicate events, mismatched lanes, malformed schemas, candidate drift,
or nondeterministic ordering produce a concise JSON diagnostic and fail.
