# Std JS WebAssembly Namespace

This directory contains generated Terlan bindings for the browser
`WebAssembly` namespace. It mirrors supported declarations from the pinned
TypeScript DOM inputs and remains specific to JavaScript browser targets.

## Responsibilities

- Expose typed modules for WebAssembly descriptors, instances, memories,
  tables, imports, exports, globals, and errors.
- Keep generated `.terl` and `.terli` surfaces deterministic.
- Preserve unsupported declaration decisions in generator manifests.

## Core Model

Generated Terlan values represent JavaScript WebAssembly host objects; they are
not the portable `std.wasm.Abi` types used by Terlan Wasm compilation.

## Integration Points

- `commands::bind` generates this namespace from pinned `.d.ts` declarations.
- the JS backend maps the typed surface to browser WebAssembly objects.

## Testing Notes

- Adjacent `*Test.terl` files validate every generated declaration family.
- Regenerate through the binding pipeline; do not hand-edit generated modules.
