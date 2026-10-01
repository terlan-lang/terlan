# Std HTTP Internals

This directory owns the portable source-level HTTP API used by Terlan handlers.
The concrete server is Rust-native compiler tooling; source code works with
typed request, response, and error modules rather than backend server values.

## Responsibilities

- Define stable HTTP request and response shapes for Terlan handlers.
- Keep transport parsing and socket storage behind the server adapter.
- Expose JSON-capable handler helpers without leaking host JSON values.
- Provide portable errors for request body, response, and serialization
  failures.

## Public Surface

- `std.http.Request.Request`: source-owned managed record with private fields.
- `std.http.Response.Response`: source-owned managed record with private fields.
- `std.http.Session.Session`: source-owned record with private identity fields.
- `std.http.Sse.Event`: source-owned event record with optional metadata.
- `std.http.Sse.Endpoint`: source-owned limits and callback declarations.
- `std.http.WebSocket.Frame`: source-owned text, ping, pong, or close intent.
- `std.http.WebSocket.Endpoint`: source-owned limits, callbacks, and pairing policies.
- `std.http.Router.Router`: immutable source-owned record with private declarations.
- `std.http.Router.Handler`: typed route handler function shape.
- `std.http.Error.HttpError`: portable HTTP helper error.
- `std.http.Cookies.Options`: typed `Set-Cookie` option contract.
- `std.http.Tls.Config`: declarative TLS configuration contract.
- `std.http.Tls.auto`, `std.http.Tls.manual`, and `std.http.Tls.internal`:
  constructors for the supported TLS configuration modes.
- `std.http.Request.method` and `std.http.Request.path`: request metadata
  accessors.
- `std.http.Request.param`, `std.http.Request.query`, and
  `std.http.Request.cookie`: optional route/query/cookie metadata accessors.
- `std.http.Request.body_text`: raw UTF-8 request body access.
- `std.http.Request.body_json`: explicit JSON request parsing.
- `std.http.Response.json`, `std.http.Response.text`,
  `std.http.Response.html`, and `std.http.Response.redirect`: response
  builders.
- `std.http.Response.status`, `std.http.Response.header`, and
  `std.http.Response.set_cookie_header`: mutable response metadata helpers.
- `std.http.Response.with_status` and `std.http.Response.with_header`:
  chainable response metadata helpers for expression-style handler code.
- `std.http.Response.cookie`, `std.http.Response.cookie_with_options`, and
  `std.http.Response.delete_cookie`: validated response cookie helpers backed
  by `std.http.Cookies`.
- `std.http.Sse.data`, event receiver metadata helpers, queued
  `std.http.Sse.response`, and endpoint-plan constructors: typed SSE surface
  for VM-owned `text/event-stream` responses.
- `std.http.Session.current`, session receiver `get`, `set`, `delete`,
  `rotate`, `expire`, and `with_response`: source-facing actor-backed session
  helpers with explicit response cookie threading.
- `std.http.Router.new`, method route builders, `sse`, `websocket`, and
  `fallback`: typed route builder contract for generated web manifests and
  VM-owned long-lived channel routes.

## Router And Middleware Composition

`Router.use` installs request middleware in declaration order. Each callback
returns either `Continue` or `Respond(Response)`, so authorization, timeout,
and recovery policies cannot accidentally fall through after producing a
response. `Router.map_response` performs typed response post-processing in
reverse order. This is the header-normalization and trace-response hook;
`Router.error` is the panic/error boundary. Request headers provide normalized
trace input, while side effects remain behind explicit NativeBoundary or actor
capabilities rather than the ordinary `Handler` function type.

Application-wide middleware belongs on the root router. A `Router.group`
builder adds scoped middleware and nested routes without new routing grammar:

```text
pub require_user(_request: Request): MiddlewareResult ->
    Continue.

pub api(router: Router): Router ->
    router
    |> Router.use(require_user)
    |> Router.get("/users/:id", show_user).

pub application_router(): Router ->
    Router.new()
    |> Router.use(propagate_trace)
    |> Router.map_response(normalize_headers)
    |> Router.group("/api", api)
    |> Router.error(recover_http_error).
```

