# Std HTTP Internals

This directory owns the portable source-level HTTP API used by Terlan handlers.
The concrete server is Rust/Tokio-native compiler tooling; source code works
with typed request, response, and error modules rather than backend server
values.

## Responsibilities

- Define stable HTTP request and response shapes for Terlan handlers.
- Keep the local server implementation and BEAM handler bridge internal.
- Expose JSON-capable handler helpers without leaking host JSON values.
- Provide portable errors for request body, response, and serialization
  failures.

## Public Surface

- `std.http.Request.Request`: opaque request handle.
- `std.http.Response.Response`: opaque response handle.
- `std.http.Error.HttpError`: portable HTTP helper error.
- `std.http.Request.body_json`: explicit JSON request parsing.
- `std.http.Response.json` and `std.http.Response.text`: response builders.

## Core Model

The HTTP server owns concrete socket, request, and response state. Terlan
source receives opaque handles and calls standard-library functions against
those handles. The 0.0.4 bridge can dispatch BEAM-backed handlers through an
internal ABI, but that ABI is not a public source contract.

The main flow is:

1. The packaged web manifest matches a request to a static asset or handler.
2. The server constructs target-owned request/response state.
3. Handler source uses `std.http.*` helpers to parse input and build output.

Important invariants:

- The internal BEAM response tuple is not a public API.
- JSON responses accept `Json` explicitly in 0.0.4.
- Mutable response updates use mutable receiver methods and return `Unit`.
- HTTP status and header manipulation remain target-owned operations.

## Integration Points

- `terlc serve`: owns local server startup, validation, and request routing.
- `std.data.Json`: provides request JSON parsing and JSON response bodies.
- `terlan_safenative`: owns Rust-native HTTP helper implementations.
- `_build/web/manifest.json`: declares static assets and handler routes.

## Edge Cases

- Missing or malformed web manifests fail during `terlc serve --check`.
- Unsafe route paths and asset paths are rejected before serving.
- Handler dispatch must report missing BEAM artifacts before attempting to run
  `erl`.

## Types And Interfaces

`Request`
: Opaque request handle passed to handlers.

`Response`
: Opaque response handle returned by handlers.

`HttpError`
: Portable HTTP error shape with code, message, and status.

## Testing Notes

- Positive HTTP std tests live beside the modules as `std/http/*_test.terl`.
- Server and handler bridge tests live under
  `crates/terlan_cli/src/commands/serve/*_test.rs`.
- Release preflight includes exact HTTP handler and installed-runner support
  checks.
