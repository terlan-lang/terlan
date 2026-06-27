# Serve Handler Internals

This directory owns development-server handler helpers. The implementation is
centered on routing requests to static assets or compiler-backed handlers. Its
most important boundary is that HTTP server plumbing stays separate from build
artifact discovery.

## Responsibilities

- Resolve dev-server requests to handler or static responses.
- Keep route diagnostics tied to build metadata.
- Avoid hand-rolled HTTP protocol behavior.

## Public Surface

- `mod.rs`: handler resolution used by the `serve` command.

## Core Model

The handler layer adapts compiler build outputs to the HTTP runtime used by the
development server.

The main flow is:

1. Receive an HTTP request shape from the server.
2. Match it against generated route/static metadata.
3. Return a response plan for the runtime to execute.

Important invariants:

- HTTP parsing belongs to the Rust HTTP stack, not this module.
- Static assets must not escape the configured output root.
- Missing handlers must return stable dev diagnostics.

## Integration Points

- `commands::serve`: owns command lifecycle and server startup.
- Build artifacts: provide route and static asset metadata.

## Testing Notes

- Add focused serve command tests when route matching behavior changes.
