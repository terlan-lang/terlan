use super::{VmExitReason, VmProcessSource, VmProcessState, VmProcessTable};

const UNICODE_PATH: &str = "/tmp/terlan/☠☠☠/erl_544.terl";

#[test]
fn unicode_source_path_survives_process_failure_stacktrace() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("wait", 2));
    let process = processes.get_mut(pid).expect("spawned process");
    process
        .enter_execution_frame(source("err", 0), 7, 12)
        .expect("enter failing frame");
    processes
        .exit_process(pid, VmExitReason::Error("err".to_string()))
        .expect("record process failure");

    let snapshot = processes.snapshot(pid).expect("postmortem snapshot");
    assert_eq!(
        snapshot.state,
        VmProcessState::Exited(VmExitReason::Error("err".to_string()))
    );
    assert_eq!(snapshot.current_stacktrace.len(), 2);
    assert!(snapshot
        .current_stacktrace
        .iter()
        .all(|frame| frame.source.source_path.as_deref() == Some(UNICODE_PATH)));
    assert_eq!(
        snapshot.current_stacktrace[0].render(),
        "erl_544.err/0 [/tmp/terlan/☠☠☠/erl_544.terl] @vm:7"
    );
    assert_eq!(
        snapshot.current_stacktrace[1].render(),
        "erl_544.wait/2 [/tmp/terlan/☠☠☠/erl_544.terl] @vm:12"
    );
}

#[test]
fn process_stack_frame_escapes_path_controls_without_escaping_unicode() {
    let mut processes = VmProcessTable::default();
    let source =
        VmProcessSource::new("app.Worker", "run", 0).with_source_path("/tmp/☠/line\nname\t.terl");
    let pid = processes.spawn_root(source);

    assert_eq!(
        processes
            .snapshot(pid)
            .expect("process snapshot")
            .current_location
            .render(),
        "app.Worker.run/0 [/tmp/☠/line\\nname\\t.terl] @vm:0"
    );
}

fn source(function: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("erl_544", function, arity).with_source_path(UNICODE_PATH)
}
