use super::actor_timer_options::{
    VmActorTimerCancelMode, VmActorTimerInformation, VmActorTimerOptionResult, VmActorTimerReadMode,
};
use super::*;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.TimerCancellationAccounting", function, 0)
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_runtime_charges_only_successful_timer_cancellation_to_owner() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("owner"));
    let requester = runtime.spawn_root(source("requester"));
    let timer = runtime
        .send_after(owner, requester, ReplValue::Int(1), 0, 20)
        .expect("schedule timer");
    let owner_before = process_reductions(&runtime, owner);
    let requester_before = process_reductions(&runtime, requester);
    let total_before = runtime.scheduler.metrics().total_reductions;

    assert_eq!(runtime.read_delayed_send(timer, 5), Ok(15));
    assert!(matches!(
        runtime
            .read_delayed_send_with_mode(requester, timer, 5, VmActorTimerReadMode::Synchronous,)
            .expect("synchronous read")
            .result,
        VmActorTimerOptionResult::Information(VmActorTimerInformation::Remaining(15))
    ));
    assert_eq!(runtime.scheduler.metrics().total_reductions, total_before);

    runtime.cancel_delayed_send(timer, 5).expect("cancel timer");
    assert_eq!(process_reductions(&runtime, owner) - owner_before, 1);
    assert_eq!(process_reductions(&runtime, requester), requester_before);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before,
        1
    );

    let total_before_stale = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .cancel_delayed_send(timer, 5)
            .expect_err("stale timer must fail"),
        format!("missing timer {}", timer.as_u64())
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_stale
    );

    let option_timer = runtime
        .send_after(owner, requester, ReplValue::Int(2), 0, 20)
        .expect("schedule option timer");
    let owner_before_option = process_reductions(&runtime, owner);
    let requester_before_option = process_reductions(&runtime, requester);
    let total_before_option = runtime.scheduler.metrics().total_reductions;
    let cancelled = runtime
        .cancel_delayed_send_with_mode(
            requester,
            option_timer,
            5,
            VmActorTimerCancelMode::Synchronous {
                include_information: true,
            },
        )
        .expect("cancel through option API");
    assert!(matches!(
        cancelled.result,
        VmActorTimerOptionResult::Information(VmActorTimerInformation::Remaining(15))
    ));
    assert_eq!(process_reductions(&runtime, owner) - owner_before_option, 1);
    assert_eq!(
        process_reductions(&runtime, requester),
        requester_before_option
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before_option,
        1
    );

    let total_before_missing_option = runtime.scheduler.metrics().total_reductions;
    let stale = runtime
        .cancel_delayed_send_with_mode(
            requester,
            option_timer,
            5,
            VmActorTimerCancelMode::Synchronous {
                include_information: true,
            },
        )
        .expect("stale option cancellation is typed information");
    assert_eq!(
        stale.result,
        VmActorTimerOptionResult::Information(VmActorTimerInformation::Missing)
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_missing_option
    );

    let active = runtime
        .send_after(owner, owner, ReplValue::Unit, 0, 20)
        .expect("schedule active timer");
    runtime
        .exit_actor(requester, VmExitReason::Normal)
        .expect("exit requester");
    let total_before_invalid_requesters = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .cancel_delayed_send_with_mode(
                VmProcessId::from_raw_for_test(404),
                active,
                5,
                VmActorTimerCancelMode::Synchronous {
                    include_information: false,
                },
            )
            .expect_err("missing requester must fail"),
        "missing sender process 404"
    );
    assert_eq!(
        runtime
            .cancel_delayed_send_with_mode(
                requester,
                active,
                5,
                VmActorTimerCancelMode::Synchronous {
                    include_information: false,
                },
            )
            .expect_err("exited requester must fail"),
        format!("sender process {} has exited", requester.as_u64())
    );
    assert_eq!(runtime.read_delayed_send(active, 5), Ok(15));
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_invalid_requesters
    );
    assert_eq!(runtime.total_memory_reductions(), 0);
}
