use super::{
    VmDriverCallback, VmDriverDescriptor, VmDriverQueuePlacement, VmDriverRuntime,
    VmDriverTimerEvent,
};
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessTable};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.DriverParity", name, 0)
}

fn descriptor(name: &str, queue_bytes: usize, callbacks: usize) -> VmDriverDescriptor {
    VmDriverDescriptor::new(name, queue_bytes, callbacks)
        .with_max_command_bytes(32)
        .with_max_environment_value_bytes(16)
}

#[test]
fn driver_suite_vectors_queues_timers_and_environment_are_transactional() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut drivers = VmDriverRuntime::default();

    for invalid in [
        descriptor("", 64, 8),
        descriptor("zero-queue", 0, 8),
        descriptor("zero-callbacks", 64, 0),
        descriptor("zero-command", 64, 8).with_max_command_bytes(0),
        descriptor("zero-environment", 64, 8).with_max_environment_value_bytes(0),
    ] {
        assert!(drivers.open(&processes, owner, invalid).is_err());
    }
    let driver = drivers
        .open(&processes, owner, descriptor("portable-driver", 64, 8))
        .expect("valid driver opens");
    assert_eq!(driver.as_u64(), 1, "failed opens must not consume ids");

    let empty: &[u8] = b"";
    let echoed = drivers
        .commandv(driver, owner, &[b"head", empty, b"-", b"tail"])
        .expect("scatter/gather command");
    assert_eq!(echoed, b"head-tail");
    let before_oversized = drivers.snapshot(driver).unwrap();
    assert_eq!(
        drivers.commandv(driver, owner, &[&[7; 33]]),
        Err("driver command is 33 bytes; limit is 32".to_string())
    );
    assert_eq!(drivers.snapshot(driver).unwrap(), before_oversized);

    assert_eq!(
        drivers.queue(driver, owner, VmDriverQueuePlacement::Back, &[b"tail"]),
        Ok(4)
    );
    assert_eq!(
        drivers.queue(
            driver,
            owner,
            VmDriverQueuePlacement::Front,
            &[b"head", b"-"]
        ),
        Ok(9)
    );
    assert_eq!(drivers.bytes_queued(driver, owner), Ok(9));
    assert_eq!(
        drivers.read_head(driver, owner, 64),
        Ok(b"head-tail".to_vec())
    );
    let before_pressure = drivers.snapshot(driver).unwrap();
    assert!(drivers
        .queue(driver, owner, VmDriverQueuePlacement::Back, &[&[0; 56]],)
        .is_err());
    assert_eq!(drivers.snapshot(driver).unwrap(), before_pressure);
    assert_eq!(drivers.dequeue(driver, owner, 5), Ok(b"head-".to_vec()));
    let before_bad_dequeue = drivers.snapshot(driver).unwrap();
    assert!(drivers.dequeue(driver, owner, 5).is_err());
    assert_eq!(drivers.snapshot(driver).unwrap(), before_bad_dequeue);
    assert_eq!(drivers.read_head(driver, owner, 4), Ok(b"tail".to_vec()));

    drivers
        .put_environment(driver, owner, "alpha", "one")
        .expect("driver-local environment put");
    drivers
        .put_environment(driver, owner, "beta", "two")
        .expect("second environment put");
    assert_eq!(drivers.environment(driver, owner, "alpha"), Ok(Some("one")));
    assert_eq!(drivers.environment(driver, owner, "missing"), Ok(None));
    let before_bad_environment = drivers.snapshot(driver).unwrap();
    assert!(drivers
        .put_environment(driver, owner, "alpha", "0123456789abcdefg")
        .is_err());
    assert!(drivers.put_environment(driver, owner, "", "value").is_err());
    assert_eq!(drivers.snapshot(driver).unwrap(), before_bad_environment);
    assert_eq!(drivers.environment(driver, owner, "alpha"), Ok(Some("one")));

    assert_eq!(drivers.set_timer(driver, owner, 30), Ok(30));
    assert_eq!(drivers.set_timer(driver, owner, 10), Ok(10));
    assert!(drivers.advance_to(9).unwrap().is_empty());
    assert_eq!(
        drivers.advance_to(10),
        Ok(vec![VmDriverTimerEvent {
            driver,
            controller: owner,
            deadline_tick: 10,
        }])
    );
    assert!(drivers.advance_to(10).unwrap().is_empty());
    assert_eq!(drivers.set_timer(driver, owner, 20), Ok(30));
    assert_eq!(drivers.cancel_timer(driver, owner), Ok(true));
    assert_eq!(drivers.cancel_timer(driver, owner), Ok(false));
    assert!(drivers.advance_to(30).unwrap().is_empty());
    assert!(drivers.advance_to(29).is_err());
    assert_eq!(drivers.current_tick(), 30);
    assert_eq!(drivers.set_timer(driver, owner, 5), Ok(35));
    assert!(drivers.advance_to(34).unwrap().is_empty());
    assert_eq!(drivers.advance_to(35).unwrap().len(), 1);

    let report = drivers
        .close(driver, owner)
        .expect("controller closes driver");
    assert_eq!(report.released_queue_bytes, 4);
    assert_eq!(report.released_environment_entries, 2);
    assert!(!report.cancelled_timer);
    assert!(drivers.snapshots().is_empty());
    assert!(drivers.close(driver, owner).is_err());
}

