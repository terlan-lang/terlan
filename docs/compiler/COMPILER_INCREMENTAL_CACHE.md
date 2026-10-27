# Compiler Incremental Cache Correctness

This document is the compiler incremental cache correctness contract for
0.0.7. The compiler cache keys must cover lexing, parsing, formatting, name
resolution, typechecking, CoreIR construction, VM lowering, generated docs,
diagnostics, source maps, package manifests, target capabilities, and
stdlib/package hashes.

Incremental builds must produce byte-for-byte equivalent public artifacts and
diagnostics to clean builds for the same inputs, target, package graph, stdlib
hash, compiler version, and feature flags.

## Cache Keys

The required compiler cache keys are:

- lexing
- parsing
- formatting
- name resolution
- typechecking
- CoreIR construction
- lexical binding identities and their evidence fingerprint
- VM lowering
- generated docs
- diagnostics
- source maps
- package manifests
- target capabilities
- stdlib/package hashes

## Invalidation

Cache invalidation must cover source edits, import graph edits,
package/lockfile edits, stdlib changes, compiler version changes, target
profile changes, generated binding changes, and formatter/lint rule changes.
Unrelated declaration insertion must not renumber stable binding identities in
an unchanged callable; a lexical-path change must invalidate the affected
binding evidence and every downstream source-navigation or backend consumer.

The invalidation matrix explicitly includes target profile changes.

Cache entries must be isolated by workspace, package, target, compiler version,
and capability set. Cache evidence must avoid source-checkout and
host-local absolute path leakage.

## Adversarial Cases

The adversarial matrix must include stale parse trees, stale type errors, stale
generated docs, stale source maps, stale package metadata, changed imported
module, changed stdlib hash, concurrent incremental builds, cache corruption,
and clean-vs-incremental diagnostic drift.

The adversarial corpus explicitly includes stale generated docs and changed
imported module cases.

The dependency fixture explicitly covers changed imported module behavior.

## Report Evidence

The executable gate persists compiler-incremental-cache-report.json with:

- fixture matrix
- clean build hashes
- incremental build hashes
- invalidation cases
- cache hit/miss counts
- diagnostic parity
- source-map parity

The report is release evidence that incremental compilation remains equivalent
to a clean build for the same inputs while still allowing cache reuse.

## Checked application image reuse

Incremental native application builds may reuse an image after the frontend has
checked the complete source and dependency closure. This avoids repeating root
pruning and native lowering for an unchanged checked application. Both borrowed
and owned CoreIR paths use the same admission rules.

The reuse key covers each supplied Core module, its public interface and lowered
interface function signatures, executable roots, exact debug source identities
and callable spans, compiler revisions, build policy, target, adapter ABI, and
linker identity. Interface signatures must be hashed explicitly because CoreIR
serialization omits its HIR interface. Dependency-free source reuse also binds
the compiler revisions and native admission policy before skipping the frontend.

A bounded, replaceable index identifies an existing native cache entry. It is
not sufficient evidence by itself: reuse requires the existing object and image
manifest hashes, sealed-image validation, target, and application identity checks.
Missing, stale, malformed, or corrupted cache evidence falls back to compilation.
Missing published output is restored from verified cache bytes. Explicit native
lowering trace requests bypass this shortcut. Non-incremental builds continue
through the ordinary lowering and code-generation path.

## Compilation latency experiments

`scripts/benchmarks/compilation` provides a Terlan-owned benchmark for generated,
modular applications. It measures compiler latency separately from compiling the
Rust bootstrap compiler or running repository validation. Build the optimized
compiler first, then use the same immutable compiler and standard library for
both sides of a comparison:

```sh
cargo build --release --locked -p terlan --bin terlc --bin terlan-vm
target/release/terlc run scripts/benchmarks/compilation/scripts/Suite.terls -- \
  "$PWD/target/release/terlc" \
  "$PWD/target/quality/compilation-run-1" 128 64 3
```

The last three arguments select leaf modules, functions per leaf, and an odd
number of repetitions (1–31). The output directory must be new; existing results
are never intentionally overwritten. Use `2 2 1` for an end-to-end smoke run.
The suite alternates scalar call graphs with record/generic call graphs. It
writes a `summary.json` containing medians and retains each sample's raw report,
command arguments, exit statuses, output, and native build-activity log.

Every sample measures:

- A build with a fresh compiler cache and output directory.
- An unchanged rebuild, requiring identical executable bytes and no native work.
- A shared implementation edit, checked by executing the changed application.
- A compatible public-interface addition.
- Cold and warm directory typechecks.

Each edited build must match an independent fresh build byte for byte. A
subsequent incompatible return-type edit must produce identical failing clean
and incremental diagnostics; repairing it must restore successful checking.
Successful samples remove their generated build and cache outputs while
retaining sources and evidence. Failed samples retain artifacts for diagnosis.

Reports bind results to the generated project (including its manifest), compiler,
and standard-library content hashes and the
native backend, target, ABI, and linker identities. Host snapshots record CPU,
affinity, governor, memory, and load, or explicitly record unavailable metrics.
"Cold" describes the compiler cache; the harness does not flush OS caches.
Compare repeated samples on a quiet host with the same compilation options.
Timings from a busy development machine are exploratory, not release guarantees.

To isolate the checked-application reuse shortcut using one compiler build:

