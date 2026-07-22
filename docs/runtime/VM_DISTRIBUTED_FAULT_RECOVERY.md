# VM Distributed Fault Recovery Contract

This specification defines the VM-owned fault, recovery, and migration safety
contract. Source programs use `std.vm.Fault`; they do not mutate scheduler
tables or infer partition state from transport errors.

## State machine

Every active node starts in `Recovered`. Legal transitions are:

```text
Recovered -> Suspected -> Degraded -> Isolated -> Recovering -> Recovered
                       \-> Isolated             \-> Isolated
```

`Suspected -> Recovered` is permitted when a suspicion is disproved. Ticks are
monotonic per node. Replaying an identical `(node, tick, target, reason)` is
idempotent; conflicting data at the same cursor is rejected. Recovery may
complete only from `Recovering`. A recovery window expiry returns the node to
`Isolated`.

The policy declares non-zero suspicion, isolation, and recovery-window
thresholds, ordered as `suspicion < isolation < recovery`. Peers with different
valid policies resolve each field to the greater (more conservative) threshold,
which is deterministic, commutative, and preserves the ordering invariant.

## Failures and diagnostics

Failures use stable envelopes containing a node, tick, reason, and one of:

- `heartbeat_missed`
- `partition_suspected`
- `migration_timeout`
- `migration_partial_commit`
- `stale_placement_update`
- `recovery_window_expired`

Transition diagnostics are `partition_onset`, `suspect_quorum`,
`node_role_demotion`, `recovery_started`, and `recovery_completion`. Migration
timeouts and partial commits emit `migration_rollback_decision`; stale placement
rejections and recovery expiry also have stable diagnostic labels. Replay reads
sort by tick, node, kind, and reason, so cross-node arrival order does not affect
the observable stream.

## Migration completion and rollback

A stateful migration progresses through `Requested`, `Snapshotting`,
`Transferring`, and `Resuming`. Entering `Transferring` proves the state snapshot
contract; entering `Resuming` proves the in-flight message contract. Commit is
rejected unless both readiness flags are true. Timeout and partial-commit
rollback remove the in-flight intent, retain one terminal outcome, and emit one
failure envelope. Identical retries replay that outcome without duplicating
state or diagnostics. A stale retry for an older sequence cannot remove a newer
in-flight migration.

## Compatibility

Compatibility is explicit:

- `supported`: the node supports partition-tolerant execution;
- `fallback_local_only`: it does not, but a local-only lane is available;
- `feature_unsupported`: neither safe distributed execution nor a local fallback
  is available.

The VM must never silently treat an unsupported node as partition tolerant.

## Acceptance and gate

`std/vm/FaultTest.terl` covers declarations and executable classification,
isolation, bounded recovery, duplicate heartbeat suppression, compatibility,
and rollback without orphaned scheduler state. Rust tests cover oscillation,
stale rejoin, stale placement, policy mismatch, rollback loops, duplicate
messages, and out-of-order replay. The owning gate is
`make vm-distributed-scheduling-check`, after
`vm_distributed_scheduler_and_migration` and before distributed state
replication/persistence.
