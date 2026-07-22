# C++ Binding Generator Components

This directory contains focused implementation modules used by
`cpp_binding_generator.rs`. Package manifests and normalized Clang metadata
remain owned by the parent module so all generated artifacts share one
validated type model.

## Native Helper

`native_helper.rs` renders the process-local NativeBoundary helper used by a
generated C++ package. One generated `HandleValue` enum owns every opaque C++
resource as its exact `cxx::UniquePtr` type. Handle entries retain both a
generation and a fully qualified Terlan type name; operation dispatch validates
the wire type, stored type, generation, and expected operation type before
accessing a pointer.

Secondary opaque-resource arguments are limited to reviewed immutable
`const T&` parameters. Dispatch validates and immutably borrows each handle
independently, passes an ordinary CXX `&T`, and releases every borrow when the
call returns. Mutable receivers with secondary resources are rejected before
source generation to avoid identity-dependent aliasing.

The renderer derives dispatch from each module's structured functions. It does
not embed fixture names, duplicate the runtime request/response structs per
resource, or recover types from C++ source text. It also assembles ordinary
copied records from reviewed primitive getter projections. It also copies
owned standard-library string, byte-vector, integer-vector, and double-vector
results into the stable `ok_string`, `ok_bytes`, `ok_ints`, and `ok_floats`
protocol forms. Copied numeric-list inputs use typed `li:` and `lf:` payloads
and become call-scoped CXX slices; an untyped empty list is resolved against
the generated operation signature. The VM owns decoded results, and no C++
container or borrowed view survives the call.
Sibling `enum_adapter.rs` generates symbolic C++ enum conversions that keep
upstream integer discriminants out of public and runtime artifacts. Sibling
`exception_adapter.rs` generates `noexcept` catch-all wrappers and opaque
result envelopes for explicitly reviewed throwing methods. It never exposes
`std::exception::what()` or another upstream payload; only the package policy's
stable code and message can reach Terlan. A null envelope is reserved for an
adapter-allocation or containment failure and becomes a transport error rather
than an application `Result`.

## External Package Gate

`cpp_package_consumer_test.rs` invokes the public `terlc bind cpp`,
`terlc package fetch`, and `terlc run` commands. It commits the generated
package to a local immutable Git revision, resolves it from a separate
consumer, removes the original repository, and executes through build-recorded
native helper metadata. The test requires exact copied values, a complete
create/call/dispose lifecycle, stable stale-handle rejection, and a second C++
build after deleting the first isolated Cargo target.

## Constraints

- Keep generated protocol errors stable and value-based.
- Require at least one reviewed producer and exactly one owning-module disposer
  for every opaque resource.
- Reject unsupported helper argument and result types before writing output.
- Store heterogeneous resources in one tagged enum; do not erase them behind
  raw pointers or untyped boxes.
- Keep this module below 1,000 lines and split future conversion families into
  sibling modules.
