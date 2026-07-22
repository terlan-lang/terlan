# Terlan Target Inference

Terlan target selection is derived from typed compiler evidence. A source file
does not need a target flag when its resolved imports, checked annotations, and
required runtime capabilities identify the runtime family.

The inference rule is:

1. Target-neutral code defaults to `vm`.
2. `std.vm.*`, `std.native.*`, `std.http.*`, and `std.db.*` evidence requires
   `vm`.
3. `std.js.*` evidence requires the narrowest JavaScript profile:
   `js.shared`, `js.browser`, or `js.worker`.
4. Mixed runtime families are compile-time conflicts.
5. Explicit CLI target flags are overrides only when they can satisfy the
   typed evidence. Otherwise they produce diagnostics.

Future Wasm, WASI, mobile, embedded, and native targets must extend this model
with typed evidence. They must not reintroduce source-location heuristics or
profile guessing that is disconnected from the typechecked program.
