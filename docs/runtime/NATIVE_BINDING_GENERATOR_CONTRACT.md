# Native Binding Generator Contract

The native binding generator exists to make curated native libraries usable from
Terlan without exposing arbitrary C or C++ directly to source code.

## Boundary Shape

The binding model is:

```text
Terlan package
-> NativeBoundary manifest
-> Rust adapter
-> cxx/bindgen/C ABI wrapper
-> native library
```

Bindings must start from a curated wrapper surface. This matches the practical
model used by Python libraries through CPython extensions, pybind11, Cython,
SWIG, or stable C ABI shims: the user-facing language surface is generated from
an adapter contract, not guessed from every native implementation detail.

The compiler-owned conformance fixture is deliberately package-neutral. It
proves that an ordinary C++ project can expose a curated, metadata-described
surface through a generated `cxx` package and a Terlan consumer. Real packages
remain external repositories; in particular, PyTorch support belongs to the
separate future `terlan-pytorch` repository and must not introduce `torch`
namespaces or LibTorch dependencies into the compiler.

## Supported Inputs

The generator may accept these input shapes:

- Rust adapter contract
- `cxx::bridge` module
- C header
- C ABI shim
- explicit binding manifest

Every accepted input must declare module identity, function identity, arity,
argument types, return type, blocking policy, resource policy, error mapping,
cleanup hooks, ownership transfer, and thread-affinity rules.

## Rejected Inputs

The generator must reject unsupported native shapes predictably:

- arbitrary C++ templates
- inheritance-heavy APIs
- overloads without explicit names
- exceptions crossing the boundary
- raw pointers crossing into Terlan
- inferred lifetime ownership
- guessed thread-affinity rules
- unchecked native handles
- untyped errors

## Generated Surface

The generator must produce:

- Terlan module signatures
- opaque/resource handle types
- ownership, transfer, cleanup, and stale-handle metadata
- primitive conversions
- string conversions
- enum conversions
- struct conversions
- vector conversions
- `Option` conversions
- `Result` conversions
- typed error values
- NativeBoundary metadata
- conformance tests through the Rust adapter
- generated documentation with maintainer overrides

Conformance tests must call through the Rust adapter. They must not mock the
native library as a substitute for validating the binding boundary.

## Non-Goals

The generator is not a C++ reflection system, a raw FFI escape hatch, or a way
to smuggle unchecked pointers into Terlan. Native code remains behind
NativeBoundary capability checks and typed resource handles.
