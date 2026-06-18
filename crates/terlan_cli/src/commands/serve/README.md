# Serve Command Internals

This directory owns `terlc serve`, the local HTTP entry point for packaged
browser artifacts. The implementation is centered on validating an existing
`_build/web` package manifest and serving files from that package root. Its most
important boundary is that callers may depend on stable CLI diagnostics and safe
artifact path handling, while HTTP implementation details stay internal.

## Responsibilities

- Parse `terlc serve` command-local arguments.
- Validate `_build/web/manifest.json` before serving.
- Serve packaged files with predictable MIME types and safe path handling.
- Validate manifest-declared dynamic handler routes before serving.
- Dispatch manifest-declared handler routes through generated BEAM artifacts
  without hard-coding application route behavior in the server.
- Reserve the local live-reload endpoint and inject reload wiring into HTML.
- Keep reload watching behind `watch.rs`; the current polling backend is a
  temporary compatibility implementation until the selected Oxc/Rsbuild/Rspack
  watch boundary is integrated.
- Provide a non-blocking `--check` mode for CI and release preflight.

## Public Surface

- `run`: executes `terlc serve`.
- `parse_serve_args`: normalizes command-local arguments into `ServeArgs`.
- `validate_web_package`: validates a packaged browser artifact.

## Core Model

The command treats `_build/web` as the release-facing browser package. The web
manifest is authoritative for the package schema, entry HTML file, asset files
copied by `terlc build --target js.browser`, and dynamic handler route metadata
that dispatches into BEAM-backed Terlan modules when sibling `_build/ebin`
artifacts are present.

The main flow is:

1. Parse host, port, optional package directory, and `--check`.
2. Validate the package manifest and referenced files.
3. In normal mode, bind a local HTTP listener and serve package files.
4. Route `/__terlan/reload` to the server-sent-events reload stream.
5. Dispatch declared handler routes by invoking `erl -noshell -pa _build/ebin`
   with the manifest target module/function and a small request map.
6. Start the reload watcher through the `watch.rs` boundary and broadcast
   reload events when that backend reports package changes.

The temporary 0.0.4 handler ABI is intentionally narrow:

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
- Handler routes must be absolute URL paths and must not collide with the
  reload endpoint.
- Handler targets must name a Terlan module, function, and arity explicitly.
- Handler execution resolves BEAM modules through the sibling build `ebin`
  directory, so `_build/web` and `_build/ebin` stay part of one build root.
- Reload watch integration must stay behind `watch.rs`; HTTP request routing
  should not depend on whether the active backend is temporary polling, Oxc, or
  Rsbuild/Rspack.
- Handler failures, missing modules, and invalid return values must produce
  stable `error[serve_handler]` diagnostics in the HTTP response body.
- `--check` must never bind a network port.
- Missing or malformed packages must fail before serving starts.
- Live reload wiring must be local-dev-only server behavior and must not mutate
  the packaged files on disk.

## Integration Points

- `commands::build::js`: produces `_build/web` and the initial empty handler
  list in the web manifest.
- `main`: routes `terlc serve` to this module.
- `tokio::net`: provides the async local HTTP listener implementation.

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

`BeamHandlerResponse`
: Parsed response returned by the current BEAM handler ABI.

## Testing Notes

- `serve_test.rs` covers parser behavior, package validation, and HTTP response
  helpers.
- `manifest_test.rs` covers manifest-driven static routing.
- `handler_test.rs` covers BEAM handler metadata and runner protocol helpers.
- `watch_test.rs` covers the reload watcher boundary and temporary polling
  snapshot behavior.
- Server socket behavior should stay thin so live reload and BEAM-backed
  handler routing can be added on the same Tokio boundary.
- Release preflight should use `terlc serve --check` before adding live server
  smoke tests.
- Rsbuild/Rspack remains hidden behind Terlan web packaging. Ordinary users
  configure web projects through `terlan.toml` and run `terlc serve`; direct
  Rsbuild configuration is reserved for a future advanced escape hatch.
