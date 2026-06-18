# J0.8 Browser Packaging Capability Decision

This decision records the 0.0.4 browser packaging boundary. It is a compiler
contract, not a user guide.

## Feature

Browser packaging, asset handling, local preview, live reload, and development
serving for `_build/web/` artifacts.

## Required Capability

- Parse and validate emitted JavaScript modules.
- Transform or print JavaScript when the compiler owns the emitted module.
- Package browser artifacts from emitted JavaScript modules, static assets,
  generated manifests, and source maps.
- Support a file-watch and reload boundary for local development.
- Serve packaged artifacts through `terlc serve` without requiring users to
  learn Node, Vite, Bun, Rsbuild, or a Rspack server process for the basic loop.

## Oxc Component Checked

- `oxc_parser`
- `oxc_ast`
- `oxc_allocator`
- `oxc_span`
- `oxc_codegen`
- `oxc_resolver`
- `oxc_transformer`
- `oxc_formatter`
- `oxc_minifier`
- `oxc_compat` / `oxc-browserslist`

## Oxc Result

Accepted for JavaScript compiler-owned layers:

- parsing emitted JavaScript;
- validating emitted JavaScript before artifact write;
- building and printing emitted JavaScript modules;
- TypeScript declaration parsing for generated `std.js` bindings;
- future resolver, transform, format, minify, and compatibility checks when
  those capabilities become release requirements.

Rejected for full browser packaging and development serving:

- no selected Oxc crate owns a complete browser artifact graph;
- no selected Oxc crate owns HTML asset injection;
- no selected Oxc crate owns HMR or live-reload orchestration;
- no selected Oxc crate owns the local HTTP server contract required by
  `terlc serve`.

## SWC Component Checked

SWC remains a fallback for JavaScript transform/minify gaps only. It is not the
selected 0.0.4 browser packaging or serving boundary because the required
release gap is browser artifact packaging and live-reload orchestration, not a
second JavaScript compiler backend.

## Rsbuild/Rspack Component Checked

Rsbuild is the preferred user-hidden web build facade when Terlan needs a full
browser application layer. Rspack is the underlying engine boundary to evaluate
for Rust/compiler integration.

- `rspack_core`
- `rspack`
- `rspack_plugin_asset`
- `rspack_plugin_html`
- `rspack_plugin_hmr`
- `rspack_watcher`

## Rsbuild/Rspack Result

Accepted as the fallback browser packaging and watch boundary when simple Oxc
compiler-owned output is insufficient:

- browser bundle and asset graph ownership;
- HTML and asset plugin ownership;
- static asset handling;
- CSS handling;
- web worker packaging;
- HMR/live-reload integration points;
- file watching for generated JavaScript, assets, route metadata, and static
  site inputs.

Rejected as the local server owner:

- `terlc serve` remains Terlan-owned Rust/Tokio host tooling;
- the basic local development loop must not require a Rspack server process;
- Rsbuild/Rspack types must stay behind browser packaging modules and must not
  leak into syntax, HIR, typecheck, CoreIR, validation, proof, SafeNative, or
  Erlang backend APIs.

Rejected as the ordinary user-facing API:

- default Terlan web projects are configured through `terlan.toml`;
- users run `terlc build --target web` or `terlc serve`, not Rsbuild directly;
- direct Rsbuild configuration is reserved for a future explicit advanced
  escape hatch.

## Selected Implementation

The 0.0.4 implementation boundary is:

1. Oxc owns emitted JavaScript parsing, validation, AST construction, and
   codegen.
2. Terlan-owned browser packaging glue writes deterministic `_build/web/`
   manifests and static artifact layouts.
3. Rsbuild/Rspack is the fallback for bundle, asset graph, static assets, CSS,
   workers, HTML asset injection, HMR, and watch behavior only when the simple
   Terlan packaging path cannot satisfy the feature.
4. `terlc serve` is implemented as Rust/Tokio-native Terlan host tooling.
5. Live reload checks Oxc first, then uses the selected Rsbuild/Rspack watch
   boundary when bundling or asset graph orchestration requires it.

## Temporary Watch Shim

The first `terlc serve` implementation may use a small Rust/Tokio polling shim
over `_build/web/` to drive local reload events while Rsbuild/Rspack watcher
integration is still being selected. This shim is explicitly temporary compiler
glue:

- it watches only generated Terlan browser package files;
- it does not parse, transform, bundle, or resolve JavaScript;
- it does not replace Rsbuild/Rspack as the fallback asset graph or watch
  boundary;
- it must stay behind `terlc serve` and must not leak into syntax, HIR,
  typecheck, CoreIR, validation, proof, SafeNative, or backend APIs.

## Reason Custom Code Is Required

Terlan must own compiler glue that is specific to Terlan source and build
artifacts:

- target-profile checks;
- `_build/web/` manifest generation;
- route metadata validation;
- stable diagnostics;
- mapping Terlan package layout into browser artifact layout;
- dispatch from the local server to static assets or BEAM-backed handlers.

This custom code must not become a custom JavaScript compiler, bundler, HTML
asset graph, minifier, formatter, worker-packaging, CSS, static asset, or
dev-server implementation.

## Release Gate

`make web-capability-decision-check` verifies that this decision remains
present and contains the required Oxc, SWC, Rsbuild/Rspack, and Terlan-owned
server boundary sections. `make release-0-0-4-preflight` runs that gate.

## Validation

J0.8 is complete when:

- this decision is checked in;
- the capability checker passes;
- the 0.0.4 roadmap no longer lists J0.8 as active work;
- later J0.9, J0.10, J0.11, and J0.12 implementation uses this boundary.
