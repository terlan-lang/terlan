# JS Browser Build Internals

This directory owns browser package emission for the JavaScript target. The
implementation is centered on browser-ready assets, route metadata, and web
manifest output. Its most important boundary is that build orchestration can use
these helpers without knowing route-discovery internals.

## Responsibilities

- Write browser package manifests.
- Copy JavaScript and static assets into build output.
- Coordinate route metadata generated from Terlan web modules.

## Public Surface

- `assets`: copies JavaScript and static assets.
- `manifest`: serializes browser package manifests.
- `routes`: discovers and validates browser route metadata.

## Core Model

The package writer receives lowered JavaScript modules plus optional web route
metadata and turns them into a deterministic browser package.

The main flow is:

1. Collect emitted JavaScript module paths.
2. Discover web route and error-handler artifacts.
3. Write the manifest and copy referenced assets.

Important invariants:

- Manifest paths must stay relative to the package output.
- Route discovery must not rewrite generated JavaScript.
- Missing referenced assets must surface as build errors.

## Integration Points

- `commands::build`: supplies emitted JS modules and output paths.
- `routes`: provides web handler rows for the manifest.
- Browser tooling: consumes the emitted manifest and copied assets.

## Testing Notes

- `../js_browser_test.rs` covers manifest rows, static responses, and route
  discovery.
