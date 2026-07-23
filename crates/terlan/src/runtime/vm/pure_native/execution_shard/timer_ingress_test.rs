//! Adversarial tests for generation-fenced execution-shard timer ingress.

use crate::runtime::vm::actor::VmActorTimerDelivery;
use crate::runtime::vm::process::VmProcessSource;
use crate::runtime::vm::pure_native::PureNativeExecutionRuntime;
use crate::runtime::vm::ReplValue;

use super::execution_shard_test::local_boundary_named;
use super::PureNativeExecutionShard;

/// A current timer event delivers exactly once and duplicate publication is suppressed.
#[test]
fn current_timer_tick_delivers_once_through_shard_owner() {
    let mut shard = PureNativeExecutionShard::with_boundary(local_boundary_named("timer", 41));
    let owner = shard
        .actors_mut()
        .spawn_root(VmProcessSource::new("test.Timer", "owner", 0));
    shard
        .actors_mut()
        .send_after(owner, owner, ReplValue::Int(7), 0, 5)
        .expect("schedule delayed message");
    let tick = shard.issue_timer_tick(5).expect("issue timer tick");

    let advance = shard
        .apply_timer_tick(tick.clone())
        .expect("apply current timer tick")
        .expect("first timer tick executes");
    assert!(matches!(
        advance.deliveries.as_slice(),
        [VmActorTimerDelivery::Delivered { .. }]
    ));
    assert_eq!(
        shard
            .actors()
            .processes()
            .get(owner)
            .expect("timer owner")
            .mailbox_len(),
        1
    );
    assert!(shard
        .apply_timer_tick(tick)
        .expect("duplicate timer tick is classified")
        .is_none());
    assert_eq!(
        shard
            .actors()
            .processes()
            .get(owner)
            .expect("timer owner")
            .mailbox_len(),
        1
    );
}

/// A clock observation issued by another shard cannot mutate local timers.
#[test]
fn foreign_shard_timer_tick_fails_before_timer_mutation() {
    let source = PureNativeExecutionShard::with_boundary(local_boundary_named("source", 42));
    let mut destination = source.fork_empty().expect("fork destination shard");
    let owner =
        destination
            .actors_mut()
            .spawn_root(VmProcessSource::new("test.Timer", "destination", 0));
    destination
        .actors_mut()
        .send_after(owner, owner, ReplValue::Int(11), 0, 3)
        .expect("schedule destination timer");
    let foreign = source.issue_timer_tick(3).expect("issue source tick");

    let error = destination
        .apply_timer_tick(foreign)
        .expect_err("foreign shard tick must fail");
    assert!(
        error.contains("error[execution_shard.timer_identity]"),
        "{error}"
    );
    assert_eq!(destination.actors().timer_snapshots().len(), 1);
    assert_eq!(
        destination
            .actors()
            .processes()
            .get(owner)
            .expect("destination owner")
            .mailbox_len(),
        0
    );
}

/// A delayed clock event from a crashed epoch cannot fire replacement timers.
#[test]
fn stale_timer_tick_cannot_cross_execution_shard_epoch() {
    let mut shard =
        PureNativeExecutionShard::with_boundary(local_boundary_named("old-timer-image", 43));
    let old_owner = shard
        .actors_mut()
        .spawn_root(VmProcessSource::new("test.Timer", "old", 0));
    shard
        .actors_mut()
        .send_after(old_owner, old_owner, ReplValue::Int(13), 0, 7)
        .expect("schedule old timer");
    let stale = shard.issue_timer_tick(7).expect("issue old timer tick");
    assert_eq!(stale.epoch().as_u64(), 1);

    shard
        .report_crash("timer owner crashed", 100)
        .expect("record timer shard crash");
    shard
        .recover_components(
            local_boundary_named("new-timer-image", 44),
            PureNativeExecutionRuntime::runtime_default().expect("replacement runtime"),
            110,
        )
        .expect("recover timer shard");
    let new_owner = shard
        .actors_mut()
        .spawn_root(VmProcessSource::new("test.Timer", "new", 0));
    assert_eq!(new_owner, old_owner, "fixture must reuse process identity");
    shard
        .actors_mut()
        .send_after(new_owner, new_owner, ReplValue::Int(17), 0, 7)
        .expect("schedule replacement timer");

    let error = shard
        .apply_timer_tick(stale)
        .expect_err("stale epoch tick must fail");
    assert!(error.contains("StaleEpoch"), "{error}");
    assert_eq!(shard.actors().timer_snapshots().len(), 1);
    assert_eq!(
        shard
            .actors()
            .processes()
            .get(new_owner)
            .expect("replacement owner")
            .mailbox_len(),
        0
    );

    let fresh = shard.issue_timer_tick(7).expect("issue replacement tick");
    assert_eq!(fresh.epoch().as_u64(), 2);
    shard
        .apply_timer_tick(fresh)
        .expect("apply replacement tick")
        .expect("replacement tick executes");
    assert_eq!(
        shard
            .actors()
            .processes()
            .get(new_owner)
            .expect("replacement owner")
            .mailbox_len(),
        1
    );
}
