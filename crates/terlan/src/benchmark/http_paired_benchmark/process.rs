use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;

use serde::Serialize;

#[derive(Clone)]
pub(super) struct ProcessSnapshot {
    processes: BTreeMap<u32, ProcessSample>,
    load_average: String,
}

#[derive(Clone)]
struct ProcessSample {
    command: String,
    cpu_ticks: u64,
    processor: usize,
}

#[derive(Serialize)]
pub(super) struct ContaminationEvidence {
    pub(super) status: &'static str,
    pub(super) tick_limit: u64,
    pub(super) load_average_before: String,
    pub(super) load_average_after: String,
    pub(super) offenders: Vec<ContaminatingProcess>,
}

#[derive(Serialize)]
pub(super) struct ContaminatingProcess {
    pub(super) pid: u32,
    pub(super) command: String,
    pub(super) cpu_ticks: u64,
    pub(super) processor: usize,
}

pub(super) fn snapshot() -> ProcessSnapshot {
    let cpus = reserved_cpus();
    let own_pid = std::process::id();
    let mut processes = BTreeMap::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse().ok())
            else {
                continue;
            };
            if pid == own_pid {
                continue;
            }
            let Some(sample) = read_process(pid) else {
                continue;
            };
            if cpus
                .as_ref()
                .is_none_or(|allowed| allowed.contains(&sample.processor))
            {
                processes.insert(pid, sample);
            }
        }
    }
    ProcessSnapshot {
        processes,
        load_average: fs::read_to_string("/proc/loadavg")
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
    }
}

pub(super) fn compare(
    before: ProcessSnapshot,
    after: ProcessSnapshot,
    tick_limit: u64,
) -> ContaminationEvidence {
    let mut offenders = Vec::new();
    for (pid, prior) in &before.processes {
        let Some(current) = after.processes.get(pid) else {
            continue;
        };
        let ticks = current.cpu_ticks.saturating_sub(prior.cpu_ticks);
        if ticks > tick_limit {
            offenders.push(ContaminatingProcess {
                pid: *pid,
                command: current.command.clone(),
                cpu_ticks: ticks,
                processor: current.processor,
            });
        }
    }
    ContaminationEvidence {
        status: if offenders.is_empty() {
            "clean"
        } else {
            "contaminated"
        },
        tick_limit,
        load_average_before: before.load_average,
        load_average_after: after.load_average,
        offenders,
    }
}

fn read_process(pid: u32) -> Option<ProcessSample> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let open = stat.find('(')?;
    let close = stat.rfind(") ")?;
    let command = stat.get(open + 1..close)?.to_string();
    let fields = stat
        .get(close + 2..)?
        .split_whitespace()
        .collect::<Vec<_>>();
    let user_ticks = fields.get(11)?.parse::<u64>().ok()?;
    let system_ticks = fields.get(12)?.parse::<u64>().ok()?;
    let processor = fields.get(36)?.parse::<usize>().ok()?;
    Some(ProcessSample {
        command,
        cpu_ticks: user_ticks.saturating_add(system_ticks),
        processor,
    })
}

fn reserved_cpus() -> Option<BTreeSet<usize>> {
    let value = env::var("TERLAN_BENCH_HTTP_CPU_LIST").ok()?;
    let mut cpus = BTreeSet::new();
    for part in value.split(',') {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            let start = start.parse::<usize>().ok()?;
            let end = end.parse::<usize>().ok()?;
            cpus.extend(start..=end);
        } else {
            cpus.insert(part.parse().ok()?);
        }
    }
    (!cpus.is_empty()).then_some(cpus)
}
