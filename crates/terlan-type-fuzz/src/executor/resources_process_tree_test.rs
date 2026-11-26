//! Linux process-tree cancellation coverage for the type-fuzz executor.

use super::capture;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const PROCESS_TREE: &str = r#"role="$1"
root="$2"
printf '%s' "$$" > "$root/$role"
case "$role" in
    compiler)
        /bin/sh "$0" worker "$root" &
        wait "$!"
        ;;
    worker)
        /bin/sh "$0" helper "$root" &
        wait "$!"
        ;;
    helper)
        while :; do :; done
        ;;
esac
"#;

fn recorded_pid(root: &Path, role: &str) -> u32 {
    fs::read_to_string(root.join(role))
        .expect("recorded process identity")
        .parse()
        .expect("numeric process identity")
}

fn process_is_terminal(proc_root: &Path, pid: u32) -> bool {
    let Ok(stat) = fs::read_to_string(proc_root.join(pid.to_string()).join("stat")) else {
        return true;
    };
    let Some((_, fields)) = stat.rsplit_once(") ") else {
        return false;
    };
    fields.starts_with('Z') || fields.starts_with('X')
}

fn assert_process_terminal(role: &str, pid: u32) {
    assert!(
        wait_process_terminal(Path::new("/proc"), pid, Duration::from_secs(2)),
        "{role} process {pid} survived cancellation"
    );
}

fn wait_process_terminal(proc_root: &Path, pid: u32, timeout: Duration) -> bool {
    let started = Instant::now();
    while !process_is_terminal(proc_root, pid) {
        if started.elapsed() >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(2));
    }
    true
}

#[test]
fn process_terminal_state_distinguishes_live_and_dead_entries() {
    let root = tempfile::tempdir().expect("synthetic proc root");
    assert!(process_is_terminal(root.path(), 7));
    assert!(wait_process_terminal(root.path(), 7, Duration::ZERO));
    fs::create_dir(root.path().join("7")).expect("synthetic process root");
    for (stat, terminal) in [
        ("malformed", false),
        ("7 (worker) R 1 7 0 0\n", false),
        ("7 (worker) Z 1 7 0 0\n", true),
        ("7 (worker) X 1 7 0 0\n", true),
    ] {
        fs::write(root.path().join("7/stat"), stat).expect("synthetic process state");
        assert_eq!(process_is_terminal(root.path(), 7), terminal);
    }
    fs::write(root.path().join("7/stat"), "7 (worker) R 1 7 0 0\n").expect("live process state");
    assert!(!wait_process_terminal(root.path(), 7, Duration::ZERO));

    let stat = root.path().join("7/stat");
    let terminal = thread::spawn(move || {
        thread::sleep(Duration::from_millis(5));
        fs::write(stat, "7 (worker) Z 1 7 0 0\n").expect("terminal process state");
    });
    assert!(wait_process_terminal(
        root.path(),
        7,
        Duration::from_secs(1)
    ));
    terminal.join().expect("terminal state writer");
}

#[test]
fn resource_cancellation_terminates_compiler_worker_and_helper() {
    let root = tempfile::tempdir().expect("process fixture root");
    let script = root.path().join("process-tree.sh");
    fs::write(&script, PROCESS_TREE).expect("write process fixture");
    let mut command = Command::new("/bin/sh");
    command
        .arg(&script)
        .arg("compiler")
        .arg(root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let result = capture(&mut command, 3_000, 1024 * 1024 * 1024, 10);

    assert!(result.output.unwrap_err().contains("cancelled"));
    assert_eq!(result.resources.limit_exceeded, Some("cpu"));
    assert!(result.resources.process_group_cleanup);
    for role in ["compiler", "worker", "helper"] {
        assert_process_terminal(role, recorded_pid(root.path(), role));
    }
}
