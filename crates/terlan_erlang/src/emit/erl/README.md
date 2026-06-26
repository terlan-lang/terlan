# Terlan Erlang Type Render Internals

This directory owns submodules for the Erlang render model.

## Responsibilities

- Keep Erlang type-expression rendering separate from expression and pattern rendering.
- Preserve deterministic output for generated Erlang specs.
- Keep helper visibility scoped to the Erlang emit module.

## Public Surface

- `types`: `ErlType` and `ErlMapTypeField` render models.

## Core Model

The type renderer accepts backend-owned Erlang type fragments and structured type shapes, then renders source text for specs and type declarations. It does not parse Terlan source and does not perform type checking.

## Integration Points

- Parent `erl` module renders specs and type declarations.
- `emit::util` lowers Terlan type text into the render model.

## Edge Cases

- Phantom type variables are marked during spec rendering.
- Raw backend type fragments are preserved exactly.

## Types And Interfaces

`ErlType`
: Structured Erlang type-expression render model.

`ErlMapTypeField`
: Erlang map type field render model.

## Testing Notes

- Add rendering regressions through adjacent Erlang emit tests.
- Keep type lowering tests in the modules that construct the type render model.
