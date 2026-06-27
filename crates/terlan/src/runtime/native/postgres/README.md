# Native Postgres Internals

This directory owns Postgres helper modules for the Rust-native adapter.

## Responsibilities

- Keep Postgres config and row decoding logic separate from pool execution.
- Use maintained Rust Postgres crates rather than hand-rolled wire protocol.
- Preserve typed error conversion for SafeNative dispatch.

## Public Surface

- `config`: Postgres connection configuration.
- `row`: typed row value accessors.

## Integration Points

- `runtime::native::postgres`: owns pool, query, transaction, and execution
  functions.
- `runtime::safenative::dispatch`: exposes Postgres operations to generated
  Terlan runtime calls.

## Testing Notes

- Add direct tests for config parsing and row decoding.
- Use container-backed tests for live database behavior.
