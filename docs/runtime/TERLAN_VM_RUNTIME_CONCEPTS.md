# Terlan VM Runtime Concept Inventory

Status: 0.0.7 executable inventory.

Terlan VM is not an OTP compatibility project. This inventory names the
BEAM-style runtime concepts that Terlan still needs, classifies each concept by
Terlan ownership, and rejects concepts that would turn the VM back into a BEAM
clone.

## Classifications

`required-vm-semantics` means the concept must be implemented by Terlan VM
because source-level actor behavior depends on it.

`library-abstraction` means the VM provides lower-level primitives, while the
public convenience API belongs in standard library modules or packages.

`distribution-machinery` means the concept is needed for multi-VM coordination
or remote actors, but local runtime execution must not pay for it by default.

`unsupported-otp-compatibility` means the behavior is intentionally outside the
Terlan VM contract.

## Runtime Mechanics Versus Runtime Policy

The VM owns hard runtime primitives. These are operations that require direct
control over scheduling, process identity, mailboxes, timers, links, monitors,
resource ownership, native parking/resume, memory accounting, and failure
propagation.

High-level service semantics belong in Terlan stdlib. Agent, Task, GenServer,
Supervisor policy wrappers, persistent actors, typed service loops, and similar
framework-level behavior should be ordinary Terlan modules compiled over the VM
primitive surface wherever the behavior can be expressed safely in Terlan.

Magic lowering is reserved for thin primitive wrappers. A lowering rule may
expose a VM primitive, but it must not hide framework policy such as call/reply
matching, stale reply handling, timeout behavior, lifecycle callbacks, state
transition ordering, or restart policy behind opaque Rust-only behavior.

This keeps runtime policy readable, testable, portable across future VM
execution modes, and available to formal proof work. It also prevents Terlan
from rebuilding OTP as hidden compiler and runtime special cases.

## Inventory

| Concept | Classification | Terlan Direction |
| --- | --- | --- |
| process identity | required-vm-semantics | VM pid, parent pid, source identity, lifecycle state, and exit reason. |
| scheduler reductions | required-vm-semantics | Reduction budgeting, fairness, cooperative yield points, and starvation diagnostics. |
| mailbox ordering | required-vm-semantics | Priority-before-ordinary mailbox lanes with FIFO inside each lane, sender identity, and monotonic message ids. |
| selective receive | required-vm-semantics | Cursor/save-queue behavior plus receive timeout integration. |
| local spawn | required-vm-semantics | Spawn AOT-compiled TVM entrypoints into VM-owned processes. |
| local send | required-vm-semantics | Route typed envelopes through the local process table. |
| self reference | required-vm-semantics | Return the actor's typed local process reference. |
| timers | required-vm-semantics | One-shot timers, cancellation, timeout receive integration, and inspection rows. |
| links | required-vm-semantics | Linked-process failure propagation in Terlan-owned terms. |
| monitors | required-vm-semantics | Monitor refs, down messages, demonitor cleanup, and stale-ref diagnostics. |
| process aliases | required-vm-semantics | Opaque local capabilities with explicit priority admission, removal, owner-exit cleanup, and one-shot reply consumption. |
| trapped exits | required-vm-semantics | Normal/abnormal exit-message policy, ignored remote normal signals, untrappable kill translation, and linked propagation. |
| supervisor trees | required-vm-semantics | Supervisor identity, child specs, restart strategies, limits, and restart history. |
| resource ownership | required-vm-semantics | NativeBoundary owner pid, transfer policy, actor-exit cleanup, and leak diagnostics. |
| heap pressure | required-vm-semantics | Process-visible accounting used by inspection and scheduler policy. |
| hot reload generations | required-vm-semantics | Module generations, generation drain rules, and active process continuation. |
| VM inspection | required-vm-semantics | Live process, supervisor, timer, resource, mailbox, and source identity snapshots. |
| VM-owned table storage | library-abstraction | Key-value primitives suitable for ETS-like local behavior without importing ETS semantics. |
| task abstraction | library-abstraction | Standard-library API over spawn, monitor, timeout, and result delivery. |
| agent abstraction | library-abstraction | Standard-library API over a stateful process with typed get/update calls. |
| gen-server abstraction | library-abstraction | Standard-library API over process state, calls, casts, and lifecycle callbacks. |
| node identity | distribution-machinery | Explicit node id, VM id, app id, cluster id, and runtime version. |
| distributed envelopes | distribution-machinery | TETF-backed message envelopes with sender, recipient, epoch, trace id, and capability metadata. |
| cluster capability checks | distribution-machinery | Remote routing only after cluster, runtime, and capability validation. |
| network partition simulation | distribution-machinery | Docker-driven validation for latency, drop, reconnect, and stale epoch cases. |
| BEAM opcode parity | unsupported-otp-compatibility | Not a Terlan goal. |
| arbitrary OTP application boot | unsupported-otp-compatibility | Not a Terlan runtime contract. |
| ERTS packaging compatibility | unsupported-otp-compatibility | Not a Terlan release contract. |
| dynamic atom creation | unsupported-otp-compatibility | Compiler-verified atoms use exact immutable Unicode text identity; dynamic creation and a mutable BEAM-style global atom table are not exposed. |

## Rules

Required VM semantics must have executable tests before release closure.
Library abstractions must compile down to VM primitives instead of introducing a
second runtime. Distribution machinery must stay explicit and capability
checked. Unsupported OTP compatibility must fail through stable diagnostics when
encountered in active product paths.

## Runtime Mechanics Vs Runtime Policy

Terlan VM owns runtime mechanics. Terlan source owns runtime policy.

Runtime mechanics are the operations that cannot be implemented safely as
ordinary source code because they require scheduler, memory, resource, or
failure ownership:

- process identity and process lifecycle;
- local spawn and process registration;
- mailbox storage, send, receive, and selective receive;
- scheduler reductions, fairness, preemption, and parked-process wakeups;
- timers, deadlines, cancellation, and owner-exit cleanup;
- links, monitors, trapped exits, and failure propagation;
- supervisor mechanics such as child identity, restart accounting, and terminal
  failure state;
- VM-owned resources, NativeBoundary parking/resume, and actor-exit cleanup.

Runtime policy is the user-facing protocol built from those mechanics. Policy
belongs in Terlan standard-library modules whenever it can be expressed in
Terlan:

- GenServer-style call/cast/state loops;
- Agent-style typed state cells;
- Task orchestration and result collection;
- Supervisor convenience APIs and child-spec builders;
- persistent actor protocols;
- typed service loops and package-provided actor frameworks.

The compiler AOT-compiles these Terlan implementations into TVM executable
images, but it must not replace them with opaque framework-specific magic
lowering unless the lowered operation is only a thin call into a required VM
primitive. For example, `std.vm.GenServer` should be ordinary Terlan stdlib code
over process, mailbox, timer, link/monitor, and supervision primitives. Compiled
native code invokes send, receive, spawn, timer, link, monitor, and exit through
typed same-shard runtime fast paths. Canonical transitions are reserved for
actual shard, process, persistence, migration, inspection, or network
boundaries; the VM does not need a serialized instruction or bespoke
`gen_server` opcode to define GenServer behavior.

This boundary keeps reliability policy inspectable, testable, and evolvable in
Terlan source while preserving VM ownership of the hard concurrency and resource
semantics.

## Gate

The inventory is guarded by:

```bash
make vm-runtime-concept-inventory-check
```
