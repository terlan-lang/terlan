//! Fixed scheduler topology and deterministic actor placement.

use std::fs;
use std::num::NonZeroU64;
use std::thread;

use serde::Serialize;

/// Environment variable that overrides the detected scheduler width.
pub(crate) const VM_SCHEDULER_WIDTH_ENV: &str = "TERLAN_VM_SCHEDULERS";
/// Maximum scheduler width admitted before NUMA-aware placement exists.
pub(crate) const VM_MAX_SCHEDULERS: usize = 32;

/// Recorded host limits used to explain one scheduler-width decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct VmSchedulerHostSnapshot {
    /// Logical CPUs visible through the Rust host-parallelism API.
    host_logical_cpus: usize,
    /// Unique Linux physical package and core pairs when discoverable.
    physical_cores: Option<usize>,
    /// Linux process affinity in canonical CPU-list form.
    process_affinity: Option<String>,
    /// Number of logical CPUs admitted by process affinity.
    process_affinity_cpus: Option<usize>,
    /// Effective cgroup cpuset in canonical CPU-list form.
    cgroup_cpuset: Option<String>,
    /// Number of logical CPUs admitted by the effective cgroup cpuset.
    cgroup_cpuset_cpus: Option<usize>,
    /// Bounded cgroup quota in microseconds, or none when unlimited.
    cgroup_cpu_quota_micros: Option<u64>,
    /// Cgroup quota period in microseconds when the controller is visible.
    cgroup_cpu_period_micros: Option<u64>,
    /// Conservative scheduler count derived from the cgroup quota.
    cgroup_quota_cpus: Option<usize>,
    /// Smallest nonzero capacity visible through host and container limits.
    effective_parallelism: usize,
    /// Explicit scheduler-width environment override when valid.
    scheduler_override: Option<usize>,
}

impl VmSchedulerHostSnapshot {
    /// Captures scheduler-relevant host, affinity, and cgroup limits.
    pub(crate) fn capture() -> Self {
        let host_logical_cpus = thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        let cgroup_cpuset = read_trimmed("/sys/fs/cgroup/cpuset.cpus.effective");
        let cgroup_cpuset_cpus = cgroup_cpuset.as_deref().and_then(cpu_list_count);
        let quota = read_trimmed("/sys/fs/cgroup/cpu.max")
            .as_deref()
            .and_then(parse_cgroup_v2_quota);
        let process_affinity = fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| process_affinity_list(&status));
        let process_affinity_cpus = process_affinity.as_deref().and_then(cpu_list_count);
        let physical_cores = fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|cpuinfo| physical_core_count(&cpuinfo));
        let effective_parallelism = effective_parallelism(
            host_logical_cpus,
            process_affinity_cpus.or(cgroup_cpuset_cpus),
            quota.and_then(|quota| quota.scheduler_limit),
        )
        .clamp(1, VM_MAX_SCHEDULERS);
        let scheduler_override = std::env::var(VM_SCHEDULER_WIDTH_ENV)
            .ok()
            .and_then(|value| value.parse::<usize>().ok());
        Self {
            host_logical_cpus,
            physical_cores,
            process_affinity,
            process_affinity_cpus,
            cgroup_cpuset,
            cgroup_cpuset_cpus,
            cgroup_cpu_quota_micros: quota.and_then(|quota| quota.quota_micros),
            cgroup_cpu_period_micros: quota.map(|quota| quota.period_micros),
            cgroup_quota_cpus: quota.and_then(|quota| quota.scheduler_limit),
            effective_parallelism,
            scheduler_override,
        }
    }

    /// Returns effective host capacity before an explicit scheduler override.
    pub(crate) const fn effective_parallelism(&self) -> usize {
        self.effective_parallelism
    }
}

/// Parsed Linux cgroup v2 CPU quota.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CgroupV2CpuQuota {
    /// Bounded execution quota in microseconds, or none for `max`.
    quota_micros: Option<u64>,
    /// Quota accounting period in microseconds.
    period_micros: u64,
    /// Conservative whole scheduler count admitted by the quota.
    scheduler_limit: Option<usize>,
}

/// Stable identity of one scheduler thread inside an execution shard.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct VmSchedulerId(usize);

impl VmSchedulerId {
    /// Returns the compatibility identity used by one-scheduler runtimes.
    pub(crate) const fn primary() -> Self {
        Self(0)
    }

    /// Returns the zero-based scheduler index used by local queue arrays.
    pub(crate) const fn index(self) -> usize {
        self.0
    }

    /// Returns the nonzero identity stored in actor ownership words.
    pub(crate) fn owner_word(self) -> NonZeroU64 {
        NonZeroU64::new(self.0 as u64 + 1).expect("scheduler owner identity is nonzero")
    }
}

/// Shard-global actor identity paired with home and current schedulers.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmFixedActorRoute {
    actor_id: NonZeroU64,
    home_scheduler: VmSchedulerId,
    scheduler: VmSchedulerId,
}

impl VmFixedActorRoute {
    /// Returns the shard-global actor identity.
    pub(crate) const fn actor_id(self) -> NonZeroU64 {
        self.actor_id
    }

    /// Returns the actor's immutable placement preference.
    pub(crate) const fn home_scheduler(self) -> VmSchedulerId {
        self.home_scheduler
    }

    /// Returns the scheduler currently authorized to execute the actor.
    pub(crate) const fn scheduler(self) -> VmSchedulerId {
        self.scheduler
    }

    /// Creates the next route while preserving actor and home identity.
    pub(crate) fn migrated_to(self, destination: VmSchedulerId) -> Result<Self, String> {
        if destination == self.scheduler {
            return Err(format!(
                "error[vm.actor_migration.destination]: actor {} already belongs to scheduler {}",
                self.actor_id,
                destination.index()
            ));
        }
        Ok(Self {
            actor_id: self.actor_id,
            home_scheduler: self.home_scheduler,
            scheduler: destination,
        })
    }
}

