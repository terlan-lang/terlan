# Bind Command Context

This module owns `terlc bind`, the package-binding generator surface.

## Responsibilities

- Generate package-owned language and native adapter surfaces.
- Validate command-local arguments and structured metadata before writing any
  partial binding.
- Report stable diagnostics for unsupported native shapes and unavailable
  generator backends.

## Current Scope

The current binding command owns four deterministic generator surfaces:

```sh
terlc bind native --crate polars --out packages/std/native/polars
terlc bind js-dom --manifest std/js/manifests/std_js_dom_inputs.json --out generated/std-js
terlc bind cpp --manifest cpp-project/native-binding.json --out generated/cpp-project
terlc bind c --manifest c-project/native-binding.json --out generated/c-project
```

The manifest-backed C++ generator is the first general C++ package path.
It reads normalized symbol metadata from maintained C++ tooling using schema
`terlan.cpp.binding.v1` and package mapping policy schema
`terlan.cpp.mapping.v1`, then writes Terlan source modules, NativeBoundary
metadata, generated docs, a skipped-symbol snapshot, and a real Rust/C++
adapter under `native/rust`. Metadata records the producing frontend, target
triple, C++ language standard, and one-based declaration source locations.
Compile provenance also records package-relative include roots, preprocessor
defines, and the exact tokenized frontend arguments. Every declaration records
documentation, export annotations, stable overload-set identity, and
structured types containing declared/canonical spelling, constness, pointer
depth, reference kind, function-pointer status, and template dependence.
Parameters additionally preserve input/output direction and defaults.
Extracted symbols contain declaration facts only. The package-owned mapping
section supplies a unique bind/reject disposition for every symbol, opaque
resource ownership and thread-safety assertions, and reviewed rejection
families. Missing, duplicate, unknown, or metadata-leaking policy entries fail
before an output directory is created.
Homogeneous C++ declarations may use `function_families` and mapping
`symbol_families`. Shared public shape and reviewed policy are written once;
each member needs only its exact extracted symbol ID unless its public name or
documentation differs. The generator expands both shorthands into ordinary
per-function and per-symbol records before the existing validation pipeline,
and the normalized generated manifest retains that explicit expanded evidence.
Generated modules may declare `type_imports` for opaque or copied types owned by
a sibling module. Imports must resolve uniquely inside the same manifest and
cannot name the importing module; emitted Terlan uses `import type` while the
native helper retains one canonical resource identity and handle table for the
whole package.
The binding manifest separately owns a structured adapter build plan. It
declares generated-adapter include roots, preprocessor definitions, library
search paths, typed static/dynamic/framework links, target OS/architecture/
environment conditions, and rebuild inputs. Paths must remain below the
generated adapter root, target selectors and library names use restricted
alphabets, duplicate conditions are rejected, and newlines are forbidden in
Cargo directive values. The generator translates this data directly into
`cxx_build::Build` calls and Cargo directives; it never constructs a shell
command.
The adapter contains a generated `cxx::bridge`, a `cxx-build` build script,
copied declared C++ inputs, and executable Terlan consumer tests. Its helper
stores all package resource types in one generated tagged enum, validates both
wire and stored type identities before access, and dispatches each operation
to its declared `UniquePtr` type. Every resource requires at least one reviewed
producer anywhere in the package and exactly one disposer in its owning module.
The generated `terlan.toml` records the public package namespace, library
artifact kind, and `[native.rust]` helper contract. Git package resolution
therefore carries the helper's package directory and isolated native target
directory into normal build metadata. `terlc run` can build the C++ adapter
from the immutable package cache and install its helper environment without a
compiler-checkout path or a manually supplied helper variable.

