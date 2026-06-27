# API Command Internals

This directory owns `terlc api` command support.

## Responsibilities

- Extract compiler-owned API contracts from Terlan route declarations.
- Render API schema artifacts without making OpenAPI the source of truth.
- Keep route, handler, and schema diagnostics stable.

## Public Surface

- Command handlers invoked from the main CLI dispatcher.

## Integration Points

- `compiler::api_contract`: builds the typed API contract.
- `commands::build::js_browser::routes`: supplies route conventions used by
  web builds.

## Testing Notes

- Add command tests for generated contract shape and diagnostics.
- Keep OpenAPI conversion tests separate from route extraction tests.
