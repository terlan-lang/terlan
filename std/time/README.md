# Time

`std.time` contains portable value types for representing durations and
instants without exposing a host runtime clock representation.

The public surface is intentionally backed by numeric milliseconds. That shape
is predictable in the Terlan VM and maps directly to JavaScript timestamp
values such as `DOMHighResTimeStamp`.

`Clock` delegates only host clock reads to `native/`, using Rust's standard
`SystemTime` and `Instant`. Calls use the generic copied-value package boundary,
not clock-specific compiler intrinsics or VM capability tags. They are
nonblocking but effectful: observations must not be cached or constant folded.
The monotonic origin is shared within one host process, not across processes.
Wall-clock errors before the Unix epoch and nanosecond overflow are reported
explicitly. Calendar and duration arithmetic remain in Terlan source.
