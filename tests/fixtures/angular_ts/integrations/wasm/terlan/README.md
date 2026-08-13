# AngularTS Terlan Wasm Integration

This package is the Terlan/Wasm integration boundary for AngularTS.

The Rust integration remains the reference implementation for the live browser
Wasm bridge. This package gives Terlan the same external integration shape:

- `src/terlan/angular/wasm/App.terl` is the Terlan-authored app boundary.
- `terlan.toml` declares the browser Wasm artifact, exports, bridge, and
  capabilities.
- `examples/basic_app/angular-ts.json` mirrors the Rust integration's app
  manifest shape.
- `tool/generate_ng_namespace_manifest.mjs` pins the real
  `@types/namespace.d.ts` input.
- `tool/check_ng_namespace_parity.mjs` verifies every direct `ng` namespace
  type is either generated as Terlan or explicitly skipped.
- `make reserved-build-check` proves `terlc build --target wasm.browser` still
  stops at the reserved backend boundary until Terlan CoreIR-to-Wasm lowering
  is implemented.

The current executable contract is app definition, namespace generation, and
reserved backend diagnostics. Once the Wasm backend is promoted, this package is
where the real browser Wasm build and Playwright smoke should land.