Opaque C++ records may be returned by value. The generator emits a catch-all
`noexcept` C++ adapter that moves the value into `std::unique_ptr`, declares it
in the generated `cxx::bridge`, and routes the helper through that generated
function. Free factories and immutable resource methods use the same rule.
Flat structural returns whose exact C++ type is `std::tuple<Resource, ...>`
use a generated opaque carrier. The carrier independently owns every moved
tuple element, generated take functions transfer each element through CXX, and
the helper publishes the ordered Terlan handle tuple only after every element
has been recovered successfully.
When a selected declaration belongs to an overload set, the adapter emits an
exact typed `static_cast` and supplies omitted reviewed trailing defaults, so
package authors do not write overload-disambiguation shims. Integer
`ArrayRef`-style parameters lower from a CXX integer slice
by constructing the call-scoped upstream view from `(data, size)`. Optional
integer ArrayRefs use the same bridge-safe slice and are constructed inside the
generated C++ call. Trailing
parameters with extracted C++ defaults may be omitted from the public Terlan
operation. Cross-namespace opaque records receive per-type CXX namespace
annotations.
Public function arguments may independently declare Terlan defaults. The
generator accepts only validated finite `Int`, `Float`, `Bool`, JSON string, or
`List[Int]` literals and requires every defaulted parameter to be trailing. It
emits the default on the original public function, so package authors do not
create separate convenience aliases or handwritten forwarding wrappers.
Exact overload calls normalize Clang's declaration-oriented braced-default
spelling before placing it in a call expression.
ATen/c10 `Scalar` inputs lower from public `Int` or `Float` arguments inside
the same generated adapter, preserving the exact extracted overload signature.
A public `value_is_int: Bool, value_int: Int, value_float: Float` triple may
also map to one extracted `Scalar` parameter. Generated C++ selects and
constructs the exact integer or floating Scalar without a package-authored
tag-dispatch wrapper.
Reviewed finite enums can lower into `std::optional<CppEnum>` parameters: the
bridge transports an integer privately, and generated C++ constructs the exact
optional enum type. Omitting a trailing optional still uses its extracted C++
default, so package authors need no optional-value shim.
Reviewed public overloads may also omit an extracted required
`std::optional<T>` parameter. The generated exact call supplies
`std::nullopt` at that position, including between mapped parameters, rather
than requiring an author-written forwarding overload.
Primitive `std::optional<T>` inputs also recognize the conventional adjacent
public pair `has_<parameter>: Bool, <parameter>: T`. Generated CXX transports
both scalars and constructs either the exact optional value or `std::nullopt`;
the convention composes with free, owned-result, and mutable-output adapters.
Copied C++ value records declared as `string_value` lower from public Terlan
`String` arguments by constructing the exact value inside generated C++,
including `std::optional<Value>` parameters. A reviewed `string_projection`
pairs an extracted resource getter with an extracted zero-argument
`std::string` method; the generator contains both calls and copies the result
back to Terlan without an author-written value or ABI wrapper.
Plain extracted `std::string_view`/`c10::string_view` inputs lower from public
`String` through generated `rust::Str` parameters and call-scoped upstream
views. A required public `String` can likewise supply a present extracted
`std::optional<string_view>` parameter; generated C++ constructs both the
call-scoped view and its optional wrapper, so package authors do not write a
string-view conversion shim.
Concrete function-template arguments remain extractor-owned metadata and are
rendered into generated contained calls, allowing an exact specialization such
as `Scalar::to<long>()` without an author-written dispatch wrapper.

Build plans may declare environment-rooted external C++ SDKs with relative
include roots, library search paths, typed linked libraries, and rebuild
inputs. Generated packages therefore resolve binary distributions such as
LibTorch at build time without copying them or embedding a machine-local path.

The same manifest can map extractor-owned C++ record fields into ordinary
copied Terlan structs. A `value_projection` operation names one reviewed
zero-argument getter per field; the helper copies those primitive results into
the `ok_record` protocol, and the VM reconstructs `ReplValue::Record` rather
than allocating a native handle. Each copied struct receives an exported
snake-case constructor so external modules do not bypass Terlan's struct
construction boundary. A function argument may declare `fields` that map each
reviewed record field to the next named scalar C++ parameter. Generation
requires a complete, unique, ordered, type-compatible mapping and the helper
checks the record identity and every `Int`, `Float`, or `Bool` field before
entering C++. An `owned_value_projection` supports a reviewed free function
returning `std::unique_ptr<Record>`. The generated helper checks the temporary
for null, invokes one complete reviewed primitive getter set, emits the
ordinary record, and drops the temporary without allocating a handle. The
returned record may be declared by another module in the same generated
package.
The executable copied-result surface also
maps owned `std::string`, `std::vector<std::uint8_t>`, and
`std::vector<std::int64_t>`, and `std::vector<double>` values into ordinary
`String`, `Bytes`, `List[Int]`, and `List[Float]` values. These C++ results must
use `std::unique_ptr`; the helper
rejects null results, copies their contents into the response protocol, and
drops the native container before returning control to Terlan.
An `int_list_projection` handles borrowed `IntArrayRef`-style getter results:
generated C++ copies the view into `std::vector<std::int64_t>` before the
getter's owner can change, and CXX then carries that owned vector through the
existing `List[Int]` result path. Inherited getters require Clang-proven public
base conversion just like enum and integer projections.
A `resource_list_projection` selects a const method returning
`std::vector<Resource>` and exposes `List[Resource]` without an author-written
wrapper. The selected callable may also be a free function, including one that
simultaneously consumes a generated resource-list collector. Generated C++
owns the vector, contains reviewed exceptions, and
copies each element into a `std::unique_ptr<Resource>`. The generated helper
collects all non-null values before allocating any public handle, so a failed
element copy cannot publish a partial list. Reviewed trailing C++ defaults may
be omitted from the public function exactly as for owned-value adapters.
Public `List[Resource]` inputs map to extracted immutable
`ArrayRef<Resource>` and `IListRef<Resource>` parameters. The VM transports a
typed handle list; the helper validates every owner, type, and generation and
copies each C++ value into one generated collector. That collector alone
crosses `cxx`, and generated C++ supplies its owned vector to the upstream
call. Empty and repeated lists are supported, no handle-table borrow escapes
the call, and package authors write neither a collector nor a list shim.
When an existing public contract separates the first required resource from a
possibly empty remainder list, the list argument may declare
`prepend_resource: true`. The generator validates an immediately preceding
matching opaque resource, copies it into the same collector before the list
elements, and still exposes exactly one `ArrayRef<Resource>` or
`IListRef<Resource>` parameter to C++. This preserves nonempty public contracts
without a package-authored forwarding wrapper.
Copied `Bytes`, `List[Int]`, and `List[Float]` arguments lower to
`rust::Slice<const std::uint8_t>`, `rust::Slice<const std::int64_t>`, and
`rust::Slice<const double>` respectively. These slices remain borrowed only
for the duration of the C++ call. Primitive `Float` and `Bool` arguments and
results use explicit protocol variants. Methods and free
functions may return any package-owned opaque resource, including a resource
declared by another generated module; the helper resolves the canonical owner,
stores the returned `UniquePtr` in that owner's handle variant, and preserves
one type identity across module boundaries. Such an operation satisfies the
returned resource's producer requirement without requiring an artificial
same-module constructor.

