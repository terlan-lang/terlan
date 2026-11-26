use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use crate::runtime::native_boundary::dispatch::capture_tool_command_with_launch;

const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub(super) struct ResourceEvidence {
    pub(super) schema: &'static str,
    pub(super) measurement_supported: bool,
    pub(super) wall_timeout_ms: u64,
    pub(super) output_limit_bytes: usize,
    pub(super) rss_limit_bytes: u64,
    pub(super) cpu_limit_ms: u64,
    pub(super) peak_rss_bytes: Option<u64>,
    pub(super) peak_cpu_ms: Option<u64>,
    pub(super) limit_exceeded: Option<&'static str>,
    pub(super) process_group_cleanup: bool,
}

impl ResourceEvidence {
    pub(super) fn unmeasured(wall_ms: u64, rss_bytes: u64, cpu_ms: u64) -> Self {
        Self {
            schema: "terlan.type-fuzz-resources.v1",
            measurement_supported: cfg!(target_os = "linux"),
            wall_timeout_ms: wall_ms,
            output_limit_bytes: OUTPUT_LIMIT,
            rss_limit_bytes: rss_bytes,
            cpu_limit_ms: cpu_ms,
            peak_rss_bytes: None,
            peak_cpu_ms: None,
            limit_exceeded: None,
            process_group_cleanup: true,
        }
    }

    pub(super) fn deterministic_success(&self) -> Self {
        let mut evidence = self.clone();
        evidence.peak_rss_bytes = None;
        evidence.peak_cpu_ms = None;
        evidence
    }
}

pub(super) struct Capture {
    pub(super) output: Result<Output, String>,
    pub(super) resources: ResourceEvidence,
}

