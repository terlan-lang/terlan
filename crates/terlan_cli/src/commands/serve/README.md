# Serve Command Internals

This directory owns `terlc serve`, the local HTTP entry point for packaged
browser artifacts. The implementation is centered on validating an existing
`_build/web` package manifest, serving files from that package root, and
dispatching manifest-declared dynamic routes. Its most important boundary is
that callers may depend on stable CLI diagnostics, route precedence, captured
params, and safe artifact path handling, while HTTP implementation details stay
internal.

## Responsibilities

- Parse `terlc serve` command-local arguments.
- Validate `_build/web/manifest.json` before serving.
- Validate adjacent web-profile Docker Compose metadata when a project
  manifest is found.
- Start the validated project-owned Postgres Compose service before binding in
  normal serve mode.
- Serve packaged files with predictable MIME types and safe path handling.
- Validate manifest-declared dynamic handler route patterns before serving.
- Dispatch manifest-declared handler routes through generated BEAM artifacts
  without hard-coding application route behavior in the server.
- Reserve the local live-reload endpoint and inject reload wiring into HTML.
- Keep reload watching behind `watch.rs`; the current polling backend is a
  compatibility implementation for generated `_build/web` files until the
  selected Oxc/Rsbuild/Rspack watch boundary is integrated.
- Provide a non-blocking `--check` mode for CI and release preflight.

## Public Surface

- `run`: executes `terlc serve`.
- `parse_serve_args`: normalizes command-local arguments into `ServeArgs`.
- `validate_web_package`: validates a packaged browser artifact.
- `logging`: request id allocation, local request log formatting, and
  browser-readable development handler error pages with optional source span
  metadata from generated manifests.
- `response`: content-type mapping, live-reload script injection, SSE headers,
  and HTTP response rendering.
- `tls`: TLS runtime boundary for manual, internal, and automatic ACME modes.
  Manual certificate and internal local modes build `rustls` server configs;
  automatic ACME mode loads deterministic project-local cache files and fails
  closed until issuance writes them. Auto TLS projects reserve
  `/.well-known/acme-challenge/<token>` for cached HTTP-01 challenge responses.
  ACME account credentials, HTTP-01 challenge bodies, and issued certificate
  material all use deterministic `.terlan/tls/acme` cache paths. The issuance
  boundary uses `instant-acme` for ACME account/order/challenge/finalization
  protocol work and `rcgen` for CSR/key generation.
- `handler::beam_eval`: temporary BEAM handler request-map and `erl -eval`
  bridge rendering.

## Core Model

The command treats `_build/web` as the release-facing browser package. The web
manifest is authoritative for the package schema, entry HTML file, asset files
copied by `terlc build --target js.browser`, and dynamic handler route metadata
that dispatches into BEAM-backed Terlan modules when sibling `_build/ebin`
artifacts are present.

The main flow is:

1. Parse host, port, optional package directory, and `--check`.
2. Validate the package manifest and referenced files.
3. In normal mode, bind a Hyper HTTP listener and serve package files.
4. Route `/__terlan/reload` to the server-sent-events reload stream.
5. Dispatch declared handler routes by resolving exact, parameter, wildcard, and
   fallback route patterns, then invoking `erl -noshell -pa _build/ebin` with
   the manifest target module/function and a small request map containing
   method, path, buffered body text, route params, raw query text, decoded
   query params, raw cookie header text, and parsed request cookies.
6. Start the reload watcher through the `watch.rs` boundary and broadcast
   reload events when that backend reports package changes.

The temporary handler ABI is intentionally narrow:

```erlang
{terlan_response, Status, ContentTypeBinary, BodyBinary}
```

The server converts that ABI to an HTTP response. BEAM-backed handlers remain
authorable as Terlan functions compiled to BEAM. Public `std.http.Request` and
`std.http.Response` stay Rust-native server/runtime values; later bridge work
should adapt native HTTP values to BEAM-callable handler functions without
exposing the tuple protocol as the user-facing HTTP model.

The bridge uses Rust-native request/response snapshots for the current adapter
boundary: request method/path are rendered into the BEAM request map, and parsed
BEAM response output flows through the same native response snapshot before the
HTTP writer receives it.

Important invariants:

- Manifest-relative paths must stay inside the web package root.
- Handler routes must be absolute URL paths, may use `:param` segments and a
  final `*` wildcard, and must not collide with the reload endpoint.
- Handler route precedence is exact, then parameter, then wildcard, then
  fallback.
- Captured handler params are UTF-8 percent-decoded with path semantics before
  they are passed to Terlan handlers.
- Handler query params are decoded with standard URL query semantics and passed
  alongside the raw query string in the temporary BEAM bridge request map.
- Handler body text is split from the buffered request after the HTTP header
  terminator and passed through the temporary BEAM bridge request map. Full
  streaming, multipart handling, and large body limits belong to the later
  production HTTP stack.
- Request cookies are split from the `Cookie` header into the handler request
  map. Dynamic handler response headers are validated and forwarded, so
  `Set-Cookie` can pass through the bridge. Public response cookie helpers now
  reuse `std.http.Cookies` validation before appending response headers; request
  jar mutation application still belongs to the later SafeNative resource
  bridge.
