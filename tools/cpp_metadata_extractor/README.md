# Terlan C++ Metadata Extractor

This standalone tool is the maintained-Clang boundary for generic C++
bindings. It uses Clang LibTooling and AST Matchers to consume a compilation
database and emit `terlan.cpp.metadata.v1`. The Terlan compiler consumes that
JSON; it does not parse C++ source or compiler-specific AST dumps.

The ordinary repository gate validates the committed fixture result offline:

```sh
make cpp-binding-metadata-extractor-check
```

Live reproduction requires CMake, Clang/LLVM development packages, and an
explicit opt-in:

```sh
TERLAN_CPP_METADATA_LIVE=1 make cpp-binding-metadata-extractor-live-check
```

The tool reads compile commands through Clang's `CommonOptionsParser`. Package
policy is deliberately absent from its output: ownership, thread safety,
selection, and rejection decisions belong to `terlan.cpp.mapping.v1`.
Complete record declarations include normalized non-static field names and
structured type metadata so package policy can review copied value mappings
without reparsing a header.
Complete enum declarations include named enumerators and exact Clang-evaluated
discriminants. Package policy maps a reviewed subset to symbolic Terlan atoms;
the integer values remain provenance and are never part of the public binding.

The live gate normalizes only the producer version, which naturally varies by
installed Clang release. All declaration, type, source, annotation, overload,
and compile-command facts must match the committed result.