Terlan source prefixes group paths, expands grouped fallback declarations into
method-specific routes, composes complete middleware lists, and promotes a child's
error handler when the parent has none. Package admission validates those lists
without rebuilding them. Package dispatch preserves parameter captures and
rejects ambiguous normalized route shapes; root-before-group ordering is already
present in the source value. Bounded SSE and WebSocket endpoint plans survive router
materialization and open live-session state with the source-declared queue and
message limits. `LiveChannelTest.terl` is the executable nested-channel
example; `RouterTest.terl` covers typed response short-circuit composition.

## Core Model

`std.http.Error` owns its constructor and accessor implementations in Terlan.
`HttpError includes Error` uses ordinary struct inclusion and field access;
the compiler does not replace these functions or supply a private error layout.
`std.http.Request` now declares its fields and implements its accessors in
Terlan. Its body decoder composes `Json.parse` with the portable `Error`
constructor. Request accessors no longer depend on compiler call substitution
or native exports. The server adapter materializes the ordinary record.
Request, cookie-jar, and session layouts follow their source declarations;
their module names no longer exempt bodyless opaque declarations from the
ordinary native-package handle representation.
The legacy native Request accessor dispatch and opaque Request resource have
been removed. Session storage, routing, and host transport integration still
require migration and verification.

Session cookie identities are issued by `native/src/session_identity` from 32
OS-random bytes using `getrandom` and the maintained Base64 URL-safe codec. The
VM no longer generates sequential identities. Issuance fails closed on entropy
failure and bounds retries against occupied identities. Rotation obtains its
replacement before removing the old identity. Tests consume actual issued
identities and cover lifecycle, application isolation, reload, replay, and
migration conflicts. This does not complete session ownership or establish a
complete authentication system: session storage and lifetime policies still
require migration out of the VM-specific implementation.

`Session.terl` owns the session record, receiver threading, pending replacement
identity, and cookie policy. Lookup returns an identity/created pair atomically;
rotation returns an identity string. The creation flag cannot be inferred by
comparing identity strings, because an unrecognized cookie can equal a newly
allocated identity. State reads/writes and liveness accept strings rather than a compiler-defined
Session aggregate. The compiler no longer aliases the Session type name or
installs its layout. The version-2 internal state-operation ABI rejects the old
opaque-record payloads and version-1 descriptors. This is not full session
ownership: actor/table storage and state-operation dispatch remain VM-specific.

Host request/response adapters, cookie parsing, serialization, and
the shared native HTTP error live in `native/` (`terlan-http-native`), without
compiler or VM dependencies. The maintained `http` crate validates host methods,
URIs, statuses, and headers. Request allocation transfer and repeated response
headers are tested at this package boundary. Request projection masks and ingress
record materialization are package-owned; the compiler supplies generic aggregate
field-use analysis. Serving integration remains outside the package pending its
migration.

Request metadata decoding and projection also live in `native/`. Serving adapters
use `RequestMetadata::from_http` instead of maintaining separate query, header,
and cookie extraction rules. Query decoding delegates to the maintained URL codec
in `std/net/native`; header names come from `http::HeaderMap`, and cookies use
the maintained cookie codec. Repeated query/header values retain their order
within each name. The existing text API decodes non-UTF-8 header values lossily.
Cookie decoding retains the existing first-header-only policy and ignores a
nontext first Cookie header; this is a preserved contract, not a claim that
multiple Cookie headers are merged. Cookie-jar projection includes incoming
cookies even when the direct cookie field is not observed. Exhaustive projection
tests compare early ingress filtering with later source-record projection.

The buffered HTTP/1 request decoder also lives in `native/`. Both incremental
and one-shot stream decoding use the same `httparse`-based head validation.
The incremental API returns the consumed byte count so callers retain pipelined
requests. This bounded UTF-8 path supports Content-Length framing, rejects
Transfer-Encoding and repeated Content-Length, and limits heads and bodies
independently. It is not the HTTP/2, HTTP/3, or streaming-upload decoder.
HTTP/1.0 version and connection policy survive parsing. Legacy diagnostic text
is retained, but its failure types and implementations are package-owned.

