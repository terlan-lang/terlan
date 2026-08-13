use super::*;

#[test]
pub(super) fn timer_table_rejects_cancellation_token_from_another_owner() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let other = processes.spawn_root(source("other"));
    let mut timers = VmTimerTable::default();
    let timer = timers
        .start_one_shot(&processes, owner, 5)
        .expect("timer should start");

    assert_eq!(
        timers
            .cancel_with_token(VmTimerCancellationToken {
                timer_id: timer,
                owner: other,
            })
            .expect_err("foreign cancellation authority should fail"),
        "timer 1 cancellation token owner mismatch: expected 1, observed 2"
    );
    assert_eq!(timers.snapshots().len(), 1);
    assert_eq!(
        timers
            .cancellation_token(VmTimerId(999))
            .expect_err("missing timer cannot grant cancellation authority"),
        "missing timer 999"
    );
}

#[test]
pub(super) fn timer_table_cancels_owner_timers_in_stable_order() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let other = processes.spawn_root(source("other"));
    let mut timers = VmTimerTable::default();
    let first = timers
        .start_one_shot(&processes, owner, 30)
        .expect("first owner timer");
    let retained = timers
        .start_one_shot(&processes, other, 10)
        .expect("other owner timer");
    let second = timers
        .start_interval(&processes, owner, 20, 5)
        .expect("second owner timer");

    assert_eq!(
        timers.cancel_owner_timers(owner),
        vec![
            VmTimerEvent::OwnerExited {
                timer_id: first,
                owner,
                kind: VmTimerKind::OneShot,
            },
            VmTimerEvent::OwnerExited {
                timer_id: second,
                owner,
                kind: VmTimerKind::Interval,
            },
        ]
    );
    assert_eq!(
        timers
            .snapshots()
            .iter()
            .map(|timer| timer.id)
            .collect::<Vec<_>>(),
        vec![retained]
    );
}

#[test]
pub(super) fn timer_table_covers_all_late_interval_overflow_boundaries() {
    let owner = VmProcessId::from_raw_for_test(1);
    let cases = [
        VmTimer {
            id: VmTimerId(1),
            owner,
            deadline_tick: 0,
            kind: VmTimerKind::Interval,
            interval_ticks: Some(1),
        },
        VmTimer {
            id: VmTimerId(2),
            owner,
            deadline_tick: 0,
            kind: VmTimerKind::Interval,
            interval_ticks: Some(2),
        },
        VmTimer {
            id: VmTimerId(3),
            owner,
            deadline_tick: u64::MAX - 5,
            kind: VmTimerKind::Interval,
            interval_ticks: Some(4),
        },
    ];
    let mut timers = VmTimerTable::default();

    for timer in cases {
        assert_eq!(
            timers.coalesce_late_interval(timer, u64::MAX),
            Some(VmTimerEvent::Overflow {
                timer_id: timer.id,
                owner,
                kind: VmTimerKind::Interval,
            })
        );
    }
}

#[test]
pub(super) fn timer_table_reports_overflow_for_late_interval_before_next_boundary() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("late-overflow"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let timer = timers
        .start_interval(&processes, owner, u64::MAX - 2, 4)
        .expect("interval should start");

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, u64::MAX - 1),
        vec![VmTimerEvent::Overflow {
            timer_id: timer,
            owner,
            kind: VmTimerKind::Interval,
        }]
    );
}

