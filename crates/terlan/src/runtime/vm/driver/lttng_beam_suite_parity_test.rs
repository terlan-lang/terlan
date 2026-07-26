use super::{
    VmDriverCallback, VmDriverDescriptor, VmDriverQueuePlacement, VmDriverRuntime,
    VmDriverTraceClass, VmDriverTraceConfig, VmDriverTraceCursor, VmDriverTraceEventKind,
};
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("parity.Lttng", name, 0)
}

fn descriptor(name: &str) -> VmDriverDescriptor {
    VmDriverDescriptor::new(name, 64, 8)
        .with_max_command_bytes(32)
        .with_max_environment_value_bytes(16)
}

/// Replaces provider-specific driver tracepoints with one typed, ordered,
/// caller-attributed VM diagnostic stream.
#[test]
fn lttng_suite_driver_lifecycle_is_provider_neutral_and_caller_attributed() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let controller = processes.spawn_root(source("controller"));
    let mut drivers = VmDriverRuntime::default();

    let untraced = drivers
        .open(&processes, owner, descriptor("untraced"))
        .expect("open while tracing is disabled");
    assert_eq!(
        drivers
            .commandv(untraced, owner, &[b"not", b"-recorded"])
            .expect("untraced command"),
        b"not-recorded"
    );
    drivers
        .close(untraced, owner)
        .expect("close untraced driver");
    assert_eq!(drivers.trace_cursor().position(), 1);

    drivers.configure_trace(VmDriverTraceConfig::all());
    let cursor = drivers.trace_cursor();
    let driver = drivers
        .open(&processes, owner, descriptor("portable-observer"))
        .expect("open traced driver");
    assert_eq!(
        drivers
            .commandv(driver, owner, &[b"ab", b"cd"])
            .expect("trace scatter/gather command"),
        b"abcd"
    );
    assert_eq!(
        drivers
            .queue(driver, owner, VmDriverQueuePlacement::Back, &[b"data"])
            .expect("queue traced bytes"),
        4
    );
    assert_eq!(
        drivers
            .dequeue(driver, owner, 2)
            .expect("dequeue traced bytes"),
        b"da"
    );
    drivers
        .submit_callback(
            driver,
            owner,
            VmDriverCallback {
                sequence: 1,
                payload: b"ready".to_vec(),
            },
        )
        .expect("submit traced readiness callback");
    assert_eq!(
        drivers
            .drain_callbacks(driver, owner, 8)
            .expect("drain traced callback")
            .len(),
        1
    );
    assert_eq!(drivers.set_timer(driver, owner, 5), Ok(5));
    assert!(drivers.advance_to(4).unwrap().is_empty());
    assert_eq!(drivers.advance_to(5).unwrap().len(), 1);
    drivers
        .connect(&processes, driver, owner, controller)
        .expect("transfer traced controller");
    assert_eq!(
        drivers
            .commandv(driver, controller, &[b"current"])
            .expect("new controller command"),
        b"current"
    );
    assert_eq!(
        drivers
            .queue(driver, controller, VmDriverQueuePlacement::Back, &[b"xyz"],)
            .expect("queue flush fixture"),
        5
    );
    assert_eq!(drivers.set_timer(driver, controller, 10), Ok(15));
    let close = drivers
        .close(driver, controller)
        .expect("close traced driver");
    assert_eq!(close.released_queue_bytes, 5);
    assert!(close.cancelled_timer);

    let trace = drivers
        .trace_since(cursor)
        .expect("read complete driver trace");
    assert_eq!(trace.events.len(), 13);
    assert!(trace
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
    assert!(trace.events.iter().all(|event| event.owner == owner));
    assert!(trace.events[..9].iter().all(|event| event.caller == owner));
    assert!(trace.events[9..]
        .iter()
        .all(|event| event.caller == controller));
    assert_eq!(
        trace
            .events
            .iter()
            .map(|event| &event.kind)
            .collect::<Vec<_>>(),
        [
            &VmDriverTraceEventKind::Opened {
                name: "portable-observer".to_string(),
            },
            &VmDriverTraceEventKind::Command {
                segments: 2,
                bytes: 4,
            },
            &VmDriverTraceEventKind::Queued {
                placement: VmDriverQueuePlacement::Back,
                bytes: 4,
                queued_bytes: 4,
            },
            &VmDriverTraceEventKind::Dequeued {
                bytes: 2,
                queued_bytes: 2,
            },
            &VmDriverTraceEventKind::CallbackSubmitted {
                callback_sequence: 1,
                bytes: 5,
            },
            &VmDriverTraceEventKind::CallbacksDrained { count: 1 },
            &VmDriverTraceEventKind::TimerSet { deadline_tick: 5 },
            &VmDriverTraceEventKind::TimerFired { deadline_tick: 5 },
            &VmDriverTraceEventKind::ControllerChanged {
                previous: owner,
                next: controller,
            },
            &VmDriverTraceEventKind::Command {
                segments: 1,
                bytes: 7,
            },
            &VmDriverTraceEventKind::Queued {
                placement: VmDriverQueuePlacement::Back,
                bytes: 3,
                queued_bytes: 5,
            },
            &VmDriverTraceEventKind::TimerSet { deadline_tick: 15 },
            &VmDriverTraceEventKind::Closed {
                process_cleanup: false,
                released_queue_bytes: 5,
                released_callbacks: 0,
                cancelled_timer: true,
                released_environment_entries: 0,
            },
        ]
    );
    assert_eq!(trace.events[7].logical_tick, 5);
    assert_eq!(
        drivers.trace_since(cursor).expect("immutable replay"),
        trace
    );
    assert!(drivers
        .trace_since(trace.next_cursor)
        .expect("delivered cursor")
        .events
        .is_empty());

    drivers.configure_trace(VmDriverTraceConfig::disabled());
    let disabled_cursor = drivers.trace_cursor();
    let disabled = drivers
        .open(&processes, owner, descriptor("disabled-again"))
        .expect("driver behavior survives disabled tracing");
    drivers.close(disabled, owner).unwrap();
    assert_eq!(drivers.trace_cursor(), disabled_cursor);
}

