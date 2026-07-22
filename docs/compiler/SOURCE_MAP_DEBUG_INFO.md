# Terlan Source Map Debug Info

This document is the source-map/debug-info contract for 0.0.7. Terlan source
spans must stay attached to user-visible program identity as compilation and
execution move through parser, typechecker, CoreIR, VM artifacts, generated
docs, package artifacts, diagnostics, support bundles, debugger commands, and
editor/LSP output.

The gate treats Terlan source spans as stable evidence, not formatting prose.

## Required Identity

Every emitted diagnostic, runtime failure, debugger event, and editor lookup
must preserve:

- Terlan module/function/source spans
- file path normalization
- module identity
- function identity
- line/column offsets

VM runtime errors, test failures, panic-like internal failures, package
resolution failures, template failures, HTTP handler failures, and
NativeBoundary failures must map back to Terlan module/function/source spans
where source is available.

The failure matrix explicitly includes package resolution failures.

## Artifact Boundaries

Source maps must survive package builds, workspace builds, incremental
rebuilds, installed release artifacts, generated bindings, and support-bundle
redaction. The support-bundle redaction path must remove host-local absolute
paths while retaining enough normalized module and source identity for
diagnostics, editor navigation, hover diagnostics, debugger breakpoints, stack
traces, and support bundles to agree.

The redaction rule must explicitly cover host-local absolute paths.
Debugger and runtime evidence must explicitly preserve stack traces.

The contract applies equally to:

- package builds
- workspace builds
- incremental rebuilds
- installed release artifacts
- generated bindings
- generated docs
- package artifacts
- VM artifacts
- editor/LSP output

## Adversarial Cases

The gate must cover stale source maps, generated file span drift, package
artifact relocation, redacted support bundles, missing package sources, invalid
UTF-8/source offsets, line-ending differences, and runtime errors without
source-linked diagnostics.

Adversarial fixtures must include package artifact relocation,
invalid UTF-8/source offsets, and runtime errors without source-linked
diagnostics.

The failure corpus must contain runtime errors without source-linked diagnostics.

## Report Evidence

The executable gate persists source-map-debug-info-report.json with:

- fixture artifacts
- span roundtrips
- stack trace mappings
- package relocation cases
- editor/LSP parity snapshots
- support-bundle redaction checks

These report categories are release evidence that the source-map/debug-info
contract remains explicit while the implementation continues moving from legacy
backend artifacts toward VM-owned artifacts.
