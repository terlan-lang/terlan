# Deferred proof review: 2026-09-25

Compiler CI for candidate `ea5b1d06a892b1bab693f068f6d3aaa5b87135c2`
failed because six blocked obligations had last been reviewed on August 19,
37 days before the run. This review refreshes the description of their actual
proof boundaries, not their completion status.

The reviewed sources remain those inventoried in `lean_proof_inventory.tsv`
and `proofs/lean/ci/lean-proof-artifacts.tsv` at that candidate. No theorem,
proof-source digest, exception approver, deadline, or coverage requirement changes.
The six existing exceptions still expire on **2026-09-30**. All six obligations
remain **blocked**; passing this metadata review does not prove them.

| Obligation | Source inspected | Remaining obligation |
| --- | --- | --- |
| Typed CoreIR preservation | `Terlan/Core/Arithmetic.lean`, `Terlan/Core/CheckedLowering.lean`, `CORE_IR_LEAN_CONFORMANCE.md` | The expression models cover integers, addition, and a spawn seed. They do not cover the full production CoreIR or prove general lowering preservation. |
| Target-profile inference | `Terlan/Core/CheckedLowering.lean`, `TERLAN_TARGET_INFERENCE.md` | Admission theorems discriminate the spawn seed; they do not model the full production target-inference algorithm. |
| VM execution subset | `Terlan/Smoke/SemanticChain.lean`, `concurrency/Concurrency.lean` | Integer evaluation, list-mailbox ordering, and queue progress do not establish refinement of the AOT scheduler, heaps, or continuations. |
| Pattern and operator coverage | `Terlan/Type/ShapeImplication.lean`, the operator and pattern support matrices | Closed-shape evidence theorems do not prove arbitrary pattern matching or operator evaluation. Executable coverage is not a substitute for those proofs. |
| Wasm CoreIR lowering | `wasm_bridge/WasmBridge.lean`, `crates/terlan/src/backends/wasm/README.md` | Signature/type mappings, resource-owner transfer, and rejection witnesses do not prove full CoreIR-to-Wasm lowering. |
| Aeneas Rust verification bridge | The full proof/artifact inventories, `native_boundary/NativeBoundary.lean` | Runtime oracles exercise implementations, but no inventoried Aeneas artifact refines the production Rust implementation to the Lean models. |

Lean paths above are relative to `proofs/lean`; compiler documents are relative
to `docs/compiler`. The review appends new evidence to the transition ledger,
preserving all earlier records. The paired TOML and TSV records continue to
bind each reason and review date by SHA-256. The TTL, exception expiry, skipped
lifecycle-state rejection, and closure-artifact requirements remain enforced.

This is a source-scope audit. Separate reproducibility and runtime gates must
still execute; neither a successful review nor a successful runtime test is a
claim of full formal verification.

## Reproducibility metadata review

The full track also identified three stale input fingerprints. Comparing their
source changes since `c760937e` found:

- The language-feature matrix adds the Unit ordering witness, updates a lambda
  source location, and names current trait-dispatch tests. The collections
  contracts and theorems are unchanged.
- Native dispatch re-exports the existing typed tool-command error.
- Native value representations add recursively owned tuples. The abstract
  handle-generation, ownership, arity and capability theorems are unchanged;
  they do not become a proof of production tuple encoding.

The two replay records now fingerprint those current source files. All 13
current proof families passed both isolated replicas (26 executions) under
the pinned Lean toolchain. The four named NativeBoundary runtime oracles and
the tuple ownership/term-bounds regression also passed using the current Rust
test harness. These checks complement, but do not close, the missing Aeneas
refinement obligation.

After that replay, the parser closure note was synchronized with the existing
artifact inventory and actual `ParserShape.lean` SHA-256
`d5ef45a6ad1f4c5c40641f947a4e37aad5f4e573fd411e71ab0f443d17b31b18`.
Its earlier closure history remains untouched.
