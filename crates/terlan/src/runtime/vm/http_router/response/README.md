# VM HTTP Response Descriptor Internals

This directory owns specialized response-descriptor decoders used by the VM
HTTP router. It keeps stream validation separate from general finite-response
decoding.

## Responsibilities

- Decode `std.http.Response.stream` values into bounded VM stream metadata.
- Validate chunk and queue limits before transport state is allocated.
- Surface invalid descriptors as `VmHttpStaticError` values.

## Integration Points

- `runtime::vm::http_router::response`: dispatches response kinds.
- `runtime::vm::http_static`: consumes validated streaming limits.

## Testing Notes

- Router stream tests cover valid descriptors, zero limits, malformed values,
  and transport materialization.
