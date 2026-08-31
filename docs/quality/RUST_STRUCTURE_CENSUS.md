# Rust Structure Census

This is the CQ-0 structural baseline for Terlan 0.0.7. It measures repository
shape and prevents existing structural debt from growing while CQ-1 through
CQ-6 remove it. It is not a target architecture and no baseline row is an
approval to add another instance.

The canonical machine report is
`docs/quality/RUST_STRUCTURE_CENSUS.json`. This JSON is the immutable CQ-0
baseline snapshot; ongoing enforcement is owned by the typed, modular Terlan
gates for file headroom, module composition, lint allowances, API boundaries,
canonical types, workspace policy, and dependency impact. This avoids scanning
and classifying every Rust source a second time in one validation cycle. The
checked timing procedure and
measurements are in `docs/quality/RUST_STRUCTURE_TIMINGS.json`. Every normal
gate run also writes the current report to
`target/quality/rust-structure-census.json`.

## Baseline

The 2026-07-28 baseline records:

| Measure | Count |
| --- | ---: |
| Rust files | 2,068 |
| Physical lines | 668,870 |
| Logical lines | 446,378 |
| Handwritten numbered fragments | 208 |
| Handwritten `include!` edges | 225 |
| Cross-tree `#[path]` edges | 83 |
| Near-limit files | 77 |
| Oversized files | 0 |
| Inline tests | 6,772 |
| Lint allowances | 280 |
| Crate/module-root allowances | 74 |
| Unsafe blocks | 24 |
| Unsafe blocks without a nearby safety comment | 1 |
| `Result<_, String>` boundaries | 3,248 |
| Type declarations | 2,445 |
| Equal-shape implementation type candidates | 115 |
| Cargo targets | 31 |
| Cargo test targets | 12 |

Counts are split in the JSON among implementation, test, generated, and
fixture source. Classification combines workspace ownership, source location,
source markers, attributes, and parsed Rust constructs; filename suffixes are
not used as the sole signal.

## Composition Classification

Every string-literal `include!` and `#[path]` edge has one classification:

- `handwritten-composition`: ordinary handwritten source composition;
- `generated-content`: a generated owner or generated destination;
- `fixture-content`: a test fixture or UI compile fixture;
- `approved-platform-boundary`: an edge directly guarded by an OS,
  architecture, Unix, or Windows configuration attribute.

Cross-tree means that a `#[path]` target traverses through `..` or leaves its
owning package. An adjacent named module does not count as cross-tree.

## No-Growth Policy

`make rust-structure-census-check` rejects:

- a new numbered fragment;
- a new handwritten `include!`;
- a new cross-tree `#[path]`;
- a new crate/module-root lint allowance;
- a new equal-shape type candidate;
- aggregate growth in those metrics, lint allowances,
  `Result<_, String>` boundaries, or unsafe blocks lacking safety comments;
- a new file above its category line limit;
- growth of an already over-limit file beyond its recorded ceiling;
- an unclassified composition edge;
- malformed or incomplete compile-timing evidence.

The CQ-0 census is immutable historical evidence and is not regenerated.
Current enforcement and intentional recording are owned by the typed modular
Rust-quality gates; do not rewrite the historical census to make a no-growth
failure pass.

## Compile Timing Procedure

The timing recorder uses `target/cq0-timings`, separate from the normal Cargo
target directory. It records a locked clean binary check, then parser,
type-model, runtime-leaf, and prewarmed LSP edits, followed by focused and full
test compilation. Source contents never change; the recorder restores the
original source timestamps after each edit simulation. Cargo HTML timing
artifacts are retained under `target/quality/rust-structure-timings` and their
SHA-256 hashes are checked into the timing JSON.

The initial wall-clock measurements were:

| Scenario | Milliseconds |
| --- | ---: |
| Clean binary check | 71,723 |
| Parser edit | 10,604 |
| Type-model edit | 9,377 |
| Runtime-leaf edit | 9,340 |
| LSP edit after feature prewarm | 6,074 |
| Focused integration-test compile | 140,370 |
| Full all-target test compile after focused-test warmup | 95,254 |

These values describe one x86_64 Linux machine with 24 logical CPUs. CQ-0
validates evidence shape and reproducibility, not a universal time threshold.
CQ-6 reruns the same procedure to compare the structural closeout.

