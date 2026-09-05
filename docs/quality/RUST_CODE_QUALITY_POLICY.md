# Rust Code Quality Policy

Terlan's Rust feedback loop is owned at the workspace root. Every workspace
member inherits package metadata, dependency versions, and lint levels from
the root `Cargo.toml`. Formatting and Clippy configuration live only in the
root `rustfmt.toml` and `clippy.toml`.

The unified gate is:

```bash
make rust-code-quality-preflight-check
```

It runs the workspace-policy validator, adversarial fixtures, and semantic
module-structure check before these five independently runnable checks:

```bash
make rust-format-check
make rust-locked-binary-check
make rust-clippy-check
make rustdoc-check
make rust-quality-check
```

Locked checking and Clippy run both the default workspace binaries and the
release feature matrix represented by `--all-features`. The adversarial gate
proves that formatting drift, an outdated lock file, Clippy diagnostics,
missing contract documentation, and Rust structural-quality violations each
fail closed.

Rustdoc is required for public APIs, unsafe boundaries, and internal items
preceded by a `rustdoc-contract:` marker. It intentionally does not require
formulaic comments on self-explanatory private helpers or test bodies.
The committed Rustdoc and inline-test debt baselines contain comments only.
`make rust-module-structure-check` rejects any debt row added to either
baseline, and the unified preflight invokes that gate directly. Tests belong
in adjacent, named test modules so production modules remain easy to scan;
compile-time assertions may remain next to the contract they prove.

Lint exceptions belong on the smallest affected item and must explain why the
code is intentionally inactive or why an ABI constraint requires the
exception. Generated libpq bindings inherit workspace policy; their generator
owns the one cross-platform integer-cast allowance and tests deterministic
regeneration. Broad Clippy-category allowances are forbidden.

Every remaining `#[allow(...)]` is recorded in
`RUST_LINT_ALLOWANCES.tsv`. A row identifies the exact source location and
lints, classifies the reason, distinguishes current `required-boundary` cases
from `structural-debt`, names the CQ owner, sets an expiry milestone, and
provides a concrete rationale. The accepted classifications are:

- `source-reuse`, `target-conditional`, `benchmark-surface`, and
  `staged-feature` for structural reachability debt owned by CQ-3;
- `generated-abi`, `generated-code`, `compatibility`, and `grammar-contract`
  for API or generation debt owned by CQ-4;
- `test-scaffold` for test-layout debt owned by CQ-5; and
- `unsafe-boundary` for a reviewed native boundary that CQ-3 must isolate and
  CQ-4 must make warning-clean.

`make rust-lint-allowance-check` rejects an unclassified source allowance,
stale or duplicate registry rows, unknown classifications, mismatched owners
or expiries, incorrect necessity status, and placeholder rationales. Moving an
allowance therefore requires re-review rather than silently carrying an
exception forward. The registry is transitional: even currently required
boundaries have an expiry, and CQ-4 and CQ-6 require the allowance count to
reach zero.

The root Clippy thresholds record current architectural shape debt rather than
making it invisible in individual modules. Later CQ slices are responsible for
reducing those centralized thresholds as responsibilities and large functions
are split.

## Continuous Improvement After CQ-6

Rust quality orchestration lives in `mk/code-quality.mk`; the root Makefile
includes that domain-owned fragment. Every target remains independently
runnable and roadmap/test-hierarchy discovery scans the fragment directly.

`make rust-dependency-impact-check` records and validates workspace dependency
fanout, source-domain size and coupling, incremental-edit ownership, and the
integration-test target budget. Direct-AOT integration coverage retains twelve
isolated integration-test binaries because those harnesses own distinct process
and environment state. The checked ceiling prevents accidental target growth,
while the workspace test profile uses line-table-only debug information to
reduce compile and link cost without weakening harness isolation or line-number
backtraces.

Internal `Result<_, String>` migration is governed by
`RUST_STRING_ERROR_BUDGETS.tsv`. Budgets are grouped by compiler, runtime,
native-boundary, command, benchmark, quality, and support ownership. The exact
path inventory may shrink, but neither rows nor sites may exceed an owner's
0.0.9 ceiling. Every nonzero ceiling also carries a strictly lower row or site
target for 0.0.9, so an unchanged inventory is not considered completion.

The hard Rust file limits remain 999 physical lines for implementation and
2,000 for tests. `make rust-file-headroom-check` adds an earlier warning-band
ratchet at 900 and 1,800 lines. Every file already inside that band has an
exact no-growth ceiling, structural owner, and 0.0.9 split milestone in
`RUST_FILE_HEADROOM.tsv`; a newly near-limit file fails instead of silently
settling just below the hard gate.

Stable AOT transition and failure contracts are owned by the dependency-inward
`terlan-runtime-abi` crate. The main crate re-exports those types at their
compatibility paths, but no longer owns duplicate definitions. The boundary
crate has no runtime implementation or third-party dependency fanout.

The high-volume lint style and formatter checks retain their exact-selector
inventories for drift auditing, while their normal gates execute canonical
module batches. Make aggregates express quality subgates as prerequisites, so
one top-level invocation traverses each shared prerequisite once.

Compile timings are diagnostic evidence. Release and code-quality correctness
never depend on CPU quietness, scaling-governor state, affinity, ambient load,
or a timing threshold measured in one run. Timing history can guide an
investigation, but it cannot reject a candidate.

Isolated Cargo build trees are temporary evidence-generation state. Timing
tools copy their compact HTML/report evidence under `target/quality` and remove
their isolated `target/cq0-timings` or `target/cq3-timings` trees in `finally`
cleanup, including after command failure. The build-artifact measurement cleans
all profiles it owns when any lane fails; after a successful measurement it
immediately runs the retention gate. That gate removes the now-recorded
`target/coverage` tree, escaped `cq0-timings`, `cq3-timings`, or `gnutsan`
trees, and test-owned `target/terlan-*` partial workspaces before the next
validation lane. It preserves compiled dependencies and outputs under
`target/debug`, the `target/release` tree, and `target/quality`; only shared
`target/debug/incremental` compiler state is pruned when it exceeds 64 GiB.

`make rust-dependency-impact-check` also records resolved workspace dependency
versions and optional-feature fanout, domain API fan-in, dependency cycles, and
the reverse transitive change blast radius. The source-domain graph must remain
acyclic. Checked ceilings for the main package's non-optional dependencies,
resolved duplicate-version families, and blast radius prevent an evidence
refresh from normalizing broader build or source coupling.
