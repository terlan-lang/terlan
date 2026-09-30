# Managed HTTP Boundary

This module retains session-specific lowering.
Request, Response, Session, cookie jars, and middleware results use ordinary source-owned
records or unions. Session storage and the remaining specialization still need
migration; this directory is not the semantic owner of HTTP values.

String concatenation uses shared typed expression lowering. Literal-prefix fusion
and concatenation flattening apply equally with or without HTTP imports; there
are no HTTP-private string operations or duplicate string literal decoders.

## Ownership Inventory

| Surface | Compiler owner | Runtime owner | Executable evidence |
| --- | --- | --- | --- |
| Request accessors | Ordinary source linking of `std/http/Request.terl`; generic struct and map lowering | generic aggregate and collection operations | `http_request_library_test`; `vm_stream_request_executes_dynamic_handler_without_hyper` |
| Response values | Ordinary source linking of `std/http/Response.terl` | generic records and lists; named-field transport admission | `http_response_library_test`; `response_policy_test`; `source_response::tests` |
| Request/session option matching and defaults | Ordinary pattern lowering and `std/core/Option.terl` source linking | generic variant tests and field projections | `option_authority_test`; `session_provider_test` |
| Middleware result variants | Ordinary source declarations in `std/http/Router.terl`; generic union and constructor lowering | generic aggregate materialization | `middleware_authority_test`; `middleware_value_test` |
| Cookie jars and response attachment | Ordinary source linking of `std/http/Cookies.terl` and `std/http/Response.terl` | generic aggregates, maps, lists, and package cookie codecs | `http_cookie_library_test`; `response_cookie_test` |
| Response cookie composition | Ordinary source linking of `std/http/Response.terl` | package-owned cookie codecs, generic capability suspension | `http_cookie_library_test`; `response_cookie_test` |
| Typed `HttpError` values | Ordinary source linking of `std/http/Error.terl`; generic struct lowering | generic aggregate operations | `make tvm-aot-http-managed-error-check` |
| `Template.Html` and fragment helpers | Ordinary opaque-alias expansion and `std/template/Template.terl` source linking | generic strings and collections | `template_library_test`; HTTP typed-template integration tests |
| Checked template render plans | `template_values/render.rs` | `managed/operation_abi/template.rs` | `make tvm-aot-http-template-expression-check` |
| `Request.body_json()` | Ordinary source composition of `Json.parse` and `Error.new` | package-owned JSON resources through generic capabilities | `hyper_server_json_body_test` (isolated-worker verification still outstanding here) |
| Session state and lifecycle | explicit native declarations in `session.rs`, `layout.rs`; no source-call name interception | `managed/operation_abi/session.rs` and `runtime/vm/http_session.rs` | `session_authority_test`; `session_provider_test`; `make tvm-aot-http-session-check` |

`make tvm-aot-http-managed-boundary-check` is the aggregate closure gate for
this inventory. It executes the complete inherited gate chain and verifies the
exact aggregate and collection metadata required by native declarations.
Response and Router imports alone install no layouts.

## Boundary Rules

`HttpError` constructors and accessors execute their Terlan bodies. Their field
order and inherited fields come from source declarations, not an HTTP-specific
compiler descriptor. Request accessors likewise execute source bodies; the
native Request operation names and opaque handle are no longer admitted.
HTTP imports do not replace `Option.with_default` or rewrite map/session option
matches. Literal payloads, guards, ordered fallthrough, and empty versus missing
values use the same pattern machinery as other source values. The separate
HTTP option matcher and session `None` predicate opcode have been removed.
Router imports do not rewrite `Continue` or install fixed `Respond` constructors.
Middleware result identity, tags, payload fields, and variant counts come from
source through the ordinary structural union registry. Provider tests change
the tags and payload arity and add a third variant under both standard and
application namespaces. The actual Router declarations are also compiled and
executed through pattern matching and handler-result dispatch. Route builder
planning and dispatch remain specialized; this is not complete Router ownership.
Generic case-return lowering preserves the declared union representation for
atom-only and payload variants, including returned callbacks. Dynamic calls
retain checked parameter types when materializing union arguments. These paths
share list-union fallback narrowing and do not contain middleware-name rules.
The other specialized surfaces above still require migration. Passing local
composition tests is not evidence of complete ownership or live-worker coverage.

Response cookie helpers are no longer compiler substitutions. Their omitted
arguments use ordinary provider-owned receiver adapters; mutable statement calls
retain checked mutability in CoreIR. The obsolete managed full-options cookie
opcode and native `set_cookie_header` declaration are removed. Status/header
updates and every builder now execute source record/list operations. There are
no Response constructor tables, special type identities, native operations, or
HTTP allocation opcodes. Source defaults, repeated headers, and redirects are
owned by Response.terl. The transport reads named fields, validates status and
headers using maintained HTTP types, and retains file-safety and stream-limit
checks. The VM returns an ordinary record rather than a special HTTP envelope.
Historical opcodes reject before heap access; legacy native operation names are
also rejected without changing resources.
The trusted-HTML overload is source composition through `Template.to_string`;
fragment helpers no longer have call-name substitutions or a builtin Html ABI.
The source cookie-header setter delegates to the ordinary header method. Retired
opcodes are rejected. Jar mutation and replay execute source bodies; only header
serialization crosses the package-native boundary. Session ingress selects its
cookie in Terlan and passes an optional string to the native lookup. Legacy
request/jar layouts and their dedicated collection schemas have been removed.
Response attachment is ordinary `Session.with_response` source composition.
The session record and receiver threading are source-owned as well. The compiler
does not install a Session layout or recognize its type name specially. Remaining
state primitives receive string identities; lookup returns a String/Bool pair,
rotation returns a String, and liveness returns a Bool. The version-2 ABI rejects version-1 descriptors and
the retired native `live_identity` operation. Only Option[String] and lookup-pair
layouts remain specialized here. Source derives pending replacement identities
from the atomic lookup creation flag, and owns cookie omission, attributes,
and expired-session deletion. The native operations never edit responses or
construct pending cookie metadata.
`session_cookie_policy_test` verifies that this path suspends and resumes with
ordered headers, and propagates codec failure. The old response-cookie policy
opcode is rejected, as are the old lookup/rotation/snapshot opcodes that carried
serialized headers. Lookup, restore, rotation, and migration now return pending
identities without constructing HTTP metadata. Session storage and lifecycle
remain specialized.

The value-only package worker path is exercised by
`vm_stream_package_value_binding_uses_default_worker`: a source handler parses
a URI through the external worker and reads managed fields after resuming.
JSON now uses package-owned bindings and generic resource contracts. Live-worker
validation remains separate: this sandbox does not permit the worker's network
isolation setup or loopback sockets. Scheduler-reply tests do not replace that
validation, and worker isolation must not be disabled to obtain a pass.

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
