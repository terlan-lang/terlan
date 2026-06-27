# JS Browser Routes Internals

This directory owns route discovery for browser/web build artifacts. The
implementation is centered on annotated Terlan handler modules. Its most
important boundary is that callers receive typed route rows instead of parsing
source annotations themselves.

## Responsibilities

- Discover web handler and error-handler annotations.
- Validate route method, path, and response metadata.
- Classify static and file-backed responses for manifest emission.

## Public Surface

- `discover_web_handlers_from_modules`: extracts route handler artifacts.
- `discover_web_error_handler_from_modules`: extracts error-handler artifacts.
- `helpers`: shared source-span and annotation helpers.
- `responses`: response classification helpers.

## Core Model

Routes are derived from syntax modules and written into a browser manifest.

The main flow is:

1. Inspect syntax declarations for web annotations.
2. Validate route metadata and response forms.
3. Return manifest rows with source spans for diagnostics.

Important invariants:

- Unsupported methods must fail before artifact emission.
- Static response paths must remain deterministic.
- Error-handler discovery must be separate from normal route discovery.

## Integration Points

- `terlan_syntax`: supplies syntax modules and spans.
- `manifest`: consumes route rows for package output.

## Testing Notes

- `../js_browser_test.rs` covers route rows and invalid route diagnostics.
