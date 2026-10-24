# JS Browser Routes Internals

This directory discovers route metadata for browser and VM-service artifacts
by executing compiled Terlan exports. Package-owned projections interpret the
returned declarations; no parallel Router or constant-string interpreter remains.

## Responsibilities

- Discover handler and channel routes and source locations.
- Retain the source router owner and diagnostic labels for actual callbacks.
- Evaluate legacy WebSocket `route()` and `protocol()` exports.
- Delegate route-namespace validation to `std/http/native`.

## Public Surface

- `discover_web_route_manifest_from_sources`: extracts production route rows.
- `discover_web_handlers_from_modules`: test adapter for handler discovery.
- `helpers`: source-path and source-span helpers.

## Core Model

Routes are projected from executed source and written into a web manifest.

The main flow is:

1. Extract public exports and spans with the existing parser; let the HTTP
   package identify its entrypoints from names and arities.
2. Compile the entrypoint's import closure and invoke public exports through the
   generic source evaluator, keeping the native image alive during projection.
3. Project and validate returned metadata in the HTTP package.
4. Return manifest rows with source spans for diagnostics.

Important invariants:

- Unsupported methods must fail before artifact emission.
- Handler providers are ordinary import dependencies, not separately discovered
  route sources. Neither request/response type spelling nor import aliases are
  used to classify them; the application closure resolves their implementations.
- Handlers remain executable calls, not statically inferred response payloads.
- Middleware and recovery callback types are checked by the ordinary compiler
  against package declarations. Discovery does not inspect their names or types.
- Legacy WebSocket exports use exact compiler import resolution. Computed strings
  and local/imported helper calls work; missing, private, wrong-arity, or non-string
  exports fail. External service waits are cancelled, not satisfied by build tools.
- The evaluator bounds resumable transitions. It is not a sandbox or a timeout for
  arbitrary native source execution; builds evaluate trusted project code.
- Serving executes the source router and retains its actual callbacks, including
  imported functions and closures. Recovery is not synthesized into a manifest
  row. Explicit legacy error-handler rows remain accepted as manifest metadata.

## Integration Points

- `terlan_syntax`: supplies syntax modules and spans.
- `manifest`: consumes route rows for package output.

## Testing Notes

- `../js_browser_test.rs` covers route rows and invalid route diagnostics.
