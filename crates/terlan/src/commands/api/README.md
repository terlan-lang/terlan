# API Command Internals

This directory owns `terlc api` command support.

## Responsibilities

- Evaluate source route declarations for the std.http API contract.
- Render API schema artifacts without making OpenAPI the source of truth.
- Keep route, handler, and schema diagnostics stable.

## Public Surface

- Command handlers invoked from the main CLI dispatcher.

## Integration Points

- `terlan_http_native::api_contract`: owns route identity, deterministic
  ordering, and minimal OpenAPI projection.
- `evaluated_contract`: shared source compilation and execution for API and
  semantic deployment plans. Computed paths, delegated groups, and captured
  handlers use ordinary Terlan semantics. The HTTP package validates returned
  descriptors and owns schema projection. This executes trusted application
  code, not sandboxed inspection; external service waits are rejected.
- `commands::build::js_browser::routes`: supplies route conventions used by
  web builds.

## Testing Notes

- Add command tests for generated contract shape and diagnostics.
- Keep OpenAPI conversion tests separate from route extraction tests.
- Package tests cover serialization and projection independently of the compiler.
