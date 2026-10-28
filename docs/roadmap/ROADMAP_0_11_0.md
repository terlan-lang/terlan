# Terlan 0.11.0 Roadmap

Runtime inspection was deferred from 0.10.0 on 2026-10-27. It is not a
prerequisite for the 0.10.0 release.

## Generated Metadata and Build Reliability

Scheduled for 0.11.0 on 2026-10-28. Keep this work outside the 0.10.0
release candidate.

The 0.10.0 interpolation change required updating syntax fingerprints in
1,451 generated JavaScript `.typi.deps` files and their manifest hashes.
Generated bookkeeping should be reproducible from authoritative inputs,
without requiring developers to maintain a large set of derived files.
Interface summaries remain useful during development; release packaging
must produce and validate a complete distributable snapshot.

- [ ] Define ownership of source, interface summaries, dependency metadata,
  and release evidence. Identify which tracked artifacts can be generated
  and which are intentional reviewed API baselines or generator inputs.
- [ ] Generate `.typi`, `.typi.deps`, and derived manifest hashes in an owned
  cache during development. Automatically regenerate missing or stale data
  using source, dependency, compiler, and syntax identities. Preserve errors
  for invalid source and incompatible dependencies.
- [ ] Remove disposable generated metadata from version control after its
  consumers support regeneration. Keep API review meaningful through stable
  interface comparisons rather than fingerprint-only diffs.
- [ ] Generate and validate release metadata from the exact tagged source.
  Package the interface summaries consumers need and verify installed usage
  without access to the repository's development cache.
- [ ] Make generation deterministic and safe under interrupted or concurrent
  builds. Bound cache storage and provide cleanup for owned artifacts.
- [ ] Add inexpensive local and CI checks for generator and consumer contracts
  before expensive compilation and packaging. Keep fixtures synchronized with
  authoritative schemas and budgets, and report failures with a repair action.

Acceptance criteria:

- A clean clone builds and checks imports without pre-generated metadata.
- Missing, stale, corrupt, and interrupted cache entries recover automatically;
  invalid programs still fail with the correct diagnostics.
- Body edits, public-interface edits, dependency edits, compiler changes, and
  syntax changes invalidate the appropriate data; cached and clean results agree.
- A syntax fingerprint change does not require mass metadata commits.
- Independent generation from identical inputs produces identical metadata,
  without machine paths or timestamps affecting its contents.
- Packaged consumers work with the shipped summaries and no development cache;
  release validation rejects metadata inconsistent with the tagged inputs.
- Concurrent generation and cache cleanup cannot expose partial output or
  remove active work. Tests verify storage bounds and ownership restrictions.
- Warm builds retain measured reuse benefits, and local contract checks catch
  the stale JavaScript metadata and budget-fixture failures seen in 0.10.0.

## Runtime Inspection

- [ ] Integrate the retained Ratatui experiment into live VM inspection.
  - Reuse `crates/terlan/src/vm/instrumentation_tui.rs` and the existing
    instrumentation model rather than introducing a separate dashboard model.
  - Connect a read-only CLI dashboard to VM-owned process, scheduler, mailbox,
    and resource snapshots; show unavailable data explicitly.
  - Handle terminal resizing, disconnection, errors, and shutdown with terminal
    state restoration. Keep noninteractive inspection available.
  - Acceptance: executable integration tests drive a live VM through changing
    state and verify displayed snapshots, bounded refresh, read-only behavior,
    and terminal cleanup. Existing renderer tests remain regression coverage,
    not evidence that live integration is already complete.