HTTP/1 response framing and finite body-chunk storage also live in `native/`.
The response writer accepts validated `http::Response` metadata, preserves
binary body bytes and repeated headers, and reports typed metadata, timeout,
disconnect, and I/O failures for both buffered and chunked output. Its existing
wire format is retained; this migration does not introduce a new protocol parser.
The package's `HttpResponseChunks` uses shared `bytes::Bytes` slices and independent
clone cursors. Callers still control when to pull the next chunk, account memory,
apply backpressure, cancel work, and release the stream. Sockets, schedulers, and
VM process types are not dependencies of these package APIs.

Production TLS I/O uses the package-owned `std/net/native` rustls transport.
`native/src/tls.rs` adapts that transport to Hyper buffers and owns HTTP ALPN
selection (`h2`, `http/1.1`, no-ALPN fallback, and rejection of other protocols).
The VM supplies only its nonblocking socket and handshake deadline at this
boundary. The synchronous upgrade path shares plaintext reads, backpressure,
and authenticated-closure behavior with the Hyper path. Package tests use real
rustls client/server handshakes over in-memory transports without a VM or sockets;
host socket integration is a separate gate. Serving policy and session ownership
are not migrated by this adapter change.

Handler header admission and shared host response construction are package-owned
as well. Header names and values are validated by the maintained `http` crate,
not a separate handwritten token recognizer in the compiler. Handlers cannot
override transport framing or the separately supplied content type. Owned bodies
and header strings retain their allocations, repeated headers retain their order,
and HEAD responses keep the original content length without emitting the body.
Compiled-source tests exercise both accepted headers and malformed or
transport-owned fields through the normal response bridge.

Header builders use package-owned `NativeBinding` entries registered by
`terlan-std-native`; their argument validation is not a compiler dispatch table.
The maintained `cookie` and `time` crates retain responsibility for formatting
cookies and parsing expiry dates. Direct cookie-header calls follow source
bodies and their declared native operations. Cookie jar state and replay execute
in Terlan; response construction and storage are source-owned as well.

Development and production security-policy defaults execute the ordinary
functions in `Response.terl`; the compiler no longer supplies duplicate policy
records or HSTS defaults. Source-authority tests cover changed providers, import
aliases, local namesakes, and renamed modules. Security-policy application also
executes in `Response.terl`, including optional headers, enum matching, and HSTS
formatting. The compiler-private policy layout and managed policy opcode have
been removed. Compiled-handler tests cover dynamic policy fields and integer
boundaries; invalid fields are rejected by ordinary type checking.
The `status`, `header`, `with_status`, and `with_header` methods execute ordinary
Terlan record and list operations. Changed-provider and renamed-module tests
protect this boundary; private fields preserve public construction discipline.

Cookie composition (`cookie`, `cookie_with_options`, `delete_cookie`, and their
`with_*` counterparts) also executes in `Response.terl`. Defaults come from the
provider declaration, not compiler cookie tables. The source calls package-owned
cookie codecs, then appends their validated header through `set_cookie_header`.
Compiled-handler tests exercise suspension, ordered replies, statement-style
receiver writeback, fluent calls, and malformed-name rejection using the real
package codecs. These tests explicitly deliver scheduler replies; they do not
claim isolated-worker or socket coverage.

`Cookies.Jar` is now a source-owned private record of incoming cookies and
pending serialized headers. Reads, persistent mutation, and `with_cookies`
replay use ordinary Terlan map, list, and receiver operations. Mutations do not
change incoming-cookie reads or earlier jar snapshots. Bound command results
(`let done = jar.set(...)`) return `Unit` while writing back the receiver through
generic compiler metadata, not an HTTP-specific rewrite. Compiled-handler tests
exercise real package codec replies, ordered replay, duplicate incoming-cookie
lookup, and invalid mutation rejection. The package's host-to-source projection
keeps the first cookie per name, matching host lookups without changing the
maintained parser's ordered output.

