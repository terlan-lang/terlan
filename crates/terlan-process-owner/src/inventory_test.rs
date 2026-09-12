use super::*;
use crate::OwnedChild;
use serde_json::Value;
use std::fs;
#[cfg(unix)]
use std::process::Stdio;

// Fault-injection fixtures own their log destinations, even when the enclosing
// validation harness is itself observed. Never fill or remove that outer log.
// Spawned probe processes do not inherit this thread-local test seam: nested
// propagation tests exercise the production environment lookup unmodified.
thread_local! {
    static ISOLATED_SCOPE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn isolated_fixture_scope() -> bool {
    ISOLATED_SCOPE.get()
}

struct Fixture(PathBuf, bool);

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "terlan-process-inventory-{}-{stamp}-{}",
            std::process::id(),
            NEXT_ATTEMPT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, ISOLATED_SCOPE.replace(true))
    }

    fn log(&self) -> PathBuf {
        self.0.join("activity.jsonl")
    }

    fn events(&self) -> Vec<Value> {
        fs::read_to_string(self.log())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        ISOLATED_SCOPE.set(self.1);
        fs::remove_dir_all(&self.0).expect("remove only this terminal inventory fixture");
    }
}

fn states(events: &[Value]) -> Vec<&str> {
    events
        .iter()
        .map(|event| event["state"].as_str().unwrap())
        .collect()
}

#[test]
fn disabled_inventory_performs_no_io() {
    let mut inventory = Inventory::begin_at(&Command::new("private-program"), None).unwrap();
    inventory.spawned(123).unwrap();
    inventory
        .spawn_failed(&io::Error::from(io::ErrorKind::NotFound))
        .unwrap();
    assert!(inventory.0.is_none());
}

#[test]
fn optional_spawn_records_absence_without_hiding_observation_not_found() {
    let fixture = Fixture::new();
    let mut missing = Command::new(fixture.0.join("missing-tool"));
    missing.env(VARIABLE, fixture.log());
    assert!(OwnedChild::spawn_optional_command(&mut missing)
        .unwrap()
        .is_none());
    let rows = fixture.events();
    assert_eq!(states(&rows), ["started", "spawn_failed"]);
    assert_eq!(rows[1]["error_kind"], "NotFound");

    let mut unobserved = Command::new(fixture.0.join("missing-tool"));
    unobserved.env(VARIABLE, fixture.0.join("missing-parent/activity.jsonl"));
    let error = match OwnedChild::spawn_optional_command(&mut unobserved) {
        Err(error) => error,
        Ok(_) => panic!("missing observation directory must not mean optional tool absence"),
    };
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn optional_spawn_preserves_permission_failure_and_owns_successful_children() {
    let fixture = Fixture::new();
    let mut denied = Command::new(&fixture.0);
    denied.env(VARIABLE, fixture.log());
    assert!(OwnedChild::spawn_optional_command(&mut denied).is_err());
    assert_eq!(states(&fixture.events()), ["started", "spawn_failed"]);
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "sleep 30"])
        .env(VARIABLE, fixture.log());
    let mut child = OwnedChild::spawn_optional_command(&mut command)
        .unwrap()
        .unwrap();
    child.finish().unwrap();
    assert_eq!(
        states(&fixture.events()),
        ["started", "spawn_failed", "started", "spawned", "reaped"]
    );
}

#[test]
fn interrupted_observation_is_not_success() {
    let fixture = Fixture::new();
    {
        let mut inventory =
            Inventory::begin_at(&Command::new("cargo"), Some(fixture.log())).unwrap();
        inventory.spawned(123).unwrap();
    }
    let events = fixture.events();
    assert_eq!(states(&events), ["started", "spawned", "abandoned"]);
    assert_eq!(events[2]["child_pid"], 123);
    assert!(events[2]["exit_success"].is_null());
}

