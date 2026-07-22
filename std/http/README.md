# Std HTTP Internals

This directory owns the portable source-level HTTP API used by Terlan handlers.
The concrete server is Rust-native compiler tooling; source code works with
typed request, response, and error modules rather than backend server values.

## Responsibilities

- Define stable HTTP request and response shapes for Terlan handlers.
- Keep the local server implementation and temporary Erlang migration handler
  bridge internal.
- Expose JSON-capable handler helpers without leaking host JSON values.
- Provide portable errors for request body, response, and serialization
  failures.

## Public Surface

- `std.http.Request.Request`: opaque request handle.
- `std.http.Response.Response`: opaque response handle.
- `std.http.Session.Session`: opaque VM-owned session actor handle.
- `std.http.Sse.Event`: opaque server-sent event descriptor.
- `std.http.Sse.Endpoint`: opaque VM-owned SSE endpoint plan.
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
- `std.http.Sse.data`, event receiver metadata helpers, queued
  `std.http.Sse.response`, and endpoint-plan constructors: typed SSE surface
  for VM-owned `text/event-stream` responses.
- `std.http.Session.current`, session receiver `get`, `set`, `delete`,
  `rotate`, `expire`, and `with_response`: source-facing actor-backed session
  helpers with explicit response cookie threading.
- `std.http.Router.new`, method route builders, `sse`, `websocket`, and
  `fallback`: typed route builder contract for generated web manifests and
  VM-owned long-lived channel routes.

## Router And Middleware Composition

`Router.use` installs request middleware in declaration order. Each callback
returns either `Continue` or `Respond(Response)`, so authorization, timeout,
and recovery policies cannot accidentally fall through after producing a
response. `Router.map_response` performs typed response post-processing in
reverse order. This is the header-normalization and trace-response hook;
`Router.error` is the panic/error boundary. Request headers provide normalized
trace input, while side effects remain behind explicit NativeBoundary or actor
capabilities rather than the ordinary `Handler` function type.

Application-wide middleware belongs on the root router. A `Router.group`
builder adds scoped middleware and nested routes without new routing grammar:

```text
pub require_user(_request: Request): MiddlewareResult ->
    Continue.

pub api(router: Router): Router ->
    router
    |> Router.use(require_user)
    |> Router.get("/users/:id", show_user).

pub application_router(): Router ->
    Router.new()
    |> Router.use(propagate_trace)
    |> Router.map_response(normalize_headers)
    |> Router.group("/api", api)
    |> Router.error(recover_http_error).
```

The VM prefixes group paths, preserves parameter captures, rejects ambiguous
normalized route shapes, and dispatches root middleware before scoped
middleware. Bounded SSE and WebSocket endpoint plans survive router
materialization and open live-session state with the source-declared queue and
message limits. `LiveChannelTest.terl` is the executable nested-channel
example; `RouterTest.terl` covers typed response short-circuit composition.

## Core Model

The HTTP server owns concrete socket, request, and response state. Terlan
source receives opaque handles and calls standard-library functions against
those handles. The current implementation can dispatch dynamic handlers through
a temporary Erlang migration bridge, but that bridge ABI is not a public source
contract.

The main flow is:

1. The packaged web manifest matches a request to a static asset or handler.
2. The server constructs target-owned request/response state.
3. Handler source uses `std.http.*` helpers to parse input and build output.

Important invariants:

- The internal migration handler response tuple is not a public API.
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
- `std.http.Router` is a source-visible route builder contract. Its
  `sse` and `websocket` builders accept endpoint plans from `std.http.Sse` and
  `std.http.WebSocket` while the VM owns stream/socket state. The compiler
  discovers route manifests, and production VM serving rematerializes the
  source graph before middleware and endpoint admission.
- `set_cookie_header` accepts a complete header value for low-level escape
  hatches. Normal handler code should prefer `cookie`, `cookie_with_options`,
  and `delete_cookie`, which reuse `std.http.Cookies` validation.
- `std.http.Cookies` owns typed SameSite and cookie option shapes. Request-
  scoped mutable cookie jars still need served-handler resource bridge wiring
  before their mutations are automatically applied to returned responses.
- `std.http.Tls` is source-visible configuration shape and helper
  constructors only. `terlan.toml` parsing, rustls/ACME integration, and
  certificate cache state remain implementation work.
- `std.http.Sse` endpoint plans open bounded VM-owned live-session streams.
  Cancellation, scheduler wakeups, and socket emission remain runtime-owned and
  are never exposed as source-side host handles.

## Integration Points

- `terlc serve`: owns local server startup, validation, and request routing.
- `std.data.Json`: provides request JSON parsing and JSON response bodies.
- NativeBoundary runtime helpers: own Rust-native HTTP helper implementations.
- `_build/web/manifest.json`: declares static assets and handler routes.

## Edge Cases

- Missing or malformed web manifests fail during `terlc serve --check`.
- Unsafe route paths and asset paths are rejected before serving.
- Handler dispatch reports missing VM handler artifacts and unavailable VM
  handler runtime support before attempting dynamic execution.

## Types And Interfaces

`Request`
: Opaque request handle passed to handlers.

`Response`
: Opaque response handle returned by handlers.

`Router`
: Opaque route builder contract materialized by compiler discovery and VM
  dispatch.

`Handler`
: Function type for handlers that accept `Request` and return `Response`.

`HttpError`
: Portable HTTP error shape with code, message, and status.

`Cookies.Options`
: Typed cookie mutation options for the future cookie jar API.

`Sse.Event`
: Opaque server-sent event descriptor.

`Sse.Endpoint`
: Opaque bounded server-sent event route policy.

`Tls.Config`
: Typed TLS configuration record for auto, manual, and internal TLS modes.

## Testing Notes

- Positive HTTP std tests live beside the modules as `std/http/*Test.terl`.
- Server and handler bridge tests live under
  `crates/terlan/src/commands/serve/*_test.rs`.
- Release preflight includes exact HTTP handler and installed-runner support
  checks.
