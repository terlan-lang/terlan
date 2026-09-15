//! Behavioral coverage of compiler tools using shared bounded process ownership.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::run_command_with_timeout;
use crate::runtime::native_boundary::dispatch::capture_optional_tool_command;
use crate::runtime::native_boundary::dispatch::capture_tool_command;
use crate::runtime::native_boundary::dispatch::capture_tool_command_with_launch;
use crate::runtime::native_boundary::dispatch::ToolCommandError;

/// Optional tools may be absent, but resource failures must never become skips.
#[test]
fn optional_tool_capture_preserves_absence_exit_and_resource_failures() {
    let timeout = Duration::from_secs(5);
    assert!(capture_optional_tool_command(
        &mut Command::new("/nonexistent/terlan-optional-tool"),
        "optional",
        timeout,
        4096
    )
    .unwrap()
    .is_none());
    let output = capture_optional_tool_command(
        &mut shell("if read value; then exit 8; fi; printf output; exit 7"),
        "optional",
        timeout,
        4096,
    )
    .unwrap()
    .unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"output");
    let timeout = capture_optional_tool_command(
        &mut shell("sleep 30"),
        "optional",
        Duration::from_millis(50),
        4096,
    )
    .unwrap_err();
    assert_eq!(timeout.code(), "timed_out");
    assert!(timeout.to_string().contains("timed out"), "{timeout}");
    let overflow = capture_optional_tool_command(
        &mut shell("head -c 4097 /dev/zero"),
        "optional",
        Duration::from_secs(5),
        4096,
    )
    .unwrap_err();
    assert_eq!(overflow.code(), "output_limit_exceeded");
    assert!(
        overflow.to_string().contains("output_limit_exceeded"),
        "{overflow}"
    );
    assert!(capture_optional_tool_command(
        &mut Command::new("/nonexistent/terlan-optional-tool"),
        "optional",
        Duration::ZERO,
        4096
    )
    .is_err());
}

/// A missing observer destination is an infrastructure error, not missing Node.
#[test]
fn optional_tool_capture_does_not_skip_failed_observation() {
    if std::env::var_os("TERLAN_OPTIONAL_OBSERVER_FIXTURE").is_none() {
        // This intentionally incomplete private log must not poison an enclosing
        // release observer. The enclosing log still records the fixture process.
        let output = capture_tool_command(
            Command::new("/usr/bin/env")
                .args(["-u", "TERLAN_PROCESS_ACTIVITY_LOG", "TERLAN_OPTIONAL_OBSERVER_FIXTURE=1"])
                .arg(std::env::current_exe().unwrap())
                .args([
                    "commands::process_runner::tests::optional_tool_capture_does_not_skip_failed_observation",
                    "--exact", "--test-threads=1",
                ]),
            "isolated optional observer fixture", Duration::from_secs(30), 64 * 1024,
        ).unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        return;
    }
    let root = crate::support::test_fs::temp_dir("process_runner", "optional_observer");
    let mut command = Command::new(root.join("missing-tool"));
    command.env(
        "TERLAN_PROCESS_ACTIVITY_LOG",
        root.join("missing-parent/events.jsonl"),
    );
    let error =
        capture_optional_tool_command(&mut command, "optional", Duration::from_secs(5), 4096)
            .unwrap_err();
    assert_eq!(error.code(), "spawn_failed");
    assert!(error.to_string().contains("spawn_failed"), "{error}");
    std::fs::remove_dir_all(root).unwrap();
}

/// The launch observer is called exactly once and only after successful spawn.
#[test]
fn tool_inventory_observes_only_successful_spawns() -> Result<(), ToolCommandError> {
    let mut observed = Vec::new();
    let output = capture_tool_command_with_launch(
        &mut shell("printf observed"),
        "observed-tool",
        Duration::from_secs(5),
        4096,
        |pid| {
            observed.push(pid);
            Ok(())
        },
    )?;
    assert!(output.status.success());
    assert_eq!(observed.len(), 1);
    assert!(observed[0] > 0);
    let failure = capture_tool_command_with_launch(
        &mut Command::new("/nonexistent/terlan-tool"),
        "missing-tool",
        Duration::from_secs(5),
        4096,
        |pid| {
            observed.push(pid);
            Ok(())
        },
    );
    assert!(failure.is_err());
    assert_eq!(observed.len(), 1);
    Ok(())
}

