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
Mutable methods cannot borrow another opaque resource because that would make
aliasing dependent on runtime handle identity; generation rejects the shape
with `cpp.lifetime.mutable_alias`.

Selected C++ enums become finite Terlan atom unions. Maintained Clang metadata
retains named enumerators and exact discriminants for provenance, while package
policy assigns public variant names and stable atoms. A generated C++ adapter
compares named enumerators and returns only the reviewed atom string. Enum
arguments take the inverse path: the helper accepts a finite atom, rejects
unselected values, and lowers the atom to its extractor-recorded integer only
at the package-owned C++ wrapper call. C++ discriminants therefore remain
private to the adapter rather than becoming public Terlan integer codes.

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
