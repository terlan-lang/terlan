use super::{VmTcpRuntime, VmTcpWake};
use crate::runtime::vm::{
    process::{VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::{VmScheduler, VmSchedulerConfig},
    tcp_scheduler::apply_tcp_wakeups,
};

#[test]
fn async_ports_suite_sustained_nonsuspending_pressure_recovers_without_false_death() {
    const PAYLOAD_BYTES: usize = 10 * 1024;
    const PRESSURE_ATTEMPTS: usize = 4_096;

    let mut processes = VmProcessTable::default();
    let first_writer =
        processes.spawn_root(VmProcessSource::new("parity.AsyncPorts", "first_writer", 0));
    let second_writer = processes.spawn_root(VmProcessSource::new(
        "parity.AsyncPorts",
        "second_writer",
        0,
    ));
    for writer in [first_writer, second_writer] {
        processes.get_mut(writer).expect("writer process").block();
    }
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(100, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("async-port-fixture").expect("listener");
    let sender = tcp
        .connect("async-port-fixture", "writers")
        .expect("sender stream");
    let receiver = tcp
        .accept(listener, "delayed-reader")
        .expect("accept")
        .expect("receiver stream");
    tcp.set_stream_inbox_limit(receiver, PAYLOAD_BYTES)
        .expect("bounded receiver");

    let first_payload = (0..PAYLOAD_BYTES)
        .map(|index| (index as u8).wrapping_mul(17))
        .collect::<Vec<_>>();
    assert_eq!(
        tcp.send(sender, first_payload.clone())
            .expect("initial send"),
        PAYLOAD_BYTES
    );

    let rejected_payload = vec![0xa5; PAYLOAD_BYTES];
    for _ in 0..PRESSURE_ATTEMPTS {
        assert_eq!(
            tcp.send(sender, rejected_payload.clone())
                .expect_err("nonblocking pressure"),
            "VM TCP peer inbox is full"
        );
    }
    let pressured = tcp.inspect_stream(receiver).expect("pressured receiver");
    assert_eq!(pressured.queued_messages, 1);
    assert_eq!(pressured.queued_bytes, PAYLOAD_BYTES);
    assert!(!pressured.closed);
    assert!(!pressured.cancelled);

    assert!(tcp
        .park_send(sender, first_writer)
        .expect("park first writer"));
    assert!(tcp
        .park_send(sender, second_writer)
        .expect("park second writer"));
    assert!(tcp
        .park_send(sender, first_writer)
        .expect("duplicate first writer is idempotent"));
    assert_eq!(
        tcp.inspect_stream(receiver)
            .expect("waiting writers")
            .waiting_writers,
        2
    );

    let (received, wakeups) = tcp
        .receive_with_wakeups(receiver, PAYLOAD_BYTES)
        .expect("delayed drain");
    assert_eq!(received, Some(first_payload));
    assert_eq!(
        wakeups,
        vec![
            VmTcpWake::Write {
                process: first_writer,
                stream: sender,
            },
            VmTcpWake::Write {
                process: second_writer,
                stream: sender,
            },
        ]
    );
    let report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);
    assert_eq!(report.write_wakeups, 2);
    assert!(report.diagnostics.is_empty());
    assert_eq!(scheduler.queued_len(), 2);
    for writer in [first_writer, second_writer] {
        assert_eq!(
            processes.get(writer).expect("writer process").state,
            VmProcessState::Runnable
        );
    }

    assert_eq!(
        tcp.send(sender, rejected_payload.clone())
            .expect("retry after readiness"),
        PAYLOAD_BYTES
    );
    assert_eq!(
        tcp.receive(receiver, PAYLOAD_BYTES)
            .expect("receive retried payload"),
        Some(rejected_payload)
    );
    assert_eq!(
        tcp.send(sender, b"test".to_vec())
            .expect("final liveness probe"),
        4
    );
    assert_eq!(
        tcp.receive(receiver, 4).expect("final probe response"),
        Some(b"test".to_vec())
    );
    let healthy = tcp.inspect_stream(receiver).expect("healthy receiver");
    assert_eq!(healthy.queued_bytes, 0);
    assert_eq!(healthy.waiting_writers, 0);
    assert!(!healthy.closed);
    assert!(!healthy.cancelled);
}
