# Std Core Internals

This directory owns target-neutral core types, traits, and primitives. These
modules define the smallest shared language surface that every supported target
must preserve.

## Responsibilities

- Define portable primitive and algebraic core types.
- Provide core traits such as equality, ordering, error, parsing, and showing.
- Keep backend primitive operation calls explicit and audited.
- Avoid target-specific behavior in portable APIs.

## Public Surface

- Primitive/core value modules: `Atom`, `Bool`, `Int`, `Float`, `String`,
  `Object`, and `Unit`.
- Algebraic modules: `Option`, `Result`, `Ordering`, and `Task`.
- Trait modules: `Equal`, error, parse/show-related contracts, and ordering
  contracts.
- `BACKEND_PRIMITIVE_CALLS.tsv`: audited primitive backend call surface.

## Core Model

Core modules describe behavior that should be valid across BEAM, JavaScript,
Rust/native, and future targets. Backends may use different runtime
representations, but Terlan source sees stable module names, receiver methods,
constructors, and traits.

The main flow is:

1. Source uses implicitly available core types or explicitly imports core
   modules.
2. Type checking validates the core operation shape.
3. The selected backend lowers to its own primitive representation.

Important invariants:

- Core APIs are target-neutral.
- Target-native strings, arrays, or promises live outside `std.core`.
- Backend primitive calls must stay listed and checked.

## Integration Points

- Every std module may depend on `std.core`.
- `terlan_typeck`: owns trait and type validation.
- Backend crates: own primitive lowering per target.

## Edge Cases

- `Unit` is both a type and value, not a nullary constructor call.
- Atoms are language-level symbolic values, not Erlang-specific syntax.
- Atom aliases use `Atom["name"]`; runtime text must not create new atoms.
  Dynamic input should stay as `String` or map through a finite checked table.
- Core string behavior is UTF-8 source behavior, even when targets represent
  strings differently.

## Types And Interfaces

`Option[T]`
: Optional value type.

`Result[T, E]`
: Success or error result type.

`Object[V]`
: Dynamic string-keyed object type backed by `Map[String, V]`.

`Equal[T]`
: Equality trait used by core and collection APIs.

## Testing Notes

- Positive tests live beside modules as `std/core/*Test.terl`.
- Primitive backend call drift is checked by stdlib validation.
- Core behavior should be covered before adding target-specific adapters.
