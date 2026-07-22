use super::apply_tcp_wakeups;
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::{VmScheduler, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome},
    tcp::{VmTcpRuntime, VmTcpWake},
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

/// Verifies TCP accept wakeups resume blocked VM processes.
///
/// Inputs:
/// - A blocked acceptor process parked on an empty VM TCP listener.
///
/// Output:
/// - Test passes when a later connection wakes and queues the process through
///   the VM scheduler.
///
/// Transformation:
/// - Exercises the TCP-to-scheduler adapter without coupling TCP storage to
///   scheduler queue internals.
#[test]
fn tcp_scheduler_adapter_wakes_blocked_accept_process() {
    let mut processes = VmProcessTable::default();
    let acceptor = processes.spawn_root(source("acceptor"));
    processes
        .get_mut(acceptor)
        .expect("acceptor should exist")
        .block();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("service").expect("listen");

    assert!(tcp.park_accept(listener, acceptor).expect("park acceptor"));
    let (_client, wakeups) = tcp
        .connect_with_wakeups("service", "client")
        .expect("connect");
    let report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);

    assert_eq!(report.accept_wakeups, 1);
    assert_eq!(report.read_wakeups, 0);
    assert!(report.diagnostics.is_empty());
    assert_eq!(
        processes.get(acceptor).expect("acceptor").state,
        VmProcessState::Runnable
    );
    assert_eq!(scheduler.queued_len(), 1);
    assert_eq!(
        scheduler
            .run_next(&mut processes, |_process, _slice| {
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("scheduled acceptor should run")
            .outcome,
        VmSchedulerOutcome::Ran
    );
}

/// Verifies TCP read wakeups resume blocked VM processes.
///
/// Inputs:
/// - A blocked reader process parked on an empty VM TCP stream.
///
/// Output:
/// - Test passes when later peer bytes wake and queue the process through the
///   VM scheduler.
///
/// Transformation:
/// - Locks the HTTP stream-read scheduling handoff before production HTTP uses
///   VM TCP resources directly.
#[test]
fn tcp_scheduler_adapter_wakes_blocked_read_process() {
    let mut processes = VmProcessTable::default();
    let reader = processes.spawn_root(source("reader"));
    processes
        .get_mut(reader)
        .expect("reader should exist")
        .block();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("service").expect("listen");
    let client = tcp.connect("service", "client").expect("connect");
    let server = tcp
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");

    assert!(tcp.park_receive(server, reader).expect("park reader"));
    let (_sent, wakeups) = tcp
        .send_with_wakeups(client, b"hello".to_vec())
        .expect("send");
    let report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);

    assert_eq!(report.accept_wakeups, 0);
    assert_eq!(report.read_wakeups, 1);
    assert!(report.diagnostics.is_empty());
    assert_eq!(
        scheduler
            .run_next(&mut processes, |_process, _slice| {
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("reader should run")
            .outcome,
        VmSchedulerOutcome::Ran
    );
    assert_eq!(
        tcp.receive(server, 1024).expect("receive"),
        Some(b"hello".to_vec())
    );
}

/// Verifies TCP write wakeups resume blocked VM processes.
///
/// Inputs:
/// - A blocked writer process parked because the peer stream inbox is full.
///
/// Output:
/// - Test passes when receiving from the peer emits a write wakeup that queues
///   the writer through the VM scheduler.
///
/// Transformation:
/// - Locks write-side backpressure handoff for future HTTP response streaming.
#[test]
fn tcp_scheduler_adapter_wakes_blocked_write_process() {
    let mut processes = VmProcessTable::default();
    let writer = processes.spawn_root(source("writer"));
    processes
        .get_mut(writer)
        .expect("writer should exist")
        .block();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("service").expect("listen");
    let client = tcp.connect("service", "client").expect("connect");
    let server = tcp
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");

    tcp.set_stream_inbox_limit(server, 3)
        .expect("set inbox limit");
    tcp.send(client, b"abc".to_vec()).expect("fill inbox");
    assert!(tcp.park_send(client, writer).expect("park writer"));
    let (_received, wakeups) = tcp
        .receive_with_wakeups(server, 3)
        .expect("receive with wakeups");
    let report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);

    assert_eq!(report.write_wakeups, 1);
    assert!(report.diagnostics.is_empty());
    assert_eq!(
        scheduler
            .run_next(&mut processes, |_process, _slice| {
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("writer should run")
            .outcome,
        VmSchedulerOutcome::Ran
    );
}

/// Verifies stale TCP wakeups become diagnostics instead of scheduler panics.
///
/// Inputs:
/// - Missing and exited process ids in TCP readiness wake intents.
///
/// Output:
/// - Test passes when diagnostics are stable and no process is queued.
///
/// Transformation:
/// - Keeps cancelled/stale stream readiness safe during actor shutdown races.
#[test]
fn tcp_scheduler_adapter_reports_missing_and_exited_wake_targets() {
    let mut processes = VmProcessTable::default();
    let exited = processes.spawn_root(source("exited"));
    processes
        .exit_process(exited, VmExitReason::Normal)
        .expect("exit");
    let missing = VmProcessId::from_raw_for_test(99);
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("service").expect("listen");
    let client = tcp.connect("service", "client").expect("connect");
    let server = tcp
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");

    let report = apply_tcp_wakeups(
        &mut processes,
        &mut scheduler,
        vec![
            VmTcpWake::Accept {
                process: missing,
                listener,
            },
            VmTcpWake::Read {
                process: exited,
                stream: server,
            },
        ],
    );

    assert_eq!(report.accept_wakeups, 0);
    assert_eq!(report.read_wakeups, 0);
    assert_eq!(
        report.diagnostics,
        vec![
            "VM TCP wake for process 99 failed: cannot wake missing process 99".to_string(),
            "VM TCP wake for process 1 failed: cannot wake exited process 1".to_string()
        ]
    );
    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(tcp.send(client, b"late".to_vec()).expect("send"), 4);
}
