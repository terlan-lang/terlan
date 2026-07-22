use super::{VmExitReason, VmProcessLocation, VmProcessSource, VmProcessState, VmProcessTable};
use crate::runtime::vm::scheduler::{VmScheduler, VmSchedulerDecision};

fn source(function: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("app.Worker", function, arity)
}

#[test]
fn process_location_starts_at_entry_instruction() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("main", 0));

    let snapshot = processes
        .snapshot(pid)
        .expect("process should be inspectable");
    let location = VmProcessLocation {
        source: source("main", 0),
        instruction_offset: 0,
    };
    assert_eq!(snapshot.current_location, location);
    assert_eq!(snapshot.current_stacktrace, [location]);
}

#[test]
fn process_stacktrace_tracks_call_entry_and_return() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("main", 0));
    let process = processes.get_mut(pid).expect("process should exist");
    process.set_current_location(source("main", 0), 10);
    process
        .enter_execution_frame(source("handle", 2), 0, 11)
        .expect("live process should enter call frame");
    process.set_current_location(source("handle", 2), 23);

    assert_eq!(
        process.current_stacktrace(),
        [
            VmProcessLocation {
                source: source("handle", 2),
                instruction_offset: 23,
            },
            VmProcessLocation {
                source: source("main", 0),
                instruction_offset: 11,
            },
        ]
    );
    assert_eq!(
        process
            .pop_execution_frame()
            .expect("called frame should return"),
        VmProcessLocation {
            source: source("handle", 2),
            instruction_offset: 23,
        }
    );
    assert_eq!(process.current_location().source, source("main", 0));
    assert_eq!(process.current_location().instruction_offset, 11);
    let root = process.current_location().clone();
    assert_eq!(
        process
            .pop_execution_frame()
            .expect_err("root frame must be retained"),
        "cannot pop the root process execution frame"
    );
    assert_eq!(process.current_location(), &root);
}

#[test]
fn nested_call_frames_restore_continuations_in_lifo_order() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("main", 0));
    let process = processes.get_mut(pid).expect("process should exist");
    process.set_current_location(source("main", 0), 5);
    process
        .enter_execution_frame(source("dispatch", 1), 0, 6)
        .expect("live process should enter dispatch frame");
    process.set_current_location(source("dispatch", 1), 10);
    process
        .enter_execution_frame(source("leaf", 2), 2, 11)
        .expect("live process should enter leaf frame");

    assert_eq!(
        process.current_stacktrace(),
        [
            VmProcessLocation {
                source: source("leaf", 2),
                instruction_offset: 2,
            },
            VmProcessLocation {
                source: source("dispatch", 1),
                instruction_offset: 11,
            },
            VmProcessLocation {
                source: source("main", 0),
                instruction_offset: 6,
            },
        ]
    );

    process
        .pop_execution_frame()
        .expect("leaf frame should return");
    assert_eq!(
        process.current_location(),
        &VmProcessLocation {
            source: source("dispatch", 1),
            instruction_offset: 11,
        }
    );
    process
        .pop_execution_frame()
        .expect("dispatch frame should return");
    assert_eq!(
        process.current_location(),
        &VmProcessLocation {
            source: source("main", 0),
            instruction_offset: 6,
        }
    );
}

#[test]
fn call_stack_snapshot_is_detached_from_later_returns() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("main", 0));
    processes
        .get_mut(pid)
        .expect("process should exist")
        .enter_execution_frame(source("worker", 1), 3, 9)
        .expect("live process should enter worker frame");
    let snapshot = processes.snapshot(pid).expect("process snapshot");

    processes
        .get_mut(pid)
        .expect("process should exist")
        .pop_execution_frame()
        .expect("worker frame should return");

    assert_eq!(snapshot.current_stacktrace.len(), 2);
    assert_eq!(snapshot.current_stacktrace[0].source, source("worker", 1));
    assert_eq!(
        processes
            .snapshot(pid)
            .expect("current process snapshot")
            .current_stacktrace
            .len(),
        1
    );
}

#[test]
fn scheduler_execution_updates_process_location() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("main", 0));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&processes, pid)
        .expect("process should schedule");

    scheduler
        .run_next(&mut processes, |process, _| {
            process.set_current_location(source("handle", 2), 17);
            VmSchedulerDecision::Yield { reductions: 3 }
        })
        .expect("scheduler run should succeed");

    let snapshot = processes.snapshot(pid).expect("process snapshot");
    assert_eq!(snapshot.source, source("main", 0));
    assert_eq!(
        snapshot.current_location,
        VmProcessLocation {
            source: source("handle", 2),
            instruction_offset: 17,
        }
    );
    assert_eq!(snapshot.current_stacktrace, [snapshot.current_location]);
}

#[test]
fn process_location_accepts_loop_back_edges() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("loop", 1));
    let process = processes.get_mut(pid).expect("process should exist");

    process.set_current_location(source("loop", 1), 31);
    process.set_current_location(source("loop", 1), 7);

    assert_eq!(process.current_location().instruction_offset, 7);
}

#[test]
fn exited_process_snapshot_retains_last_execution_location() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("main", 0));
    processes
        .get_mut(pid)
        .expect("process should exist")
        .enter_execution_frame(source("crash", 1), 23, 1)
        .expect("live process should enter crash frame");

    processes
        .exit_process(pid, VmExitReason::Error("boom".to_string()))
        .expect("process should exit");

    let snapshot = processes.snapshot(pid).expect("exited process snapshot");
    assert_eq!(
        snapshot.state,
        VmProcessState::Exited(VmExitReason::Error("boom".to_string()))
    );
    assert_eq!(
        snapshot.current_location,
        VmProcessLocation {
            source: source("crash", 1),
            instruction_offset: 23,
        }
    );
    assert_eq!(snapshot.current_stacktrace.len(), 2);
    assert_eq!(snapshot.current_stacktrace[0], snapshot.current_location);
}

#[test]
fn exited_process_rejects_call_entry_without_mutating_postmortem_stack() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("main", 0));
    processes
        .get_mut(pid)
        .expect("process should exist")
        .enter_execution_frame(source("crash", 1), 23, 1)
        .expect("live process should enter crash frame");
    processes
        .exit_process(pid, VmExitReason::Error("boom".to_string()))
        .expect("process should exit");
    let before = processes.snapshot(pid).expect("postmortem snapshot");

    let error = processes
        .get_mut(pid)
        .expect("exited record should remain inspectable")
        .enter_execution_frame(source("late", 0), 0, 24)
        .expect_err("exited process must reject a new call frame");

    assert_eq!(
        error,
        "cannot enter an execution frame for an exited process"
    );
    assert_eq!(processes.snapshot(pid).expect("unchanged snapshot"), before);
}
