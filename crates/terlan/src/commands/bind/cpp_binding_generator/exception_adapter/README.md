# C++ Exception Containment

This module generates `noexcept` C++ wrappers for callables selected with an
explicit package-owned exception policy. A shared opaque envelope carries only
success data or the policy's stable error code and message.

The wrapper catches every exception, suppresses `what()` and unknown payloads,
and catches envelope-allocation failures before returning null. The Rust helper
turns successful envelopes into `Result` values, stable failures into
`std.core.Error.Error`, and null envelopes into a transport-level containment
failure. No C++ unwind may cross `cxx`.
