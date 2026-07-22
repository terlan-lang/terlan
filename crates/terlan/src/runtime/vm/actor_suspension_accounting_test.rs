use super::*;
use crate::runtime::vm::process::VmProcessState;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.SuspensionAccounting", function, 0)
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_runtime_charges_only_successful_suspension_operations_to_actor() {
    let mut runtime = VmActorRuntime::default();
    let runnable = runtime.spawn_root(source("runnable"));
    let runnable_before = process_reductions(&runtime, runnable);
    let total_before_runnable = runtime.scheduler.metrics().total_reductions;

    runtime.suspend(runnable).expect("suspend runnable actor");
    runtime
        .suspend(runnable)
        .expect("idempotent suspension remains successful");
    runtime.resume(runnable).expect("resume runnable actor");

    assert_eq!(process_reductions(&runtime, runnable) - runnable_before, 3);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before_runnable,
        3
    );

    let blocked = runtime.spawn_root(source("blocked"));
    assert_eq!(
        runtime
            .receive_next_or_block(blocked)
            .expect("block actor before suspension"),
        VmActorReceive::Blocked
    );
    let blocked_before = process_reductions(&runtime, blocked);
    let total_before_blocked = runtime.scheduler.metrics().total_reductions;
    runtime.suspend(blocked).expect("suspend blocked actor");
    runtime.resume(blocked).expect("resume blocked actor");
    assert_eq!(process_reductions(&runtime, blocked) - blocked_before, 2);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before_blocked,
        2
    );
    assert_eq!(
        runtime.processes.get(blocked).expect("blocked actor").state,
        VmProcessState::Blocked
    );

    let exited = runtime.spawn_root(source("exited"));
    runtime
        .exit_actor(exited, VmExitReason::Normal)
        .expect("exit actor");
    let missing = VmProcessId::from_raw_for_test(404);
    let total_before_rejections = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .suspend(missing)
            .expect_err("missing actor cannot suspend"),
        "cannot suspend missing process 404"
    );
    assert_eq!(
        runtime
            .resume(missing)
            .expect_err("missing actor cannot resume"),
        "cannot resume missing process 404"
    );
    assert_eq!(
        runtime
            .suspend(exited)
            .expect_err("exited actor cannot suspend"),
        format!("cannot suspend exited process {}", exited.as_u64())
    );
    assert_eq!(
        runtime
            .resume(exited)
            .expect_err("exited actor cannot resume"),
        format!("cannot resume exited process {}", exited.as_u64())
    );
    assert_eq!(
        runtime
            .resume(runnable)
            .expect_err("runnable actor cannot resume"),
        format!(
            "cannot resume process {}: process is not suspended",
            runnable.as_u64()
        )
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_rejections
    );
    assert_eq!(runtime.total_memory_reductions(), 0);
}
