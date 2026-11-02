# Terlan protocol toolkit executable roadmap

Status date: 2026-11-02

## Outcome

Terlan will provide a protocol toolkit for implementing standards-track
protocols in Terlan itself. The toolkit must make protocol wire formats,
finite-state transitions, timers, ownership, and actor scheduling explicit and
testable. It must not depend on bespoke compiler syntax for each protocol.

The first production tracks are:

1. HTTP semantics and HTTP/1.1: RFC 9110, RFC 9111, and RFC 9112.
2. WebSocket: RFC 6455, followed by RFC 8441 after HTTP/2.
3. RTP and RTCP: RFC 3550, RFC 3551, and RFC 5761, followed by SRTP RFC 3711.
4. HTTP/2 and HPACK: RFC 9113 and RFC 7541.
5. QUIC: RFC 9000, RFC 9001, and RFC 9002.
6. HTTP/3 and QPACK: RFC 9114 and RFC 9204.

HTTP/3 is not considered supported until the QUIC, HTTP/3, and QPACK tracks
all pass the three independent gates in this roadmap.

Authoritative specifications:

- [HTTP/1.1, RFC 9112](https://www.rfc-editor.org/rfc/rfc9112.html)
- [HTTP/2, RFC 9113](https://www.rfc-editor.org/rfc/rfc9113.html)
- [HTTP/3, RFC 9114](https://www.rfc-editor.org/rfc/rfc9114.html)
- [WebSocket, RFC 6455](https://www.rfc-editor.org/rfc/rfc6455.html)
- [RTP and RTCP, RFC 3550](https://www.rfc-editor.org/rfc/rfc3550.html)
- [QUIC, RFC 9000](https://www.rfc-editor.org/rfc/rfc9000.html)

## Policy boundary

The old blanket "no handrolling" rule does not apply to this roadmap. Replace
it with these rules:

- Reuse is the default when an external component is mature, portable, and
  does not distort Terlan semantics or builds.
- A specification-driven Terlan implementation is permitted when it closes a
  missing platform capability, removes an unsuitable dependency boundary, or
  validates Terlan's language and runtime model.
- Cryptographic primitives use audited providers initially. Protocol state,
  framing, key scheduling orchestration, and provider boundaries may be Terlan.
- Codecs carried by RTP are providers, not part of the RTP core track.
- Reference implementations are black-box test peers. Their internal
  architecture and undocumented behavior are not the Terlan specification.
- No new language feature is accepted solely to simplify one protocol. A
  feature proposal requires repeated evidence from at least two protocol
  tracks and a demonstration that a library, shape, implication, compiler
  derivation, or actor contract cannot express the requirement safely.

This roadmap does not block compiler self-hosting. It depends on stable binary
access, typed FSM representation, deterministic actor testing, and the runtime
contracts named by each milestone.

## Architecture

```text
RFC text + errata + requirement matrix
                 |
                 v
       std.protocol specifications
      /            |              \
 wire schemas   typed FSMs    actor sessions
      \            |              /
                 v
       transport and crypto traits
                 |
                 v
 stream / datagram / secure provider adapters
                 |
                 v
 deterministic trace, replay, and gate reports
```

Protocol packages own standards behavior. The VM owns memory isolation,
scheduling, timers, I/O completion, and native-boundary enforcement. The
compiler may derive encoders, decoders, transition tables, and bounded-memory
plans from ordinary Terlan declarations, but the generated artifacts must be
inspectable and deterministic.

## Three independent acceptance gates

Every RFC track must pass all three gates. Passing one gate cannot waive or
substitute for another.

### G1: Normative conformance

Each track maintains a machine-readable requirement matrix with one row per
applicable MUST, MUST NOT, REQUIRED, SHALL, and SHALL NOT statement. SHOULD and
MAY statements require either implementation evidence or an explicit,
reviewed disposition.

Required evidence:

- RFC number, section, requirement identifier, applicability, and errata state.
- Terlan implementation symbol and positive, boundary, and negative fixtures.
- Published vectors where available and repository-owned derived vectors where
  the RFC supplies prose or ABNF only.
- Parser rejection tests for truncation, overflow, duplicate fields, invalid
  state transitions, and resource-exhaustion inputs.
- A zero-unaccounted-normative-requirements report.

Planned commands:

```bash
terlc protocol requirements protocol/<track>/track.toml --check
terlc protocol conformance protocol/<track>/track.toml \
  --out target/protocol/<track>/g1
```

Gate artifact: `target/protocol/<track>/g1/report.json` with
`status = "passed"` and `unaccounted_requirements = 0`.

### G2: Differential behavior

Each track runs the same generated and captured cases against at least two
independent mature implementations. Version, build options, platform, and
artifact digest are pinned in the track manifest. Values that are intentionally
nondeterministic, such as connection IDs, masks, random padding, and clock
samples, are normalized by a documented comparator rather than ignored.

Required evidence:

- Wire bytes or normalized frame trees for successful cases.
- Error class, protocol error code, and connection or stream disposition for
  malformed cases.
- State transition and timer outcome for timeout, loss, and retry cases.
- A reviewed divergence file for behavior where references disagree or retain
  compatibility behavior that the current RFC rejects.
- No unexplained Terlan-only divergence.

Planned command:

```bash
terlc protocol differential protocol/<track>/track.toml \
  --references protocol/<track>/references.lock \
  --out target/protocol/<track>/g2
```

Gate artifact: `target/protocol/<track>/g2/report.json` with
`status = "passed"` and `unexplained_divergences = 0`.

### G3: Live interoperability

Each role implemented by Terlan must communicate with at least two external
implementations. Client-only or server-only success is insufficient when the
RFC defines both roles. Datagram protocols must run deterministic loss,
duplication, corruption, reordering, and delay profiles. Stream protocols must
run fragmentation, partial-write, backpressure, half-close, and abrupt-close
profiles.

Required evidence:

- Successful Terlan-to-reference and reference-to-Terlan scenarios.
- Negotiation, downgrade, extension, graceful shutdown, and error shutdown.
- Bounded memory and mailbox growth under a declared hostile-input budget.
- Deterministic trace capture and replay for every failing seed.
- Platform coverage for Linux plus every platform promoted as supported by the
  protocol package.

Planned command:

```bash
terlc protocol interop protocol/<track>/track.toml \
  --network-profile protocol/profiles/all.toml \
  --out target/protocol/<track>/g3
```

Gate artifact: `target/protocol/<track>/g3/report.json` with
`status = "passed"`, `failed_scenarios = 0`, and no exceeded resource budget.

## Shared executable contract

PT0 introduces these repository-owned inputs:

```text
protocol/
  requirements.schema.json
  report.schema.json
  profiles/
  <track>/
    track.toml
    requirements.tsv
    references.lock
    vectors/
    traces/
    divergences.toml
```

`track.toml` must declare:

- RFCs, updates, errata snapshot, roles, and optional extensions.
- Source modules and public API version.
- Resource budgets for input size, allocations, mailbox depth, and timers.
- Required reference implementations and their pinned artifacts.
- Commands and expected reports for G1, G2, and G3.
- Unsupported features with explicit diagnostics and no silent fallback.

The aggregate command is:

```bash
terlc protocol gate protocol/<track>/track.toml \
  --out target/protocol/<track>
```

It succeeds only if G1, G2, and G3 were produced from the same source digest,
toolchain digest, requirement snapshot, and reference lock. CI publishes the
three reports, replayable traces, coverage matrix, and aggregate decision.

## Toolkit milestones

### PT0: Contracts and harness

- [ ] Add `protocol/requirements.schema.json` and `protocol/report.schema.json`.
- [ ] Add the `terlc protocol requirements`, `conformance`, `differential`,
  `interop`, and `gate` commands.
- [ ] Define deterministic random seeds, virtual clocks, packet capture, and
  trace replay artifacts.
- [ ] Add network profiles for fragmentation, loss, duplication, corruption,
  reordering, latency, jitter, and bandwidth limits.
- [ ] Pin reference artifacts by digest and reject mutable tags.
- [ ] Make every skipped normative requirement and test visible in reports.

Gate: a deliberately incomplete sample protocol must fail each gate for a
different reason, while a small complete fixture must produce three passing
reports and one aggregate decision.

### PT1: `std.protocol` primitives

- [ ] Add bounded byte cursors, immutable slices, endian integers, bit fields,
  QUIC-style variable integers, and checked length arithmetic.
- [ ] Add incremental decoders that distinguish `NeedMore`, `Decoded`, and
  `Rejected` without consuming bytes on incomplete input.
- [ ] Add deterministic encoders with exact encoded-size calculation.
- [ ] Add typed FSM declarations using ordinary sums, structs, pattern matching,
  and exhaustive transition functions.
- [ ] Add stream and datagram transport traits with backpressure and explicit
  ownership transfer.
- [ ] Add actor session primitives for deadlines, idle timers, retransmission,
  cancellation, supervision, and deterministic virtual time.
- [ ] Add bounded queue, reassembly, and flow-control utilities.
- [ ] Add provider traits for entropy, clocks, TLS, AEAD, hashes, and codecs.
- [ ] Add compiler derivation experiments for wire codecs and transition tables
  without making them language syntax.

Gate: one incremental binary fixture and one timed request/response FSM pass all
three gates with allocation, mailbox, and replay evidence.

## Selected RFC implementation tracks

### PT2: HTTP semantics and HTTP/1.1

Specifications: RFC 9110, RFC 9111, and RFC 9112.

- [ ] Model methods, status codes, fields, requests, responses, trailers, and
  body framing as typed values.
- [ ] Implement incremental request and response parsing and deterministic
  serialization.
- [ ] Implement content length, chunked coding, connection persistence,
  informational responses, upgrade, and close-delimited bodies.
- [ ] Implement client and server connection FSMs as actor sessions.
- [ ] Reject request smuggling and response splitting ambiguity rather than
  applying undocumented tolerance.
- [ ] Implement cache behavior as a separate RFC 9111 module.
- [ ] Differential references: pinned curl plus two independently implemented
  HTTP servers or parsers, with exact choices fixed in `references.lock`.

Gate: G1, G2, and G3 pass for client and server roles. The standard HTTP API
uses the Terlan implementation by default for HTTP/1.1.

### PT3A: WebSocket

Specifications: RFC 6455, with RFC 8441 added after PT4.

- [ ] Implement opening handshake validation over the shared HTTP model.
- [ ] Implement masking, fragmentation, continuation, control frames, UTF-8
  validation, close codes, and extension negotiation.
- [ ] Model `Connecting | Open | Closing | Closed` as an exhaustive FSM.
- [ ] Run the Autobahn test corpus as G1 evidence, not as a substitute for the
  normative requirement matrix.
- [ ] Differential references: two pinned independent WebSocket libraries.
- [ ] Add RFC 8441 extended CONNECT interoperability after HTTP/2 passes.

Gate: G1, G2, and G3 pass for client and server roles, including fragmented and
adversarial frame streams.

### PT3B: RTP and RTCP

Specifications: RFC 3550, RFC 3551, and RFC 5761. SRTP RFC 3711 follows after
the unencrypted core is accepted.

- [ ] Implement RTP and compound RTCP packet codecs with checked extension and
  padding lengths.
- [ ] Implement sequence rollover, timestamp domains, SSRC lifecycle, sender
  and receiver reports, participant timeout, and collision handling.
- [ ] Implement jitter estimation, reorder windows, loss accounting, and RTCP
  transmission scheduling with virtual-time replay.
- [ ] Model each session as a real-time actor with explicit packet and timer
  budgets; payload codecs remain provider-owned.
- [ ] Implement RTP/RTCP port multiplexing from RFC 5761.
- [ ] Differential and interoperability references: pinned GStreamer and Pion
  peers, plus captured standards-compliant traces.
- [ ] Add SRTP framing and state with cryptographic operations supplied by an
  audited provider boundary.

Gate: G1, G2, and G3 pass under clean, lossy, reordered, duplicated, and jittered
datagram profiles without unbounded mailbox or jitter-buffer growth.

### PT4: HTTP/2 and HPACK

Specifications: RFC 9113 and RFC 7541.

- [ ] Implement frame codecs, stream identifiers, settings, errors, and HPACK.
- [ ] Model connection and stream FSMs separately and prove that illegal stream
  transitions cannot reach the frame emitter.
- [ ] Implement flow control, prioritization behavior required by RFC 9113,
  continuation sequencing, graceful shutdown, and retry boundaries.
- [ ] Enforce header-list, dynamic-table, concurrent-stream, and mailbox budgets.
- [ ] Differential and interoperability references: pinned nghttp2 plus one
  independent HTTP/2 implementation.
- [ ] Enable RFC 8441 WebSocket interoperability after the base track passes.

Gate: G1, G2, and G3 pass for client and server roles with multiplexed streams,
backpressure, cancellation, malformed frames, and connection draining.

### PT5: QUIC transport

Specifications: RFC 9000, RFC 9001, and RFC 9002.

- [ ] Implement QUIC packet, frame, variable-integer, connection-ID, and
  transport-parameter codecs.
- [ ] Implement connection and stream FSMs, packet-number spaces, ACK handling,
  loss detection, PTO, congestion control, flow control, migration, and close.
- [ ] Integrate TLS 1.3 through a provider that exposes QUIC handshake secrets
  without exposing unsafe memory to protocol actors.
- [ ] Make UDP ownership, pacing deadlines, retransmission queues, and key-phase
  transitions explicit actor contracts.
- [ ] Differential and interoperability references: at least two of ngtcp2,
  quiche, picoquic, or another independent implementation, pinned by digest.

Gate: G1, G2, and G3 pass for client and server roles under deterministic loss,
reordering, migration, retry, version negotiation, and key-update scenarios.

### PT6: HTTP/3 and QPACK

Specifications: RFC 9114 and RFC 9204.

- [ ] Map shared HTTP semantics onto QUIC request, control, push, and QPACK
  streams without duplicating the HTTP data model.
- [ ] Implement unidirectional stream types, settings, request cancellation,
  server push policy, graceful shutdown, and connection errors.
- [ ] Implement QPACK encoder and decoder streams, blocked-stream accounting,
  dynamic-table budgets, and decompression failure handling.
- [ ] Differential and interoperability references: two independent HTTP/3
  stacks selected from the QUIC reference set.
- [ ] Add standard HTTP client and server negotiation across HTTP/1.1, HTTP/2,
  and HTTP/3 with explicit fallback evidence.

Gate: G1, G2, and G3 pass for client and server roles. Only then may release
documentation claim first-party HTTP/3 support.

### PT7: Language and runtime findings

- [ ] Record every protocol workaround and classify it as library, compiler,
  VM, actor-runtime, FFI, or language debt.
- [ ] Require evidence from at least two tracks before proposing new syntax or
  type-system behavior.
- [ ] Evaluate bounded integers and bitfields, ownership transfer, zero-copy
  slice lifetimes, effect tracking, deadline types, session-state constraints,
  and exhaustiveness diagnostics from measured implementation pressure.
- [ ] Publish accepted and rejected feature proposals with migration impact and
  generated-code alternatives.

Gate: the review contains evidence from HTTP/1.1, one multiplexed HTTP protocol,
and RTP or QUIC. No feature is accepted from hypothetical need alone.

## Completion criteria

The roadmap is complete only when:

- `std.protocol` is implemented in Terlan and used by all selected tracks.
- HTTP/1.1, WebSocket, RTP/RTCP, HTTP/2, QUIC, and HTTP/3/QPACK each have
  passing G1, G2, and G3 reports from one reproducible aggregate command.
- HTTP/3 is available through the standard HTTP API without a Rust protocol
  implementation being the semantic authority.
- External cryptography and codec providers cross documented, memory-safe VM
  boundaries and can be replaced without changing protocol state machines.
- Every accepted language or runtime change is backed by cross-protocol
  evidence, and unresolved limitations remain explicit in release metadata.
- All failing randomized and network-profile cases produce deterministic,
  replayable traces.
