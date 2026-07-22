# Managed HTTP Boundary

This module owns the direct-AOT representation of portable HTTP values. Public
Terlan modules define the source API; this compiler module selects closed
managed layouts and operations; the native-image runtime executes those
operations against actor-owned memory.

## Ownership Inventory

| Surface | Compiler owner | Runtime owner | Executable evidence |
| --- | --- | --- | --- |
| Request and response values | `http_values.rs`, `layout.rs`, `constructors.rs`, `receiver.rs`, `option_string.rs` | `managed/operation_abi.rs` | `make tvm-aot-http-response-mutation-check` |
| Cookie jars and response cookies | `cookies.rs`, `layout.rs` | `managed/operation_abi/http.rs` | `make tvm-aot-http-typed-metadata-check` |
| Typed `HttpError` values | `error.rs`, `layout.rs`, `constructors.rs` | aggregate operations in `managed/operation_abi.rs` | `make tvm-aot-http-managed-error-check` |
| `Template.Html` and render plans | sibling `template_values.rs` and `template_values/render.rs` | `managed/operation_abi/template.rs` | `make tvm-aot-http-template-expression-check` |
| `Request.body_json()` | `body_json.rs` | `managed/operation_abi/json.rs` | `make tvm-aot-http-body-json-check` |
| Session state and lifecycle | `session.rs`, `layout.rs` | `managed/operation_abi/session.rs` and `runtime/vm/http_session.rs` | `make tvm-aot-http-session-check` |

`make tvm-aot-http-managed-boundary-check` is the aggregate closure gate for
this inventory. It executes the complete inherited gate chain and verifies the
exact aggregate and collection metadata admitted by a module that imports all
managed HTTP surfaces.

## Boundary Rules

- Public HTTP values never contain host pointers, JSON handles, cookie handles,
  or session-store handles.
- The compiler emits every aggregate layout, collection schema, constructor,
  and operation descriptor required by the admitted source imports.
- Managed operations validate their encoded contract before reading or writing
  actor-owned memory.
- Cookie serialization is delegated to the maintained Rust cookie adapter.
- Session storage is shared by an admitted image but request execution remains
  isolated in independent actor heaps.
- Transport adaptation may decode and encode wire data, but it cannot become
  the semantic owner of HTTP values.

Asynchronous handler I/O, WebSocket, and SSE continuation orchestration are not
part of this value boundary. They remain in AOT-5E.
