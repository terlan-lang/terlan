# Std Data Internals

This directory owns portable data value modules. The 0.0.4 release surface is
centered on `std.data.Json`, which provides a target-neutral JSON value API
while delegating concrete parsing, rendering, and storage to Rust-native
SafeNative operations.

## Responsibilities

- Define portable data value types that application code can use across
  compiler targets.
- Keep backend parser and storage details behind opaque source-level types.
- Provide typed builders and accessors with stable `Result`-based failure
  shapes.
- Preserve a path for later trait-based struct encoding without widening the
  0.0.4 HTTP response API to arbitrary values.

## Public Surface

- `std.data.Json.Json`: opaque JSON value handle.
- `std.data.Json.JsonError`: portable JSON operation error.
- `std.data.Json`: JSON builders, parser, renderer, object lookup, array
  lookup, and typed scalar accessors.

## Core Model

The source language sees JSON as an opaque value with explicit builder and
accessor functions. The selected backend owns the actual JSON representation.
In 0.0.4, pure JSON operations lower to Rust-native SafeNative calls backed by
`serde_json`.

The main flow is:

1. Terlan source creates or parses a `Json` value.
2. The compiler resolves each operation to the stable `std.data.json.*`
   native operation id.
3. The backend returns either a portable value or a `JsonError`.

Important invariants:

- `Json` representation is never exposed in Terlan source.
- Accessor failures use `Result`, not nullable or unchecked host exceptions.
- JSON arrays use JSON-native `array` terminology.
- Generic struct-to-JSON conversion is deferred to a later encoding trait.

## Integration Points

- `std.http.Response`: accepts `Json` for explicit JSON responses.
- `std.http.Request`: parses request bodies into `Json`.
- `terlan_safenative`: owns Rust-native JSON operation implementations.
- `std/RUST_BACKED_MANIFEST.tsv`: records native operation ownership.

## Edge Cases

- Non-finite floating-point values must fail instead of silently producing
  invalid JSON.
- Object and array accessors fail when the receiver has the wrong JSON kind.
- Builder mutation is explicit through mutable receiver methods and returns
  `Unit`.

## Types And Interfaces

`Json`
: Opaque portable JSON value.

`JsonError`
: Portable error returned by JSON parsing, rendering, and accessor operations.

## Testing Notes

- Positive source tests live beside the module as `std/data/JsonTest.terl`.
- Release API coverage is recorded in `tests/std/RELEASE_API_TESTS.tsv`.
- Native artifact drift is checked by `make stdlib-check`.
