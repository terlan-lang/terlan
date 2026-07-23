use super::*;

/// Pause and continue clear stale step authority deterministically.
#[test]
fn debugger_pause_and_continue_own_runnable_service() {
    let mut control = VmDebuggerScheduleControl::running();
    assert_eq!(
        control.claim_runnable_slice(),
        Some(VmDebuggerSlicePermit::Running)
    );
    assert_eq!(
        control
            .apply(VmDebuggerControlCommand::Pause)
            .expect("pause"),
        VmDebuggerControlSnapshot {
            state: VmDebuggerExecutionState::Paused,
            remaining_step_slices: 0,
        }
    );
    assert_eq!(control.claim_runnable_slice(), None);
    assert_eq!(
        control
            .apply(VmDebuggerControlCommand::Continue)
            .expect("continue")
            .state,
        VmDebuggerExecutionState::Running
    );
    assert_eq!(
        control.claim_runnable_slice(),
        Some(VmDebuggerSlicePermit::Running)
    );
}

/// Step permits execute exactly once each and return the scheduler to pause.
#[test]
fn debugger_step_permits_are_bounded_and_consumed_exactly_once() {
    let mut control = VmDebuggerScheduleControl::running();
    control
        .apply(VmDebuggerControlCommand::Pause)
        .expect("pause");
    control
        .apply(VmDebuggerControlCommand::Step { slices: 2 })
        .expect("step");
    assert_eq!(
        control.claim_runnable_slice(),
        Some(VmDebuggerSlicePermit::Step)
    );
    assert_eq!(control.snapshot().remaining_step_slices, 1);
    assert_eq!(
        control.claim_runnable_slice(),
        Some(VmDebuggerSlicePermit::Step)
    );
    assert_eq!(
        control.snapshot(),
        VmDebuggerControlSnapshot {
            state: VmDebuggerExecutionState::Paused,
            remaining_step_slices: 0,
        }
    );
    assert_eq!(control.claim_runnable_slice(), None);
}

/// Invalid step requests cannot mutate scheduler debugger state.
#[test]
fn debugger_step_rejects_running_zero_and_unbounded_requests() {
    let mut control = VmDebuggerScheduleControl::running();
    assert!(control
        .apply(VmDebuggerControlCommand::Step { slices: 1 })
        .expect_err("running step")
        .contains("step_running"));
    control
        .apply(VmDebuggerControlCommand::Pause)
        .expect("pause");
    for slices in [0, VM_DEBUGGER_MAX_STEP_SLICES + 1] {
        assert!(control
            .apply(VmDebuggerControlCommand::Step { slices })
            .expect_err("invalid step count")
            .contains("step_count"));
        assert_eq!(control.snapshot().state, VmDebuggerExecutionState::Paused);
    }
}
