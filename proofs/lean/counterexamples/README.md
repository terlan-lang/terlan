# Lean proof counterexamples

This directory is the proof-to-regression boundary. `proof-obligations.json`
classifies failed or stale theorem obligations independently of their
counterexamples. Each record in `cases/` contains the minimal Terlan AST and
the expected negative runtime oracle. The counterexample gate deterministically
converts those records into the committed `fixtures/terlc/` and `fixtures/vm/`
forms, rejects conversion drift, and executes the `terlc` fixture.

`regression_guarded` means the proof obligation has produced a runnable,
continuously checked negative fixture. It does not mean the theorem itself has
been proved. `unresolved` records remain in the triage backlog and are governed
by `docs/runtime/lean-proof-counterexample-policy.toml`.
