use super::*;
use crate::runtime::vm::acme_worker::{VmAcmeMode, VmAcmeWorkerRequest, VmAcmeWorkerRuntime};
use crate::runtime::vm::debugger_transport::{VmDebuggerCommand, VmDebuggerTransportRuntime};
use crate::runtime::vm::package_transport::VmPackageDownloadRuntime;
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};
use crate::runtime::vm::scheduler::{VmScheduler, VmSchedulerConfig};
use crate::runtime::vm::tcp::VmTcpRuntime;
use crate::runtime::vm::timer::VmTimerTable;
use crate::runtime::vm::udp::VmUdpRuntime;

fn block_process(processes: &mut VmProcessTable, process: VmProcessId) {
    processes
        .get_mut(process)
        .expect("test process should exist")
        .block();
}

fn spawn_named(processes: &mut VmProcessTable, function: &str) -> VmProcessId {
    processes.spawn_root(VmProcessSource::new("reactor_test", function, 0))
}

#[test]
fn vm_io_reactor_loop_drains_mixed_wakeups_in_deterministic_order() {
    let mut processes = VmProcessTable::default();
    let tcp_acceptor = spawn_named(&mut processes, "tcp_acceptor");
    let udp_receiver = spawn_named(&mut processes, "udp_receiver");
    let package_receiver = spawn_named(&mut processes, "package_receiver");
    let debugger_receiver = spawn_named(&mut processes, "debugger_receiver");
    let acme_owner = spawn_named(&mut processes, "acme_owner");
    let timer_owner = spawn_named(&mut processes, "timer_owner");
    for process in [
        tcp_acceptor,
        udp_receiver,
        package_receiver,
        debugger_receiver,
        acme_owner,
        timer_owner,
    ] {
        block_process(&mut processes, process);
    }

    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("vm://http").expect("listener");
    assert!(tcp
        .park_accept(listener, tcp_acceptor)
        .expect("park accept"));
    let (_, tcp_wakes) = tcp
        .connect_with_wakeups("vm://http", "client")
        .expect("connect");

    let mut udp = VmUdpRuntime::new();
    let source_socket = udp.bind("vm://udp-source", "source").expect("source");
    let target_socket = udp.bind("vm://udp-target", "target").expect("target");
    assert!(udp
        .park_receive(target_socket, udp_receiver)
        .expect("park udp"));
    let udp_wakes = udp
        .send_to_with_wakeups(source_socket, "vm://udp-target", b"packet".to_vec())
        .expect("send udp");

    let mut package = VmPackageDownloadRuntime::new();
    let download = package
        .start_download("https://packages.example/test", "owner", 4)
        .expect("download");
    assert!(package
        .park_receive(download, package_receiver)
        .expect("park package"));
    let package_wakes = package
        .enqueue_chunk(download, b"chunk".to_vec())
        .expect("chunk");

    let mut debugger = VmDebuggerTransportRuntime::new();
    let session = debugger.open_session("debugger", 4, 4).expect("session");
    assert!(debugger
        .park_command_receive(session, debugger_receiver)
        .expect("park command"));
    let debugger_wakes = debugger
        .enqueue_command(session, VmDebuggerCommand::Step)
        .expect("debugger command");

    let mut acme = VmAcmeWorkerRuntime::new();
    let worker = acme
        .start_worker(
            acme_owner,
            VmAcmeWorkerRequest::new("example.com", "account", "cache", VmAcmeMode::Staging),
        )
        .expect("worker");
    let acme_wake = acme
        .prepare_http01_challenge(worker, "token", "key-auth")
        .expect("challenge wake");

    let mut timers = VmTimerTable::default();
    let mut timer_scheduler = VmScheduler::new(VmSchedulerConfig::default());
    let timer = timers
        .start_one_shot(&processes, timer_owner, 5)
        .expect("timer");
    let timer_event = timers.cancel(timer).expect("timer cancellation");
    block_process(&mut processes, timer_owner);
    let fired_timer = timers
        .start_one_shot(&processes, timer_owner, 6)
        .expect("fired timer");
    assert_ne!(timer, fired_timer);
    let timer_events = timers.advance_clock(&mut processes, &mut timer_scheduler, 6);

    let mut reactor = VmIoReactorLoop::new();
    for wake in tcp_wakes {
        reactor.enqueue_tcp_wake(wake);
    }
    for wake in udp_wakes {
        reactor.enqueue_udp_wake(wake);
    }
    for wake in package_wakes {
        reactor.enqueue_package_download_wake(wake);
    }
    for wake in debugger_wakes {
        reactor.enqueue_debugger_wake(wake);
    }
    reactor.enqueue_acme_worker_wake(acme_wake);
    reactor.enqueue_timer_event(timer_event);
    for event in timer_events {
        reactor.enqueue_timer_event(event);
    }

    assert_eq!(reactor.pending_len(), 6);

    let mut scheduler = VmScheduler::new(VmSchedulerConfig::default());
    let drain = reactor.drain_ready(&mut processes, &mut scheduler);

    assert_eq!(drain.total_wakeups(), 6);
    assert_eq!(drain.counts.tcp, 1);
    assert_eq!(drain.counts.udp, 1);
    assert_eq!(drain.counts.package_download, 1);
    assert_eq!(drain.counts.debugger, 1);
    assert_eq!(drain.counts.acme_worker, 1);
    assert_eq!(drain.counts.timer, 1);
    assert!(drain.diagnostics.is_empty(), "{:?}", drain.diagnostics);
    assert_eq!(
        drain.deterministic_trace,
        vec![
            format!("tcp.accept:{}", tcp_acceptor.as_u64()),
            format!("udp.receive:{}", udp_receiver.as_u64()),
            format!("package.chunk:{}", package_receiver.as_u64()),
            format!("debugger.command:{}", debugger_receiver.as_u64()),
            format!("acme.challenge_ready:{}", acme_owner.as_u64()),
            format!("timer.fired:{}", timer_owner.as_u64()),
        ]
    );
    assert_eq!(scheduler.queued_len(), 6);
    assert_eq!(reactor.pending_len(), 0);
    assert_eq!(drain.timer_outcomes.len(), 2);
    assert_eq!(drain.timer_outcomes[0].outcome, "cancelled");
    assert_eq!(drain.timer_outcomes[1].outcome, "fired");
}

