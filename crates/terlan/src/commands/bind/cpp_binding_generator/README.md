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
call returns. For mutable free functions, the helper copies each secondary
resource before borrowing the mutation target; repeated target/input handles
therefore remain valid without creating a Rust alias. Exception-contained
mutable methods use the same copy-before-mutable-borrow path; uncontained
mutable methods with secondary resources remain rejected before generation.
Lists of opaque resources use the same validation per element. The helper
decodes typed `lh:` payloads, copies each live C++ value into a generated
resource collector, and passes only that collector through `cxx`. Generated
C++ lowers its owned vector into an extracted `ArrayRef<T>` or `IListRef<T>`;
empty and repeated-handle lists need no package-authored adapter.
For a public leading resource plus remainder-list contract,
`prepend_resource: true` on the list makes argument alignment consume both
public values for one extracted resource-list parameter. Validation requires
matching immutable resource types, and helper generation copies the leading
value before the decoded list into the same compiler-owned collector.

Generated operation metadata distinguishes copied values, call-scoped borrowed
views, owned non-null handles, mutable handles, nullable C++ handle results,
disposal, and transfer. A nullable producer does not publish a null handle to
Terlan: it must declare a finite package-owned `null_failure` policy, and the
helper converts the null branch into that stable error before allocating a
handle token.
Exact `std::tuple<Resource, ...>` results use a compiler-generated opaque C++
carrier whose fields independently own each moved resource. Generated take
functions transfer the elements into their canonical handle variants, and the
helper emits the existing ordered tuple-handle protocol only after every field
has passed null and type validation.

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
Sibling `function_family.rs` expands homogeneous exact-symbol selections and
shared classification policies before validation. Public names and docs may
come directly from Clang metadata; normalized package evidence contains only
the expanded one-function and one-classification records.
Generated modules may also expose a reviewed ordinary Terlan composition with
`terlan_body` while keeping its exact native leaves `visibility: private`.
Source-composed functions never enter the CXX bridge or native-helper dispatch;
validation rejects C++ symbols, projections, and fallibility metadata on them.
Private native leaves retain full metadata and policy validation, render
without `pub`, and remain callable only inside their generated module. This
supports stable public selector APIs without generating or hand-writing a C++
switch wrapper.
Sibling modules may explicitly import one another's declared types. Validation
rejects unresolved, duplicate, and self-owned type imports; generated source
renders them as type-only imports while the helper keeps one package-wide typed
resource identity. Public primitive defaults are validated as safe trailing
Terlan literals and rendered on the original declaration.
Sibling `enum_adapter.rs` generates symbolic C++ enum conversions that keep
upstream integer discriminants out of public and runtime artifacts. Sibling
`string_adapter.rs` composes an extracted value getter with its reviewed
`std::string` method and copies the result through CXX. String-constructed
value inputs use the same manifest type without exposing the C++ record.
Raw immutable C++ string-view inputs use generated `rust::Str` parameters and
are reconstructed only for the duration of the selected call.
Primitive C++ optionals use an adjacent `has_<parameter>, <parameter>` public
pair when absence must remain observable. The generated adapter aligns that
pair to one extracted parameter and emits the `std::optional<T>`/`std::nullopt`
selection without package glue.
Opaque values accepted by `std::optional<Resource>`—including immutable
optional references—remain ordinary resource arguments in the public API;
generated C++ constructs the reviewed optional value. A public overload may
omit trailing optional parameters in an extracted signature. Alignment
uses exact parameter names to distinguish equal shapes and validated bridge
shapes for upstream spelling differences, then supplies `std::nullopt` without
publishing the omitted parameter through CXX. Compatibility APIs may retain an
integer enum representation only when the direct or optional C++ parameter
type is an exact extracted enum declaration; the generated adapter performs
the reviewed `static_cast` or optional construction without package glue.
The same explicit-absence path accepts a reviewed C++ optional whether or not
Clang recorded a declaration default: exact function-pointer calls always
materialize the omitted parameter as `std::nullopt` without inventing a C++
overload.
Compatibility-only positional values may be marked `cpp_ignore: true` while
every forwarded value explicitly names its extracted `cpp_parameter`. The
helper retains the public arity and validates the ignored value's Terlan type,
but omits it from CXX declarations and the exact upstream invocation. Validation
rejects ignored resources, mutation targets, copied records, field projections,
and arguments that also name a C++ parameter.
The narrow `positive_*`/`pos_*` spelling equivalence also disambiguates ATen
optional parameters without requiring a package-side adapter or renaming a
stable public argument.
An adjacent `value_is_int`, `value_int`, `value_float` public triple similarly
aligns to one extracted ATen/c10 `Scalar` or a present
`std::optional<Scalar>`; generated C++ constructs the selected integer or
floating value without a package-authored dispatch shim.
Sibling `collection_adapter.rs` immediately copies borrowed integer-array
views into owned C++ vectors before encoding ordinary Terlan lists. It also
generates exception-contained result classes for methods returning
`std::vector<Resource>` and for equivalent free functions, including
list-to-list calls: each element is copied to an independent CXX
`UniquePtr`, and the helper publishes no handle until every copy succeeds.
Sibling
`exception_adapter.rs` generates `noexcept` catch-all wrappers and opaque
result envelopes for explicitly reviewed throwing methods and inherited
primitive projections. It never exposes
`std::exception::what()` or another upstream payload; only the package policy's
stable code and message can reach Terlan. A null envelope is reserved for an
adapter-allocation or containment failure and becomes a transport error rather
than an application `Result`.
Sibling `mutation_adapter.rs` applies that envelope to throwing free functions
whose first parameter is the mutable opaque resource and to reviewed mutable
methods. Generated C++ accepts the target through `Pin<&mut T>`, lowers copied
collection, Scalar-choice, and secondary-resource inputs, discards a returned
receiver alias, and exposes `Unit` only after successful completion. A
logically-mutating C++ `const` method is admitted only when its extracted result
is a mutable receiver reference. The helper copies all resource inputs before
borrowing the mutable handle-table entry, preventing a repeated input handle
from forming a Rust alias.
Additional public resource arguments marked mutable map to exact non-const C++
references and become additional CXX `Pin<&mut T>` parameters. Dispatch rejects
repeated output handle IDs, temporarily removes every output from the handle
table for a safe simultaneous borrow, and reinserts them before interpreting
the exception envelope. Ordered tuples of retained output aliases may be
discarded for a public `Unit` result.

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
