# JavaScript Emission Test Internals

This directory contains split integration tests for `terlc emit-js` behavior.

## Responsibilities

- Verify generated declaration files and their public type shapes.
- Cover deterministic output and rejected unsupported declarations.
- Keep test-only fixtures out of production emission modules.

## Testing Notes

Run the exact declaration tests and the JavaScript type-emission contract gate.
