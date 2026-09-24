# Durable checkpoint storage

This crate implements the local persistence engine for VM-owned logical
checkpoints. The standalone VM now connects the public durable lifecycle and transactions
through sandboxed capability workers. It does not implement replication, and
the remaining public storage contracts are still release blockers.

## Volatile local backend

`LocalCheckpointStore` is a separate bounded, pure-Rust in-memory backend.
It accepts explicit byte and checkpoint-count limits, performs no filesystem or
blocking work, and loses its contents on drop. Append/replay/CAS/schema decisions
share the durable engine's transaction rules. Validation and quota checks finish
before any batch entry is installed; compaction reclaims retained bytes without
lowering the committed sequence. Borrowed checkpoints are immutable.

This engine does not confer durable-flush, replication, or host-resource authority.
Public `force_local()` and requested `local_only()` policies now execute directly
on the owning VM thread, without a storage binding or worker protocol. Each
adapter owns an independent store; equal policy names do not share its contents.
Local `flush` is an in-memory ordering barrier, not persistence. Requesting durable
flush support returns `unsupported`; requesting a durable proof fails explicitly.
Closing denies subsequent operations until reopen. The same adapter retains its
volatile state across close/reopen, but actor exit or VM shutdown destroys it.

Each local adapter reserves 16 MiB of logical checkpoint bytes and capacity for
1,024 checkpoints, including a bounded allowance for keys and index metadata.
All storage descriptors and retained views share a 64 MiB / 4,096-resource owner
budget and a 256 MiB / 16,384-resource runtime budget. These are logical allocation
bounds, not exact allocator RSS measurements. Loaded outcomes and extracted
snapshot copies are charged separately; compaction cannot reclaim retained views.
The owner releases its resources and reservations together on exit. Local
mutations check receipt capacity before changing state. There is currently no
individual descriptor-disposal API: long-lived actors must respect these limits.

The compiled Terlan fixture executes the local lifecycle, independent stores,
CAS, failed-batch rollback, replay, schema changes, compaction, retained views,
restore, close/reopen, and explicit durability rejection without a storage binding.
That fixture currently runs on Linux alongside the durable restart case; this
is not yet public-API execution coverage for every supported target.

## Ownership and durability

- SQLite owns the database format, WAL, transactions, locks, and recovery.
  No custom journal or consensus implementation is introduced.
- Every connection uses WAL and `synchronous=FULL`; successful append and
  compaction return only after the transaction commits. Durability still
  depends on the filesystem and device honoring synchronization.
- The capability worker must run blocking database work off shard-owner loops.
  It must authorize the database path and enforce owner, request-generation,
  deadline, cancellation, and resource limits. The engine does not itself
  confer filesystem authority or create an actor scheduler.
- A path must be explicit and absolute. Parent directories must already exist
  and be capability-authorized. WAL databases require a supported local
  filesystem; sharing a database file over a network mount is not replication.
- A storage error or a lost reply is not proof that a transaction failed to
  commit. Retry with the same checkpoint identities, metadata, and bytes;
  exact retained checkpoints return `Replayed`, conflicting reuse fails.
- Compaction removes old payloads but never lowers the durable high-water
  sequence. A retry whose checkpoint was compacted can no longer be positively
  identified as replay and must not recreate it with an old sequence.
- Explicit flush uses SQLite's FULL WAL checkpoint and returns the committed
  schema/sequence boundary observed before it. Reader/writer contention reports
  `storage.busy`, never a successful flush proof. Committed appends remain durable
  even when checkpoint completion is blocked. Concurrent commits can exceed the
  returned boundary; flush does not authorize copying only the main database
  file as a backup.

## Data contract

Checkpoint payloads are bounded encoded logical application state. The VM must
validate admitted types, atoms, schema identity, and unsupported resources;
native pointers, live actor heaps, continuations, open handles, timers, and
in-flight effects must not become portable payloads.

The engine validates checkpoint identities and increasing nonzero sequences,
limits individual payloads to 16 MiB, and limits batches to 1,024 checkpoints
and 64 MiB. Schema metadata and length-delimited payload bytes are bound into a
domain-separated SHA-256 digest. A digest detects corruption, not unauthorized
filesystem modification: worker sandboxing and path permissions remain required.
SQLite's own row-length limit also bounds reads from corrupted databases before
copying payloads into Rust. The connection disallows attached databases and
rejects a symlink in the final database path component; authorization of the
parent directory remains the worker's responsibility.

