# Release Cache Retention

Updated: 2026-09-10. Scoped V9-1 cleanup evidence, not full release acceptance.

## Interrupted Rust Writers

`terlan-build-cache` now retires abandoned `-working` sessions as well as
redundant finalized sessions in this checkout's `target/debug/incremental`.
The policy follows the pinned
[Rust 1.96 incremental filesystem implementation](https://github.com/rust-lang/rust/blob/ac68faa20/compiler/rustc_incremental/src/persist/fs.rs):
an active writer holds the session lock exclusively, while readers of finalized
sessions hold shared locks. Our cleaner requires an existing uncontended exclusive
lease and a five-minute age grace. Missing, replaced or busy locks never grant
deletion authority. Recent/future-dated directories and the newest finalized
session of each crate/flavor remain protected unless an older configuration is
superseded under the explicit age policy below.

An interrupted working directory can contain an incomplete or absent dep-graph
header. Such payloads are never promoted or reused as completed cache entries.
Deletion validates the entire recognized flat namespace, holds the lease through
retirement, and renames into the owned retirement directory before unlinking
individual files. Unknown files, nested directories and symlinks fail closed.
The next prune recovers an interrupted retirement, including partial working
payloads. Completed-cache headers retain the pinned-version check. This policy
assumes the same trusted local filesystem and functioning locks as rustc.

## Superseded Configuration Retention

Keeping one finalized session for every stable crate ID indefinitely retained
obsolete build configurations: 622 sessions occupied 65,674,170,368 allocated
bytes, despite the previous cleaner finding almost no redundant sessions. That
left only 7.5 GiB free and correctly stopped the next large Rust build at admission.

The default policy now retires a configuration's newest finalized session only
when another completed configuration of the same rustc target name has a strictly
newer timestamp and the old session is at least 72 hours old. The newest target
configuration, equal timestamps, recent sessions and targets with only one
configuration stay protected. A working or future-dated configuration cannot
authorize retirement of the last completed cache. Existing shared/exclusive
rustc leases, missing-lock refusal and the five-minute grace still apply. The
policy never scans or removes `deps`, executables, release archives or evidence.
Session age is creation age, not filesystem access time; rustc creates a new
timestamped session on reuse. This is a bounded retention tradeoff: revisiting an
expired build configuration may regenerate its incremental cache.

Lease owners explicitly unlock before closing. Closing alone can leave the lock
temporarily alive in a descriptor inherited by a concurrent subprocess launch.
A duplicated-file-description regression verifies release and reacquisition
without waiting for that duplicate to close.

- `target/v9-cache-generations-closeout.log`: 30 unit tests, three real Make
  integration tests, strict all-target Clippy and executable build pass. Coverage
  includes age boundaries, cross-configuration leases, interrupted retirement,
  preservation of compiled dependency bytes and real rustc cache reuse.
- `target/v9-cache-generation-admission.json`: 263 superseded configurations and
  three redundant sessions retired; 356 sessions retained. Unique-inode allocated
  bytes fell to 37,863,145,472. Admission passed with 33,944,629,248 user-available
  bytes. These allocated-byte and filesystem-free measurements are distinct.
- `target/v9-cache-generation-after.json`: no remaining eligible sessions, no
  unmeasured sessions, passing byte/entry budgets.
- `target/v9-cache-generation-preserved.log`: compiler, VM, native worker,
  promotion image and proof-release evidence SHA-256 hashes exactly match their
  pre-cleanup values. The removed caches are regenerable.

The first parallel unit run exposed the lock-release race; a subsequent run
passed tests but failed Clippy's test-module placement rule. Neither failed run
was used to authorize cleanup; the final closeout log is the passing evidence.

## Evidence

- `target/v9-abandoned-tests.log`: 21 cache tests, strict all-target Clippy and
  executable build pass. Coverage includes read-only audit, age/future dates,
  shared/exclusive leases, missing locks, unknown payloads, partial retirement,
  hardlinks and preservation of completed compiler outputs.
- The real-writer test parks an actual `rustc` inside a procedural macro after
  incremental admission. Pruning cannot touch its live session. After killing
  and reaping rustc, the abandoned session is reclaimed while the previous rlib
  and object inodes survive; an unchanged subsequent build reuses those objects.
- `target/v9-abandoned-audit.json` identified 18 abandoned and 10 redundant
  sessions. `target/v9-abandoned-prune.json` removed those 28 derived directories,
  retaining 593 protected sessions. Unique-inode allocated cache bytes fell from
  63,343,697,920 to 60,383,010,816 (2,960,687,104 bytes). Hardlinks mean this is not
  necessarily equal to the filesystem's free-space change.
- `target/v9-abandoned-after.json` reports zero eligible sessions and a passing
  byte/entry budget. `target/v9-abandoned-preserved.log` verifies unchanged SHA-256
  hashes for the compiler, VM, native worker and release-promotion AOT image.
  Removed caches are regenerable; source, executables and evidence remain intact.
- `target/v9-abandoned-closeout.log`: fresh Rust AST/module structure, file
  headroom, repository build/release contract, Rust quality/docs, formatting and
  whitespace checks pass. No changed Rust file exceeds its source/test size cap.

## Compiler Bootstrap Admission

Linux `terlan-compiler-bootstrap` now has a shared Make prerequisite that builds
only `terlan-build-cache` with one Cargo job, prunes eligible caches, and checks
user-available filesystem bytes before the large compiler build. The default
floor is 8 GiB (`TERLAN_BUILD_MINIMUM_FREE_BYTES`); it is a configurable positive
operational floor, not an idle-host condition or proof of sufficient space for
every build. It provides margin over the earlier bootstrap failure with roughly
4.6 GiB initially free. Zero, negative, malformed and overflowing floors fail.

The `admit --minimum-free-bytes N` command measures the actual debug build
filesystem with `fstatvfs`, excluding superuser-reserved blocks, and emits
`terlan.build-resource-admission.v1`. Cache-budget failure, filesystem errors or
insufficient headroom stop the compiler launch. A cold non-incremental cache is
allowed; unsafe/symlinked namespaces are not. The report explicitly states that
this is not a space reservation. Verified prebuilt consumers bypass this build
boundary; publication verification does not compile or prune.

- `target/v9-admission-unit.log`: 23 unit tests pass, strict all-target Clippy and
  the executable build pass.
- `target/v9-admission-make.log` and `target/v9-admission-closeout.log`: three
  integration tests execute the real Makefile and admission binary with a small
  instrumented Cargo boundary. Low space or unsafe caches stop before compiler
  launch; parallel consumers share one bootstrap and prebuilt use replays none.
- `target/v9-admission-measure.log`: an actual warm small-tool bootstrap plus
  cache/space admission took 0.44 s wall time, 0.21 s user and 0.21 s system,
  with 80,676 KiB peak RSS. This is not a cold-build or full-cycle measurement.
- Plan inspection confirms the prerequisite precedes compilation in both
  `check` and publication refresh. Publication verification has zero build/test
  replay; refresh records five literal Cargo calls with no duplicate commands,
  within its existing six-call cap. The full preparation-wide allowance rises
  from seven to eight for this one compiler-independent build, rather than
  combining cleanup with the large compiler that may fail for lack of space.

The existing `rustix 1.1.4` dependency supplies the safe filesystem query; no new
dependency version was selected. Of 72 proof candidate materials, only Cargo.lock
changed, and removing this single dependency edge reproduces its prior hash.
All fourteen slice traces are unchanged. The reviewed proof baseline updates
candidate/normalized identities only; it does not certify fresh underlying
proof/runtime execution. Proposal: `target/v9-admission-proof-proposal.log`.
The accepted identities pass the proof evidence checker (14 slices) and release
mode (six stages) in `target/v9-admission-proof-check.log`. Structural/headroom,
build/release contract, Rust quality/docs, formatting and whitespace closeout
also pass in `target/v9-admission-closeout.log`.

## Remaining Scope

This is point-in-time maintenance of the owned Linux debug incremental namespace.
Initial compiler admission does not establish ongoing resource reservation, retention across
other profiles/targets, stale executable cleanup, orphan-lock budgets, or full
candidate cold/warm/interrupted acceptance. A passing cache budget is not proof
that the filesystem has enough free space for concurrent links. These integration
requirements remain open under V9-1.
