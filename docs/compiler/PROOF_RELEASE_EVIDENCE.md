# Proof Release Validation

Terlan validates proofs by executing the current theorem sources against the
current release candidate. Stored evidence hashes, dated snapshots, aggregate
checksums, and a second release-evidence composer are not release inputs.

`make release-proof-evidence-check` runs the direct proof replay and proof-gap
hygiene checks before expensive compilation. `make
release-artifacts-closeout-check` depends on the same shared Make node, so it
does not repeat completed proof work.

Each current proof family records its path, theorem scope, source manifests,
execution command, expected process result, and toolchain. The runner executes
each family twice in isolated workspaces and compares normalized semantic
outcomes. Runtime and negative counterexample checks remain executable gates.

Generated reports live under ignored `target/quality/proof-artifacts`. They are
diagnostics only: they are not checked into source control and do not need a
manual metadata update after a valid source change.
