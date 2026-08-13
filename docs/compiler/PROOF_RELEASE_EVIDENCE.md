# Proof Release Evidence

Terlan 0.0.7 uses one local, content-addressed proof evidence manifest. It
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

Intentional semantic changes are reviewed by running:

```bash
make terlan-self-validation-bootstrap
TERLAN_PROOF_RELEASE_ROOT="$PWD" \
  target/debug/terlan-vm run \
  target/self-validation/proof-release-evidence/vm/scripts_ProofReleaseEvidence.tvm \
  --script-eval -- record-baseline
```

The normal closeout gate never updates the baseline. Missing proofs, stale
inputs, duplicate events, mismatched lanes, malformed schemas, candidate drift,
or nondeterministic ordering produce a concise JSON diagnostic and fail.
