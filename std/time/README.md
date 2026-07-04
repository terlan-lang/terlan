# Time

`std.time` contains portable value types for representing durations and
instants without exposing a host runtime clock representation.

The public surface is intentionally backed by numeric milliseconds. That shape
is predictable in the Terlan VM and maps directly to JavaScript timestamp
values such as `DOMHighResTimeStamp`.
