# Terlan VM Ownership Classification

Status: 0.0.7 baseline contract.

The Terlan VM is not an OTP compatibility project. Runtime work is classified
by Terlan product ownership before it becomes a release gate.

Terlan VM is the runtime. It owns actor scheduling, async wakeups, timers,
mailboxes, cancellation, resource lifecycle, backpressure, failure propagation,
and runtime observability. Tokio is not a Terlan runtime layer and must be
removed from product runtime paths. Maintained Rust protocol libraries may be
used behind typed native boundaries only when Terlan VM owns scheduling,
cancellation, resource lifecycle, and observability.

The VM is distribution-ready, not implicitly distributed. Local actor sends
route through the local process table and mailbox. Future distributed sends
must route through typed node/cluster references, message envelopes, capability
checks, serialization boundaries, and cluster membership metadata. `ActorRef`
may represent a local or remote actor only when the type and capability
contract allows it; `LocalRef`, `RemoteRef`, and `ClusterRef` should make that
distinction explicit as the public API grows.

The VM provides reliable primitives for distributed algorithms, not the
algorithms themselves. Stable identity, epochs, message ids, timers, failure
signals, durable append/log hooks, fencing support, mailbox ordering metadata,
resource ownership, and inspection events are VM-owned. Consensus protocols,
replication policies, CRDTs, and domain-specific distributed coordination
belong in libraries or packages built on those primitives.

Multiple Terlan apps and VM instances coordinate through explicit metadata:
application id, VM instance id, node id, cluster id, epoch, runtime version, and
declared capabilities. Coordination never implies automatic trust. A peer must
match the intended cluster/runtime boundary and must advertise every capability
needed by the message or resource operation before the VM can route work to it.

Network behavior for multi-VM coordination must be tested outside the VM core.
Docker-based harnesses may simulate latency, jitter, loss, reconnects, and
partitions, but those harnesses validate the VM coordination contract rather
than becoming runtime dependencies.

## Categories

`compiler-owned` behavior is behavior the compiler can lower, specialize,
inline, fold, or emit to another target without runtime visibility. Examples
include pure terms, arithmetic, local pure calls, simple recursion, literal
construction, target-specific code emission, and VM artifact emission.
Native-owned pure/static code stays compiler-owned unless it crosses a typed
host, resource, or IO boundary.

Purity does not make the physical storage of every value compiler-owned. The
compiler owns optimization of collection expressions, including elimination,
fusion, scalar replacement, and fixed-small-value specialization. A collection
that is actually materialized on an actor heap or in shared immutable storage
uses the execution-shard runtime's canonical storage implementation through the
typed internal runtime ABI. For maps, that implementation is adaptive CHAMP;
for lists, it is an adaptive RRB vector with compact small-list and transient
uniquely-owned forms. The application image owns the compiled call site and
optimization decision, not the materialized collection-node layout.

`vm-owned` behavior is behavior that must execute inside the Terlan runtime
because it defines actor/runtime semantics. Examples include process identity,
message passing, selective receive, scheduler reductions, timers, links,
monitors, exits, supervision, actor-visible heap and garbage collection, and
runtime diagnostics.
VM-owned behavior is limited to runtime semantics.
Distribution metadata for actor identity, node identity, routing, and
cluster-visible supervision is VM-owned when distribution mode is enabled.
Consensus algorithms are not VM-owned merely because they are distributed; only
the reliability primitives they require are VM-owned.

VM ownership of actor-visible heap and garbage collection includes allocation,
tracing, accounting, transfer, and reclamation of materialized collection
storage. It does not make pure map operations scheduler operations: compiled
code calls typed map entries directly inside the execution shard without
NativeBoundary transport or supervisor mediation.

`boundary-owned` behavior crosses typed host, native, resource, or IO edges.
Examples include HTTP, Postgres, filesystem, native modules, mobile shell
bridges, WASI workers, native resources, cancellation, backpressure, cleanup,
and host capability validation.
Boundary-owned behavior can use maintained Rust crates for protocol and OS
integration, but adapter-local async machinery is migration debt unless it is
owned and driven by the Terlan VM.

`reference-only` behavior may use OTP, Erlang, BEAM, benchmarks, or external
runtime material as semantic evidence, migration evidence, or historical
comparison. It must not become a default Terlan runtime requirement unless it
is reclassified.

`out-of-contract` behavior is intentionally unsupported by the default Terlan
runtime. Examples include OTP NIF ABI compatibility, BEAM opcode parity as a
goal, arbitrary OTP application boot, Erlang GUI tooling, Java/Jinterface, and
runtime atom creation from dynamic strings.

## Rules

Every retained VM/runtime gate must name the product capability it validates.
Compiler-owned behavior must not be promoted into VM-owned behavior only
because OTP executes it in BEAM. Reference-only corpus material must not be a
release gate by itself. Out-of-contract behavior must fail with a typed
unsupported-capability diagnostic when encountered in active product paths.

## Gate

The contract is guarded by:

```bash
make vm-ownership-classification-check
```
