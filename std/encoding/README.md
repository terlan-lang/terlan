# Std Encoding Internals

This directory owns portable encoding and legacy integrity helpers. Backend
implementations are delegated to maintained Rust crates through NativeBoundary.

## Responsibilities

- Expose deterministic encoding/decoding APIs.
- Keep backend codec libraries behind portable source-level functions.
- Return typed errors or stable diagnostics for invalid input.
- Avoid hand-written codec implementations when mature host libraries exist.

## Public Surface

- `std.encoding.Base64`: Base64 encode/decode helpers.
- `std.encoding.Md5`: legacy MD5 integrity compatibility; never use it for a
  security decision.

## Core Model

Encoding modules are pure helper APIs. They should lower directly to native
functions where supported and should not require BEAM process wrappers or
long-lived native resources.

The main flow is:

1. Source calls an encoding helper.
2. Type checking validates input and output shapes.
3. The backend delegates to the selected native codec implementation.

Important invariants:

- Codec behavior must be deterministic.
- Invalid encoded input must not silently produce partial output.
- Encoding APIs remain target-neutral unless explicitly placed under a target
  namespace.

## Integration Points

- `terlan_native_boundary`: owns Rust-backed codec operations.
- `std/RUST_BACKED_MANIFEST.tsv`: records native operation ownership.
- HTTP and data modules may use encoding helpers later for protocol work.

## Edge Cases

- Invalid padding and invalid alphabet characters must return stable errors.
- Binary/text boundary rules must remain explicit as binary support expands.

## Types And Interfaces

`Base64`
: Portable Base64 helper module.

`Md5`
: Legacy deterministic integrity digest backed by RustCrypto.

## Testing Notes

- Positive tests should live beside the module as `*Test.terl` sources when
  the API becomes release-tested.
- Native artifact drift is checked by `make stdlib-check`.
- Add negative fixtures for malformed input when typed decoding errors land.
