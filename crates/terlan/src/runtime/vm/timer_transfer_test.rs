use super::transfer::VmTimerTransfer;
use super::*;
use crate::runtime::vm::process::VmProcessSource;

fn owner_process() -> (VmProcessTable, VmProcessId) {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Timer", "run", 0));
    (processes, owner)
}

#[test]
fn timer_transfer_preserves_ids_deadlines_kinds_and_clock_position() {
    let (mut processes, owner) = owner_process();
    let mut source = VmTimerTable::default();
    source
        .start_one_shot(&processes, owner, 20)
        .expect("one-shot timer");
    source
        .start_interval(&processes, owner, 30, 5)
        .expect("interval timer");
    source.advance_clock(&mut processes, &mut VmScheduler::default(), 7);
    let transfer = source.detach_owner_timer_state(owner);
    assert_eq!(transfer.owner(), owner);
    assert_eq!(transfer.len(), 2);
    assert!(source.snapshots().is_empty());

    let mut destination = VmTimerTable::default();
    destination
        .import_timer_transfer(transfer)
        .expect("import timers");
    assert_eq!(destination.current_tick(), 7);
    let snapshots = destination.snapshots();
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].deadline_tick, 20);
    assert_eq!(snapshots[0].kind, VmTimerKind::OneShot);
    assert_eq!(snapshots[1].deadline_tick, 30);
    assert_eq!(snapshots[1].kind, VmTimerKind::Interval);
}

#[test]
fn timer_identity_or_clock_collision_returns_state_for_rollback() {
    let (mut processes, owner) = owner_process();
    let mut source = VmTimerTable::default();
    source
        .start_one_shot(&processes, owner, 20)
        .expect("source timer");
    source.advance_clock(&mut processes, &mut VmScheduler::default(), 5);
    let transfer = source.detach_owner_timer_state(owner);
    let mut destination = VmTimerTable::default();
    destination
        .start_one_shot(&processes, owner, 40)
        .expect("destination collision");
    destination.advance_clock(&mut processes, &mut VmScheduler::default(), 4);
    let failure = destination
        .import_timer_transfer(transfer)
        .expect_err("clock mismatch must preserve timers");
    assert!(failure.reason().contains("clock mismatch"));
    source
        .import_timer_transfer(failure.into_transfer())
        .expect("restore source timers");
    assert_eq!(
        source
            .snapshots()
            .into_iter()
            .filter(|timer| timer.owner == owner)
            .count(),
        1
    );
}

#[test]
fn timer_transfer_is_send_even_when_empty() {
    fn assert_send<T: Send>() {}
    assert_send::<VmTimerTransfer>();
    let (_, owner) = owner_process();
    assert_eq!(
        VmTimerTable::default()
            .detach_owner_timer_state(owner)
            .len(),
        0
    );
}