- Same-shape parameter routes for one method are ambiguous and fail package
  validation before serving.
- Handler targets must name a Terlan module, function, and arity explicitly.
- Handler, static-response, and file-response manifest entries may include
  project-relative `source` path, line, and column metadata. The server
  validates that metadata before serving. Dynamic-handler logs, static-route
  logs, file-route logs, and development error pages include source metadata
  when the selected manifest row provides it.
- Handler execution resolves BEAM modules through the sibling build `ebin`
  directory, so `_build/web` and `_build/ebin` stay part of one build root.
- Auto TLS projects reserve `/.well-known/acme-challenge/<token>` before
  static or handler routing. Challenge bodies are read from
  `.terlan/tls/acme/http-01/<token>` and token names must stay URL-safe.
- Auto TLS account credentials are cached at `.terlan/tls/acme/account.json`;
  issued certificate material is cached at `.terlan/tls/acme/fullchain.pem`
  and `.terlan/tls/acme/privkey.pem` before being loaded by `rustls`.
- Reload watch integration must stay behind `watch.rs`; HTTP request routing
  should not depend on whether the active backend is temporary polling, Oxc, or
  Rsbuild/Rspack.
- Handler failures, missing modules, and invalid return values must produce
  stable `error[serve_handler]` diagnostics in the HTTP response body.
- When a route manifest declares a router-level error handler, serve attempts
  that typed `HttpError -> Response` callback before falling back to the built
  in development error page.
- `--check` must never bind a network port.
- Adjacent Docker Compose validation checks only the project-owned Postgres
  development service contract.
- Normal `terlc serve` may run only
  `docker compose -f <project-compose> up -d postgres` for that validated
  dependency. It is not a generic Docker command wrapper.
- Missing or malformed packages must fail before serving starts.
- Live reload wiring must be local-dev-only server behavior and must not mutate
  the packaged files on disk.

## Integration Points

- `commands::build::js`: produces `_build/web` and the initial empty handler
  list in the web manifest.
- `main`: routes `terlc serve` to this module.
- Hyper plus `tokio::net`: provide the async local HTTP listener and protocol
  boundary.
- `logging`: owns local source-aware request logs and dev error page rendering.
- `response`: owns socket response bytes, content type selection, and reload
  response helpers.
- `handler::beam_eval`: owns the temporary BEAM process invocation expression
  and request map literal formatting.
- `handler::response_bridge`: owns the internal handler response wrapper and
  response-header safety validation shared by BEAM, native HTTP snapshots, and
  static manifest responses.

## File Layout

- `mod.rs`: serve command dispatch, package validation, listener setup, request
  routing, and Docker-aware local dependency startup.
- `compose_check.rs`: Docker Compose validation for project-owned development
  dependencies.
- `handler.rs`: manifest handler validation, manifest route lookup, BEAM
  handler invocation, and response ABI parsing.
- `handler/beam_eval.rs`: BEAM request-map and `erl -eval` rendering.
- `handler/response_bridge.rs`: handler response wrapper plus response header
  validation.
- `handler/route.rs`: route selection, precedence, captured params, and
  ambiguity validation.
- `handler/types.rs`: manifest handler/static/file/error response data shapes.
- `logging.rs`: request ids, local request logs, and development error pages.
- `manifest.rs`: web package manifest loading and validation data model.
- `response.rs`: HTTP response serialization and live-reload response helpers.
- `tls.rs`: `rustls` TLS runtime setup for manual/internal modes plus
  deterministic ACME account, challenge, certificate cache handling and
  HTTP-01 challenge serving for automatic certificate mode. Live issuance uses
  the maintained `instant-acme` flow and remains separate from request routing.
- `watch.rs`: live-reload watcher boundary.

## Edge Cases

- Missing manifests produce stable `error[serve_package]` diagnostics.
- Traversal-like paths are rejected before filesystem reads.
- Malformed handler routes or targets fail package validation.
- `HEAD` requests return headers without a response body.

## Types And Interfaces

`ServeArgs`
: Parsed command-local arguments.

`WebPackageManifest`
: Minimal deserialized subset of `_build/web/manifest.json`.

`WebPackageHandler`
: Manifest-declared dynamic route target dispatched through BEAM artifacts.

`MatchedWebPackageHandler`
: Request-selected handler plus captured route parameters.

`BeamHandlerResponse`
: Parsed response returned by the current BEAM handler ABI.

## Testing Notes

- `serve_test.rs` covers parser behavior, package validation, and HTTP response
  helpers.
- `manifest_test.rs` covers manifest-driven static routing.
- `handler_test.rs` covers dynamic route matching, route params, ambiguity
  checks, BEAM handler metadata, runner protocol helpers, BEAM eval request map
  formatting, and router error-handler eval formatting.
- `watch_test.rs` covers the reload watcher boundary and temporary polling
  snapshot behavior.
- Server socket behavior should stay thin so live reload and BEAM-backed
  handler routing can be added on the same Tokio boundary.
- Release preflight should use `terlc serve --check` before adding live server
  smoke tests.
- Rsbuild/Rspack remains hidden behind Terlan web packaging. Ordinary users
  configure web projects through `terlan.toml` and run `terlc serve`; direct
  Rsbuild configuration is reserved for a future advanced escape hatch.