Database format identity is separate from application payload schema. Format 3
retains the application schema and its checkpoint boundary, and adds a persistent
32-byte instance identity generated with operating-system randomness. Format 2
is upgraded in one synchronized transaction without rewriting checkpoint payloads
or application schema. Invalid metadata cannot be relabelled or silently assigned
a replacement identity. Foreign, format-1, and future databases are rejected.
Application schema changes are transactional compare-and-swap transitions;
they do not convert existing payloads. Old checkpoints retain their schema tags
and remain readable/replayable; new checkpoints must use the current schema.
One status query reads the schema and sequence together. Failed or stale schema
changes do not advance either boundary, including after reopening the database.

Open observes identity, schema, and sequence in one SQL statement. The VM pins
the first valid identity to the adapter; a different identity on reopen yields
`storage_identity_mismatch`, fences the adapter, and requires explicit rebinding.
This identity is public metadata, not a credential or proof of resource custody.
Moving or copying a database preserves its logical identity. Copies therefore
cannot serve as evidence of independent replication, and authenticated peer
identity remains a separate required contract. Entropy failure has a typed
unavailable outcome; there is no deterministic or weak-random fallback.

The identity/upgrade backend and VM projection tests pass. The rebuilt executables
also pass the real sandboxed-worker restart and compiled Terlan AOT lifecycle,
separate-VM restart, and corruption/proof rejection checks on Linux. Both strict
production Clippy profiles pass. This does not close authentication, replication,
or execution coverage on other supported targets.

Resource-name registration now resolves supervisor-authorized, identity-pinned
database bindings and checks the real worker before installing an actor-local
registration. Unknown or unpinned names cannot create authority. Restored strings
do not restore registrations: new actors and restarted VMs must register and
check the resource again against the supervisor's retained identity pin. Logical
database identity alone is still not a credential or independent-replica proof.

### Logical state representation

The VM's logical checkpoint codec uses TETF version 1, profile 3, independently
of SQLite's database-format version and the backend's application-schema tag.
It writes a big-endian entry count followed by strictly ordered, unique scopes.
Each entry retains its namespace/key, owner node, writer sequence/node, conflict
policy, and canonical TETF value. Payloads are encoded directly from borrowed
entries; sorting does not copy their values. Restore validates the entire
representation before registering a new mutable state store.

Limits are 16 MiB of encoded bytes, 16,384 state entries, 65,536 logical value
nodes, and the shared TETF nesting bound. Collection slots are reserved against
the value limit **before** allocating, including outstanding siblings in nested
collections. The byte limit alone would not bound the memory expansion of
many compact scalar tags. Worker frame and filesystem quotas still apply
separately; accepting an in-memory checkpoint does not guarantee it can be
appended under a particular worker policy.

The codec preserves Maps, Sets, bitstrings, bytes, records, and finite declared
atoms. Atom names must be admitted by the restoring image; persisted bytes
cannot extend its atom vocabulary. Reserved native-handle fields, invalid
metadata, unknown policies/profiles/versions, duplicate or unordered scopes,
truncation, and trailing data fail explicitly. The storage engine verifies its
SHA-256 binding before a persisted payload is passed to this decoder.

`DistributedStorage.checkpoint` captures immutable encoded state;
`Snapshot.restore` returns a new actor-owned store. They are local logical-state
operations, not append/flush acknowledgements or independent-peer replication.

### Standalone VM binding

Run a built image with `terlan-vm run app.tvm --storage primary=/absolute/private/directory`.
The directory must already exist with private permissions on a supported local
filesystem. The VM starts its adjacent `terlan-native-worker` executable before
entering generated code. Source selects `policy("primary", durable(), true)`;
the Boolean is a request, not permission to create a binding. Up to 16 distinct
backend names/directories are admitted. Missing or duplicate bindings fail.

For recovery across complete VM restarts, the supervisor can pin the expected
logical identity with
`--storage "primary@${TERLAN_STORAGE_ID}=/absolute/private/directory"`.
`TERLAN_STORAGE_ID` is the nonzero 32-byte identity encoded as exactly 64 lowercase
hexadecimal digits. Trusted provisioning code obtains `StorageObservation.identity`
from the original store and retains the pin outside the replaceable database
directory. Do not learn a new pin automatically when recovery reports a mismatch.
Malformed pins and duplicate backend names are rejected before workers start.

