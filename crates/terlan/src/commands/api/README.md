# API Command Internals

This directory owns `terlc api` command support.

## Responsibilities

- Discover static route declarations for the std.http API contract.
- Render API schema artifacts without making OpenAPI the source of truth.
- Keep route, handler, and schema diagnostics stable.

## Public Surface

- Command handlers invoked from the main CLI dispatcher.

## Integration Points

- `terlan_http_native::api_contract`: owns route identity, deterministic
  ordering, and minimal OpenAPI projection.
- `source_contract`: CLI-only syntax discovery shared with deployment tooling.
  This adapter does not evaluate Router source or resolve computed routes;
  it requires literal paths and direct handler references.
- `commands::build::js_browser::routes`: supplies route conventions used by
  web builds.

## Testing Notes

- Add command tests for generated contract shape and diagnostics.
- Keep OpenAPI conversion tests separate from route extraction tests.
- Package tests cover serialization and projection independently of the compiler.