#[test]
fn driver_suite_callbacks_control_transfer_and_owner_cleanup_are_exact() {
    const CALLBACKS: u64 = 512;

    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let next_controller = processes.spawn_root(source("next_controller"));
    let stranger = processes.spawn_root(source("stranger"));
    let exited = processes.spawn_root(source("exited"));
    processes
        .exit_process(exited, VmExitReason::Normal)
        .unwrap();
    let mut drivers = VmDriverRuntime::default();
    let driver = drivers
        .open(
            &processes,
            owner,
            descriptor("async-driver", 64, CALLBACKS as usize),
        )
        .unwrap();

    for sequence in 1..=CALLBACKS {
        drivers
            .submit_callback(
                driver,
                owner,
                VmDriverCallback {
                    sequence,
                    payload: vec![(sequence % 251) as u8],
                },
            )
            .expect("bounded callback burst");
    }
    let full = drivers.snapshot(driver).unwrap();
    assert_eq!(full.pending_callbacks, CALLBACKS as usize);
    assert!(drivers
        .submit_callback(
            driver,
            owner,
            VmDriverCallback {
                sequence: CALLBACKS + 1,
                payload: vec![1],
            },
        )
        .is_err());
    assert!(drivers
        .submit_callback(
            driver,
            owner,
            VmDriverCallback {
                sequence: 1,
                payload: vec![1],
            },
        )
        .is_err());
    assert_eq!(drivers.snapshot(driver).unwrap(), full);

    let mut received = Vec::new();
    while drivers.snapshot(driver).unwrap().pending_callbacks > 0 {
        received.extend(drivers.drain_callbacks(driver, owner, 37).unwrap());
    }
    assert_eq!(received.len(), CALLBACKS as usize);
    assert_eq!(
        received
            .iter()
            .map(|callback| callback.sequence)
            .collect::<Vec<_>>(),
        (1..=CALLBACKS).collect::<Vec<_>>()
    );
    for callback in &received {
        assert_eq!(callback.payload, vec![(callback.sequence % 251) as u8]);
    }
    drivers
        .submit_callback(
            driver,
            owner,
            VmDriverCallback {
                sequence: CALLBACKS + 1,
                payload: b"retry".to_vec(),
            },
        )
        .expect("capacity rejection must not consume callback identity");

    let before_bad_transfer = drivers.snapshot(driver).unwrap();
    assert!(drivers
        .connect(&processes, driver, stranger, next_controller)
        .is_err());
    assert!(drivers.connect(&processes, driver, owner, exited).is_err());
    assert_eq!(drivers.snapshot(driver).unwrap(), before_bad_transfer);
    drivers
        .connect(&processes, driver, owner, next_controller)
        .expect("controller transfer");
    assert!(drivers.commandv(driver, owner, &[b"stale"]).is_err());
    assert_eq!(
        drivers.commandv(driver, next_controller, &[b"current"]),
        Ok(b"current".to_vec())
    );

    let second = drivers
        .open(&processes, owner, descriptor("second", 16, 2))
        .unwrap();
    drivers
        .queue(second, owner, VmDriverQueuePlacement::Back, &[b"owned"])
        .unwrap();
    drivers.set_timer(second, owner, 100).unwrap();
    processes.exit_process(owner, VmExitReason::Normal).unwrap();
    let cleanup = drivers.cleanup_process(owner);
    assert_eq!(cleanup.len(), 2);
    assert_eq!(cleanup[0].id, driver);
    assert_eq!(cleanup[0].released_callbacks, 1);
    assert_eq!(cleanup[1].id, second);
    assert_eq!(cleanup[1].released_queue_bytes, 5);
    assert!(cleanup[1].cancelled_timer);
    assert!(drivers.snapshots().is_empty());
}