#[test]
fn command_metadata_is_redacted_and_framed() {
    let fixture = Fixture::new();
    let mut command = Command::new("/private/secret-program");
    command
        .args(["secret-argument", "two"])
        .env("SECRET_KEY", "secret-value")
        .current_dir("/secret-cwd");
    drop(Inventory::begin_at(&command, Some(fixture.log())).unwrap());
    let raw = fs::read_to_string(fixture.log()).unwrap();
    for secret in ["secret", "SECRET_KEY", "/private"] {
        assert!(!raw.contains(secret), "inventory disclosed {secret}");
    }
    let events = fixture.events();
    assert_eq!(events[0]["program_kind"], "other");
    assert_eq!(events[0]["declaration_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(events[0].as_object().unwrap().len(), 11);
    assert_ne!(
        declaration(Command::new("a").arg("bc")),
        declaration(Command::new("ab").arg("c"))
    );
    assert_ne!(
        declaration(Command::new("x").args(["a", "b"])),
        declaration(Command::new("x").arg("ab"))
    );
    assert_ne!(
        declaration(Command::new("x").env("K", "")),
        declaration(Command::new("x").env_remove("K"))
    );
    assert_ne!(
        declaration(Command::new("x").current_dir("a")),
        declaration(Command::new("x").current_dir("b"))
    );
    assert_ne!(
        declaration(Command::new("x").env("K", "a")),
        declaration(Command::new("x").env("K", "b"))
    );
    assert_eq!(
        declaration(Command::new("x").env(VARIABLE, "a")),
        declaration(Command::new("x").env(VARIABLE, "b"))
    );
    assert_eq!(
        declaration(Command::new("x").env("A", "1").env("B", "2")),
        declaration(Command::new("x").env("B", "2").env("A", "1"))
    );
}

#[test]
fn invalid_destinations_fail_before_launch() {
    let fixture = Fixture::new();
    let full = fixture.0.join("full");
    fs::File::create(&full)
        .unwrap()
        .set_len(MAX_LOG_BYTES)
        .unwrap();
    for path in [
        PathBuf::new(),
        fixture.0.clone(),
        fixture.0.join("missing/log"),
        full.clone(),
    ] {
        let mut command = Command::new("this-program-must-not-be-launched");
        command.env(VARIABLE, path);
        let error = OwnedChild::spawn(command).err().unwrap();
        assert!(!error
            .to_string()
            .contains("this-program-must-not-be-launched"));
    }
    assert_eq!(fs::metadata(full).unwrap().len(), MAX_LOG_BYTES);
}

#[test]
fn failed_spawn_is_terminal_without_an_invented_pid() {
    let fixture = Fixture::new();
    let mut command = Command::new(fixture.0.join("missing-private-executable"));
    command.env(VARIABLE, fixture.log());
    assert_eq!(
        OwnedChild::spawn(command).err().unwrap().kind(),
        io::ErrorKind::NotFound
    );
    let events = fixture.events();
    assert_eq!(states(&events), ["started", "spawn_failed"]);
    assert_eq!(events[1]["error_kind"], "NotFound");
    assert!(events.iter().all(|event| event["child_pid"].is_null()));
    assert!(!fs::read_to_string(fixture.log())
        .unwrap()
        .contains("private-executable"));
}

#[test]
fn concurrent_writers_preserve_complete_unique_attempts() {
    let fixture = Fixture::new();
    std::thread::scope(|scope| {
        for _ in 0..16 {
            let path = fixture.log();
            scope.spawn(move || {
                let mut inventory =
                    Inventory::begin_at(&Command::new("cargo"), Some(path)).unwrap();
                inventory
                    .spawn_failed(&io::Error::from(io::ErrorKind::NotFound))
                    .unwrap();
            });
        }
    });
    let events = fixture.events();
    let mut attempts = std::collections::BTreeMap::new();
    for event in &events {
        attempts
            .entry(event["attempt"].as_str().unwrap())
            .or_insert_with(Vec::new)
            .push(event.clone());
    }
    assert_eq!(attempts.len(), 16);
    assert_eq!(events.len(), 32);
    for events in attempts.values() {
        assert_eq!(states(events), ["started", "spawn_failed"]);
    }
}

#[cfg(unix)]
fn shell(fixture: &Fixture, source: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", source])
        .env(VARIABLE, fixture.log())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[cfg(unix)]
fn wait(child: &mut OwnedChild) -> ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "owned child did not finish"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(unix)]
#[test]
fn invalid_inventory_cannot_execute_a_valid_command() {
    let fixture = Fixture::new();
    let full = fixture.0.join("full");
    fs::File::create(&full)
        .unwrap()
        .set_len(MAX_LOG_BYTES)
        .unwrap();
    for path in [
        PathBuf::new(),
        fixture.0.clone(),
        fixture.0.join("missing/log"),
        full,
    ] {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf launched > marker"])
            .current_dir(&fixture.0)
            .env(VARIABLE, path);
        assert!(OwnedChild::spawn(command).is_err());
        assert!(!fixture.0.join("marker").exists());
    }
}

#[cfg(unix)]
#[test]
fn relative_log_scope_is_resolved_before_child_changes_directory() {
    let fixture = Fixture::new();
    let parent = std::env::current_dir().unwrap();
    let mut relative = PathBuf::new();
    for _ in parent.ancestors().skip(1) {
        relative.push("..");
    }
    relative.push(fixture.log().strip_prefix("/").unwrap());
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["inventory::tests::nested_owned_launch_probe", "--exact"])
        .current_dir(&fixture.0)
        .env(NESTED_PROBE, "1")
        .env(VARIABLE, relative)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = OwnedChild::spawn(command).unwrap();
    assert!(wait(&mut child).success());
    assert_eq!(fixture.events().len(), 6);
}

#[cfg(unix)]
#[test]
fn actual_launch_and_nonzero_exit_are_observed_once() {
    for code in [0, 7] {
        let fixture = Fixture::new();
        let mut child = OwnedChild::spawn(shell(&fixture, &format!("exit {code}"))).unwrap();
        let pid = child.id();
        assert_eq!(wait(&mut child).code(), Some(code));
        assert_eq!(child.try_wait().unwrap().unwrap().code(), Some(code));
        assert_eq!(child.finish().unwrap().code(), Some(code));
        drop(child);
        let events = fixture.events();
        assert_eq!(states(&events), ["started", "spawned", "reaped"]);
        assert!(events[0]["child_pid"].is_null());
        assert_eq!(events[1]["child_pid"], pid);
        assert_eq!(events[2]["child_pid"], pid);
        assert_eq!(events[2]["exit_code"], code);
        assert_eq!(events[2]["exit_success"], code == 0);
        assert!(events
            .iter()
            .all(|event| event["owner_pid"] == std::process::id()));
        assert!(events
            .windows(2)
            .all(|pair| pair[0]["attempt"] == pair[1]["attempt"]));
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn timeout_and_unwind_record_reaping() {
    for unwind in [false, true] {
        let fixture = Fixture::new();
        let command = shell(&fixture, "sleep 30");
        if unwind {
            let child = OwnedChild::spawn(command).unwrap();
            assert!(std::panic::catch_unwind(move || {
                let _child = child;
                panic!("probe cleanup");
            })
            .is_err());
        } else {
            let error = crate::capture_stdout(&mut { command }, Duration::from_millis(50), 1024)
                .unwrap_err();
            assert_eq!(error.kind, "timed-out");
        }
        let events = fixture.events();
        assert_eq!(states(&events), ["started", "spawned", "reaped"]);
        assert_eq!(events[2]["exit_success"], false);
        let pid = rustix::process::Pid::from_raw(events[1]["child_pid"].as_i64().unwrap() as i32)
            .unwrap();
        assert!(matches!(
            rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::NOHANG),
            Err(rustix::io::Errno::CHILD)
        ));
    }
}

#[cfg(unix)]
#[test]
fn reap_log_failure_does_not_leave_a_live_child() {
    let fixture = Fixture::new();
    let mut child = OwnedChild::spawn(shell(&fixture, "sleep 30")).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(fixture.log())
        .unwrap()
        .set_len(MAX_LOG_BYTES)
        .unwrap();
    assert!(child.finish().is_err());
    assert!(child.try_wait().unwrap().is_some());
    // The full log remains incomplete. A later successful wait must not forge
    // a terminal observation that could admit this failed inventory.
    let raw = fs::read(fixture.log()).unwrap();
    assert!(!raw.windows(b"reaped".len()).any(|part| part == b"reaped"));
}

#[cfg(unix)]
const NESTED_PROBE: &str = "TERLAN_PROCESS_INVENTORY_NESTED_PROBE";

#[cfg(unix)]
#[test]
fn nested_owned_launch_probe() {
    let Ok(depth) = std::env::var(NESTED_PROBE) else {
        return;
    };
    if depth == "reset" {
        let scope = std::env::var_os(VARIABLE).unwrap();
        let nested = PathBuf::from(&scope).with_file_name("nested.jsonl");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .env_clear()
            .args(["inventory::tests::nested_owned_launch_probe", "--exact"])
            .env(NESTED_PROBE, "1")
            .env(VARIABLE, nested)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = OwnedChild::spawn(command).unwrap();
        assert!(wait(&mut child).success());
        return;
    }
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 7"]);
    let mut child = OwnedChild::spawn(command).unwrap();
    assert_eq!(wait(&mut child).code(), Some(7));
}

#[cfg(unix)]
#[test]
fn scope_survives_environment_reset_and_redirect() {
    let fixture = Fixture::new();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["inventory::tests::nested_owned_launch_probe", "--exact"])
        .env(NESTED_PROBE, "reset")
        .env(VARIABLE, fixture.log())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = OwnedChild::spawn(command).unwrap();
    assert!(wait(&mut child).success());
    let events = fixture.events();
    assert_eq!(events.len(), 9);
    let mut owners = std::collections::BTreeMap::new();
    for event in &events {
        owners
            .entry(event["owner_pid"].as_u64().unwrap())
            .or_insert_with(Vec::new)
            .push(event.clone());
    }
    assert_eq!(owners.len(), 3);
    for events in owners.values() {
        assert_eq!(states(events), ["started", "spawned", "reaped"]);
    }
    assert_eq!(
        events
            .iter()
            .filter(|event| event["exit_success"] == false)
            .count(),
        1
    );
    let nested: Vec<Value> = fs::read_to_string(fixture.0.join("nested.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(nested.len(), 6);
    for event in nested {
        assert!(events.contains(&event), "nested owner detached its caller");
    }
}

#[test]
fn scope_encoding_is_bounded_and_deduplicated() {
    let root = std::env::temp_dir().join("process-root.jsonl");
    let local = std::env::temp_dir().join("process-local.jsonl");
    let paths = vec![root.clone(), local.clone()];
    let encoded = scope::encode(&paths).unwrap();
    assert_eq!(
        scope::collect([root.into_os_string(), encoded]).unwrap(),
        paths
    );
    assert_eq!(
        scope::collect([scope::encode(std::slice::from_ref(&local)).unwrap()]).unwrap(),
        vec![local]
    );
    for invalid in [
        "",
        "terlan-process-scopes-v1:[]",
        "terlan-process-scopes-v1:[false]",
        "terlan-process-scopes-v1:[\"\"]",
        "terlan-process-scopes-v1:not-json",
    ] {
        assert!(scope::collect([invalid.into()]).is_err());
    }
    let too_many = (0..9).map(|index| {
        std::env::temp_dir()
            .join(format!("process-{index}.jsonl"))
            .into_os_string()
    });
    assert!(scope::collect(too_many).is_err());
    assert!(scope::collect(["x".repeat(32769).into()]).is_err());
    let mut deep = "/log".to_owned();
    for _ in 0..10 {
        deep = format!(
            "terlan-process-scopes-v1:{}",
            serde_json::to_string(&[deep]).unwrap()
        );
    }
    assert!(scope::collect([deep.into()]).is_err());
}

#[cfg(unix)]
#[test]
fn invalid_additional_scope_prevents_launch() {
    let fixture = Fixture::new();
    let encoded = scope::encode(&[fixture.log(), fixture.0.join("absent/log")]).unwrap();
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "printf launched > marker"])
        .current_dir(&fixture.0)
        .env(VARIABLE, encoded);
    assert!(OwnedChild::spawn(command).is_err());
    assert!(!fixture.0.join("marker").exists());
    assert_eq!(states(&fixture.events()), ["started"]);
}

#[cfg(unix)]
#[test]
fn inherited_scope_records_real_nested_owners() {
    let fixture = Fixture::new();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["inventory::tests::nested_owned_launch_probe", "--exact"])
        .env(NESTED_PROBE, "1")
        .env(VARIABLE, fixture.log())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = OwnedChild::spawn(command).unwrap();
    let outer_pid = child.id();
    assert!(wait(&mut child).success());
    let events = fixture.events();
    assert_eq!(events.len(), 6);
    for owner in [std::process::id(), outer_pid] {
        let owned: Vec<_> = events
            .iter()
            .filter(|event| event["owner_pid"] == owner)
            .cloned()
            .collect();
        assert_eq!(states(&owned), ["started", "spawned", "reaped"]);
        assert_eq!(owned[2]["exit_success"], owner == std::process::id());
    }
}
