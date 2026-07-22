use super::super::process::VmProcessId;
use super::{VmDebuggerCommand, VmDebuggerEvent, VmDebuggerTransportRuntime, VmDebuggerWake};

/// Verifies debugger command/event queues park and wake through VM state.
#[test]
fn debugger_transport_parks_and_wakes_command_and_event_receivers() {
    let mut runtime = VmDebuggerTransportRuntime::new();
    let session = runtime
        .open_session("debugger", 4, 4)
        .expect("open session");
    let debuggee = VmProcessId::from_raw_for_test(10);
    let debugger = VmProcessId::from_raw_for_test(20);

    assert!(runtime
        .park_command_receive(session, debuggee)
        .expect("park command"));
    assert_eq!(
        runtime
            .enqueue_command(session, VmDebuggerCommand::Step)
            .expect("enqueue command"),
        vec![VmDebuggerWake::Command {
            process: debuggee,
            session
        }]
    );
    assert_eq!(
        runtime.receive_command(session).expect("receive command"),
        Some(VmDebuggerCommand::Step)
    );

    assert!(runtime
        .park_event_receive(session, debugger)
        .expect("park event"));
    assert_eq!(
        runtime
            .enqueue_event(
                session,
                VmDebuggerEvent::Stopped {
                    process: debuggee,
                    reason: "breakpoint".to_string(),
                },
            )
            .expect("enqueue event"),
        vec![VmDebuggerWake::Event {
            process: debugger,
            session
        }]
    );
    assert_eq!(
        runtime.receive_event(session).expect("receive event"),
        Some(VmDebuggerEvent::Stopped {
            process: debuggee,
            reason: "breakpoint".to_string(),
        })
    );

    let info = runtime.inspect_session(session).expect("inspect");
    assert_eq!(info.queued_commands, 0);
    assert_eq!(info.queued_events, 0);
    assert_eq!(info.waiting_command_receivers, 0);
    assert_eq!(info.waiting_event_receivers, 0);
}

/// Verifies debugger transport backpressure, validation, and cleanup.
#[test]
fn debugger_transport_enforces_backpressure_and_closes_owner_sessions() {
    let mut runtime = VmDebuggerTransportRuntime::new();
    assert_eq!(
        runtime
            .open_session("debugger", 0, 1)
            .expect_err("zero command queue"),
        "VM debugger command queue limit must be greater than 0"
    );
    assert_eq!(
        runtime
            .open_session("debugger", 1, 0)
            .expect_err("zero event queue"),
        "VM debugger event queue limit must be greater than 0"
    );
    let session = runtime
        .open_session("debugger", 1, 1)
        .expect("open session");

    runtime
        .enqueue_command(
            session,
            VmDebuggerCommand::SetBreakpoint {
                source_map_id: "app.Main:checksum".to_string(),
                line: 7,
            },
        )
        .expect("first command");
    assert_eq!(
        runtime
            .enqueue_command(session, VmDebuggerCommand::Continue)
            .expect_err("command queue full"),
        "VM debugger command queue is full"
    );
    assert_eq!(
        runtime
            .enqueue_command(
                session,
                VmDebuggerCommand::SetBreakpoint {
                    source_map_id: "".to_string(),
                    line: 1,
                },
            )
            .expect_err("invalid breakpoint"),
        "VM debugger breakpoint source_map_id cannot be empty"
    );

    runtime
        .enqueue_event(session, VmDebuggerEvent::Output("paused".to_string()))
        .expect("event");
    assert_eq!(
        runtime
            .enqueue_event(session, VmDebuggerEvent::Diagnostic("second".to_string()))
            .expect_err("event queue full"),
        "VM debugger event queue is full"
    );

    assert_eq!(runtime.close_owner_sessions("debugger"), vec![session]);
    assert_eq!(
        runtime
            .receive_command(session)
            .expect_err("closed command receive"),
        "VM debugger session is closed"
    );
    assert_eq!(
        runtime
            .enqueue_event(session, VmDebuggerEvent::Output("late".to_string()))
            .expect_err("closed enqueue"),
        "VM debugger session is closed"
    );
}