```sh
target/release/terlc run scripts/benchmarks/compilation/scripts/Compare.terls -- \
  "$PWD/target/release/terlc" \
  "$PWD/target/quality/compilation-comparison-1" 128 64 3
```

This runs both workloads in paired control and reuse modes, reversing mode order
on alternate repetitions. The control removes only `native-aot/core-reuse-*.json`
lookup indices from its newly created, benchmark-owned cache before each build,
outside the timed command. It keeps frontend entries, native objects, sealed
images, and their validation manifests. An unchanged control build must remove
at least one index, so a missing cache path cannot silently invalidate the
experiment. Both modes retain all runtime, artifact, and diagnostic parity
checks. Each raw report records its mode and the number of indices removed.
The `control/summary.json` and `reuse/summary.json` files contain the medians.
This estimates the shortcut's contribution with current compiler code; it is
not a historical comparison between different compiler releases.

These workloads model repetitive modular code, including one form of generic
record usage. They do not establish performance for arbitrary LLM-generated
applications. Large-project speed is a hypothesis to measure alongside richer
source shapes, memory use, diagnostics, and cache-correctness tests. This suite
supplements the adversarial cache gate above; it does not replace it or impose
wall-clock thresholds on ordinary precommit runs.

The harness has focused ownership, evidence, and statistics regression tests:

```sh
target/release/terlc test \
  scripts/benchmarks/compilation/src/compilation_benchmark/RunnerTest.terl \
  scripts/benchmarks/compilation/src/compilation_benchmark/SuiteTest.terl
```

### Measured checked-image reuse: October 27, 2026

The [retained comparison](../../scripts/benchmarks/compilation/results/checked-reuse-20261027.json)
contains all twelve raw sample reports, tool hashes, host snapshots, and the
calculated medians. Each workload has 128 generated leaf modules with 64
functions each (8,192 leaf functions, plus application helpers). The optimized
0.10.0 candidate compiler used eight native build jobs on an Intel i9-12950HX
Linux workstation. Each mode/workload pair has three observations.

Median wall times, in seconds:

| Workload | Phase | Control | Checked-image reuse |
| --- | --- | ---: | ---: |
| Scalar | Fresh build | 13.199 | 13.265 |
| Scalar | Unchanged rebuild | 11.116 | 3.359 |
| Scalar | Implementation edit | 11.906 | 12.111 |
| Scalar | Public-interface edit | 12.020 | 12.240 |
| Scalar | Cold typecheck | 12.130 | 12.200 |
| Scalar | Warm typecheck | 0.830 | 0.834 |
| Records/generics | Fresh build | 13.913 | 13.904 |
| Records/generics | Unchanged rebuild | 11.760 | 3.380 |
| Records/generics | Implementation edit | 12.675 | 12.725 |
| Records/generics | Public-interface edit | 12.793 | 12.956 |
| Records/generics | Cold typecheck | 12.493 | 12.506 |
| Records/generics | Warm typecheck | 0.842 | 0.842 |

The unchanged rebuild takes about 70% less time for scalar code and 71% less for
records/generics. Both modes already avoid native object compilation on an
unchanged build; the additional saving comes from reusing the verified image
before repeating application pruning and native lowering. Frontend checks and
cache admission remain enforced. Edited inputs invalidate this shortcut; these
measurements do not show an edit-build improvement.

All twelve samples passed runtime output checks, byte-for-byte comparison with
fresh builds after each edit, identical clean/incremental rejection diagnostics,
and successful checking after repair. Compiler and native-worker hashes stayed
unchanged. Compiler, standard-library, and native-toolchain identities agree
across samples, and generated source identities agree within each workload.
Every completed sample removed its build/cache outputs.

These are synthetic workloads on one development workstation, with some other
repository validation during the run. Alternating order reduces ordering bias
but does not establish statistical confidence or performance for arbitrary
LLM-generated programs. Interrupted earlier measurements are excluded.

The optimized executable-size precommit validator separately completed in
**1.50 seconds**, including its compilation and execution; the
[validation observation](../../scripts/benchmarks/compilation/results/validation-overhead-20261027.json)
records its command and input hashes. This is one validator observation, not
the cost of the full precommit suite. The hook now selects the already-built
optimized compiler before invoking this validator.

### Rust bootstrap measurements

Rust bootstrap and repository validation costs are separate from the Terlan
application timings above. The retained canonical measurement in
`scripts/benchmarks/compilation/results/rust-bootstrap-20261024.json` records
these historical observations from October 24, 2026:

| Rust bootstrap lane | Wall time |
| --- | ---: |
| Clean debug build | 176.602 s |
| Unchanged debug build | 0.441 s |
| Implementation edit | 19.929 s |
| Public-interface edit | 22.955 s |
| Optimized build | 278.147 s |
| Coverage test compilation, without executing tests | 330.492 s |

These are single observations of the source and profile fingerprints recorded
in that report, not current-version medians or Terlan application timings. The
debug lanes used normal Cargo incremental behavior. Validation runs explicitly
configured with `CARGO_INCREMENTAL=0` are not comparable to those edit timings.
The report includes exact commands, toolchain identity, artifact counts, cache
sizes, and the source digest; it is retained as historical evidence, not reused
as approval for a newer candidate. A fresh canonical measurement is available
through `BuildArtifactBudgetTest.terl --measure`; it performs clean builds and
requires its own disk and time budget.
