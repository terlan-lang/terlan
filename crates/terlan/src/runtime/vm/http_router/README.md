# VM HTTP Router Internals

This directory owns descriptor decoding, response materialization, and focused
router integration tests for the VM-owned HTTP router. The parent module owns
route selection and middleware continuations.

## Responsibilities

- Decode `std.http.Router` descriptors into validated VM route plans.
- Preserve root/scoped middleware order and typed short-circuit continuations.
- Convert handler results into finite, streaming, SSE, or WebSocket responses.
- Bind materialized SSE and WebSocket endpoint limits to admitted live-session
  state rather than reopening transport defaults.
- Reject malformed descriptors with stable typed diagnostics.
- Test middleware, live-channel, and response dispatch end to end.

## Integration Points

- `runtime::vm::http_router`: owns route tables and dispatch state.
- `std.http`: defines source-visible router and response descriptor contracts.
- `runtime::vm::http_static`, `sse`, and `websocket`: own transport-specific
  response state.

## Testing Notes

- Adjacent tests cover valid descriptors, malformed input, and route dispatch.
- `route_concurrency_test.rs` keeps concurrent routing smoke work explicitly
  bounded while exercising many routes and middleware selection.
- `live_session_activation_test.rs` proves source-materialized channel plans
  retain their queue, message-size, and keep-alive policies when admitted.
- Descriptor changes require both positive and adversarial cases.
