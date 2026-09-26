#![cfg(any(target_os = "linux", target_os = "macos"))]

use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROBE_GROUP: &str = "TERLAN_PROCESS_OWNER_MOVED_GROUP";
const PROBE_READY: &str = "TERLAN_PROCESS_OWNER_MOVED_READY";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("terlan-moved-child-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove only the owned terminal fixture");
    }
}

fn ready(path: &Path) {
    let start = Instant::now();
    while !path.try_exists().unwrap() {
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "child failed to move groups"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn escaped_fixture() -> bool {
    let Ok(group) = std::env::var(PROBE_GROUP) else {
        return false;
    };
    let group = Pid::from_raw(group.parse().unwrap()).unwrap();
    rustix::process::setpgid(None, Some(group)).expect("join the peer's group");
    fs::write(std::env::var_os(PROBE_READY).unwrap(), "ready").unwrap();
    // A broken owner fails within a finite time, rather than hanging the suite.
    thread::sleep(Duration::from_secs(3));
    true
}

fn command(group: u32, ready: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "child::tests::moving_child_cannot_escape_direct_termination",
            "--exact",
        ])
        .env(PROBE_GROUP, group.to_string())
        .env(PROBE_READY, ready)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    command
}

#[test]
fn moving_child_cannot_escape_direct_termination() {
    if escaped_fixture() {
        return;
    }
    let fixture = Fixture::new();
    let mut peer_command = Command::new("/bin/sh");
    peer_command.args(["-c", "sleep 30"]);
    let mut peer = OwnedChild::spawn(peer_command).unwrap();
    for mode in ["finish", "unwind", "timeout", "observation-failure"] {
        let path = fixture.0.join(mode);
        let mut command = command(peer.id(), &path);
        let started = Instant::now();
        let mut pid = None;
        match mode {
            "finish" => {
                let mut child = OwnedChild::spawn(command).unwrap();
                pid = Pid::from_raw(child.id() as i32);
                ready(&path);
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(child.finish().unwrap().signal(), Some(9));
            }
            "unwind" => {
                let child = OwnedChild::spawn(command).unwrap();
                pid = Pid::from_raw(child.id() as i32);
                ready(&path);
                assert!(std::panic::catch_unwind(move || {
                    let _child = child;
                    panic!("producer failed after changing process group");
                })
                .is_err());
            }
            _ => {
                let error = crate::capture_stdout_with_launch(
                    &mut command,
                    Duration::from_millis(100),
                    1024 * 1024,
                    |child| {
                        pid = Pid::from_raw(child as i32);
                        ready(&path);
                        if mode == "observation-failure" {
                            Err("cannot persist launch".into())
                        } else {
                            Ok(())
                        }
                    },
                )
                .unwrap_err();
                assert_eq!(
                    error.kind,
                    if mode == "timeout" {
                        "timed-out"
                    } else {
                        "observation-failed"
                    }
                );
            }
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{mode} waited for natural exit"
        );
        assert!(
            matches!(
                rustix::process::waitpid(pid, rustix::process::WaitOptions::NOHANG),
                Err(rustix::io::Errno::CHILD)
            ),
            "{mode} failed to reap its own child"
        );
        assert!(
            peer.try_wait().unwrap().is_none(),
            "{mode} killed a foreign group"
        );
    }
    peer.finish().unwrap();
}

fn await_exit(child: &OwnedChild) -> Pid {
    let pid = Pid::from_raw(child.id() as i32).unwrap();
    let start = Instant::now();
    loop {
        if waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .is_some()
        {
            return pid;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn natural_exit_preserves_status_and_reaps_retained_group_leader() {
    for code in [0, 7] {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", &format!("exit {code}")]);
        let mut child = OwnedChild::spawn(command).unwrap();
        let pid = await_exit(&child);
        assert_eq!(child.try_wait().unwrap().unwrap().code(), Some(code));
        assert_eq!(child.try_wait().unwrap().unwrap().code(), Some(code));
        assert_eq!(child.finish().unwrap().code(), Some(code));
        assert!(matches!(
            waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG
            ),
            Err(rustix::io::Errno::CHILD)
        ));
    }
}

#[test]
fn group_permission_errors_remain_errors_for_live_children() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 30"]);
    let mut child = OwnedChild::spawn(command).unwrap();
    let pid = Pid::from_raw(child.id() as i32).unwrap();
    assert_eq!(
        child
            .check_group_error(pid, rustix::io::Errno::PERM)
            .unwrap_err()
            .raw_os_error(),
        Some(rustix::io::Errno::PERM.raw_os_error())
    );
    child.finish().unwrap();
}

#[test]
fn exited_leader_does_not_hide_permission_errors_for_remaining_members() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 30 & exit 7"]);
    let mut child = OwnedChild::spawn(command).unwrap();
    let pid = await_exit(&child);
    assert_eq!(
        child
            .check_group_error(pid, rustix::io::Errno::PERM)
            .unwrap_err()
            .raw_os_error(),
        Some(rustix::io::Errno::PERM.raw_os_error())
    );
    assert_eq!(child.finish().unwrap().code(), Some(7));
}
