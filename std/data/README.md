# Std Data Internals

This directory owns portable data value modules. `std.data.Json` provides a
target-neutral JSON API. Its maintained parser, encoder, storage adapter, and
remaining native projections live in the independent `native/` crate here,
not in the compiler's runtime implementation directory.

## Responsibilities

- Define portable data value types that application code can use across
  compiler targets.
- Keep backend parser and storage details behind opaque source-level types.
- Provide typed builders and accessors with stable `Result`-based failure
  shapes.
- Preserve a path for later trait-based struct encoding without widening the
  HTTP response API to arbitrary values.

## Public Surface

- `std.data.Json.Json`: opaque JSON value handle.
- `std.data.Json.JsonError`: portable JSON operation error.
- `std.data.Json`: JSON builders, parser, renderer, object lookup, array
  lookup, and typed scalar accessors.

## Core Model

The source language sees JSON as an opaque value with explicit builder and
accessor functions. The selected backend owns the actual JSON representation.
Parsing and storage still use resource-backed NativeBoundary calls backed by
`serde_json`. `string_fields` validates the receiver, selects optional strings,
and preserves requested order in ordinary Terlan source; it has no dedicated
native operation. Error accessors and report string quoting are also source
bodies.

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
- `native/`: owns serde-backed JSON behavior and its Rust tests. It depends on
  `serde_json` and the neutral runtime ABI, not the compiler or VM. Its operation
  contracts own arity and direct-versus-`Result` returns. Projection records use
  the shared one-to-one native record conversion, including nested lists and
  options; the compiler boundary no longer defines those record layouts.
- `native::invoke` and `native::mutate`: own operation selection, argument shape
  validation, and calls to the maintained adapter for all JSON operations.
  Mutations consume owned argument snapshots and return no new receiver handle.
- `native::RESOURCE_ADAPTER`: executes those callbacks through the shared
  owner-checked resource-store interface. Read operations borrow the stored JSON
  value; mutable calls snapshot source arguments before borrowing the receiver,
  preserving self-aliasing semantics. The same adapter runs with the neutral
  typed registry or the legacy mixed registry used by database callers.
- Default HTTP handlers execute JSON operations through actor-scoped native
  workers, using the shared resource transport also used by database workers.
  Package contracts declare resource types and return shapes. The host checks
  owner, generation, and type before translating source handles, preserves
  mutable receiver identity, and retires resources on actor exit or worker loss.
  Both serving schedulers use this path without trusted-host mode.
- Resource workers currently use one private process per live actor, with at
  most 16 owners per dispatcher. This is a bounded functional implementation,
  not a claim of high-concurrency performance or complete package ownership.
- `terlan_native_boundary`: still owns legacy JSON value/handle adaptation for
  in-process and database callers. The runtime module re-export is an interim
  caller facade; that compatibility path and more Terlan-owned projection
  policy remain to be migrated.
- `std/RUST_BACKED_MANIFEST.tsv`: records native operation ownership.

## Edge Cases

- Non-finite floating-point values must fail instead of silently producing
  invalid JSON.
- Object and array accessors fail when the receiver has the wrong JSON kind.
- Builder mutation is explicit through mutable receiver methods and returns
  the mutated `Json` for fluent calls.

## Types And Interfaces

`Json`
: Opaque portable JSON value.

`JsonError`
: Portable error returned by JSON parsing, rendering, and accessor operations.

## Testing Notes

- Positive source tests live beside the module as `std/data/JsonTest.terl`.
- Release API coverage is recorded in `tests/std/RELEASE_API_TESTS.tsv`.
- Native artifact drift is checked by `make stdlib-check`.
- `cargo test -p terlan-data-native` runs the package-owned native tests without
  building the compiler. The canonical Rust orchestration uses `--workspace`,
  so these tests remain part of the full suite.
- `JsonTest.terl` owns string-field ordering, duplicate names, empty strings,
  non-string values, and object validation with an empty field list.
- Package registration tests reject unknown and lookalike operation names and
  keep resource operations out of the value-only binding registry. Runtime
  integration tests compare contracts with parsed `Json.terl` signatures.
  Registering a resource contract does not grant worker admission or permission
  to transport a handle without ownership and lifetime checks.
- Invocation tests exercise every registered operation and corrupt each argument
  independently. Runtime bridge tests cover self-referential input snapshots,
  stable receiver handles, cross-owner and stale inputs, resource-kind confusion,
  and mutation failures that must not change the registry.
- Socket-level HTTP tests parse request bodies and preserve mutation aliases on
  the default worker path. Both-scheduler tests cover cancellation and repeated
  owner retirement beyond the worker-owner limit; projection tests reject
  forged handles and unexpected resource returns.