`Response.set_cookie_header` is ordinary Terlan composition over `header`;
there is no duplicate native cookie-header setter. Response mutation calls are
resolved through provider methods, not intercepted because they are named
`status`, `header`, or `set_cookie_header`. Source-authority tests preserve
command Unit results and persistent snapshots. Every Response builder is source
code, including defaults, content types, redirects, file descriptors, and stream
limits. There are no native Response declarations or compiler-owned layouts.
Compiled-handler tests cover application-defined defaults alongside the standard
builders.
`Response.json` is source composition over `Json.to_string` and `json_text`.
The JSON package renders validated values through its maintained backend;
HTTP lowering no longer projects JSON tuple storage, and the retired
`std.http.response.json` operation has no native dispatch or resource bridge.
Compiled-handler tests exercise live JSON resources, escaped content, explicit
and default statuses, and source header composition through scheduler replies.
The unused VM JSON parse/result-predicate opcode family is removed. Compiled
request tests now verify canonical body decoding, source error mapping, and
ordinary `Result.is_ok` behavior; retired opcode envelopes reject before heap
reads or allocations. These scheduler-reply tests do not replace socket/worker
integration evidence.
Response storage is a private source record. The VM materializes it with generic
record machinery; named-field transport admission validates protocol metadata,
file paths, and stream bounds. The old Response allocation opcode and direct
managed-response fast path are removed. Native Response operation names are
retired rather than left as a second implementation.
Native result decoding and reusable actor entry are now generic; no HTTP result
projection flag or separate HTTP completion type remains in the native VM boundary.
Response overloads use ordinary typed resolution, including imported qualified
types such as `Template.Html`. Middleware and long-lived channels use the same
source Request record projection as direct handlers; the old borrowed tuple
mapper has been removed.

The source-reload cookie fixture now uses the production TLS/protocol-owner path,
shared with the URI and JSON package integration fixtures, so package calls can
suspend and resume through the default external worker. It retains the source
reload, ordered cookie, and security-header assertions. This integration remains
unverified in the restricted environment: loopback listener binding is denied
before dispatch. Explicit scheduler-reply cookie tests do not close that gap,
and worker isolation must not be disabled to obtain a pass.

The duplicate Rust jar, native jar resource, and jar dispatch operations have
been removed. Retired operation names reject live, malformed, and forged handle
arguments without changing the resource store; retired cookie VM opcodes are
rejected before reading heap words. Request and Cookies imports no longer
install legacy HTTP layouts, and their qualified nominal types use ordinary
package identities. Retired compiler-private request accessors have no native
type or lowering; source methods determine request reads.

Session calls now resolve to provider bodies through ordinary application
linking. Importing Session does not intercept unrelated `get`, `set`, or other
method names, nor does it install native session layouts. Only explicit session
native declarations select the remaining specialized operations. Source-provider
override tests and a canonical compiled-session test cover command results,
reads, deletion, rotation, expiration, and stale-handle rejection.

`Session.current` selects the reserved cookie through `Request.cookie` in Terlan
and passes only `Option[String]` to its private native lookup. The compiler's
legacy request/jar layouts and associated collection schemas have been removed;
the native session operation no longer reads request fields or cookie maps.
`Session.with_response` also executes its Terlan body: it checks identity
liveness, reads the source record's pending identity, omits empty identities, and
serializes new/rotated identities with the package cookie codec. The cookie
name, Path=/, HttpOnly, and SameSite=Lax policy now live in `Session.terl`, not a
VM string formatter. The maintained codec determines attribute ordering.
Headers attach through the ordinary response method. Expired identities call
`Cookies.delete_header` from Terlan; the package codec can suspend and resume
the handler. Session-native
operations no longer know the response header field, header list, or response
layout. The retired response-attachment opcode is rejected rather than retained
as a compatibility path. The former VM response-cookie policy opcode is also
rejected. `session_cookie_policy_test` covers pending-cookie replay, omission,
expiration, ordered attachment, and codec failure with explicit scheduler replies;
it does not replace isolated-worker or socket verification. The compiled
`session_lifecycle_cookie_test` covers lookup, reuse, rotation with preserved
values, and stale-cookie replacement. The production protocol fixture retains
its reload/persistence checks but needs loopback and isolated-worker support.
Session storage and lifecycle still require migration before HTTP ownership is complete.