Record a new reviewed timing matrix with:

```text
make rust-structure-census-record-timings
```

## CQ-6 Structural Closeout

The checked closeout reports are the immutable CQ completion artifact
`RUST_CODE_QUALITY_CLOSEOUT.json` and
`RUST_STRUCTURE_TIMINGS_CLOSEOUT.json`. The structural comparison uses the
immutable CQ-0 completion evidence in the code-quality roadmap; the working
no-growth census may shrink as reviewed debt is removed and therefore is not a
historical comparison source.

| Measure | CQ-0 | CQ-6 | Delta |
| --- | ---: | ---: | ---: |
| Rust files | 2,068 | 2,105 | +37 |
| Physical lines | 668,847 | 672,997 | +4,150 |
| Logical lines | 446,941 | 442,855 | -4,086 |
| Numbered fragments | 208 | 0 | -208 |
| Handwritten `include!` edges | 225 | 0 | -225 |
| Cross-tree `#[path]` edges | 83 | 0 | -83 |
| Crate/module-root allowances | 71 | 0 | -71 |
| `Result<_, String>` boundaries | 3,248 | 3,192 | -56 |
| Duplicate type candidates | 115 | 104 | -11 |

Physical lines and files increased because chronological fragments and inline
tests became semantic modules with explicit ownership, checked documentation,
and permanent adversarial gates. Logical implementation volume decreased, and
the source, module, build, lint, and canonical-type gates all pass.

The same 24-logical-CPU x86_64 GNU host, Rust 1.96.0 toolchain, isolated target,
commands, edit simulation, and sampling order produced:

| Scenario | CQ-0 ms | CQ-6 ms | Ratio |
| --- | ---: | ---: | ---: |
| Clean binary check | 71,723 | 36,780 | 0.5128 |
| Parser edit | 10,604 | 4,071 | 0.3839 |
| Type-model edit | 9,377 | 4,121 | 0.4395 |
| Runtime-leaf edit | 9,340 | 4,220 | 0.4518 |
| LSP edit | 6,074 | 4,234 | 0.6971 |
| Focused test compile | 140,370 | 68,702 | 0.4894 |
| Full test compile | 95,254 | 82,780 | 0.8690 |

One reviewed migration inventory remains, expiring at 0.0.9:

- 453 internal string-error rows covering 3,018 sites, divided among seven
  domain owners with checked no-growth ceilings and lower 0.0.9 targets.

There are no lint allowances, oversized files, undocumented unsafe blocks,
Rustdoc debt rows, inline-test debt rows, or file-size debt rows. The 30
canonical-type candidates are reviewed phase, platform, metadata, or
invariant distinctions; the AST audit reports zero exact normalized duplicate
types and zero parse failures.

## Post-CQ-6 Continuous Improvement

The statistically controlled recurring dedicated-host run after the CQ-6
improvements on 2026-07-29 used three samples per scenario and their medians.
Every median remains better than the CQ-6 reference:

| Scenario | CQ-6 ms | Current ms | Current/CQ-6 |
| --- | ---: | ---: | ---: |
| Clean binary check | 36,780 | 36,023 | 0.9794 |
| Parser edit | 4,071 | 4,035 | 0.9912 |
| Type-model edit | 4,121 | 4,056 | 0.9842 |
| Runtime-leaf edit | 4,220 | 4,005 | 0.9491 |
| LSP edit | 4,234 | 4,119 | 0.9728 |
| Focused test compile | 68,702 | 56,719 | 0.8256 |
| Full test compile | 82,780 | 69,150 | 0.8353 |

Direct-AOT coverage retains its twelve isolated integration-test binaries;
ABI-1 evidence adds one dedicated deterministic producer, for thirteen total
integration-test targets. Merging them would conflate process and environment
ownership across harnesses.
The workspace test profile instead emits line-table-only debug information,
which preserves line-number backtraces while reducing full-test compile time by
20.0% relative to CQ-6. The checked dependency-impact report records 24 source
domains, 29 cross-domain reference edges, zero workspace dependency cycles,
and a no-growth ceiling of thirteen integration-test targets.

All five reviewed duplicate-helper groups were extracted into canonical
quality and syntax support modules, leaving the duplicate-helper baseline
empty. The native-worker executable boundary now returns `BoundaryError`, and
the remaining string-error inventory has per-domain no-growth budgets.
