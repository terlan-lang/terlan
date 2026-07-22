# VM HTTP Static Response Internals

This directory owns byte-range parsing and bounded response streaming for
VM-managed static and generated HTTP bodies. The parent module owns asset
metadata and route-indexed storage.

## Responsibilities

- Parse and validate HTTP byte ranges against response lengths.
- Split response bodies into bounded chunks with explicit backpressure.
- Encode HTTP/1 response heads and chunked stream parts in order.
- Preserve terminal stream state across completion, cancellation, and failure.

## Integration Points

- `runtime::vm::http_static`: supplies response bodies and error types.
- `runtime::vm::tcp`: owns queued writes and transport backpressure.
- `runtime::vm::http_router`: materializes handler response descriptors.

## Testing Notes

- Adjacent tests cover partial flushes, queue pressure, terminal races, and TCP
  failures.
- Range and stream changes require boundary and adversarial regressions.