The HTTP server owns socket and transport state. It supplies a managed request
record, including its source-owned cookie jar, to the compiled Terlan handler.
Response uses a source-owned private record; session storage remains specialized.
Source-owned Request records currently use complete ingress rather than the old
fixed-tuple scalar shortcut. Generic projection optimization and performance
revalidation remain separate from the ownership migration.

Router middleware result variants now use ordinary source union layouts.
Importing Router no longer installs compiler-defined `Continue`/`Respond`
constructors, response layouts, or header collections, and a structurally
similar application union is not coerced into a reserved middleware type.
The compiled middleware tests cover both continuation and a response payload;
provider tests also change tags, payload arity, and variant count. Live channel
scheduling still requires ownership migration.

`Router.terl` implements route, middleware, fallback, admission-policy, and
channel builders as ordinary source functions. Each returns a new declaration
sequence without modifying earlier values. Groups invoke their configuration
callback once, compose paths (including nested channel paths), expand only their
own fallback across the seven supported methods, and retain balanced nested
scope markers. New targets capture the current scope's middleware. Registering
middleware later inserts it after earlier callbacks from the same scope and
before nested callbacks on every existing target, including fallbacks and
channels. Group composition prepends the parent's callbacks to the already
composed child lists. Earlier router values remain unchanged. Native dispatch
uses these complete lists directly; it no longer stores or merges a separate
global middleware list. When the parent has no error handler, source
also emits the child's selected error handler in the parent scope. The first
group supplying recovery wins; an existing parent handler takes precedence.
The stored recovery callback is a source closure accepting a failure message.
It constructs `HttpError` through `Error.new` before calling the public
`ErrorHandler`; the compiler does not inject its error atom and the host does
not construct an HTTP-specific record. Renamed and modified provider tests
verify that error codes and status values follow the source implementation.
Explicit duplicate error registrations still fail admission, including a direct
registration after an inherited handler. Native admission validates scopes, callback
arities, and duplicate policies but does not synthesize paths, fallback routes,
inherited middleware, or error-handler inheritance. Router-returning helpers
follow ordinary application reachability; they are no longer discarded by a
compiler rule. Source-execution tests cover all builders, renamed providers,
changed provider bodies, nested groups, and the actual `RouterTest.terl` fixture.
Production serving executes `router/0` once per admitted image generation.
The HTTP package validates the returned declarations, nested scopes, endpoint
limits, and callback arities. Computed paths and captured handlers are ordinary
source values, not compiler-interpreted builder syntax. Static router plans are
no longer persisted; loading a saved image executes its source router again.
The old interpreter remains test-only during migration. The HTTP package now
owns route-table admission and live route dispatch; the VM module only aliases
package types to its callable representation. Session scheduling still requires
ownership migration. Lifecycle and overload declarations are decoded but are
not yet admitted for source routers.

`native/src/route_pattern` supplies the same route grammar, ambiguity checks,
precedence, and percent-decoded captures to manifest selection and live routing.
Typed `Int` and `Bool` captures must validate before dispatch. Group fallbacks
match their path prefix, including its root, without matching sibling prefixes;
exact routes precede captures, which precede wildcards. Package tests cover
adversarial captures and ambiguous registrations. A compiled Terlan router is
also exercised over sockets with typed captures and nested group fallbacks.

For dynamic HTTP requests, the package-owned callback chain orders request
middleware, honors short-circuit responses, unwinds response middleware, and
invokes recovery without replaying completed callbacks. The host supplies the
generic callable suspend/resume mechanism, including native-worker completion
and cancellation. Path matching is package-owned Rust, not Terlan source.
Channel scheduling still requires ownership migration, and callback sequencing
has not yet moved into Terlan source.

