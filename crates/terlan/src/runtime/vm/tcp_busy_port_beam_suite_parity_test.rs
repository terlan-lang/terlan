use super::VmTcpRuntime;
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::{VmScheduler, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome},
    tcp_scheduler::apply_tcp_wakeups,
};

fn writer_source(index: usize) -> VmProcessSource {
    VmProcessSource::new("parity.BusyPort", format!("writer_{index}"), 0)
}

#[test]
fn busy_port_suite_nonsuspending_pressure_is_atomic_and_recovers_liveness() {
    const PRESSURE_ATTEMPTS: usize = 4_096;

    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("busy-port-pressure").expect("listener");
    let sender = tcp.connect("busy-port-pressure", "writer").expect("sender");
    let receiver = tcp
        .accept(listener, "reader")
        .expect("accept")
        .expect("receiver");
    tcp.set_stream_inbox_limit(receiver, 4)
        .expect("bounded receiver");

    let retained = vec![1, 2, 3, 4];
    assert_eq!(tcp.send(sender, retained.clone()), Ok(4));
    for attempt in 0..PRESSURE_ATTEMPTS {
        let rejected = vec![0xa0 | (attempt as u8 & 0x0f); 4];
        assert_eq!(
            tcp.send(sender, rejected),
            Err("VM TCP peer inbox is full".to_string())
        );
    }

    let pressured = tcp.inspect_stream(receiver).expect("pressured receiver");
    assert_eq!(pressured.queued_messages, 1);
    assert_eq!(pressured.queued_bytes, 4);
    assert!(!pressured.closed);
    assert!(!pressured.cancelled);
    assert_eq!(tcp.receive(receiver, 4), Ok(Some(retained)));

    assert_eq!(tcp.send(sender, b"live".to_vec()), Ok(4));
    assert_eq!(tcp.receive(receiver, 4), Ok(Some(b"live".to_vec())));
    let recovered = tcp.inspect_stream(receiver).expect("recovered receiver");
    assert_eq!(recovered.queued_messages, 0);
    assert_eq!(recovered.queued_bytes, 0);
}

#[test]
fn busy_port_suite_wakes_surviving_writers_in_fifo_order() {
    let mut processes = VmProcessTable::default();
    let writers = (1..=5)
        .map(|index| processes.spawn_root(writer_source(index)))
        .collect::<Vec<_>>();
    for writer in &writers {
        processes.get_mut(*writer).expect("writer process").block();
    }

    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("busy-port-writers").expect("listener");
    let sender = tcp.connect("busy-port-writers", "writers").expect("sender");
    let receiver = tcp
        .accept(listener, "reader")
        .expect("accept")
        .expect("receiver");
    tcp.set_stream_inbox_limit(receiver, 1)
        .expect("bounded receiver");
    tcp.send(sender, vec![42]).expect("fill receiver");
    for writer in &writers {
        assert!(tcp.park_send(sender, *writer).expect("park writer"));
    }
    assert_eq!(
        tcp.inspect_stream(receiver)
            .expect("blocked writers")
            .waiting_writers,
        5
    );

    processes
        .exit_process(writers[0], VmExitReason::Killed)
        .expect("exit first writer");
    processes
        .exit_process(writers[2], VmExitReason::Killed)
        .expect("exit third writer");

    let (received, wakeups) = tcp
        .receive_with_wakeups(receiver, 1)
        .expect("release capacity");
    assert_eq!(received, Some(vec![42]));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);
    assert_eq!(report.write_wakeups, 3);
    assert_eq!(
        report.diagnostics,
        vec![
            format!(
                "VM TCP wake for process {} failed: cannot wake exited process {}",
                writers[0].as_u64(),
                writers[0].as_u64()
            ),
            format!(
                "VM TCP wake for process {} failed: cannot wake exited process {}",
                writers[2].as_u64(),
                writers[2].as_u64()
            ),
        ]
    );
    assert_eq!(scheduler.queued_len(), 3);

    let mut scheduled = Vec::new();
    for _ in 0..3 {
        let run = scheduler
            .run_next(&mut processes, |_process, _slice| {
                VmSchedulerDecision::Block { reductions: 1 }
            })
            .expect("run surviving writer");
        assert_eq!(run.outcome, VmSchedulerOutcome::Blocked);
        scheduled.push(run.pid.expect("scheduled writer"));
    }
    assert_eq!(scheduled, vec![writers[1], writers[3], writers[4]]);
    for writer in [writers[1], writers[3], writers[4]] {
        assert_eq!(
            processes.get(writer).expect("surviving writer").state,
            VmProcessState::Blocked
        );
    }
}

