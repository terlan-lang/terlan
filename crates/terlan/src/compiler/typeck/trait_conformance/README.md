# Trait Conformance Checks

## Purpose

This directory contains focused helpers for source-level trait conformance validation.
The parent `trait_conformance.rs` module owns orchestration; files here own one
validation concern at a time so diagnostics stay easy to audit.

## Inputs

- Parsed syntax output from `terlan_syntax`.
- Resolved module metadata from the typechecker.
- Parsed trait signatures and inherited trait method summaries.

## Outputs

- Stable `Diagnostic` values for malformed trait declarations, explicit impls,
  declaration-site `implements`, receiver methods, and related conformance
  obligations.

## Transformation

- Convert source-level trait shapes into normalized comparable signatures.
- Substitute trait type parameters with conformance arguments.
- Compare method arity, mutability, parameter types, return types, and coverage.

## Files

- `impls.rs`: validates explicit `impl Trait[...] for Type` blocks against
  inherited trait method requirements.
- `syntax.rs`: validates syntax-level kind diagnostics, macro return
  signatures, and public constructor return-type visibility.
