# Typecheck Calls Internals

This directory owns type inference for call expressions. The implementation is
centered on routing source calls through constructors, local functions, imports,
receiver methods, traits, and intrinsics. Its most important boundary is that
syntax routing and backend emission stay outside this module.

## Responsibilities

- Infer local, imported, receiver, and function-value calls.
- Validate named and defaulted arguments.
- Select overloads and report stable type diagnostics.

## Public Surface

- `local`: local function and constructor-adjacent call inference.
- `imported`: selected import call inference.
- `receiver`: receiver-method call inference.

## Core Model

Call inference starts with already-inferred argument types and resolves the call
head according to Terlan call precedence.

The main flow is:

1. Infer source argument types.
2. Route the callee through known call categories.
3. Return the selected return type or diagnostics.

Important invariants:

- Named/defaulted argument handling must be shared across call categories.
- Receiver calls must validate the receiver type before method arguments.
- Backend-specific ABI details must not enter type inference.

## Integration Points

- `terlan_syntax`: supplies call trees and argument names.
- `terlan_hir`: supplies function and imported interface metadata.
- `terlan_erlang`: later consumes validated call shapes for emission.

## Testing Notes

- Expression typecheck tests cover call routing, named args, defaults, and
  receiver methods.