#[test]
fn busy_port_suite_delayed_capacity_preserves_ordered_retries() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("busy-port-order").expect("listener");
    let sender = tcp
        .connect("busy-port-order", "ordered-writer")
        .expect("sender");
    let receiver = tcp
        .accept(listener, "ordered-reader")
        .expect("accept")
        .expect("receiver");
    tcp.set_stream_inbox_limit(receiver, 1)
        .expect("one-byte receiver");

    tcp.send(sender, vec![1]).expect("first command");
    let mut delivered = Vec::new();
    for value in 2_u8..=50 {
        assert_eq!(
            tcp.send(sender, vec![value]),
            Err("VM TCP peer inbox is full".to_string())
        );
        let current = tcp
            .receive(receiver, 1)
            .expect("drain current command")
            .expect("queued current command");
        delivered.extend(current);
        assert_eq!(tcp.send(sender, vec![value]), Ok(1));
    }
    delivered.extend(
        tcp.receive(receiver, 1)
            .expect("drain final command")
            .expect("queued final command"),
    );

    assert_eq!(delivered, (1_u8..=50).collect::<Vec<_>>());
    assert_eq!(
        tcp.inspect_stream(receiver)
            .expect("drained receiver")
            .queued_bytes,
        0
    );
}

#[test]
fn busy_port_suite_close_and_cancel_release_buffers_and_waiters() {
    let mut processes = VmProcessTable::default();
    let reader = processes.spawn_root(writer_source(1));
    let first_writer = processes.spawn_root(writer_source(2));
    let second_writer = processes.spawn_root(writer_source(3));

    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("busy-port-cleanup").expect("listener");
    let sender = tcp
        .connect("busy-port-cleanup", "cleanup-writer")
        .expect("sender");
    let receiver = tcp
        .accept(listener, "cleanup-reader")
        .expect("accept")
        .expect("receiver");
    tcp.set_stream_inbox_limit(receiver, 4)
        .expect("bounded receiver");
    tcp.send(sender, b"full".to_vec()).expect("fill receiver");
    assert!(tcp.park_send(sender, first_writer).expect("park first"));
    assert!(tcp.park_send(sender, second_writer).expect("park second"));
    assert!(tcp.park_receive(sender, reader).expect("park reader"));
    assert_eq!(tcp.metrics().waiting_readers, 1);
    assert_eq!(tcp.metrics().waiting_writers, 2);
    assert_eq!(tcp.metrics().queued_bytes, 4);

    tcp.close_stream(receiver).expect("close receiver");
    assert_eq!(
        tcp.send(sender, b"late".to_vec()),
        Err("VM TCP peer stream is closed".to_string())
    );
    tcp.cancel_stream(sender).expect("cancel sender");
    tcp.close_listener(listener).expect("close listener");

    let metrics = tcp.metrics();
    assert_eq!(metrics.open_listeners, 0);
    assert_eq!(metrics.open_streams, 0);
    assert_eq!(metrics.queued_messages, 0);
    assert_eq!(metrics.queued_bytes, 0);
    assert_eq!(metrics.waiting_readers, 0);
    assert_eq!(metrics.waiting_writers, 0);
    assert_eq!(
        tcp.send(sender, b"after-cancel".to_vec()),
        Err("VM TCP stream is cancelled".to_string())
    );
}
