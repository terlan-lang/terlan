# Std Log Internals

This directory owns portable source-level logging helpers. `std.log` is the
handler-facing logging surface used by application code instead of depending on
console output, BEAM logger calls, Rust tracing APIs, or JavaScript logging
objects directly.

## Responsibilities

- Provide stable log-level helper functions for Terlan source.
- Keep backend logging framework details outside application modules.
- Preserve a path from local console output to structured server/cloud logs.
- Avoid target-specific logger names in portable handlers.

## Public Surface

- `std.log.debug`: debug-level application message.
- `std.log.info`: info-level application message.
- `std.log.warn`: warning-level application message.
- `std.log.error`: error-level application message.

## Core Model

The first implementation delegates to `std.io.Console.println` so the API is
immediately executable on the existing backend path. Server runtimes can later
attach request id, route, handler, release/build id, and duration context
around the same source-level calls without changing user code.

Important invariants:

- Source code imports `std.log`, not a backend logging module.
- Logging helpers return `Unit`.
- Log message formatting remains ordinary Terlan source behavior.
- Backend-specific structured metadata is runtime context, not part of the
  first source-level function signature.

## Integration Points

- `std.io.Console`: current portable output fallback.
- `terlc serve`: owns local request logs and dev error pages.
- Future Terlan Cloud observability: can route log events with source/module
  metadata.

## Edge Cases

- Logging must not expose secrets or backend internals by default.
- Production server error reporting should keep stack traces and raw backend
  errors out of user-visible responses.
- Structured request metadata belongs to the runtime event envelope, not to
  every handler log call.

## Types And Interfaces

`std.log`
: Portable application logging module.

## Testing Notes

- Positive tests live beside the module as `std/log/LogTest.terl`.
- Release API coverage is recorded in `tests/std/RELEASE_API_TESTS.tsv`.
- Server request logging behavior is tested in `terlc serve` command tests.