#[test]
pub(super) fn timer_event_mailbox_encoding_covers_every_outcome() {
    let owner = VmProcessId::from_raw_for_test(7);
    let timer_id = VmTimerId(9);
    let events = [
        VmTimerEvent::DeadlineMissed {
            timer_id,
            owner,
            kind: VmTimerKind::ReceiveTimeout,
            late_by_ticks: 3,
        },
        VmTimerEvent::Coalesced {
            timer_id,
            owner,
            kind: VmTimerKind::Interval,
            skipped_intervals: 4,
            next_deadline_tick: 20,
        },
        VmTimerEvent::Overflow {
            timer_id,
            owner,
            kind: VmTimerKind::Interval,
        },
        VmTimerEvent::Cancelled {
            timer_id,
            owner,
            kind: VmTimerKind::OneShot,
        },
        VmTimerEvent::OwnerExited {
            timer_id,
            owner,
            kind: VmTimerKind::OneShot,
        },
    ];

    let encoded = events
        .iter()
        .map(|event| {
            assert_eq!(event.timer_id(), timer_id);
            assert_eq!(timer_event_owner(event), owner);
            assert_eq!(
                event.kind(),
                match event {
                    VmTimerEvent::DeadlineMissed { .. } => VmTimerKind::ReceiveTimeout,
                    VmTimerEvent::Coalesced { .. } | VmTimerEvent::Overflow { .. } => {
                        VmTimerKind::Interval
                    }
                    VmTimerEvent::Cancelled { .. } | VmTimerEvent::OwnerExited { .. } => {
                        VmTimerKind::OneShot
                    }
                    VmTimerEvent::Fired { .. } => unreachable!("fixture has no fired event"),
                }
            );
            timer_event_mailbox_value(event)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        encoded,
        vec![
            ReplValue::Tuple(vec![
                ReplValue::Atom("timer_outcome".to_string()),
                ReplValue::String("9".to_string()),
                ReplValue::Atom("receive_timeout".to_string()),
                ReplValue::Atom("deadline_missed".to_string()),
                ReplValue::String("3".to_string()),
            ]),
            ReplValue::Tuple(vec![
                ReplValue::Atom("timer_outcome".to_string()),
                ReplValue::String("9".to_string()),
                ReplValue::Atom("interval".to_string()),
                ReplValue::Atom("coalesced".to_string()),
                ReplValue::Tuple(vec![
                    ReplValue::String("4".to_string()),
                    ReplValue::String("20".to_string()),
                ]),
            ]),
            ReplValue::Tuple(vec![
                ReplValue::Atom("timer_outcome".to_string()),
                ReplValue::String("9".to_string()),
                ReplValue::Atom("interval".to_string()),
                ReplValue::Atom("overflow".to_string()),
                ReplValue::Unit,
            ]),
            ReplValue::Tuple(vec![
                ReplValue::Atom("timer_outcome".to_string()),
                ReplValue::String("9".to_string()),
                ReplValue::Atom("one_shot".to_string()),
                ReplValue::Atom("cancelled".to_string()),
                ReplValue::Unit,
            ]),
            ReplValue::Tuple(vec![
                ReplValue::Atom("timer_outcome".to_string()),
                ReplValue::String("9".to_string()),
                ReplValue::Atom("one_shot".to_string()),
                ReplValue::Atom("owner_exited".to_string()),
                ReplValue::Unit,
            ]),
        ]
    );
}

#[test]
pub(super) fn timer_table_allows_nested_timer_at_current_monotonic_tick() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("nested"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let outer = timers
        .start_one_shot(&processes, owner, 10)
        .expect("outer timer");
    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 10),
        vec![VmTimerEvent::Fired {
            timer_id: outer,
            owner,
            kind: VmTimerKind::OneShot,
        }]
    );

    let nested = timers
        .start_one_shot(&processes, owner, 10)
        .expect("nested timer");
    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 10),
        vec![VmTimerEvent::Fired {
            timer_id: nested,
            owner,
            kind: VmTimerKind::OneShot,
        }]
    );
}

#[test]
pub(super) fn timer_table_rejects_backward_clock_without_losing_pending_timer() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("clock-drift"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let timer = timers
        .start_one_shot(&processes, owner, 20)
        .expect("pending timer");

    assert!(timers
        .advance_clock(&mut processes, &mut scheduler, 10)
        .is_empty());
    assert!(timers
        .advance_clock(&mut processes, &mut scheduler, 9)
        .is_empty());
    assert_eq!(timers.snapshots()[0].id, timer);
    assert_eq!(timers.metrics().clock_drift_rejections.len(), 1);
    assert_eq!(
        timers.metrics().clock_drift_rejections[0].diagnostic,
        "timer clock moved backwards: previous tick 10, observed tick 9"
    );
    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 20),
        vec![VmTimerEvent::Fired {
            timer_id: timer,
            owner,
            kind: VmTimerKind::OneShot,
        }]
    );
}

#[test]
pub(super) fn timer_table_delivers_typed_outcomes_to_live_owner_mailbox() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("mailbox-timer"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let timer = timers
        .start_one_shot(&processes, owner, 5)
        .expect("mailbox timer");
    let fired = timers
        .advance_clock(&mut processes, &mut scheduler, 5)
        .pop()
        .expect("fired event");
    processes.get_mut(owner).expect("owner").block();

    assert_eq!(
        timers
            .deliver_event_to_mailbox(&mut processes, &mut scheduler, &fired)
            .expect("deliver fired outcome"),
        Some(1)
    );
    let message = processes
        .get_mut(owner)
        .expect("owner")
        .receive_next()
        .expect("timer mailbox message");
    assert_eq!(message.sender, owner);
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("timer_outcome".to_string()),
            ReplValue::String(timer.as_u64().to_string()),
            ReplValue::Atom("one_shot".to_string()),
            ReplValue::Atom("fired".to_string()),
            ReplValue::Unit,
        ])
    );
    assert_eq!(timers.metrics().mailbox_deliveries, 1);
}