#[test]
fn vm_io_reactor_preserves_all_typed_timer_outcomes_without_waking_terminal_events() {
    let mut processes = VmProcessTable::default();
    let owner = spawn_named(&mut processes, "timer-outcomes");
    let exiting_owner = spawn_named(&mut processes, "timer-owner-exit");
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::default());
    let mut reactor = VmIoReactorLoop::new();

    let mut timers = VmTimerTable::default();
    let fired = timers
        .start_one_shot(&processes, owner, 1)
        .expect("fired timer");
    assert_eq!(fired.as_u64(), 1);
    for event in timers.advance_clock(&mut processes, &mut scheduler, 1) {
        reactor.enqueue_timer_event(event);
    }
    let late = timers
        .start_one_shot(&processes, owner, 2)
        .expect("late timer");
    assert_eq!(late.as_u64(), 2);
    for event in timers.advance_clock(&mut processes, &mut scheduler, 3) {
        reactor.enqueue_timer_event(event);
    }
    let cancelled = timers
        .start_one_shot(&processes, owner, 4)
        .expect("cancelled timer");
    reactor.enqueue_timer_event(timers.cancel(cancelled).expect("cancel event"));
    timers
        .start_one_shot(&processes, exiting_owner, 4)
        .expect("owner exit timer");
    processes
        .exit_process(
            exiting_owner,
            crate::runtime::vm::process::VmExitReason::Killed,
        )
        .expect("exit owner");
    for event in timers.cancel_owner_timers(exiting_owner) {
        reactor.enqueue_timer_event(event);
    }

    let mut interval_timers = VmTimerTable::default();
    interval_timers
        .start_interval(&processes, owner, 5, 2)
        .expect("coalesced timer");
    for event in interval_timers.advance_clock(&mut processes, &mut scheduler, 10) {
        reactor.enqueue_timer_event(event);
    }
    let mut overflow_timers = VmTimerTable::default();
    overflow_timers
        .start_interval(&processes, owner, u64::MAX, 1)
        .expect("overflow timer");
    for event in overflow_timers.advance_clock(&mut processes, &mut scheduler, u64::MAX) {
        reactor.enqueue_timer_event(event);
    }

    block_process(&mut processes, owner);
    let drain = reactor.drain_ready(&mut processes, &mut scheduler);
    let outcomes = drain
        .timer_outcomes
        .iter()
        .map(|outcome| outcome.outcome)
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        vec![
            "fired",
            "deadline_missed",
            "cancelled",
            "owner_exited",
            "coalesced",
            "overflow",
        ]
    );
    assert_eq!(drain.counts.timer, 4);
    assert_eq!(
        drain.timer_outcomes[1].detail.as_deref(),
        Some("late_by_ticks=1")
    );
    assert_eq!(drain.timer_outcomes[2].kind, "one_shot");
    assert_eq!(drain.timer_outcomes[4].kind, "interval");
}

