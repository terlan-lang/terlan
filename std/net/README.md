# Std Net Internals

This directory owns portable network data helpers. The current release surface
is URI parsing/formatting support.

## Responsibilities

- Define target-neutral networking data helpers.
- Keep parser implementation details behind portable result values.
- Avoid exposing target-specific URL or URI object models in core modules.
- Provide stable behavior for HTTP and cloud tooling integrations.

## Public Surface

- `std.net.Uri`: URI helper module.

## Core Model

Networking helpers are pure data operations unless a module explicitly owns IO.
URI behavior should therefore lower directly to target helper functions where
available instead of using process or bridge abstractions.

The main flow is:

1. Source calls a URI helper.
2. Type checking validates the function shape.
3. The backend delegates parsing or rendering to the selected target helper.

Important invariants:

- URI helpers do not perform network IO.
- Host URL object models must not leak into portable source APIs.
- Error shapes must be stable before widening the URI API.

## Integration Points

- `std.http`: can use URI helpers for request routing and parsing later.
- `terlan_safenative`: may own Rust-backed URI implementation.
- Web/cloud packaging: may use URI validation in manifests.

## Edge Cases

- Percent encoding, invalid UTF-8, and relative URI handling need explicit
  tests before broadening the public API.
- Target-specific URL normalization must not change portable semantics.

## Types And Interfaces

`Uri`
: Portable URI helper module.

## Testing Notes

- Positive tests should live beside the module as `*Test.terl` sources when
  the API becomes release-tested.
- Add negative tests for malformed URI input when typed errors are finalized.
- HTTP server manifest tests should cover route/URI interactions separately.