pub(super) fn capture(command: &mut Command, wall_ms: u64, rss_bytes: u64, cpu_ms: u64) -> Capture {
    let evidence = Arc::new(Mutex::new(ResourceEvidence::unmeasured(
        wall_ms, rss_bytes, cpu_ms,
    )));
    let stopped = Arc::new(AtomicBool::new(false));
    let mut monitor = None;
    let output = capture_tool_command_with_launch(
        command,
        "type-fuzz check oracle",
        Duration::from_millis(wall_ms),
        OUTPUT_LIMIT,
        |pid| launch_monitor(pid, rss_bytes, cpu_ms, &stopped, &evidence, &mut monitor),
    )
    .map_err(|error| error.to_string());
    stopped.store(true, Ordering::Release);
    if let Some(thread) = monitor {
        let _ = thread.join();
    }
    let resources = evidence
        .lock()
        .map(|value| value.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
    Capture { output, resources }
}

#[cfg(target_os = "linux")]
fn launch_monitor(
    pid: u32,
    rss_limit: u64,
    cpu_limit_ms: u64,
    stopped: &Arc<AtomicBool>,
    evidence: &Arc<Mutex<ResourceEvidence>>,
    monitor: &mut Option<std::thread::JoinHandle<()>>,
) -> Result<(), String> {
    let stopped = Arc::clone(stopped);
    let evidence = Arc::clone(evidence);
    *monitor = Some(
        std::thread::Builder::new()
            .name("type-fuzz-resources".into())
            .spawn(move || monitor_linux(pid, rss_limit, cpu_limit_ms, &stopped, &evidence))
            .map_err(|error| format!("cannot start resource monitor: {error}"))?,
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn launch_monitor(
    _pid: u32,
    _rss_limit: u64,
    _cpu_limit_ms: u64,
    _stopped: &Arc<AtomicBool>,
    _evidence: &Arc<Mutex<ResourceEvidence>>,
    _monitor: &mut Option<std::thread::JoinHandle<()>>,
) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn monitor_linux(
    pid: u32,
    rss_limit: u64,
    cpu_limit_ms: u64,
    stopped: &AtomicBool,
    evidence: &Mutex<ResourceEvidence>,
) {
    while !stopped.load(Ordering::Acquire) {
        let Some((rss, cpu_ms)) = linux_group_sample(pid) else {
            return;
        };
        let exceeded = if rss > rss_limit {
            Some("rss")
        } else if cpu_ms > cpu_limit_ms {
            Some("cpu")
        } else {
            None
        };
        if let Ok(mut current) = evidence.lock() {
            current.peak_rss_bytes = Some(current.peak_rss_bytes.unwrap_or(0).max(rss));
            current.peak_cpu_ms = Some(current.peak_cpu_ms.unwrap_or(0).max(cpu_ms));
            current.limit_exceeded = current.limit_exceeded.or(exceeded);
        }
        if exceeded.is_some() {
            terminate_group(pid);
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
fn linux_group_sample(group: u32) -> Option<(u64, u64)> {
    let mut rss = 0_u64;
    let mut cpu_ms = 0_u64;
    let mut found = false;
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Some((process_group, process_rss, process_cpu)) = linux_process_sample(pid) else {
            continue;
        };
        if process_group == group {
            found = true;
            rss = rss.saturating_add(process_rss);
            cpu_ms = cpu_ms.saturating_add(process_cpu);
        }
    }
    found.then_some((rss, cpu_ms))
}

#[cfg(target_os = "linux")]
fn linux_process_sample(pid: u32) -> Option<(u32, u64, u64)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, stat_fields) = stat.rsplit_once(") ")?;
    let process_group = stat_fields.split_whitespace().nth(2)?.parse::<u32>().ok()?;
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let rss_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    let schedstat = std::fs::read_to_string(format!("/proc/{pid}/schedstat")).ok()?;
    let cpu_ns = schedstat.split_whitespace().next()?.parse::<u64>().ok()?;
    Some((
        process_group,
        rss_kib.saturating_mul(1024),
        cpu_ns / 1_000_000,
    ))
}

#[cfg(target_os = "linux")]
fn terminate_group(pid: u32) {
    use rustix::process::{kill_process_group, Pid, Signal};
    if let Some(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw) {
        let _ = kill_process_group(pid, Signal::KILL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmeasured_evidence_retains_every_limit() {
        let evidence = ResourceEvidence::unmeasured(20, 30, 40);
        assert_eq!(evidence.wall_timeout_ms, 20);
        assert_eq!(evidence.rss_limit_bytes, 30);
        assert_eq!(evidence.cpu_limit_ms, 40);
        assert_eq!(evidence.limit_exceeded, None);
        let mut measured = evidence.clone();
        measured.peak_rss_bytes = Some(99);
        measured.peak_cpu_ms = Some(88);
        assert_eq!(measured.deterministic_success().peak_rss_bytes, None);
        assert_eq!(measured.deterministic_success().peak_cpu_ms, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn samples_the_current_linux_process() {
        let (group, rss, _cpu) =
            linux_process_sample(std::process::id()).expect("current process sample");
        assert!(rss > 0);
        let (group_rss, _group_cpu) = linux_group_sample(group).expect("process group sample");
        assert!(group_rss >= rss);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn resource_limit_child() {
        match std::env::var("TERLAN_FUZZ_RESOURCE_CHILD").as_deref() {
            Ok("rss") => {
                let mut allocations = Vec::new();
                loop {
                    allocations.push(vec![0_u8; 1024 * 1024]);
                    std::hint::black_box(&allocations);
                }
            }
            Ok("cpu") => loop {
                std::hint::spin_loop();
            },
            _ => {}
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kills_the_process_group_at_the_rss_limit() {
        let mut command = resource_child("rss");
        let capture = capture(&mut command, 5_000, 1024 * 1024, 5_000);
        assert!(capture.output.is_err());
        assert_eq!(capture.resources.limit_exceeded, Some("rss"));
        assert!(capture.resources.peak_rss_bytes.is_some());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kills_the_process_group_at_the_cpu_limit() {
        let mut command = resource_child("cpu");
        let capture = capture(&mut command, 5_000, 1024 * 1024 * 1024, 10);
        assert!(capture.output.is_err());
        assert_eq!(capture.resources.limit_exceeded, Some("cpu"));
        assert!(capture.resources.peak_cpu_ms.is_some());
    }

    #[cfg(target_os = "linux")]
    fn resource_child(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "commands::fuzz::resources::tests::resource_limit_child",
                "--nocapture",
            ])
            .env("TERLAN_FUZZ_RESOURCE_CHILD", mode);
        command
    }
}
