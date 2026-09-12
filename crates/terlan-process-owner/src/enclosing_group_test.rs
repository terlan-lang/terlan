#![cfg(any(target_os = "linux", target_os = "macos"))]

use super::*;
use crate::ProcessControl;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ROLE: &str = "TERLAN_ENCLOSING_GROUP_FIXTURE_ROLE";
const ROOT: &str = "TERLAN_ENCLOSING_GROUP_FIXTURE_ROOT";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-enclosing-group-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("clean only the owned terminal process fixture");
    }
}

fn command(role: &str, root: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "child::enclosing_group_test::enclosing_fixture"])
        .env(ROLE, role)
        .env(ROOT, root)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    command
}

fn ready(path: &Path) {
    let started = Instant::now();
    while !path.try_exists().unwrap() {
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "fixture never became ready: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

fn reaped(pid: u32) {
    assert!(matches!(
        rustix::process::waitpid(
            Pid::from_raw(pid as i32),
            rustix::process::WaitOptions::NOHANG
        ),
        Err(rustix::io::Errno::CHILD)
    ));
}

#[test]
fn enclosing_fixture() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    let group = rustix::process::getpgrp().as_raw_pid() as u32;
    let control = ProcessControl::new(Duration::from_secs(10)).with_enclosing_process_group(group);
    match role.as_str() {
        "membership" => {
            fs::write(
                root.join("membership"),
                format!("{} {group}", std::process::id()),
            )
            .unwrap();
        }
        "inner" => {
            let mut sibling =
                OwnedChild::spawn_in_enclosing_group(&mut shell("exec sleep 30"), group).unwrap();
            let mut builder = command("membership", &root);
            use std::os::unix::process::CommandExt;
            builder.process_group(0);
            let mut member = OwnedChild::spawn_in_enclosing_group(&mut builder, group).unwrap();
            ready(&root.join("membership"));
            member.finish().unwrap();
            let observed = fs::read_to_string(root.join("membership")).unwrap();
            assert_eq!(
                observed.split_whitespace().nth(1).unwrap(),
                group.to_string()
            );
            for mode in [
                "run",
                "capture",
                "failed-capture",
                "timeout",
                "cancel",
                "observer",
            ] {
                let flag = AtomicBool::new(false);
                let controlled = control.with_cancellation(&flag);
                let mut pid = 0;
                let mut launched = |child| {
                    pid = child;
                    if mode == "cancel" {
                        flag.store(true, Ordering::Release);
                    }
                    if mode == "observer" {
                        Err("cannot record nested launch".into())
                    } else {
                        Ok(())
                    }
                };
                match mode {
                    "run" => controlled.run(&mut shell("exit 0"), &mut launched).unwrap(),
                    "capture" => assert_eq!(
                        controlled
                            .capture_stdout(&mut shell("printf nested"), 64, &mut launched)
                            .unwrap(),
                        b"nested"
                    ),
                    "failed-capture" => {
                        let result = controlled
                            .capture_stdout_result(
                                &mut shell("printf failure; exit 7"),
                                64,
                                &mut launched,
                            )
                            .unwrap();
                        assert_eq!(result.stdout, b"failure");
                        assert_eq!(result.outcome.unwrap_err().kind, "failed");
                    }
                    _ => {
                        let failure = controlled
                            .limited_to(Duration::from_millis(50))
                            .capture_stdout(&mut shell("exec sleep 30"), 64, &mut launched)
                            .unwrap_err();
                        assert_eq!(
                            failure.kind,
                            match mode {
                                "timeout" => "timed-out",
                                "cancel" => "cancelled",
                                _ => "observation-failed",
                            }
                        );
                    }
                }
                reaped(pid);
                assert!(
                    sibling.try_wait().unwrap().is_none(),
                    "{mode} killed a sibling"
                );
            }
            let nested =
                OwnedChild::spawn_in_enclosing_group(&mut shell("exec sleep 30"), group).unwrap();
            let pid = nested.id();
            assert!(std::panic::catch_unwind(move || {
                let _nested = nested;
                panic!("nested owner unwind");
            })
            .is_err());
            reaped(pid);
            assert!(sibling.try_wait().unwrap().is_none());
            sibling.finish().unwrap();
        }
        "outer" => control
            .run(&mut command("leaf", &root), |_| Ok(()))
            .unwrap(),
        "leaf" => {
            fs::write(root.join("leaf"), format!("{} {group}", std::process::id())).unwrap();
            let mut child = command("grandchild", &root).spawn().unwrap();
            child.wait().unwrap();
        }
        "grandchild" => {
            fs::write(
                root.join("grandchild"),
                format!("{} {group}", std::process::id()),
            )
            .unwrap();
            thread::sleep(Duration::from_secs(10));
        }
        _ => panic!("unknown fixture role"),
    }
}

#[test]
fn nested_owners_preserve_membership_reap_direct_children_and_leave_siblings_alive() {
    let fixture = Fixture::new();
    ProcessControl::new(Duration::from_secs(10))
        .run(&mut command("inner", &fixture.0), |_| Ok(()))
        .unwrap();
}

#[test]
fn invalid_enclosing_group_cannot_launch_or_invoke_the_observer() {
    for group in [0, u32::MAX] {
        let mut observed = false;
        let error = ProcessControl::new(Duration::from_secs(1))
            .with_enclosing_process_group(group)
            .run(&mut shell("exit 0"), |_| {
                observed = true;
                Ok(())
            })
            .unwrap_err();
        assert_eq!(error.kind, "launch-failed");
        assert!(!observed);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn enclosing_timeout_and_cancellation_terminate_nested_children_and_grandchildren() {
    for cancel in [false, true] {
        let fixture = Fixture::new();
        let flag = AtomicBool::new(false);
        let mut root_pid = 0;
        let error = ProcessControl::new(Duration::from_millis(200))
            .with_cancellation(&flag)
            .run(&mut command("outer", &fixture.0), |pid| {
                root_pid = pid;
                ready(&fixture.0.join("grandchild"));
                if cancel {
                    flag.store(true, Ordering::Release);
                }
                Ok(())
            })
            .unwrap_err();
        assert_eq!(error.kind, if cancel { "cancelled" } else { "timed-out" });
        reaped(root_pid);
        for name in ["leaf", "grandchild"] {
            let record = fs::read_to_string(fixture.0.join(name)).unwrap();
            let fields = record.split_whitespace().collect::<Vec<_>>();
            assert_eq!(fields[1], root_pid.to_string());
            let started = Instant::now();
            loop {
                match fs::read_to_string(format!("/proc/{}/stat", fields[0])) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                    Ok(stat) if stat.rsplit_once(") ").unwrap().1.starts_with(['Z', 'X']) => break,
                    other => {
                        assert!(
                            started.elapsed() < Duration::from_secs(2),
                            "nested {name} survived outer cleanup: {other:?}"
                        );
                        thread::sleep(Duration::from_millis(2));
                    }
                }
            }
        }
    }
}
