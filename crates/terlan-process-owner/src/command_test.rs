#![cfg(any(target_os = "linux", target_os = "macos"))]

use super::*;

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

#[test]
fn preflight_timeout_cap_never_extends_deadlines_or_drops_cancellation() {
    let cancelled = AtomicBool::new(false);
    let control = ProcessControl::new(Duration::from_secs(2)).with_cancellation(&cancelled);
    assert_eq!(
        control.limited_to(Duration::from_secs(30)).timeout,
        Duration::from_secs(2)
    );
    let capped = control.limited_to(Duration::from_millis(10));
    assert_eq!(capped.timeout, Duration::from_millis(10));
    cancelled.store(true, Ordering::Release);
    assert_eq!(capped.check(Instant::now()).unwrap_err().kind, "cancelled");
    let mut launched = false;
    let failure = capped
        .run(&mut shell("exit 0"), |_| {
            launched = true;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(failure.kind, "cancelled");
    assert!(!launched);
}

#[test]
fn capture_preserves_binary_bytes_and_closes_stdin() {
    let output = capture_stdout(
        &mut shell("if read value; then exit 9; fi; printf '\\377\\000x'"),
        Duration::from_secs(2),
        3,
    )
    .unwrap();
    assert_eq!(output, [255, 0, b'x']);
}

#[test]
fn streaming_observer_runs_on_owner_thread_before_child_can_finish() {
    use std::cell::Cell;
    let pid = Cell::new(None);
    let mut observed = Vec::new();
    let mut released = false;
    let owner = thread::current().id();
    let captured = ProcessControl::new(Duration::from_secs(3))
        .capture_stdout_observed(
            &mut shell(
                "trap 'printf done; exit 0' USR1; printf ready; while :; do sleep 0.01; done",
            ),
            1024,
            |child| {
                pid.set(rustix::process::Pid::from_raw(child as i32));
                Ok(())
            },
            |bytes| {
                assert_eq!(thread::current().id(), owner);
                observed.extend_from_slice(bytes);
                if !released && observed.starts_with(b"ready") {
                    released = true;
                    rustix::process::kill_process(
                        pid.get().unwrap(),
                        rustix::process::Signal::USR1,
                    )
                    .unwrap();
                }
                Ok(())
            },
        )
        .unwrap();
    captured.outcome.unwrap();
    assert!(released);
    assert_eq!(observed, b"readydone");
    assert_eq!(captured.stdout, observed);
}

#[test]
fn streaming_observer_preserves_large_binary_output_and_nonzero_outcome() {
    let mut observed = Vec::new();
    let captured = ProcessControl::new(Duration::from_secs(5))
        .capture_stdout_observed(
            &mut shell("dd if=/dev/zero bs=8192 count=256 2>/dev/null; printf '\\377'; exit 7"),
            2 * 1024 * 1024 + 1,
            |_| Ok(()),
            |bytes| {
                observed.extend_from_slice(bytes);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(captured.outcome.unwrap_err().kind, "failed");
    assert_eq!(observed.len(), 2 * 1024 * 1024 + 1);
    assert!(observed[..observed.len() - 1].iter().all(|byte| *byte == 0));
    assert_eq!(observed.last(), Some(&255));
    assert_eq!(observed, captured.stdout);
}

#[test]
fn streaming_callback_failure_panic_and_cancellation_reap_the_child() {
    for mode in ["failure", "panic", "cancellation"] {
        let flag = AtomicBool::new(false);
        let mut pid = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ProcessControl::new(Duration::from_secs(5))
                .with_cancellation(&flag)
                .capture_stdout_observed(
                    &mut shell("while :; do printf 'producer-output'; done"),
                    16 * 1024 * 1024,
                    |child| {
                        pid = rustix::process::Pid::from_raw(child as i32);
                        Ok(())
                    },
                    |_| match mode {
                        "failure" => Err("cannot persist output".into()),
                        "panic" => panic!("fixture callback panic"),
                        _ => {
                            flag.store(true, Ordering::Release);
                            Ok(())
                        }
                    },
                )
        }));
        if mode == "panic" {
            assert!(result.is_err());
        } else {
            assert_eq!(
                result.unwrap().unwrap_err().kind,
                if mode == "failure" {
                    "output-observation-failed"
                } else {
                    "cancelled"
                }
            );
        }
        assert!(pid.is_some());
        assert!(matches!(
            rustix::process::waitpid(pid, rustix::process::WaitOptions::NOHANG),
            Err(rustix::io::Errno::CHILD)
        ));
    }
}

#[test]
fn streaming_callbacks_cannot_turn_overflow_or_timeout_into_success() {
    for (script, limit, expected) in [
        ("printf overflow", 3, "output-limit"),
        ("printf ready; sleep 5", 1024, "timed-out"),
    ] {
        let mut observed = Vec::new();
        let error = ProcessControl::new(Duration::from_millis(100))
            .capture_stdout_observed(
                &mut shell(script),
                limit,
                |_| Ok(()),
                |bytes| {
                    observed.extend_from_slice(bytes);
                    Ok(())
                },
            )
            .unwrap_err();
        assert_eq!(error.kind, expected);
        assert!(observed.len() <= limit);
    }
}

#[test]
fn stopped_streaming_reader_does_not_block_on_a_full_queue() {
    let (sender, _receiver) = mpsc::sync_channel(1);
    sender.send(ReadEvent::Chunk(vec![1])).unwrap();
    let stopped = AtomicBool::new(true);
    assert!(!send_event(&sender, ReadEvent::Chunk(vec![2]), &stopped));
}

#[test]
fn capture_result_retains_failure_diagnostics_without_turning_exit_failure_into_success() {
    let mut launches = 0;
    let output = ProcessControl::new(Duration::from_secs(2))
        .capture_stdout_result(
            &mut shell("printf 'assertion failed\\n'; exit 7"),
            1024,
            |_| {
                launches += 1;
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(output.stdout, b"assertion failed\n");
    assert_eq!(output.outcome.unwrap_err().kind, "failed");
    assert_eq!(launches, 1);
    let failure = ProcessControl::new(Duration::from_secs(2))
        .capture_stdout_result(&mut shell("printf overflow; exit 7"), 3, |_| Ok(()))
        .unwrap_err();
    assert_eq!(failure.kind, "output-limit");
}

#[test]
fn capture_rejects_overflow_including_short_lived_producers() {
    for script in ["printf abcd", "while :; do printf abcdefgh; done"] {
        let error = capture_stdout(&mut shell(script), Duration::from_secs(2), 3).unwrap_err();
        assert_eq!(error.kind, "output-limit");
    }
}

#[test]
fn inherited_output_run_closes_stdin() {
    run(
        &mut shell("if read value; then exit 9; else exit 0; fi"),
        Duration::from_secs(2),
    )
    .unwrap();
}

#[test]
fn invalid_limits_fail_before_launch() {
    for (timeout, limit) in [
        (Duration::ZERO, 1),
        (Duration::from_secs(1), 0),
        (Duration::from_secs(1), MAX_CAPTURE_BYTES + 1),
    ] {
        let error = capture_stdout(
            &mut Command::new("missing-process-owner-probe"),
            timeout,
            limit,
        )
        .unwrap_err();
        assert_eq!(error.kind, "invalid-limits");
    }
}

#[test]
fn cancellation_before_spawn_never_calls_the_launch_observer() {
    let flag = AtomicBool::new(true);
    let control = ProcessControl::new(Duration::from_secs(5)).with_cancellation(&flag);
    let mut observed = false;
    let error = control
        .run(&mut Command::new("missing-cancelled-producer"), |_| {
            observed = true;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind, "cancelled");
    let error = control
        .capture_stdout(
            &mut Command::new("missing-cancelled-producer"),
            1024,
            |_| {
                observed = true;
                Ok(())
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, "cancelled");
    assert!(!observed);
}

#[test]
fn cancellation_after_spawn_reaps_both_execution_modes() {
    for capture in [false, true] {
        let flag = AtomicBool::new(false);
        let control = ProcessControl::new(Duration::from_secs(5)).with_cancellation(&flag);
        let mut pid = None;
        let observe = |child| {
            pid = rustix::process::Pid::from_raw(child as i32);
            flag.store(true, Ordering::Release);
            Ok(())
        };
        let error = if capture {
            control
                .capture_stdout(&mut shell("sleep 5"), 1024, observe)
                .map(|_| ())
        } else {
            control.run(&mut shell("sleep 5"), observe)
        }
        .unwrap_err();
        assert_eq!(error.kind, "cancelled");
        assert!(matches!(
            rustix::process::waitpid(pid, rustix::process::WaitOptions::NOHANG),
            Err(rustix::io::Errno::CHILD)
        ));
    }
}

#[test]
fn observers_only_receive_successful_os_spawns() {
    let mut observed = None;
    run_with_launch(&mut shell("exit 0"), Duration::from_secs(2), |pid| {
        observed = Some(pid);
        Ok(())
    })
    .unwrap();
    assert!(observed.unwrap() > 0);
    let mut called = false;
    assert!(capture_stdout_with_launch(
        &mut Command::new("missing-process-owner-probe"),
        Duration::from_secs(1),
        1024,
        |_| {
            called = true;
            Ok(())
        }
    )
    .is_err());
    assert!(!called);
}

#[test]
fn observer_failure_does_not_release_a_running_child() {
    let mut pid = None;
    let error = capture_stdout_with_launch(
        &mut shell("sleep 5"),
        Duration::from_secs(2),
        1024,
        |child| {
            pid = rustix::process::Pid::from_raw(child as i32);
            Err("cannot persist launch".into())
        },
    )
    .unwrap_err();
    assert_eq!(error.kind, "observation-failed");
    assert!(matches!(
        rustix::process::waitpid(pid, rustix::process::WaitOptions::NOHANG),
        Err(rustix::io::Errno::CHILD)
    ));
}

#[test]
fn failed_exit_is_not_a_successful_empty_capture() {
    let error = capture_stdout(
        &mut shell("printf partial; exit 7"),
        Duration::from_secs(2),
        1024,
    )
    .unwrap_err();
    assert_eq!(error.kind, "failed");
    assert!(error.detail.contains('7'));
}

#[test]
fn successful_leader_cannot_leave_group_pipes_open() {
    let started = Instant::now();
    let output = capture_stdout(
        &mut shell("sleep 5 & printf complete; exit 0"),
        Duration::from_secs(2),
        1024,
    )
    .unwrap();
    assert_eq!(output, b"complete");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn timeout_terminates_term_ignoring_group_and_reader() {
    let started = Instant::now();
    let error = capture_stdout(
        &mut shell("trap '' TERM; sleep 5 & wait"),
        Duration::from_millis(100),
        1024,
    )
    .unwrap_err();
    assert_eq!(error.kind, "timed-out");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn child_ownership_survives_unwinding() {
    let child = OwnedChild::spawn(shell("sleep 5")).unwrap();
    let pid = rustix::process::Pid::from_raw(child.id() as i32).unwrap();
    let result = std::panic::catch_unwind(move || {
        let _child = child;
        panic!("producer panicked");
    });
    assert!(result.is_err());
    assert!(matches!(
        rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::NOHANG),
        Err(rustix::io::Errno::CHILD)
    ));
}

#[test]
fn completed_owner_can_be_observed_again_without_reaping_another_process() {
    let mut child = OwnedChild::spawn(shell("exit 0")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        thread::sleep(POLL_INTERVAL);
    }
    assert!(child.try_wait().unwrap().unwrap().success());
    assert!(child.finish().unwrap().success());
}

#[cfg(target_os = "linux")]
#[test]
fn escaped_session_pipe_cannot_extend_the_capture_deadline() {
    // The escaped session expires itself; a process group cannot contain setsid.
    // This specifically exercises the nonblocking reader's stop/join path.
    let started = Instant::now();
    let error = capture_stdout(
        &mut shell("setsid /bin/sh -c 'printf ready; sleep 1' & wait"),
        Duration::from_millis(150),
        1024,
    )
    .unwrap_err();
    assert_eq!(error.kind, "timed-out");
    assert!(started.elapsed() < Duration::from_millis(800));
    thread::sleep(Duration::from_secs(1));
}
