# Symbolic C++ Enum Adapters

This module generates C++ conversion functions for selected enum-returning
methods. The adapter compares the result against named C++ enumerators and
returns the reviewed Terlan atom spelling as an owned `std::string`.

Upstream integer discriminants remain extractor provenance. They are not
emitted into Terlan modules, helper requests, helper replies, or Rust bridge
types. Unselected or unknown enum values produce a null adapter result, which
the helper converts into a stable `native_unknown_enum` error.
