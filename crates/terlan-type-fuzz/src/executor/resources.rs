use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use terlan_process_owner::{CapturedCommandOutput, ProcessControl};

pub(super) const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub(super) struct ResourceEvidence {
    pub(super) schema: &'static str,
    pub(super) measurement_supported: bool,
    pub(super) wall_timeout_ms: u64,
    pub(super) wall_elapsed_ms: Option<u64>,
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
            wall_elapsed_ms: None,
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
        evidence.wall_elapsed_ms = None;
        evidence.peak_rss_bytes = None;
        evidence.peak_cpu_ms = None;
        evidence
    }
}

pub(super) struct Capture {
    pub(super) output: Result<CapturedCommandOutput, String>,
    pub(super) resources: ResourceEvidence,
}

pub(super) fn capture(command: &mut Command, wall_ms: u64, rss_bytes: u64, cpu_ms: u64) -> Capture {
    let started = Instant::now();
    let evidence = Arc::new(Mutex::new(ResourceEvidence::unmeasured(
        wall_ms, rss_bytes, cpu_ms,
    )));
    let monitor_stopped = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut monitor = None;
    let output = ProcessControl::new(Duration::from_millis(wall_ms))
        .with_cancellation(&cancelled)
        .capture_output_result(command, OUTPUT_LIMIT, |pid| {
            launch_monitor(
                pid,
                rss_bytes,
                cpu_ms,
                &monitor_stopped,
                &cancelled,
                &evidence,
                &mut monitor,
            );
            Ok(())
        })
        .map_err(|error| format!("{}: {}", error.kind, error.detail));
    monitor_stopped.store(true, Ordering::Release);
    if let Some(thread) = monitor {
        let _ = thread.join();
    }
    let mut resources = clone_evidence(&evidence);
    resources.wall_elapsed_ms =
        Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
    Capture { output, resources }
}