/// Failed launch accounting cannot leave its already-spawned child alive.
#[test]
fn tool_inventory_failure_retains_process_cleanup_ownership() {
    let started = Instant::now();
    let result = capture_tool_command_with_launch(
        &mut shell("sleep 30"),
        "unrecorded-tool",
        Duration::from_secs(5),
        4096,
        |_| Err("inventory unavailable".to_owned()),
    );
    assert!(
        matches!(result, Err(error) if error.code() == "launch_observation_failed"
            && error.to_string().contains("launch_observation_failed: inventory unavailable"))
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

/// Output exceeding a pipe buffer must be drained before the child exits.
#[test]
fn verbose_tool_does_not_deadlock_on_full_pipes() -> Result<(), String> {
    let output = run_command_with_timeout(
        &mut shell("head -c 262144 /dev/zero; head -c 262144 /dev/zero >&2"),
        "verbose-tool",
        Duration::from_secs(5),
    )?;
    assert!(output.status.success());
    assert_eq!(output.stdout, vec![0; 262144]);
    assert_eq!(output.stderr, vec![0; 262144]);
    Ok(())
}

/// An explicitly piped input request cannot make a compiler tool interactive.
#[test]
fn tools_receive_closed_stdin_and_preserve_exit_status() -> Result<(), String> {
    let mut command = shell("if read value; then exit 8; fi; printf closed; exit 7");
    command.stdin(Stdio::piped());
    let output = run_command_with_timeout(&mut command, "stdin-tool", Duration::from_secs(5))?;
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"closed");
    Ok(())
}

/// No UTF-8 conversion is permitted between a prepared Command and its launch.
#[test]
fn tools_preserve_native_arguments_environment_and_directory() -> Result<(), String> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let mut command = shell("printf '%s' \"$1\"; printf '%s' \"$TERLAN_CAPTURE_TEST\"; pwd");
    command
        .arg("capture")
        .arg(OsString::from_vec(vec![255, b'x']))
        .env("TERLAN_CAPTURE_TEST", "value")
        .current_dir("/");
    let output = run_command_with_timeout(&mut command, "byte-tool", Duration::from_secs(5))?;
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        [vec![255, b'x'], b"value/\n".to_vec()].concat()
    );
    Ok(())
}

/// A successful leader cannot leave a descendant keeping capture pipes alive.
#[test]
fn exited_tool_reclaims_descendant_pipes() -> Result<(), String> {
    let started = Instant::now();
    let output = run_command_with_timeout(
        &mut shell("sleep 30 & printf complete; exit 3"),
        "descendant-tool",
        Duration::from_secs(5),
    )?;
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(output.stdout, b"complete");
    assert!(started.elapsed() < Duration::from_secs(10));
    Ok(())
}

/// Timeouts terminate the owned group instead of waiting for inherited pipes.
#[test]
fn tool_timeout_covers_descendant_pipe_drain() {
    let started = Instant::now();
    let result = run_command_with_timeout(
        &mut shell("sleep 30 & wait"),
        "stalled-linker",
        Duration::from_millis(100),
    );
    assert_eq!(
        result.err().as_deref(),
        Some("stalled-linker timed out after 100 milliseconds")
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// A noisy tool is stopped at the combined limit, even if it exits immediately.
#[test]
fn combined_tool_output_limit_is_enforced() {
    let result = capture_tool_command(
        &mut shell("head -c 3000 /dev/zero; head -c 3000 /dev/zero >&2"),
        "noisy-linker",
        Duration::from_secs(5),
        4096,
    );
    assert!(
        matches!(result, Err(error) if error.code() == "output_limit_exceeded"
            && error.to_string().contains("noisy-linker output_limit_exceeded"))
    );
}

/// Configuration errors are rejected before attempting a tool launch.
#[test]
fn invalid_tool_limits_fail_before_spawn() {
    for (timeout, bytes) in [(Duration::ZERO, 1), (Duration::from_secs(1), 0)] {
        let result = capture_tool_command(
            &mut Command::new("/nonexistent/terlan-tool"),
            "invalid-tool",
            timeout,
            bytes,
        );
        assert!(
            matches!(result, Err(error) if error.code() == "invalid_limits"
                && error.to_string().contains("positive timeout and output limits"))
        );
    }
}

/// A launch failure names the requested tool rather than an unrelated stage.
#[test]
fn tool_spawn_failure_is_attributed() {
    let result = run_command_with_timeout(
        &mut Command::new("/nonexistent/terlan-tool"),
        "missing-linker",
        Duration::from_secs(1),
    );
    assert!(matches!(result, Err(message) if message.contains("missing-linker spawn_failed")));
}
