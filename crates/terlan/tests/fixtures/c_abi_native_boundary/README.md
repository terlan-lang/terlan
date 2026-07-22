# C ABI NativeBoundary generator fixture

This package-neutral C project proves Terlan's automatic C ABI binding lane.
Maintained Clang tooling supplies normalized declaration metadata; the compiler
does not parse the header.

```text
normalized Clang metadata
-> generated Terlan package and NativeBoundary manifest
-> generated Rust extern "C" declarations
-> generated safe Rust ownership adapter
-> cc C compilation and static linkage
-> opaque NativeBoundary handle ownership
-> stateful helper
-> executable Terlan package consumer
```

The fixture deliberately uses the same opaque-handle and status-code patterns
found in versioned native C APIs. It is not a PJRT, PyTorch, XLA, or OpenCV
implementation.