fn clone_evidence(evidence: &Mutex<ResourceEvidence>) -> ResourceEvidence {
    match evidence.lock() {
        Ok(value) => value.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

#[cfg(target_os = "linux")]
fn launch_monitor(
    pid: u32,
    rss_limit: u64,
    cpu_limit_ms: u64,
    stopped: &Arc<AtomicBool>,
    cancelled: &Arc<AtomicBool>,
    evidence: &Arc<Mutex<ResourceEvidence>>,
    monitor: &mut Option<std::thread::JoinHandle<()>>,
) {
    let stopped = Arc::clone(stopped);
    let cancelled = Arc::clone(cancelled);
    let evidence = Arc::clone(evidence);
    *monitor = Some(std::thread::spawn(move || {
        monitor_linux(
            pid,
            rss_limit,
            cpu_limit_ms,
            &stopped,
            &cancelled,
            &evidence,
        );
    }));
}

#[cfg(not(target_os = "linux"))]
fn launch_monitor(
    _pid: u32,
    _rss_limit: u64,
    _cpu_limit_ms: u64,
    _stopped: &Arc<AtomicBool>,
    _cancelled: &Arc<AtomicBool>,
    _evidence: &Arc<Mutex<ResourceEvidence>>,
    _monitor: &mut Option<std::thread::JoinHandle<()>>,
) {
}

#[cfg(target_os = "linux")]
fn monitor_linux(
    pid: u32,
    rss_limit: u64,
    cpu_limit_ms: u64,
    stopped: &AtomicBool,
    cancelled: &AtomicBool,
    evidence: &Mutex<ResourceEvidence>,
) {
    while !stopped.load(Ordering::Acquire) {
        let Some((rss, cpu_ms)) = linux_group_sample(pid) else {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        };
        let exceeded = if rss > rss_limit {
            Some("rss")
        } else if cpu_ms > cpu_limit_ms {
            Some("cpu")
        } else {
            None
        };
        record_sample(evidence, rss, cpu_ms, exceeded);
        if exceeded.is_some() {
            cancelled.store(true, Ordering::Release);
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
fn record_sample(
    evidence: &Mutex<ResourceEvidence>,
    rss: u64,
    cpu_ms: u64,
    exceeded: Option<&'static str>,
) {
    if let Ok(mut current) = evidence.lock() {
        current.peak_rss_bytes = Some(current.peak_rss_bytes.unwrap_or(0).max(rss));
        current.peak_cpu_ms = Some(current.peak_cpu_ms.unwrap_or(0).max(cpu_ms));
        current.limit_exceeded = current.limit_exceeded.or(exceeded);
    }
}

#[cfg(target_os = "linux")]
fn linux_group_sample(group: u32) -> Option<(u64, u64)> {
    linux_group_sample_in(std::path::Path::new("/proc"), group)
}

#[cfg(target_os = "linux")]
fn linux_group_sample_in(proc_root: &std::path::Path, group: u32) -> Option<(u64, u64)> {
    let mut rss = 0_u64;
    let mut cpu_ms = 0_u64;
    let mut found = false;
    for entry in std::fs::read_dir(proc_root).ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Some((process_group, process_rss, process_cpu)) =
            linux_process_sample_in(proc_root, pid)
        else {
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
fn linux_process_sample_in(proc_root: &std::path::Path, pid: u32) -> Option<(u32, u64, u64)> {
    let process_root = proc_root.join(pid.to_string());
    let stat = std::fs::read_to_string(process_root.join("stat")).ok()?;
    let (_, stat_fields) = stat.rsplit_once(") ")?;
    let process_group = stat_fields.split_whitespace().nth(2)?.parse::<u32>().ok()?;
    let status = std::fs::read_to_string(process_root.join("status")).ok()?;
    let rss_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    let cpu_ms = linux_cpu_ms_from_task_dir(&process_root.join("task"))?;
    Some((process_group, rss_kib.saturating_mul(1024), cpu_ms))
}

#[cfg(target_os = "linux")]
fn linux_cpu_ms_from_task_dir(task_directory: &std::path::Path) -> Option<u64> {
    let mut cpu_ns = 0_u64;
    let mut found = false;
    for entry in std::fs::read_dir(task_directory).ok()?.flatten() {
        let Ok(schedstat) = std::fs::read_to_string(entry.path().join("schedstat")) else {
            continue;
        };
        let Some(value) = schedstat
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        cpu_ns = cpu_ns.saturating_add(value);
        found = true;
    }
    found.then_some(cpu_ns / 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmeasured_evidence_retains_every_limit() {
        let evidence = ResourceEvidence::unmeasured(20, 30, 40);
        assert_eq!(evidence.wall_timeout_ms, 20);
        assert_eq!(evidence.wall_elapsed_ms, None);
        assert_eq!(evidence.rss_limit_bytes, 30);
        assert_eq!(evidence.cpu_limit_ms, 40);
        assert_eq!(evidence.limit_exceeded, None);
        let mut measured = evidence.clone();
        measured.wall_elapsed_ms = Some(77);
        measured.peak_rss_bytes = Some(99);
        measured.peak_cpu_ms = Some(88);
        assert_eq!(measured.deterministic_success().peak_rss_bytes, None);
        assert_eq!(measured.deterministic_success().peak_cpu_ms, None);
        assert_eq!(measured.deterministic_success().wall_elapsed_ms, None);

        let poisoned = Mutex::new(evidence);
        let _ = std::panic::catch_unwind(|| {
            let _guard = poisoned.lock().expect("evidence lock");
            panic!("poison evidence for recovery coverage");
        });
        assert_eq!(clone_evidence(&poisoned).wall_timeout_ms, 20);

        record_sample(&poisoned, 10, 20, Some("cpu"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn samples_the_current_linux_process() {
        let (group, rss, _cpu) =
            linux_process_sample_in(std::path::Path::new("/proc"), std::process::id())
                .expect("current process sample");
        assert!(rss > 0);
        let (group_rss, _group_cpu) = linux_group_sample(group).expect("process group sample");
        assert!(group_rss >= rss);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn resource_limit_child() {
        if let Ok(mode) = std::env::var("TERLAN_FUZZ_RESOURCE_CHILD") {
            resource_work(&mode, None);
        }
    }

    #[cfg(target_os = "linux")]
    fn resource_work(mode: &str, iterations: Option<usize>) {
        let mut index = 0;
        let mut allocations = Vec::new();
        loop {
            match mode {
                "rss" => {
                    allocations.push(vec![0_u8; 1024 * 1024]);
                    std::hint::black_box(&allocations);
                }
                "cpu" => std::hint::spin_loop(),
                "output" => eprint!("0123456789abcdef"),
                _ => return,
            }
            index += 1;
            if iterations == Some(index) {
                return;
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn enforces_rss_cpu_wall_and_output_limits() {
        for mode in ["rss", "cpu", "output", "unknown"] {
            resource_work(mode, Some(1));
        }
        for (mode, wall, rss, cpu, expected) in [
            ("rss", 5_000, 1024 * 1024, 5_000, "rss"),
            ("cpu", 5_000, 1024 * 1024 * 1024, 10, "cpu"),
        ] {
            let mut command = resource_child(mode);
            let result = capture(&mut command, wall, rss, cpu);
            assert!(result.output.is_err());
            assert_eq!(result.resources.limit_exceeded, Some(expected));
            assert!(result.resources.process_group_cleanup);
        }

        let mut command = resource_child("output");
        let result = capture(&mut command, 5_000, 1024 * 1024 * 1024, 5_000);
        assert!(result.output.unwrap_err().contains("output-limit"));

        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "sleep 30 & wait"]);
        let result = capture(&mut command, 20, 1024 * 1024 * 1024, 5_000);
        assert!(result.output.unwrap_err().contains("timed-out"));
        assert!(result.resources.wall_elapsed_ms.is_some());
    }

    #[cfg(target_os = "linux")]
    fn resource_child(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "executor::resources::tests::resource_limit_child",
                "--nocapture",
            ])
            .env("TERLAN_FUZZ_RESOURCE_CHILD", mode);
        command
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cpu_accounting_ignores_vanished_or_malformed_task_entries() {
        let root = tempfile::tempdir().expect("task root");
        for task in ["1", "2", "3"] {
            std::fs::create_dir(root.path().join(task)).expect("task directory");
        }
        std::fs::write(root.path().join("1/schedstat"), "4000000 0 0\n").expect("valid task");
        std::fs::write(root.path().join("2/schedstat"), "invalid\n").expect("invalid task");
        assert_eq!(linux_cpu_ms_from_task_dir(root.path()), Some(4));
        assert_eq!(
            linux_cpu_ms_from_task_dir(&root.path().join("missing")),
            None
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn proc_sampling_handles_every_missing_or_malformed_field() {
        fn sample(
            stat: Option<&str>,
            status: Option<&str>,
            schedstat: Option<&str>,
        ) -> Option<(u32, u64, u64)> {
            let root = tempfile::tempdir().expect("proc root");
            let process = root.path().join("7");
            std::fs::create_dir_all(process.join("task/7")).expect("process directories");
            if let Some(value) = stat {
                std::fs::write(process.join("stat"), value).expect("stat");
            }
            if let Some(value) = status {
                std::fs::write(process.join("status"), value).expect("status");
            }
            if let Some(value) = schedstat {
                std::fs::write(process.join("task/7/schedstat"), value).expect("schedstat");
            }
            linux_process_sample_in(root.path(), 7)
        }

        let valid_stat = "7 (worker) S 1 9 0 0\n";
        let valid_status = "Name:\tworker\nVmRSS:\t4 kB\n";
        assert_eq!(sample(None, None, None), None);
        assert_eq!(sample(Some("malformed"), None, None), None);
        assert_eq!(sample(Some("7 (worker) S 1"), None, None), None);
        assert_eq!(sample(Some("7 (worker) S 1 bad"), None, None), None);
        assert_eq!(sample(Some(valid_stat), None, None), None);
        assert_eq!(
            sample(Some(valid_stat), Some("Name:\tworker\n"), None),
            None
        );
        assert_eq!(sample(Some(valid_stat), Some("VmRSS:\n"), None), None);
        assert_eq!(
            sample(Some(valid_stat), Some("VmRSS:\tbad kB\n"), None),
            None
        );
        assert_eq!(sample(Some(valid_stat), Some(valid_status), None), None);
        assert_eq!(
            sample(Some(valid_stat), Some(valid_status), Some("3000000 0 0\n")),
            Some((9, 4096, 3))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn group_sampling_handles_missing_invalid_and_unrelated_processes() {
        let root = tempfile::tempdir().expect("proc root");
        assert_eq!(linux_group_sample_in(&root.path().join("missing"), 9), None);
        std::fs::create_dir(root.path().join("not-a-pid")).expect("invalid proc entry");
        assert_eq!(linux_group_sample_in(root.path(), 9), None);

        let process = root.path().join("7");
        std::fs::create_dir_all(process.join("task/7")).expect("process directories");
        std::fs::write(process.join("stat"), "7 (worker) S 1 9 0 0\n").expect("stat");
        std::fs::write(process.join("status"), "VmRSS:\t4 kB\n").expect("status");
        std::fs::write(process.join("task/7/schedstat"), "3000000 0 0\n").expect("schedstat");
        assert_eq!(linux_group_sample_in(root.path(), 8), None);
        assert_eq!(linux_group_sample_in(root.path(), 9), Some((4096, 3)));
    }
}