The generic in-process value boundary preserves captured source functions in
these declarations. Compiled tests return a router from one execution shard,
destroy that shard, and extend the same immutable router in another shard while
retaining both handlers' captures. Closures require matching admitted image
generations, callable membership, signatures, and capture types. Managed graph
decoding uses a bounded explicit work stack; failed capture allocation rolls
back atomically. These image-local function values cannot be serialized onto
the distribution wire or passed to external native providers. Native-resource
captures require an explicit ownership-transfer contract and are rejected by
this copied-value boundary.

Serving can invoke these owned callbacks through the generic native call entry,
without converting a captured function into a named-export tuple. The same
actor entry, trace, continuation, receive/wake, and cancellation paths serve both
forms. Compiled-source tests invoke a captured `Handler` after its creator shard
has been destroyed, exercise protocol-owner and dedicated-owner calls, and
reject stale generations, unknown callables, malformed captures, and incorrect
arguments. Immediate callback APIs cancel unsupported waits rather than blocking.
Source-router admission uses this same callable boundary. Manifest route
selection remains a separate serving input; executing source declarations does
not yet replace the web-package manifest discovery mechanism.

Live SSE and WebSocket plans also retain source function values, including
captures in pairing and restoration callbacks. They enter the same serving
callable path as HTTP handlers, with per-channel busy checks, receive/wake, and
terminal cancellation. Package admission consumes executed endpoint records
without changing queue limits, keep-alive settings, or recovery policy. Live
closure values are not persisted; each admitted image recreates its callbacks.
Compiled-source tests cover producer teardown, both channel lifecycles, captured
recovery payloads, stale callbacks, foreign modules, and terminal waits. This
does not yet migrate session scheduling into the package.

The main flow is:

1. The packaged web manifest matches a request to a static asset or handler.
2. The server materializes the source-owned request record from parsed input.
3. Handler source uses `std.http.*` helpers to parse input and build output.

Important invariants:

- The internal migration handler response tuple is not a public API.
- JSON responses accept `Json` explicitly.
- Request metadata accessors return values captured by the generated route
  manifest and server bridge.
- Raw body access does not parse or validate content type; higher-level form,
  JSON, and multipart helpers should live on explicit std APIs.
- HTML responses accept already-rendered HTML strings; typed template rendering
  remains a separate compiler/template responsibility.
- Redirect responses use the default temporary redirect shape and can be
  refined later with richer status support.
- Mutable response updates use mutable receiver methods and return `Unit`.
- HTTP status, header, and interim cookie-header manipulation remain
  target-owned operations.
- `with_status` and `with_header` are pure source ergonomics over mutable
  receiver continuation; they do not introduce a separate response storage
  model.
- `std.http.Router` is a source-visible route builder contract. Its
  `sse` and `websocket` builders accept endpoint plans from `std.http.Sse` and
  `std.http.WebSocket` while the VM owns stream/socket state. The compiler
  discovers route manifests, and production serving materializes its extracted
  plan before middleware and endpoint admission.
- `set_cookie_header` accepts a complete header value for low-level escape
  hatches. Normal handler code should prefer `cookie`, `cookie_with_options`,
  and `delete_cookie`, which reuse `std.http.Cookies` validation.
- `std.http.Cookies` owns typed SameSite, options, and cookie-jar state. Jar
  mutations are applied explicitly with `response.with_cookies(jar)`; merely
  changing a jar does not attach headers to a response.
- `std.http.Tls` is source-visible configuration shape and helper
  constructors only. `terlan.toml` parsing, rustls/ACME integration, and
  certificate cache state remain implementation work.
- `std.http.Sse` endpoint plans open bounded VM-owned live-session streams.
  Cancellation, scheduler wakeups, and socket emission remain runtime-owned and
  are never exposed as source-side host handles.

## Integration Points

- `terlc serve`: owns local server startup, validation, and request routing.
- `std.data.Json`: provides request JSON parsing and JSON response bodies.
- NativeBoundary runtime helpers: own Rust-native HTTP helper implementations.
- `_build/web/manifest.json`: declares static assets and handler routes.

## Edge Cases

- Missing or malformed web manifests fail during `terlc serve --check`.
- Unsafe route paths and asset paths are rejected before serving.
- Handler dispatch reports missing VM handler artifacts and unavailable VM
  handler runtime support before attempting dynamic execution.