Every new durable adapter inherits its configured pin. A fresh VM opening a
different logical database returns `storage_identity_mismatch`, keeps the adapter
closed, and denies subsequent mutations and proofs; creating another adapter or
retrying open cannot override the supervisor's pin. The unchanged original pin
accepts the original store again. Without an explicit pin, the existing binding
form learns identity on first open and protects only that adapter's lifetime;
it does not verify continuity across VM restarts. Neither form proves physical
replica independence or authenticates a copied database with the same identity.

### Resource-name validation

`db.NAME` refers only to the supervisor-pinned storage binding `NAME`. The
database provider is currently the only supported resource provider. Registration
asynchronously queries the worker's actual identity before installing the name
and expected identity; repeating registration is idempotent, not a new grant.
Validation requires existing registrations and queries each worker again. A
local-only adapter can validate these external resources, but its own checkpoint
storage remains volatile.

Each call accepts at most 16 unique names, each at most 67 UTF-8 bytes. Duplicate
or oversized lists are invalid requests. Unknown, unregistered, unpinned,
replaced, or revoked bindings fail without advancing the previous validation
proof. Malformed worker replies, actor loss, and exhausted receipt budgets cannot
install registrations. The adapter reserves bounded space for registrations and
in-flight probes; actor exit releases it. All probes of one call share the same
30-second deadline and retained owner/epoch context rather than extending the
deadline with each continuation. The owner never blocks on worker I/O.

Proofs are immutable historical observations: their count covers the completed
registration set or requested validation set, and their sequence is the adapter's
observed storage boundary. Validation is sequential across workers, not a global
atomic snapshot or a lease guaranteeing future liveness. Empty validation records
count zero. A failed check leaves previous proof metadata intact; an old proof
does not authorize subsequent resource use. Without any pinned resource provider,
the capability is unsupported and proof issuance fails explicitly.

The real Terlan AOT fixture exercises all eight resource-validation declarations
against two independently spawned workers, plus unconfigured/unknown names,
replacement rejection, immutable failure observations, and fresh-VM
re-registration. This is Linux execution evidence, not cross-target closeout or
authenticated replication evidence.

Public `open`, `append`, `flush`, `load_snapshot`, and `close` park their actor
and submit bounded worker requests. The VM owner polls generation-correlated
replies and services resident capability waits; transport threads never execute
generated code. The owner parks only when no root/resident capability work or
completion is ready. A 30-second request deadline bounds waits. Cancellation,
worker loss, and lost acknowledgments require reconciliation, not an assumption
of rollback. Existing one-MiB wire frames and sandbox quotas remain enforced.

Successful open/flush/close observe real schema/sequence state; close flushes
before revoking that adapter's open state. Independent adapters may share one
binding, with durable CAS rejecting concurrent stale writes. Exact append replay
does not lower the adapter's known sequence. A loaded checkpoint is a new
immutable actor-owned value; restoration validates its admitted atom vocabulary.
`checkpoint` defaults to application schema 1; `checkpoint_with_schema` tags
state with an explicit positive schema, and `checkpoint_schema` reads it.
Schema migration changes metadata transactionally, not existing payloads or
their logical codec. Restoring a checkpoint does not implicitly migrate it.

Public batch append, compare-and-swap append, schema migration, and compaction
use real transactions. Compaction reports its retained count and high-water
sequence from the same transaction. Atomic, schema, and CAS observations query
the worker; batch and flush proofs retain acknowledged operation boundaries.
Proof-returning calls fail explicitly when no observation is available, rather
than returning an outcome of a different type.

Snapshot-isolation proofs read and validate a committed checkpoint through the
worker, then retain immutable identity, sequence, and checksum observations in
the owning actor. Later writes, compaction, or closing the adapter cannot rewrite
that observation. Missing and corrupt checkpoints cannot produce proofs. This
is not a long-lived database transaction or permission to read deleted payloads.

Source `checkpoint_id`, `checksum`, and `expected_checksum` describe the actual
operation/checkpoint. Integer checksums are only the leading 32 SHA-256 bits for
diagnostics; they are not integrity decisions or authentication tokens. Reads
compare the entire stored 256-bit digest. A digest mismatch may therefore have
equal displayed checksums and still must fail. Full mismatch detection supplies
typed stored/calculated prefixes without echoing payloads or database paths.