#[test]
fn vm_io_reactor_loop_reports_stale_processes_without_stopping_later_wakeups() {
    let mut processes = VmProcessTable::default();
    let live = spawn_named(&mut processes, "live");
    block_process(&mut processes, live);
    let stale = VmProcessId::from_raw_for_test(999_001);

    let mut reactor = VmIoReactorLoop::new();
    reactor.enqueue_wake(VmIoReactorWake::UdpReceive { process: stale });
    reactor.enqueue_wake(VmIoReactorWake::TimerFired { process: live });

    let mut scheduler = VmScheduler::new(VmSchedulerConfig::default());
    let drain = reactor.drain_ready(&mut processes, &mut scheduler);

    assert_eq!(drain.total_wakeups(), 2);
    assert_eq!(drain.counts.udp, 1);
    assert_eq!(drain.counts.timer, 1);
    assert_eq!(
        drain.deterministic_trace,
        vec![
            format!("udp.receive:{}", stale.as_u64()),
            format!("timer.fired:{}", live.as_u64()),
        ]
    );
    assert_eq!(drain.diagnostics.len(), 1);
    assert!(drain.diagnostics[0].contains("cannot wake missing process"));
    assert_eq!(scheduler.queued_len(), 1);
}

#[test]
fn vm_io_reactor_loop_interleaves_non_timer_wake_after_timer_storm_budget() {
    let mut processes = VmProcessTable::default();
    let timer_owners = (0..33)
        .map(|index| {
            let owner = spawn_named(&mut processes, &format!("timer_owner_{index}"));
            block_process(&mut processes, owner);
            owner
        })
        .collect::<Vec<_>>();
    let tcp_reader = spawn_named(&mut processes, "tcp_reader");
    block_process(&mut processes, tcp_reader);

    let mut reactor = VmIoReactorLoop::new();
    for owner in &timer_owners {
        reactor.enqueue_wake(VmIoReactorWake::TimerFired { process: *owner });
    }
    reactor.enqueue_wake(VmIoReactorWake::TcpRead {
        process: tcp_reader,
    });

    let mut scheduler = VmScheduler::new(VmSchedulerConfig::default());
    let drain = reactor.drain_ready(&mut processes, &mut scheduler);

    assert_eq!(drain.total_wakeups(), 34);
    assert_eq!(drain.counts.timer, 33);
    assert_eq!(drain.counts.tcp, 1);
    assert!(drain.diagnostics.is_empty(), "{:?}", drain.diagnostics);
    assert_eq!(
        drain.deterministic_trace[32],
        format!("tcp.read:{}", tcp_reader.as_u64())
    );
    assert_eq!(
        drain.deterministic_trace[33],
        format!("timer.fired:{}", timer_owners[32].as_u64())
    );
    assert_eq!(drain.max_consecutive_timer_wakeups, 32);
    assert_eq!(drain.fairness_interleaves, 1);
    assert_eq!(scheduler.queued_len(), 34);
    assert_eq!(reactor.pending_len(), 0);
}

#[test]
fn vm_io_reactor_loop_drains_timer_only_storm_after_fairness_budget() {
    let mut processes = VmProcessTable::default();
    let timer_owners = (0..40)
        .map(|index| {
            let owner = spawn_named(&mut processes, &format!("timer_only_owner_{index}"));
            block_process(&mut processes, owner);
            owner
        })
        .collect::<Vec<_>>();

    let mut reactor = VmIoReactorLoop::new();
    for owner in &timer_owners {
        reactor.enqueue_wake(VmIoReactorWake::TimerFired { process: *owner });
    }

    let mut scheduler = VmScheduler::new(VmSchedulerConfig::default());
    let drain = reactor.drain_ready(&mut processes, &mut scheduler);

    assert_eq!(drain.total_wakeups(), 40);
    assert_eq!(drain.counts.timer, 40);
    assert_eq!(drain.counts.tcp, 0);
    assert!(drain.diagnostics.is_empty(), "{:?}", drain.diagnostics);
    assert_eq!(drain.max_consecutive_timer_wakeups, 40);
    assert_eq!(drain.fairness_interleaves, 0);
    assert_eq!(scheduler.queued_len(), 40);
    assert_eq!(reactor.pending_len(), 0);
}
