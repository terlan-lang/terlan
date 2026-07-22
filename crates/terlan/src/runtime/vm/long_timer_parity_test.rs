use std::collections::BTreeMap;

use super::super::process::{VmProcessId, VmProcessSource, VmProcessState, VmProcessTable};
use super::super::scheduler::{VmScheduler, VmSchedulerConfig};
use super::{VmTimerEvent, VmTimerId, VmTimerKind, VmTimerTable};

const TICKS_PER_MINUTE: u64 = 60_000;
const MAX_TIMEOUT_MINUTES: u64 = 60;

/// Replaces OTP's wall-clock long timer suite with deterministic logical time.
///
/// The legacy suite started receive-after and BIF timers at every minute from
/// one through sixty. Its node, C driver, CPU sampler, and lateness allowance
/// are host-runtime machinery; the portable contract is exact full-width
/// deadlines for both ordinary and receive timers.
#[test]
fn long_timer_horizons_fire_once_at_exact_logical_deadlines() {
    let mut processes = VmProcessTable::default();
    let one_shot_owner = processes.spawn_root(VmProcessSource::new("timer.Long", "one_shot", 0));
    let mut scheduler =
        VmScheduler::new(VmSchedulerConfig::new(10, MAX_TIMEOUT_MINUTES as usize + 1));
    let mut timers = VmTimerTable::default();
    let mut expected = BTreeMap::new();

    for minute in 1..=MAX_TIMEOUT_MINUTES {
        let deadline = minute * TICKS_PER_MINUTE;
        let one_shot = timers
            .start_one_shot(&processes, one_shot_owner, deadline)
            .expect("long one-shot timer should start");
        let receive_owner =
            processes.spawn_root(VmProcessSource::new("timer.Long", "receive_after", 0));
        let receive_timeout = timers
            .start_receive_timeout(&mut processes, &mut scheduler, receive_owner, 0, deadline)
            .expect("long receive timeout should start");
        expected.insert(deadline, (one_shot, receive_timeout, receive_owner));
    }

    assert_eq!(timers.active_count(), expected.len() * 2);
    assert_eq!(timers.metrics().max_active, expected.len() * 2);

    for (deadline, (one_shot, receive_timeout, receive_owner)) in expected {
        assert!(timers
            .advance_clock(&mut processes, &mut scheduler, deadline - 1)
            .is_empty());
        assert_eq!(timers.remaining_ticks(one_shot, deadline - 1), Ok(1));
        assert_eq!(timers.remaining_ticks(receive_timeout, deadline - 1), Ok(1));
        assert_eq!(
            timers.advance_clock(&mut processes, &mut scheduler, deadline),
            vec![
                fired(one_shot, one_shot_owner, VmTimerKind::OneShot),
                fired(receive_timeout, receive_owner, VmTimerKind::ReceiveTimeout,),
            ]
        );
        assert_eq!(
            processes
                .get(receive_owner)
                .expect("long receive owner should remain live")
                .state,
            VmProcessState::Runnable
        );
    }

    assert_eq!(timers.active_count(), 0);
    assert_eq!(timers.metrics().fired, MAX_TIMEOUT_MINUTES * 2);
    assert_eq!(scheduler.queued_len(), MAX_TIMEOUT_MINUTES as usize);
    assert!(timers
        .advance_clock(
            &mut processes,
            &mut scheduler,
            MAX_TIMEOUT_MINUTES * TICKS_PER_MINUTE,
        )
        .is_empty());
}

fn fired(timer_id: VmTimerId, owner: VmProcessId, kind: VmTimerKind) -> VmTimerEvent {
    VmTimerEvent::Fired {
        timer_id,
        owner,
        kind,
    }
}
