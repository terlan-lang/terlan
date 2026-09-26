//! Real process accounting, closed stdin, deadlines and ordinary descendant cleanup.
#![cfg(target_os = "linux")]
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "terlan-owned-command-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn command(&self, seconds: &str, script: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_terlan-test-orchestrator"));
        command
            .current_dir(&self.0)
            .args([
                "--run-owned",
                "--timeout-seconds",
                seconds,
                "--",
                "/bin/sh",
                "-c",
                script,
            ])
            .env("TERLAN_PROCESS_ACTIVITY_LOG", self.0.join("events.jsonl"));
        command
    }

    fn records(&self, success: bool) {
        let text = fs::read_to_string(self.0.join("events.jsonl")).unwrap();
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        // The enclosing test owner and the CLI's producer both report here.
        // Require both complete lifecycles, rather than discarding outer events.
        assert_eq!(rows.len(), 6, "{text}");
        let started: Vec<_> = rows
            .iter()
            .filter(|row| row["state"] == "started")
            .collect();
        assert_eq!(started.len(), 2, "{text}");
        assert_ne!(started[0]["attempt"], started[1]["attempt"]);
        for start in &started {
            let attempt: Vec<_> = rows
                .iter()
                .filter(|row| row["attempt"] == start["attempt"])
                .collect();
            assert_eq!(attempt.len(), 3, "{text}");
            assert_eq!(attempt[0]["state"], "started");
            assert_eq!(attempt[1]["state"], "spawned");
            assert_eq!(attempt[2]["state"], "reaped");
            assert_eq!(attempt[2]["exit_success"], success);
            assert!(attempt[1]["child_pid"].as_u64().unwrap() > 0);
            assert_eq!(attempt[1]["child_pid"], attempt[2]["child_pid"]);
            assert!(attempt
                .iter()
                .all(|row| row["owner_pid"] == start["owner_pid"]));
        }
        let outer_spawn = rows
            .iter()
            .find(|row| row["attempt"] == started[0]["attempt"] && row["state"] == "spawned")
            .unwrap();
        assert_eq!(outer_spawn["child_pid"], started[1]["owner_pid"]);
    }

    fn descendant_stopped(&self) {
        let pid: u32 = fs::read_to_string(self.0.join("descendant"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match fs::read_to_string(format!("/proc/{pid}/stat")) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Ok(stat) if stat.rsplit_once(") ").unwrap().1.starts_with('Z') => return,
                _ => assert!(
                    Instant::now() < deadline,
                    "ordinary descendant {pid} still running"
                ),
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn owned_command_observes_success_failure_and_eof_without_starting_the_suite() {
    for success in [true, false] {
        let fixture = Fixture::new();
        let script = if success {
            "if read value; then exit 9; fi; printf owned-output"
        } else {
            "exit 9"
        };
        let captured = ProcessControl::new(Duration::from_secs(5))
            .capture_stdout_result(&mut fixture.command("2", script), 1024, |_| Ok(()))
            .unwrap();
        assert_eq!(captured.outcome.is_ok(), success);
        assert_eq!(
            captured.stdout,
            if success {
                b"owned-output".as_slice()
            } else {
                b""
            }
        );
        fixture.records(success);
    }
}

const HANG: &str = "trap '' TERM; sleep 300 & echo $! > descendant; while :; do sleep 1; done";

#[test]
fn owned_command_deadline_stops_ordinary_descendants_and_records_reap() {
    let fixture = Fixture::new();
    let captured = ProcessControl::new(Duration::from_secs(6))
        .capture_stdout_result(&mut fixture.command("1", HANG), 1024, |_| Ok(()))
        .unwrap();
    assert!(captured.outcome.is_err());
    fixture.records(false);
    fixture.descendant_stopped();
}

#[test]
fn owned_command_cancellation_stops_ordinary_descendants_and_records_reap() {
    let fixture = Fixture::new();
    let ready = fixture.0.join("descendant");
    let mut notifier = None;
    let captured = ProcessControl::new(Duration::from_secs(8))
        .capture_stdout_result(&mut fixture.command("30", HANG), 1024, |pid| {
            notifier = Some(std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(3);
                while !ready.exists() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
                // Also cancel on a failed readiness probe so it cannot leak a producer.
                let ready = ready.exists();
                rustix::process::kill_process(
                    rustix::process::Pid::from_raw(i32::try_from(pid).unwrap()).unwrap(),
                    rustix::process::Signal::TERM,
                )
                .unwrap();
                ready
            }));
            Ok(())
        })
        .unwrap();
    assert!(notifier.unwrap().join().unwrap());
    assert!(captured.outcome.is_err());
    fixture.records(false);
    fixture.descendant_stopped();
}