/// Immutable scheduler count and fixed actor-placement policy for one shard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSchedulerTopology {
    width: usize,
}

impl VmSchedulerTopology {
    /// Creates a topology from an explicit bounded scheduler count.
    pub(crate) fn new(width: usize) -> Result<Self, String> {
        if !(1..=VM_MAX_SCHEDULERS).contains(&width) {
            return Err(format!(
                "error[vm.scheduler_width]: expected 1..={VM_MAX_SCHEDULERS}, found {width}"
            ));
        }
        Ok(Self { width })
    }

    /// Detects effective host capacity unless an explicit override is present.
    pub(crate) fn from_environment() -> Result<Self, String> {
        if let Some(value) = std::env::var_os(VM_SCHEDULER_WIDTH_ENV) {
            let value = value.to_string_lossy();
            let width = value.parse::<usize>().map_err(|_| {
                format!(
                    "error[vm.scheduler_width]: {VM_SCHEDULER_WIDTH_ENV} must be an integer in 1..={VM_MAX_SCHEDULERS}"
                )
            })?;
            return Self::new(width);
        }
        Self::new(detect_effective_parallelism())
    }

    /// Returns the number of scheduler threads in this shard.
    pub(crate) const fn width(&self) -> usize {
        self.width
    }

    /// Returns scheduler identities in deterministic index order.
    pub(crate) fn schedulers(&self) -> impl ExactSizeIterator<Item = VmSchedulerId> + '_ {
        (0..self.width).map(VmSchedulerId)
    }

    /// Assigns one nonzero shard-global actor identity to its fixed home.
    pub(crate) fn home_scheduler(&self, actor_id: NonZeroU64) -> VmSchedulerId {
        let index = (actor_id.get() - 1) % self.width as u64;
        VmSchedulerId(index as usize)
    }

    /// Creates one fixed route from a shard-global actor identity.
    pub(crate) fn route(&self, actor_id: NonZeroU64) -> VmFixedActorRoute {
        VmFixedActorRoute {
            actor_id,
            home_scheduler: self.home_scheduler(actor_id),
            scheduler: self.home_scheduler(actor_id),
        }
    }
}

/// Returns effective host capacity after affinity and cgroup quota limits.
fn detect_effective_parallelism() -> usize {
    VmSchedulerHostSnapshot::capture().effective_parallelism()
}

/// Applies every visible host limit without allowing a zero-width runtime.
fn effective_parallelism(host: usize, cpuset: Option<usize>, quota: Option<usize>) -> usize {
    [Some(host.max(1)), cpuset, quota]
        .into_iter()
        .flatten()
        .filter(|limit| *limit > 0)
        .min()
        .unwrap_or(1)
}

/// Counts unique logical CPUs in Linux's comma/range list format.
fn cpu_list_count(value: &str) -> Option<usize> {
    let mut cpus = std::collections::BTreeSet::new();
    for part in value.trim().split(',').filter(|part| !part.is_empty()) {
        let (start, end) = match part.split_once('-') {
            Some((start, end)) => (start.parse::<usize>().ok()?, end.parse::<usize>().ok()?),
            None => {
                let cpu = part.parse::<usize>().ok()?;
                (cpu, cpu)
            }
        };
        if start > end {
            return None;
        }
        cpus.extend(start..=end);
    }
    (!cpus.is_empty()).then_some(cpus.len())
}

/// Reads one nonempty, trimmed host metadata file.
fn read_trimmed(path: &str) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Extracts the Linux process affinity list from `/proc/self/status`.
fn process_affinity_list(status: &str) -> Option<String> {
    status.lines().find_map(|line| {
        line.strip_prefix("Cpus_allowed_list:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

/// Counts unique Linux physical package and core identity pairs.
fn physical_core_count(cpuinfo: &str) -> Option<usize> {
    let mut cores = std::collections::BTreeSet::new();
    for block in cpuinfo.split("\n\n") {
        let mut physical_id = None;
        let mut core_id = None;
        for line in block.lines() {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            match name.trim() {
                "physical id" => physical_id = value.trim().parse::<usize>().ok(),
                "core id" => core_id = value.trim().parse::<usize>().ok(),
                _ => {}
            }
        }
        if let (Some(physical_id), Some(core_id)) = (physical_id, core_id) {
            cores.insert((physical_id, core_id));
        }
    }
    (!cores.is_empty()).then_some(cores.len())
}

/// Converts a cgroup v2 CPU quota into a conservative scheduler count.
#[cfg(test)]
fn cgroup_v2_quota(value: &str) -> Option<usize> {
    parse_cgroup_v2_quota(value)?.scheduler_limit
}

/// Parses bounded and unlimited cgroup v2 CPU quota records.
fn parse_cgroup_v2_quota(value: &str) -> Option<CgroupV2CpuQuota> {
    let mut fields = value.split_whitespace();
    let quota = fields.next()?;
    let period = fields.next()?.parse::<u64>().ok()?;
    if fields.next().is_some() || period == 0 {
        return None;
    }
    if quota == "max" {
        return Some(CgroupV2CpuQuota {
            quota_micros: None,
            period_micros: period,
            scheduler_limit: None,
        });
    }
    let quota = quota.parse::<u64>().ok()?;
    let rounded_up = quota.checked_add(period - 1)?.checked_div(period)?;
    Some(CgroupV2CpuQuota {
        quota_micros: Some(quota),
        period_micros: period,
        scheduler_limit: usize::try_from(rounded_up.max(1)).ok(),
    })
}

#[cfg(test)]
#[path = "scheduler_topology_test.rs"]
mod scheduler_topology_test;
