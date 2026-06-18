# Bind Command Context

This module owns `terlc bind`, the future package-binding generator surface.

## Responsibilities

- Reserve the public command shape for native binding generation.
- Validate command-local arguments before generator implementation exists.
- Report stable diagnostics for unsupported binding targets and unavailable
  generator backends.

## Current Scope

The current binding command owns two deterministic generator surfaces:

```sh
terlc bind rust --crate polars --out packages/std/native/polars
terlc bind js-dom --manifest std/js/manifests/std_js_dom_inputs.json --out generated/std-js
```

The Rust implementation generates the curated Polars package skeleton only. It
writes deterministic templates for the manifest, Terlan DataFrame module,
`.typi` interface summary, Rust crate mapping metadata, native ABI metadata,
Rust adapter `Cargo.toml`, and Rust adapter ABI stub with local smoke tests. It
does not inspect the upstream crate or produce broad bindings yet.

The 0.0.4 TypeScript DOM implementation reads a pinned input manifest, validates
committed `.d.ts` hashes, parses declarations through Oxc, maps supported
interfaces into `std.js.Dom.*` module plans, and writes deterministic `.terl`,
`.terli`, `.typi`, and generated binding manifest files. It does not use npm
resolution, Node package lookup, or the network during normal generation.

## Boundaries

- Do not fetch Cargo metadata from the network.
- Do not inspect Rust crate sources.
- Do not generate cache `.deps` summaries until interface dependency hashing is
  wired into the binding pipeline.
- Do not link the real `polars` crate until the DataFrame native smoke wrapper
  slice opens.
- Do not add non-Rust binding targets until a concrete package probe requires
  them.
- Do not silently approximate complex TypeScript unions; record a stable
  skipped-declaration reason instead.
- Do not resolve TypeScript packages dynamically during normal generation; use
  pinned manifests and committed input hashes.