#[test]
pub(super) fn timer_table_does_not_deliver_owner_exited_outcome_to_dead_mailbox() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("dead-timer-owner"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    timers
        .start_one_shot(&processes, owner, 5)
        .expect("owner timer");
    processes
        .exit_process(owner, VmExitReason::Killed)
        .expect("exit owner");
    let event = timers
        .cancel_owner_timers(owner)
        .pop()
        .expect("owner exited event");

    assert_eq!(
        timers
            .deliver_event_to_mailbox(&mut processes, &mut scheduler, &event)
            .expect("owner exited outcome is observation-only"),
        None
    );
    assert_eq!(timers.metrics().mailbox_deliveries, 0);
}

#[test]
pub(super) fn timer_table_writes_deadline_report_from_runtime_events() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("report-owner"));
    let exiting_owner = processes.spawn_root(source("report-exit"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();

    let first = timers.start_one_shot(&processes, owner, 5).expect("first");
    let second = timers.start_one_shot(&processes, owner, 5).expect("second");
    let fired_events = timers.advance_clock(&mut processes, &mut scheduler, 5);
    assert_eq!(
        fired_events,
        vec![
            VmTimerEvent::Fired {
                timer_id: first,
                owner,
                kind: VmTimerKind::OneShot,
            },
            VmTimerEvent::Fired {
                timer_id: second,
                owner,
                kind: VmTimerKind::OneShot,
            },
        ]
    );
    timers
        .deliver_event_to_mailbox(&mut processes, &mut scheduler, &fired_events[0])
        .expect("deliver report event");
    processes
        .get_mut(owner)
        .expect("report owner")
        .receive_next()
        .expect("reported timer message");

    timers.start_one_shot(&processes, owner, 6).expect("late");
    timers.advance_clock(&mut processes, &mut scheduler, 8);
    let cancelled = timers
        .start_one_shot(&processes, owner, 20)
        .expect("cancelled");
    timers.cancel(cancelled).expect("cancel timer");
    timers
        .start_one_shot(&processes, exiting_owner, 20)
        .expect("owner cleanup");
    processes
        .exit_process(exiting_owner, VmExitReason::Killed)
        .expect("exit timer owner");
    timers.cancel_owner_timers(exiting_owner);
    timers
        .start_interval(&processes, owner, 10, 2)
        .expect("coalesced interval");
    timers.advance_clock(&mut processes, &mut scheduler, 15);
    timers
        .start_interval(&processes, owner, u64::MAX, 1)
        .expect("overflow interval");
    timers.advance_clock(&mut processes, &mut scheduler, u64::MAX);

    let metrics = timers.metrics();
    assert_eq!(metrics.started, 7);
    assert_eq!(metrics.fired, 2);
    assert_eq!(metrics.deadline_missed, 1);
    assert_eq!(metrics.coalesced, 1);
    assert_eq!(metrics.overflow, 2);
    assert_eq!(metrics.cancelled, 1);
    assert_eq!(metrics.owner_exited, 1);
    assert_eq!(metrics.late_by_ticks_total, 2);
    assert_eq!(metrics.mailbox_deliveries, 1);
    assert_eq!(metrics.ordering_trace, vec![1, 2, 3, 4, 5, 6, 6, 7]);

    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/vm-timer-deadline-report.json");
    timers
        .write_deadline_report(&report_path, 32, 1)
        .expect("write timer deadline report");
    let report = std::fs::read_to_string(report_path).expect("read timer deadline report");
    let report: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(report["schema"], "terlan-vm-timer-deadline-report-v1");
    assert_eq!(report["timerCounts"]["started"], 7);
    assert_eq!(report["timerCounts"]["active"], 0);
    assert_eq!(report["timerCounts"]["mailboxDeliveries"], 1);
    assert_eq!(report["lateFireCount"], 1);
    assert_eq!(report["lateByTicksTotal"], 2);
    assert_eq!(report["schedulerPressureDeltas"]["fairnessInterleaves"], 1);
    assert_eq!(report["cancellationDecisions"].as_array().unwrap().len(), 2);
    assert!(report["clockDriftRejections"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
pub(super) fn timer_table_reports_deadline_report_directory_and_write_failures() {
    let timers = VmTimerTable::default();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/vm-timer-report-errors");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("error fixture root");

    let parent_file = root.join("parent-file");
    std::fs::write(&parent_file, "not a directory").expect("parent fixture");
    assert!(timers
        .write_deadline_report(&parent_file.join("report.json"), 0, 0)
        .expect_err("file parent should reject directory creation")
        .starts_with("failed to create VM timer report directory:"));

    assert!(timers
        .write_deadline_report(&root, 0, 0)
        .expect_err("directory path should reject report write")
        .starts_with("failed to write VM timer deadline report:"));
    assert!(timers
        .write_deadline_report(std::path::Path::new(""), 0, 0)
        .expect_err("empty parentless path should reject report write")
        .starts_with("failed to write VM timer deadline report:"));

    std::fs::remove_dir_all(root).expect("cleanup report fixtures");
}
