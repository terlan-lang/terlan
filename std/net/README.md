# Std Net Internals

This directory owns portable network data helpers, maintained native codecs,
and the nonblocking socket transport used by production HTTP serving.
The current public Terlan surface is URI parsing/formatting support.

## Responsibilities

- Define target-neutral networking data helpers.
- Keep parser implementation details behind portable result values.
- Avoid exposing target-specific URL or URI object models in core modules.
- Provide stable behavior for HTTP and cloud tooling integrations.

## Public Surface

- `std.net.Uri`: URI helper module.

## Core Model

Networking helpers are pure data operations unless a module explicitly owns IO.
`Uri.terl` owns the URI value, private component fields, public accessors, and
`UriError` construction. These are ordinary Terlan source bodies, not compiler
substitutions or VM URI operations.

The main flow is:

1. `Uri.parse` calls the private `parse_parts` package binding.
2. `native/` uses the maintained Rust `url` parser. The shared `native_record!`
   binding macro projects its declared accessors into the managed `Uri` record
   through the generic value ABI, or returns the parser error message.
3. Terlan maps parser failures to `UriError`. Accessors and rendering read the
   stored fields without further native calls.

Private fields mirror the selected upstream accessor names and optionality:
`as_str`, `scheme`, `host_str`, `path`, `query`, and `fragment`. No positional
tuple mapper or handwritten field-renaming adapter is maintained. Public
conveniences such as `host()` and `to_string()` remain ordinary Terlan methods.
The macro is shared Rust binding machinery, not a new Terlan annotation or
automatic discovery of every method exposed by an upstream crate.

The `Uri.query_pairs` binding delegates form-url-encoded query
decoding to `url::form_urlencoded`. HTTP request metadata and WebSocket identity
restoration share this codec. It preserves wire order and duplicate keys;
callers own lookup policy.

Important invariants:

- URI helpers do not perform network IO.
- Host URL object models must not leak into portable source APIs.
- Error shapes must be stable before widening the URI API.

## Integration Points

- `std.http`: can use URI helpers for request routing and parsing later.
- `native/`: owns the parser adapter and adversarial parser tests; it has no
  dependency on the compiler or VM. The `terlan-std-native` composition crate
  (`std/Cargo.toml`) registers the binding in `std/native/packages.rs` for the
  generic VM value adapter and external capability worker. Consumers use a
  Cargo dependency, not cross-tree Rust source inclusion.
  Worker admission uses the exact registry entry, `package-native` authority,
  and the `fast` worker class. It does not admit a namespace of operations or
  accept resource handles. Shared binding metadata checks arity before decoding
  values or invoking package code.
- Web/cloud packaging: may use URI validation in manifests.

## Edge Cases

- Percent encoding, invalid UTF-8, and relative URI handling need explicit
  tests before broadening the public API.
- Target-specific URL normalization must not change portable semantics.

## Native TLS Material

`native/src/tls.rs` owns certificate-chain and private-key PEM decoding through
rustls, plus ring-backed server configuration using rustls's safe protocol
defaults. Both the VM TLS adapter and HTTP certificate-cache validation use
this implementation. Empty chains, malformed PEM, invalid identities, and
mismatched keys fail closed; no custom PEM parser or cryptography is maintained.

This is an internal Rust package API, not a new public Terlan TLS API. It does
not read files, choose HTTP ALPN protocols, manage ACME, or schedule connections.
Those callers still own their existing policy, I/O, and transport integration;
their migration is not complete merely because codec duplication is removed.

`native/src/tls_stream.rs` also owns production nonblocking TLS record I/O over
an owner-supplied `Read + Write` transport. rustls authenticates records and
closure; synchronous and polled reads share the same truncation rules. Writes
drain buffered ciphertext before accepting more plaintext, and shutdown flushes
`close_notify` before invoking the transport's write-half shutdown. Handshakes
accept a caller-owned deadline future; cancellation drops the transport without
spawning background work. Socket readiness registration and wakeups remain the
transport owner's responsibility. There is no package-created executor or timer.

The VM production adapter now supplies its socket and deadline to this API.
HTTP ALPN decisions and TLS Hyper buffer adaptation live in `std/http/native`, not
this network codec. The older logical VM TLS plan/stream API still needs migration.

## Native Socket Transport

`native/src/tcp/ready.rs` supplies maintained Mio TCP listeners and connections
through the runtime ABI's optional `poll-io` interfaces: `IncomingStreams` and
`ReadinessStream`. The package owns nonblocking setup, acceptance, scalar and
vectored I/O, write-half shutdown, and Mio source delegation. The VM executor
accepts these generic interfaces, not a TCP listener or TCP stream.

The shared ABI `ReadyStream` facade pairs the stream with the VM's
`WriteInterest` implementation. Only a stalled write requests writable
interest, preserving read interest and the owner's generation-qualified token.
Successful registration happens once; a failed registration propagates its
error and can retry later. Readiness wakes, cancellation, deadlines, and actor
scheduling stay with the VM. The package creates no executor or worker thread.
`native/src/transport.rs` re-exports this facade and supplies the TLS half-close
trait adaptation. It does not duplicate the ABI implementation. Only initialized
read buffers cross the safe extension boundary.

`native/src/tcp.rs` owns address resolution and listener construction, including
address-family selection, Unix reuse flags, nonblocking mode, and the existing
1,024-connection listen backlog. It tries resolved addresses in order and
preserves the established bind diagnostics. The serving host calls this package
directly and supplies its listener adapter to the VM. The VM's old TCP convenience
entrypoint remains only in test builds.

This is an internal Rust package interface, not a new public Terlan TCP API.
The legacy `std.vm.Tcp` API still lacks native lowering and needs migration.
Session lifecycle policy also remains unfinished; moving
socket mechanics does not establish source-owned actors.

Transport tests cover partial/vectored transfers, EOF, interruption, repeated
backpressure, failed readiness registration, and drop cleanup. A real TCP
loopback test checks receive and half-close semantics; it requires an
environment that permits opening sockets and must not silently pass when
socket creation is denied.

## Types And Interfaces

`Uri`
: Portable URI helper module.

## Testing Notes

- `UriTest.terl` and `UriPropertyTest.terl` exercise the public source API.
- `cargo test -p terlan-net-native` checks parser normalization, missing versus
  empty components, malformed input, and invalid binding arguments. It also
  checks TLS chain ordering, missing and malformed material, supported key
  formats, encrypted-key rejection, and identity validation.
- HTTP server manifest tests should cover route/URI interactions separately.
- `vm_stream_package_value_binding_uses_default_worker` executes a Terlan URI
  handler over TLS through the default external worker, including parser
  failure and subsequent recovery. It rejects trusted in-process mode.
- `native_worker::protocol::value_packages` tests owned-value transport,
  capability and scheduler admission, forged handles, result budgets, and
  credit release. The separate HTTP JSON-body integration still fails because
  legacy resource-backed JSON operations are not registered package bindings.
