# Backend Internals

This directory owns target backends that turn checked Terlan compiler artifacts
into runtime-specific output.

## Responsibilities

- Keep backend emission behind explicit target modules.
- Preserve source-to-output traceability for diagnostics and debug maps.
- Keep backend-specific runtime assumptions out of parser and typechecker code.

## Public Surface

- `erlang`: BEAM/Erlang source and artifact emission.
- `wasm`: reserved WebAssembly backend ownership boundary.

## Integration Points

- `compiler::typeck`: supplies checked CoreIR and typed syntax artifacts.
- `commands::build`: selects the backend and writes release artifacts.

## Testing Notes

- Backend tests live beside the backend module that owns the emission rule.
- Add focused fixtures whenever a source form gains backend support.
