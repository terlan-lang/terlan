# Standard Library Doc LSP Hover Parity

This document is the contract for standard-library docs and editor hover parity
in 0.0.7.

Every public stdlib module, type, shape, trait, constructor, function, method,
field, constant, operator helper, and VM-owned primitive must have structured
documentation in the source of truth used by generated docs and editor hover.
This source of truth is structured documentation.

## Preserved Metadata

Documentation must preserve:

- formatting offsets
- examples
- parameter names
- return types
- error conditions
- mutability semantics
- target-profile availability
- capability requirements
- package provenance

The same metadata must surface through LSP hover and generated docs.

## Hover Stability

Hover output must be stable in text and JSON mode.
Hover output must link to the defining source symbol.
Hover output must reject stale docs generated from old stdlib snapshots.
Hover output must reject stale docs generated from package metadata.

## Adversarial Cases

The adversarial matrix must include:

- missing docs
- malformed doc comments
- outdated type signatures
- renamed parameters
- overloaded methods
- generated TypeScript/WASM/C++ bindings
- package imports
- private symbols
- cross-module re-exports

## Report Evidence

The executable gate persists std-doc-lsp-hover-parity-report.json with:

- public API inventory
- missing-doc entries
- generated-doc hashes
- hover fixtures
- source-definition links
- stale-metadata rejection results

## Release Decision

Release cannot pass if a public stdlib API is undocumented.
Release cannot pass if editor hover disagrees with generated docs.
Release cannot pass if editor hover disagrees with current type signatures.
Release cannot pass if editor hover disagrees with source-definition links.

The gate fails if docs formatting loses required spacing.
The gate fails if docs formatting loses offsets.
The gate fails if docs formatting loses examples.
The gate fails if docs formatting loses target/capability availability metadata.