An immutable method or free function may also accept a reviewed package-owned
opaque resource through a `const T&` C++ parameter. The public argument names
the resource type, CXX receives `&T`, and the helper independently validates
the secondary handle's owner, type, and generation before borrowing it only
for that call. Passing the receiver itself is valid for immutable operations.
Mutable methods without generated containment cannot borrow another opaque
resource because that would make aliasing dependent on runtime handle identity;
generation rejects the shape with `cpp.lifetime.mutable_alias`. A reviewed
mutable free function or exception-contained mutable method can accept
secondary `const T&` resources: the helper validates and copies them through a
generated CXX value adapter before borrowing the target, including when an
input handle equals the output handle. A contained method whose C++ receiver is
`const` may still declare public mutation when its extracted result is a mutable
receiver alias; the adapter strengthens only its generated receiver parameter
to preserve CXX `Pin<&mut T>` and handle-table exclusivity.
Mutable free functions may retain more than one package resource when every
additional output is explicitly marked mutable and maps to an exact non-const
C++ reference. The helper validates distinct output handle IDs, copies all
read-only inputs first, temporarily removes every output from the handle table,
passes simultaneous `Pin<&mut T>` values through generated CXX, and reinserts
all outputs before decoding success or failure. Exact tuples of output aliases
may be discarded for public `Unit` results.

Selected C++ enums become finite Terlan atom unions. Maintained Clang metadata
retains named enumerators and exact discriminants for provenance, while package
policy assigns public variant names and stable atoms. A generated C++ adapter
compares named enumerators and returns only the reviewed atom string. Enum
arguments take the inverse path: the helper accepts a finite atom, rejects
unselected values, and lowers the atom to its extractor-recorded integer only
at the package-owned C++ wrapper call. C++ discriminants therefore remain
private to the adapter rather than becoming public Terlan integer codes.
The generated public union is written directly in terms of atom literals, with
named singleton aliases retained for ergonomics, so native calls transport the
enum as a scalar rather than an actor-owned managed allocation. Enum getters
declared on an extracted base class may be projected from an opaque derived
resource when Clang metadata proves the inheritance relation.
Throwing zero-argument primitive getters use `scalar_projection` to reuse the
same generated containment envelope while exposing an ordinary public `Int`,
`Float`, or `Bool`. This permits inherited metadata such as rank,
element-count, or device predicates only
when Clang proves the concrete resource's public derived-to-base conversion.
Trailing C++ parameters may remain absent from the public Terlan signature
when every omitted parameter has an extracted default; the generated adapter
invokes the method without those arguments and lets the C++ declaration own
their exact semantics.

Selected throwing C++ methods require an explicit package-owned exception
policy. The policy defines a stable lowercase error code and public one-line
message. A generated `noexcept` C++ adapter catches every exception before it
can cross `cxx`, suppresses upstream exception payloads, and returns an opaque
success/error envelope. The helper decodes that envelope into
`Result[T, std.core.Error.Error]`; failure to allocate an envelope remains a
separate native transport error. Throwing symbols without this complete policy
are rejected before generation.

Unsupported pointers, unreviewed borrowed lifetimes, templates, uncontained exception
crossings, overloads, callbacks, variadics, inheritance, unknown ownership,
and unmapped types receive stable `cpp.*` rejection families and are never
emitted as partial bindings.

