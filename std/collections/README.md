# Std Collections Internals

This directory owns portable collection modules and traits. The current release
surface includes list, map, set, iterator, iterable/enumerable, and index
contracts used by portable source code.

## Responsibilities

- Define target-neutral collection types and collection traits.
- Keep portable collection APIs separate from target-native collections.
- Provide receiver-method surfaces that work naturally with pipes.
- Preserve explicit mutation semantics through `mut` receiver methods.

## Public Surface

- `std.collections.List`: portable ordered sequence.
- `std.collections.Map`: portable key/value container.
- `std.collections.Set`: portable unique-value container.
- `std.collections.Iterator`, `Iterable`, and `Enumerable`: traversal
  contracts.
- `std.collections.Index`: index get/set traits for bracket syntax.

## Core Model

Portable collections describe source-level behavior. Each target decides how to
represent the collection internally, but user code sees the same typed receiver
methods and trait contracts.

The main flow is:

1. Source imports a collection module or trait.
2. Type checking validates receiver methods and trait implementations.
3. The backend lowers the collection operation to the selected target
   representation.

Important invariants:

- Portable collections must not expose backend data structures.
- Mutating methods require explicit `mut` receiver syntax.
- Index access is trait-backed so it can apply beyond one collection type.

## Integration Points

- `std.native.collections`: explicit target-native collection escape hatch.
- `std.core.Equal`: equality trait for collection operations that compare
  values.
- Backends: own actual collection representations and mutation lowering.

## Edge Cases

- Map iteration yields key/value pair shapes.
- Set iteration yields values.
- Portable collections should prefer typed errors or stable diagnostics for
  invalid indexed access.

## Types And Interfaces

`List[T]`
: Portable ordered sequence.

`Map[K, V]`
: Portable key/value collection.

`Set[T]`
: Portable unique-value collection.

`Iterable[T]`
: Traversal trait for values.

## Testing Notes

- Positive tests live beside modules as `std/collections/*_test.terl`.
- Release API coverage is recorded in `tests/std/RELEASE_API_TESTS.tsv`.
- Backend lowering tests should cover each target-specific representation.
