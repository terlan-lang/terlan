# JS Browser Routes Internals

This directory currently discovers route metadata for browser/web artifacts
from simple Router builder expressions. This syntax inspection is a remaining
migration boundary, not the authority for executed HTTP policy.

## Responsibilities

- Discover handler and channel routes and source locations.
- Resolve direct handler references and their route-parameter signatures.
- Delegate route-namespace validation to `std/http/native`.

## Public Surface

- `discover_web_route_manifest_from_sources`: extracts production route rows.
- `discover_web_handlers_from_modules`: test adapter for handler discovery.
- `helpers`: source-span and builder-expression helpers.

## Core Model

Routes are derived from syntax modules and written into a browser manifest.

The main flow is:

1. Inspect router functions for supported route-builder expressions.
2. Validate route metadata using the HTTP package's shared route contract.
3. Return manifest rows with source spans for diagnostics.

Important invariants:

- Unsupported methods must fail before artifact emission.
- Handlers remain executable calls, not statically inferred response payloads.
- Middleware and recovery callback types are checked by the ordinary compiler
  against package declarations. Discovery does not inspect their names or types.
- Serving executes the source router and retains its actual callbacks, including
  imported functions and closures. Recovery is not synthesized into a manifest
  row. Explicit legacy error-handler rows remain accepted as manifest metadata.

## Integration Points

- `terlan_syntax`: supplies syntax modules and spans.
- `manifest`: consumes route rows for package output.

## Testing Notes

- `../js_browser_test.rs` covers route rows and invalid route diagnostics.