The checked `cpp_native_boundary` fixture is package-neutral and models the
curated-wrapper approach used by Python extension projects. Real packages are
external consumers of this generator. PyTorch bindings live in the separate
`terlan-pytorch` Git repository, not in the compiler.

The manifest-backed C ABI generator consumes schema
`terlan.c-abi.binding.v1` with normalized declaration metadata schema
`terlan.c.metadata.v1`. It writes raw Rust `extern "C"` declarations, a safe
owned-handle adapter, NativeBoundary metadata, stable skipped-symbol output,
and an executable Terlan consumer. Bundled inputs use a `cc` build script;
external distributions instead declare a root environment variable, library
search paths, dynamic libraries, and runtime search paths. Normalized C aliases
are resolved before Rust FFI emission, including aliases for status values and
opaque pointer handles. Generated adapter crates opt out of enclosing Cargo
workspaces so package repositories remain independently buildable. Producers
identify metadata as either normalized Clang LibTooling output or a reviewed,
curated declaration snapshot; curated inputs cannot claim the Clang format.
Declared `.c` sources compile as C11 while `.cc`, `.cpp`, and `.cxx` adapters
compile separately as C++17, allowing a package to contain its C++ API behind a
metadata-described C ABI without teaching the compiler about that library.
Packages may also declare one package-owned Rust extension source with exact
stable dependency versions. The generator copies and re-exports that source so
reviewed unsafe integration details remain inside the generated NativeBoundary
crate instead of leaking into the safe VM runtime.
Direct borrowed `const char *` inputs map from Terlan `String` through a
call-scoped `CString`; interior NUL bytes are rejected before native execution.
Each manifest-declared opaque C resource receives its own generated Rust owner,
producer validation, and exactly one typed destructor. The native helper stores
all such owners in a tagged enum and checks wire identity, stored identity, and
generation before a call-scoped borrow. Immutable operations may therefore
borrow one opaque resource as receiver and another as an argument, returning
any declared owned resource without exposing or confusing their C pointers.
Pointers are admitted only when metadata supplies direction and ownership;
borrowed integer arrays additionally require an opaque owner, a reviewed
length-symbol reference, and an immediate-copy policy. Generated Rust validates
the length and copies the array into owned memory before the owner borrow ends.
Reviewed dispatcher bindings describe every StableIValue stack slot explicitly.
Required opaque handles use owned duplicated handles; present optional handles
use `owned_optional_handle_copy`, which allocates validated StableIValue backing
storage before transferring any nested handle ownership. Integer optionals use
the same allocator/destructor contract. Owned integer lists use separately
validated allocate/push/delete symbols, populate elements under an armed cleanup
guard, and transfer the finished list into the dispatcher stack. This lowering
is schema-generic and contains no package or operator names. Fixed schema string
values use `owned_string_literal` with separately validated allocate/delete
symbols; generated Rust copies the metadata bytes under an armed guard and
transfers the owned string only when the complete stack is dispatched.
Other borrowed results, missing destructors, callbacks, variadics, unsupported unions,
unversioned ABI structures, and thread-local errors are rejected. The checked
compiler fixture remains package-neutral; PyTorch-specific metadata and its
ABI-contract fixture are owned by `terlan-pytorch`.

The Rust implementation generates the curated Polars package skeleton only. It
writes deterministic templates for the manifest, Terlan DataFrame module,
`.typi` interface summary, Rust crate mapping metadata, native ABI metadata,
Rust adapter `Cargo.toml`, and Rust adapter ABI stub with local smoke tests. It
does not inspect the upstream crate or produce broad bindings yet.

The TypeScript DOM implementation reads a pinned input manifest, validates
committed `.d.ts` hashes, parses declarations through Oxc, maps supported
interfaces into `std.js.Dom.*` module plans, and writes deterministic `.terl`,
`.typi`, and generated binding manifest files. It does not use npm
resolution, Node package lookup, or the network during normal generation.

## Boundaries

- Do not fetch Cargo metadata from the network.
- Do not inspect Rust crate sources.
- Do not parse C or C++ source in the compiler. Consume normalized metadata
  from maintained tooling and copy only explicitly declared build inputs.
- Do not generate cache `.deps` summaries until interface dependency hashing is
  wired into the binding pipeline.
- Do not link the real `polars` crate until the DataFrame native smoke wrapper
  slice opens.
- Do not add real third-party native libraries to the compiler workspace;
  package repositories consume generated adapters externally.
- Do not silently approximate complex TypeScript unions; record a stable
  skipped-declaration reason instead.
- Do not resolve TypeScript packages dynamically during normal generation; use
  pinned manifests and committed input hashes.
