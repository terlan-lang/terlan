# Terlan 0.10.0 Roadmap

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
