# C++ NativeBoundary generator fixture

This is a package-neutral C++ project used to prove Terlan's automatic binding
boundary. It follows the same curated-wrapper model used by Python extension
projects: maintained C++ tooling supplies normalized symbol metadata, the
generator selects the safe public surface, and generated language adapters own
conversion and lifecycle policy.

The fixture proves this chain without parsing its header inside the compiler:

```text
normalized clang-libtooling metadata
-> versioned package mapping policy
-> generated Terlan package and NativeBoundary manifest
-> generated Rust cxx::bridge
-> cxx-build C++ compilation and linking
-> multiple opaque UniquePtr-backed resource types
-> one tagged stateful helper with per-type handle validation
-> executable Terlan package consumers
```

The two independent `NativeBoundary` and `NativeGauge` resources prove that
generation is not fixed to one concrete `UniquePtr` type or one operation set.
Each resource has its own constructor, immutable and mutable methods, disposer,
and live-count operation. Both generated Terlan modules execute through the
same helper process while retaining distinct type identities.
`NativeSnapshot` separately proves ordinary copied values. Its extractor-owned
C++ fields are mapped one-to-one into a generated Terlan struct. Both an
existing resource projection and a free factory returning a temporary
`std::unique_ptr<NativeSnapshot>` populate the record through reviewed const
getters, transport it as an `ok_record` value, and expose normal Terlan field
access. The temporary is dropped after projection and never receives handle
identity, a disposer, or a native lifetime visible to Terlan.

The same resource returns an owned string, byte vector, and integer vector.
The generated `cxx` bridge represents these as `UniquePtr<CxxString>` and
`UniquePtr<CxxVector<T>>`; the helper copies their concrete values into the
wire reply and drops each temporary. The full-cycle test compiles the C++,
checks exact helper payloads, and invokes all three operations from generated
Terlan source.

The fixture also accepts copied integer and floating-point lists. Generated
bridges lower `List[Int]` and `List[Float]` into call-scoped CXX slices, while
the helper resolves the protocol's untyped empty-list form from the declared
parameter type. Compile, link, helper, and external-consumer tests execute
nonempty and empty inputs so packages do not depend on fixture-only adapters.

`BoundaryMode` proves symbolic enum conversion. Its selected C++ values are
deliberately non-sequential (`7`, `41`, and `99`), but generated Terlan exposes
only the finite `Raw | Doubled | Offset` atom union. The generated C++ adapter
compares enumerator names and the helper returns `ok_atom doubled` for the full-cycle
fixture; no discriminant crosses the runtime boundary.
An additional upstream-only `Hidden = 123` value is deliberately omitted from
the public mapping. Returning it produces the stable `native_unknown_enum`
error instead of dynamically creating an atom.

`tripled_or_throw` proves exception containment. Positive input returns a
typed `Result` success through the generated opaque envelope. Negative input
throws a C++ exception containing a deliberately sensitive payload; the
generated `noexcept` wrapper catches it and returns only the reviewed
`boundary_operation_failed` code and public message. Full-cycle tests assert
that the original payload cannot enter the helper protocol or Terlan value.

The external-package gate generates this fixture into an independent Git
package, commits its native Cargo lockfile, fetches it into a separate Terlan
consumer cache, and deletes the source repository before execution. The
consumer validates copied values and normal disposal through `terlc run`, then
executes a second program that must fail with `stale_handle`. Deleting the
isolated native target and running again proves C++ compilation and linkage can
be reproduced from only the verified package cache.

The normalized fixture records target/language provenance and a one-based
source location for every declaration. Its rejected corpus covers every
stable `cpp.*` family required by the OpenCV roadmap, including an explicitly
modeled function-like macro that cannot be promoted to a typed callable ABI.
Generator tests also
rewrite this fixture into a second package-neutral namespace to prevent
fixture names from leaking into generated paths or opaque resource types.

The `cpp_metadata` section is extractor-owned and contains declaration facts.
The `mapping` section is package-owned and contains the reviewed disposition,
ownership, thread-safety, and rejection policy for every extracted symbol.
The generator rejects cross-contamination between these contracts as well as
missing, duplicate, or unknown mapping entries.

The extractor contract uses structured compile and type facts. It preserves
the target, language standard, include roots, defines, exact argument vector,
annotations, overload sets, canonical types, pointer/reference shape,
function-pointer status, template dependence, parameter direction, and default
expressions. Generator validation consumes those fields directly instead of
recovering safety facts from C++ type strings.

This fixture is not a PyTorch implementation or a compiler-local prototype of
one. The external `terlan-pytorch` package consumes the generic stable-C
generator from its own Git repository; the companion `torch.*` C++ fixture
only proves that generated package namespaces remain package-owned.
