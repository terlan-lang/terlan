# Std JS Internals

This directory owns JavaScript platform standard-library modules. These modules
are target-specific and intentionally separate from portable `std.core`
modules. The initial 0.0.4 surface includes JS-native wrappers and generated
DOM seed bindings.

## Responsibilities

- Provide explicit JavaScript platform APIs under `std.js.*`.
- Keep JS-native values distinct from portable core values.
- Record generated binding inputs and skipped declarations.
- Preserve deterministic generated sources and summaries for release builds.

## Public Surface

- `std.js.String.JsString`: JavaScript-native string wrapper.
- `std.js.Array.Array[T]`: JavaScript-native array wrapper.
- `std.js.Promise.Promise[T]`: JavaScript-native promise wrapper.
- `std.js.Dom.*`: generated browser DOM seed bindings.

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
- Unsupported TypeScript shapes are skipped with stable manifest diagnostics.

## Integration Points

- `terlan_cli::commands::bind`: generates JS binding seed files.
- `terlan_cli::commands::emit_js`: emits Oxc-backed JavaScript modules.
- `terlan_typeck`: validates target-profile import compatibility.
- `std/js/manifests`: records generation inputs, outputs, and skipped shapes.

## Edge Cases

- Optional TypeScript parameters become `Option[T]`.
- Nullable returns become `Option[T]`.
- Mutable properties require explicit generated setter methods.
- Complex unions are skipped until their source-level representation is stable.

## Types And Interfaces

`JsString`
: Target-native JavaScript string wrapper.

`Array[T]`
: Target-native JavaScript array wrapper.

`Promise[T]`
: Target-native JavaScript promise wrapper with explicit conversion paths.

## Testing Notes

- Positive std.js tests live beside the modules as `std/js/*_test.terl`.
- Generated DOM seed tests live under `std/js/dom/*_test.terl`.
- Binding generation drift is checked by `make stdlib-check` and the 0.0.4
  release preflight.
