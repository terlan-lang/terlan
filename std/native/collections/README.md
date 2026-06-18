# Std Native Collections Internals

This directory owns platform-specific collection modules that are intentionally
outside portable `std.collections`. The current surface is
`std.native.collections.Vector`, an indexed native storage API for algorithms
and target paths that can provide efficient vector-like memory.

## Responsibilities

- Define target-native collection shapes without polluting portable core
  collection APIs.
- Expose indexed access through typed receiver methods and collection traits.
- Keep allocation, storage, and mutation target-owned behind native operation
  ids.
- Preserve explicit conversion paths between portable collections and native
  collections.

## Public Surface

- `std.native.collections.Vector.Vector[T]`: opaque native indexed collection.
- `std.native.collections.Vector.new`: native vector allocation.
- `std.native.collections.Vector.from_list`: portable list to vector
  conversion.
- `std.native.collections.Vector.to_list`: vector to portable list conversion.
- Vector receiver methods for length, index read/write, swap, and push.

## Core Model

Native collections are explicit target-specific APIs. Source code can use them
when it has selected a target profile that supports the required native
capability, but portable modules must not depend on them by default.

The main flow is:

1. Source imports a native collection module explicitly.
2. Type checking validates the native module and trait contracts.
3. The backend lowers operations to the selected native collection capability.

Important invariants:

- Native vectors are not portable `std.collections.List` values.
- Mutating vector operations require `mut` receiver methods.
- Bracket access is trait-backed, not a special vector-only parser path.
- Unsupported targets must fail before artifact emission.

## Integration Points

- `std.collections.Index`: supplies index get/set traits for bracket syntax.
- `terlan_safenative`: owns Rust-native operation implementations when the
  target supports them.
- `std/RUST_BACKED_MANIFEST.tsv`: records native operation ownership.
- Target-profile validation: rejects native collection APIs on unsupported
  targets.

## Edge Cases

- Out-of-range reads and writes must become stable target diagnostics or typed
  errors when the API is widened.
- Conversion to and from portable lists may copy values depending on backend
  representation.
- Native vectors should not become an implicit replacement for portable lists.

## Types And Interfaces

`Vector[T]`
: Opaque target-native indexed collection.

`IndexGet`
: Trait connection used by bracket read syntax.

`IndexSet`
: Trait connection used by bracket assignment syntax.

## Testing Notes

- Positive vector tests should live beside the module as
  `std/native/collections/vector_test.terl`.
- Native operation metadata is checked by `make stdlib-check`.
- Target-profile tests should cover unsupported target rejection when new
  vector backends are added.