/// Proves tracing is bounded, filterable, and explicit about consumer lag
/// while failed driver operations consume no trace sequence.
#[test]
fn lttng_suite_trace_filter_capacity_and_cursor_validation_are_exact() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut drivers = VmDriverRuntime::default();
    drivers.configure_trace(VmDriverTraceConfig::selected([VmDriverTraceClass::Io]));
    let driver = drivers
        .open(&processes, owner, descriptor("bounded"))
        .expect("lifecycle event is filtered");
    let initial = drivers.trace_cursor();
    assert_eq!(initial.position(), 1);

    assert_eq!(
        drivers.commandv(driver, owner, &[&[0; 33]]),
        Err("driver command is 33 bytes; limit is 32".to_string())
    );
    assert_eq!(drivers.trace_cursor(), initial);

    for _ in 0..5_000 {
        assert_eq!(
            drivers
                .commandv(driver, owner, &[b"x"])
                .expect("bounded traced command"),
            b"x"
        );
    }
    assert_eq!(
        drivers.trace_since(initial),
        Err("VM driver trace cursor 1 expired; oldest retained sequence is 905".to_string())
    );
    let oldest = drivers.oldest_trace_cursor();
    assert_eq!(oldest.position(), 905);
    let retained = drivers
        .trace_since(oldest)
        .expect("read complete retained trace window");
    assert_eq!(retained.events.len(), 4_096);
    assert_eq!(retained.events.first().unwrap().sequence, 905);
    assert_eq!(retained.events.last().unwrap().sequence, 5_000);
    assert_eq!(retained.next_cursor.position(), 5_001);
    assert_eq!(retained.dropped_events, 904);
    assert!(retained.events.iter().all(|event| matches!(
        event.kind,
        VmDriverTraceEventKind::Command {
            segments: 1,
            bytes: 1
        }
    )));
    assert_eq!(
        drivers.trace_since(VmDriverTraceCursor::from_position(5_002)),
        Err("VM driver trace cursor 5002 exceeds next sequence 5001".to_string())
    );

    drivers.close(driver, owner).expect("close filtered driver");
    assert_eq!(drivers.trace_cursor().position(), 5_001);
}
