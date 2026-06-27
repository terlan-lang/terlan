# Std JS Internals

This directory owns JavaScript platform standard-library modules. These modules
are target-specific and intentionally separate from portable `std.core`
modules. The surface is generated from pinned TypeScript standard-library
declaration inputs wherever possible, with small hand-authored wrappers only
for Terlan-native bridge types such as `JsString`, `Array`, `Promise`, and
`JsNumber`.

## Responsibilities

- Provide explicit JavaScript platform APIs under `std.js.*`.
- Keep JS-native values distinct from portable core values.
- Record generated binding inputs and skipped declarations.
- Preserve TypeScript declaration documentation on generated Terlan wrappers
  whenever the source declaration supplies JSDoc.
- Preserve deterministic generated sources and summaries for release builds.
- Treat every unsupported TypeScript declaration as a reviewed skip entry with
  a stable source, reason, and detail.

## Public Surface

- `std.js.String.JsString`: JavaScript-native string wrapper.
- `std.js.Array.Array[T]`: JavaScript-native array wrapper.
- `std.js.Promise.Promise[T]`: JavaScript-native promise wrapper.
- `std.js.Number.JsNumber`: JavaScript-native number wrapper.
- `std.js.Map`, `std.js.Set`, `std.js.WeakMap`, `std.js.WeakSet`, and readonly
  or constructor variants generated from `lib.es2015.collection.d.ts`.
- `std.js.Dom.*`: generated browser DOM bindings from pinned DOM declarations.

## Core Model

Terlan source must import JavaScript platform modules explicitly. Generated
bindings preserve TypeScript source shape where possible while using Terlan
naming conventions in source. The JavaScript backend maps generated snake_case
names back to JavaScript runtime names when needed.

The main flow is:

1. Maintainer-side binding generation reads pinned TypeScript declarations.
2. Generated Terlan modules and manifests are committed under `std/js`.
3. Target-profile validation decides whether an import is legal for the
   selected JavaScript profile.
4. JS emission lowers the typed Terlan surface to deterministic ES modules.

Important invariants:

- Browser globals are never ambient in Terlan source.
- `std.js` APIs are rejected for non-JS targets.
- DOM APIs require a browser-capable target profile.
- Supported TypeScript declarations are included in committed Terlan sources.
- Supported TypeScript JSDoc is copied into generated `.terl`, `.terli`, and
  `.typi` artifacts instead of being discarded.
- Unsupported TypeScript shapes are skipped with stable manifest diagnostics.
- Missing TypeScript declarations without a corresponding generated module or
  skip row are generator defects.

## Integration Points

- `terlan::commands::bind`: generates JS binding seed files.
- `terlan::commands::emit_js`: emits Oxc-backed JavaScript modules.
- `terlan_typeck`: validates target-profile import compatibility.
- `std/js/manifests`: records generation inputs, outputs, and skipped shapes.

## Edge Cases

- Optional TypeScript parameters become `Option[T]`.
- Nullable returns become `Option[T]`.
- Mutable properties require explicit generated setter methods.
- Complex unions are skipped until their source-level representation is stable.
- TypeScript `this` return types, overloaded constructor signatures, top-level
  constructor variables, and `any` parameters are skipped until the generator
  can represent them honestly.

## Types And Interfaces

`JsString`
: Target-native JavaScript string wrapper.

`Array[T]`
: Target-native JavaScript array wrapper.

`Promise[T]`
: Target-native JavaScript promise wrapper with explicit conversion paths.

`JsNumber`
: Target-native JavaScript number wrapper.

`Map[K, V]`, `Set[T]`, `WeakMap[K, V]`, `WeakSet[T]`
: Generated ES2015 collection wrappers.

## Testing Notes

- Positive std.js tests live beside the modules as `std/js/*Test.terl`.
- Generated DOM seed tests live under `std/js/dom/*Test.terl`.
- Binding generation drift is checked by `make stdlib-check` and the 0.0.5
  release preflight.
