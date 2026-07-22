# `std.range`

`std.range` defines finite integer ranges as ordinary Terlan values.

Rules:

- Ranges are public standard-library values, not private compiler-only runtime
  shapes.
- `Int..Int` syntax lowers to the public concrete `std.range.Range.Range`
  value.
- Invalid step values return typed errors; they must not become unchecked
  runtime crashes.
- Iterator conversion goes through `std.collections`, so comprehensions and
  traversal can share the same implementation.
