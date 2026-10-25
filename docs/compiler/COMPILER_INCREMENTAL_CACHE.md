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
