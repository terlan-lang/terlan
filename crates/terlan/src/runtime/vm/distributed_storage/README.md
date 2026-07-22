# Distributed Storage Internals

This directory owns deadline coordination for VM-managed distributed storage.
The parent module owns storage policies, snapshots, checksums, and adapter
lifecycle; this directory keeps asynchronous completion races explicit.

## Responsibilities

- Bind each pending checkpoint flush to one live process and one VM timer.
- Complete, cancel, or time out a flush exactly once.
- Preserve deterministic snapshot sequence and checksum validation.
- Keep durable state unchanged when a deadline wins the completion race.

## Core Model

`VmCheckpointFlushDeadlineQueue` indexes pending work by timer and owner. A
successful completion must first cancel its timer, proving that it still owns
the race, before the adapter advances its durable boundary. Timer delivery
removes pending state before returning a typed terminal outcome.

## Invariants

- A process may own at most one pending checkpoint flush.
- Only one-shot VM timers may resolve checkpoint deadlines.
- Owner, timer, and sequence identity must match before state changes.
- Timeout, cancellation, and owner exit never perform a partial flush.

## Testing Notes

- `deadline_test.rs` covers completion races, cancellation, timeout, owner
  exit, identity mismatches, overflow, and duplicate pending work.
- Parent distributed-storage tests cover checksum, snapshot, lifecycle, and
  corruption behavior.
