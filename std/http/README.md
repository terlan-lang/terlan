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
- `std.http.Router.Router`: opaque route builder contract.
- `std.http.Router.Handler`: typed route handler function shape.
- `std.http.Error.HttpError`: portable HTTP helper error.
- `std.http.Cookies.Options`: typed `Set-Cookie` option contract.
- `std.http.Tls.Config`: declarative TLS configuration contract.
- `std.http.Tls.auto`, `std.http.Tls.manual`, and `std.http.Tls.internal`:
  constructors for the supported TLS configuration modes.
- `std.http.Request.method` and `std.http.Request.path`: request metadata
  accessors.
- `std.http.Request.param`, `std.http.Request.query`, and
  `std.http.Request.cookie`: optional route/query/cookie metadata accessors.
- `std.http.Request.body_text`: raw UTF-8 request body access.
- `std.http.Request.body_json`: explicit JSON request parsing.
- `std.http.Response.json`, `std.http.Response.text`,
  `std.http.Response.html`, and `std.http.Response.redirect`: response
  builders.
- `std.http.Response.status`, `std.http.Response.header`, and
  `std.http.Response.set_cookie_header`: mutable response metadata helpers.
- `std.http.Response.with_status` and `std.http.Response.with_header`:
  chainable response metadata helpers for expression-style handler code.
- `std.http.Response.cookie`, `std.http.Response.cookie_with_options`, and
  `std.http.Response.delete_cookie`: validated response cookie helpers backed
  by `std.http.Cookies`.
- `std.http.Router.new`, method route builders, and `fallback`: typed route
  builder contract for generated web manifests.

## Core Model

The HTTP server owns concrete socket, request, and response state. Terlan
source receives opaque handles and calls standard-library functions against
those handles. The current bridge can dispatch BEAM-backed handlers through an
internal ABI, but that ABI is not a public source contract.

The main flow is:

1. The packaged web manifest matches a request to a static asset or handler.
2. The server constructs target-owned request/response state.
3. Handler source uses `std.http.*` helpers to parse input and build output.

Important invariants:

- The internal BEAM response tuple is not a public API.
- JSON responses accept `Json` explicitly.
- Request metadata accessors return values captured by the generated route
  manifest and server bridge.
- Raw body access does not parse or validate content type; higher-level form,
  JSON, and multipart helpers should live on explicit std APIs.
- HTML responses accept already-rendered HTML strings; typed template rendering
  remains a separate compiler/template responsibility.
- Redirect responses use the default temporary redirect shape and can be
  refined later with richer status support.
- Mutable response updates use mutable receiver methods and return `Unit`.
- HTTP status, header, and interim cookie-header manipulation remain
  target-owned operations.
- `with_status` and `with_header` are pure source ergonomics over mutable
  receiver continuation; they do not introduce a separate response storage
  model.
- `std.http.Router` is a source-visible route builder contract. Compiler
  lowering from router values into route manifests is pending.
- `set_cookie_header` accepts a complete header value for low-level escape
  hatches. Normal handler code should prefer `cookie`, `cookie_with_options`,
  and `delete_cookie`, which reuse `std.http.Cookies` validation.
- `std.http.Cookies` owns typed SameSite and cookie option shapes. Request-
  scoped mutable cookie jars still need served-handler resource bridge wiring
  before their mutations are automatically applied to returned responses.
- `std.http.Tls` is source-visible configuration shape and helper
  constructors only. `terlan.toml` parsing, rustls/ACME integration, and
  certificate cache state remain implementation work.

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

`Router`
: Opaque route builder contract consumed by future route-manifest lowering.

`Handler`
: Function type for handlers that accept `Request` and return `Response`.

`HttpError`
: Portable HTTP error shape with code, message, and status.

`Cookies.Options`
: Typed cookie mutation options for the future cookie jar API.

`Tls.Config`
: Typed TLS configuration record for auto, manual, and internal TLS modes.

## Testing Notes

- Positive HTTP std tests live beside the modules as `std/http/*Test.terl`.
- Server and handler bridge tests live under
  `crates/terlan_cli/src/commands/serve/*_test.rs`.
- Release preflight includes exact HTTP handler and installed-runner support
  checks.