Rejected obsolete checkpoint sequences are distinct from CAS-token conflicts.
For ordinary append/batch operations, a backend-observed high-water mark at or
above the rejected incoming sequence yields `stale_snapshot`, with
`local_sequence`, `incoming_sequence`, and `reject_replay` recovery metadata.
The observation comes from the transaction, not the adapter's cached sequence.
An explicit stale CAS token retains `cas_token_mismatch` and its expected/actual
sequence metadata. Exact retained replay still succeeds; compacted checkpoints
cannot be resurrected by replay, and failed writes do not change the high-water mark.

`expected_entries` and `persisted_entries` describe partial-write receipts only.
The current local/SQLite transactions do not emit such receipts, so their
non-partial outcomes return zero. In particular, these getters cannot establish
rollback after an indeterminate commit. A partial-write label without real
acknowledged counts is rejected, never projected as invented progress. This is
not evidence for the still-unimplemented peer partial-commit recovery contract.

Domain failures use a closed `StorageFailureV1` record within the existing
reply protocol. This is distinct from transport success and preserves exact
schema/sequence conflict metadata without database diagnostic text. Database
errors fence the adapter until reopen/reconciliation. Worker loss, timeouts,
and invalid batch resource bounds still raise runtime errors rather than typed
outcomes. Remaining proof/metadata APIs, other VM entry points,
and real peer replication still require integration.

## Verification

The native capability worker now has internal `runtime.storage.sequence`,
`append`, `load`, `compact`, `status`, `flush`, and `migrate_schema` operations. These are transport operations,
not evidence that the public `DistributedStorage` declarations execute in AOT.
The supervisor supplies a private, canonical directory outside worker scratch;
it is mounted at `/storage`, and requests cannot choose database paths or SQL.
The binding requires a dedicated `storage`-only worker with `blocking`
admission. SQLite opens lazily on its executor thread, not on a shard owner.
Scratch teardown does not delete the separately owned durable directory.

Linux worker startup probes the opened directory with `fstatfs`/`fstatvfs`.
Only writable ext-family, XFS, and Btrfs filesystems are admitted. Memory-backed,
network, FUSE, overlay, and unknown filesystem types fail before worker launch;
there is no permissive fallback. The ext family shares one kernel identity, so
this is not an ext4-specific check. This conservative type admission does not
prove persistent hardware, safe mount options, or power-loss behavior. Operators
must still provide persistent media that honors synchronization. Direct engine
callers remain responsible for filesystem admission; the VM worker path enforces
it before mounting the durable directory.

The existing frame, term, lifetime, and sandbox resource limits still apply;
in particular, the worker's 16 MiB per-file limit can reject a database/WAL
growth before the engine's maximum payload size is reached. Append also checks
that every resulting checkpoint can fit in a subsequent read reply. No quota
was raised to admit storage. Timeout or cancellation after execution still
requires commit reconciliation; it is not a rollback guarantee. The public AOT
restart test compiles one source image, checks typed denial without a binding,
then writes and restores in separate VM processes. It also checks competing
writers, failed-batch rollback, schema transitions, and retained checkpoints
after compaction and restart. This is not power-loss or
independent-peer replication evidence; full public API coverage remains required.
The fixture also mutates only the tail of one stored digest after all workers
exit. Its unchanged diagnostic prefix cannot make the corrupted checkpoint
loadable or eligible for a proof; missing-checkpoint proof requests also fail.

Run `cargo test --locked -p terlan-storage` and strict all-target Clippy.
Tests use temporary real files and separate child processes. They cover
reopening, competing connections, CAS conflicts, exact replay, transaction
rollback, corruption, invalid inputs, compaction, foreign database preservation,
reader/writer checkpoint contention, and abrupt process exit before and after commit. Process-exit tests do not
simulate a power failure or prove network replication.

0.0.9 release closeout additionally requires production Terlan/AOT execution,
worker isolation and cancellation, and cross-process restart/restore for claimed
local capabilities. By the 2026-09-24 scope decision, authenticated quorum
replication, automatic failover and the replaceable provider contract belong to
0.0.10. In 0.0.9 cluster capability queries are false and replication calls must
return explicit unsupported outcomes without writing locally. Passing rejection
tests is not replication coverage. Existing persistence evidence is retained;
this scope decision does not itself establish release readiness.

SQLite references: [WAL](https://www.sqlite.org/wal.html) and
[synchronization](https://www.sqlite.org/pragma.html#pragma_synchronous).
