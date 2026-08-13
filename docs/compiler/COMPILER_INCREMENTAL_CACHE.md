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