## Types And Interfaces

`Request`
: Source-owned managed record with private fields, passed to handlers.

`Response`
: Source-owned response record with private fields, returned by handlers.

`Router`
: Source-owned immutable declarations. Production admission still uses static
  compiler discovery and VM dispatch pending migration.

`Handler`
: Function type for handlers that accept `Request` and return `Response`.

`HttpError`
: Portable HTTP error shape with code, message, and status.

`Cookies.Options`
: Typed cookie mutation options used by the source-owned cookie jar.

`Sse.Event`
: Source-owned record containing data, optional id, event name, and retry delay.

`Sse.Endpoint`
: Source-owned bounded server-sent event route policy.

`WebSocket.Frame`
: Source-owned union of `TextFrame`, `PingFrame`, `PongFrame`, and `CloseFrame`.

`WebSocket.Endpoint`
: Source-owned queue/frame-size limits and callback/pairing declarations.

`Tls.Config`
: Typed TLS configuration record for auto, manual, and internal TLS modes.

## Testing Notes

- SSE event builders and WebSocket frame builders execute as ordinary Terlan
  functions, without native helpers. Their descriptor tests assert defaults,
  metadata replacement, unchanged earlier values, payloads, and distinct tags.
  Renamed-provider and changed-body tests verify source authority.
- These values do not themselves perform wire encoding or open channels.
  Endpoint bounds are validated at route admission. Endpoint callback builders
  now retain typed source closures in private immutable declaration lists;
  WebSocket pairing builders also retain their payloads, callbacks, and
  restoration settings. Repeated and conflicting declarations are retained,
  not silently overwritten, so admission can reject them. Construction invokes
  no callbacks and leaves earlier endpoint values unchanged. Compiled tests
  invoke stored callbacks, including captured closures and tuple-returning
  state transitions, under both standard and renamed module identities.
  Registration also completes with callbacks that would suspend if invoked.
  Imported SSE and WebSocket providers coexist in the same application closure;
  receiver dispatch retains each module's distinct `Endpoint` identity.
  Production live callback registration consumes these executed source values
  through package-owned descriptor validation.
- `native/src/channel_plan` owns immutable SSE/WebSocket endpoint descriptors
  and their admission rules. Callback identities are generic, with no dependency
  on compiler or VM value types. Deserialization validates positive limits,
  keep-alive intervals, and exclusive callback/pairing modes before persisted
  endpoint metadata can reach a live session. `native/src/source_descriptor`
  validates executed source records using a generic borrowed value view; host
  adapters retain callable execution authority. VM adapters retain queue
  allocation and consume the package descriptor's limits. Session scheduling
  is not yet package-owned.
- `native/src/websocket.rs` owns maintained tungstenite protocol state, upgrade
  response construction, and transport-error classification. The live Hyper
  pump and the in-memory channel integration use this same package codec.
  Endpoint byte limits constrain both frames and reassembled messages before
  callback admission. The codec never waits or invokes callbacks; readiness,
  cancellation, pairing, and scheduling remain outside it. Compiler/VM production
  dependencies no longer include tungstenite directly; legacy VM protocol tests
  retain it as a test dependency while their ownership migration continues.
  Codec tests cover split input, fragmented-message limits, oversized declared
  lengths, partial writes, masking, UTF-8, and maintained pong/close replies.
- Queued `Sse.response` executes in Terlan and composes `Response.stream` with
  the package's event encoder. Axum supplies SSE framing with default features
  disabled: no Tokio executor, server, or scheduler is used by the codec.
  CRLF and CR normalize to LF before framing. Empty data omits the data field,
  matching the previous behavior; metadata containing CR, LF, or NUL and
  nonpositive retry delays are rejected at encoding.
  Tests execute the compiled handler through generic package replies and the
  production streaming response bridge, including HTTP/1 serialization.
- Positive HTTP std tests live beside the modules as `std/http/*Test.terl`.
- Server and handler bridge tests live under
  `crates/terlan/src/commands/serve/*_test.rs`.
- Release preflight includes exact HTTP handler and installed-runner support
  checks.
