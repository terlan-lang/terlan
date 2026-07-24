//! In-worker attestation for the mandatory capability sandbox profile.

#[cfg(target_os = "linux")]
use std::collections::BTreeSet;
#[cfg(target_os = "linux")]
use std::path::Path;

use crate::terlan_native_boundary::capability_sandbox::{
    CapabilitySandboxHost, CapabilitySandboxLimits, CapabilitySandboxProfile, SANDBOX_LOCALE,
    SANDBOX_TEMP_DIR, SANDBOX_WORK_DIR,
};

/// Verifies the process boundary before any capability request is accepted.
pub(crate) fn verify_capability_worker_sandbox(
    profile: CapabilitySandboxProfile,
) -> Result<(), String> {
    if profile.host() != CapabilitySandboxHost::current() {
        return Err(format!(
            "error[capability_worker.sandbox]: profile `{}` does not match host `{}`",
            profile.name(),
            CapabilitySandboxHost::current().name()
        ));
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(
            "error[capability_worker.sandbox]: Linux sandbox profile used on another platform"
                .to_string(),
        )
    }
    #[cfg(target_os = "linux")]
    {
        verify_working_directory()?;
        verify_environment()?;
        verify_resource_limits(CapabilitySandboxLimits::linux_default())?;
        verify_file_descriptors()
    }
}

/// Requires the fixed private working directory mounted by bubblewrap.
#[cfg(target_os = "linux")]
fn verify_working_directory() -> Result<(), String> {
    let current = std::env::current_dir().map_err(|error| {
        format!("error[capability_worker.sandbox]: cannot inspect working directory: {error}")
    })?;
    if current == Path::new(SANDBOX_WORK_DIR) {
        Ok(())
    } else {
        Err(format!(
            "error[capability_worker.sandbox]: expected working directory `{SANDBOX_WORK_DIR}`, found `{}`",
            current.display()
        ))
    }
}

/// Requires an exact deterministic environment with no ambient application data.
#[cfg(target_os = "linux")]
fn verify_environment() -> Result<(), String> {
    let observed = std::env::vars_os()
        .map(|(key, value)| {
            let key = key.into_string().map_err(|_| {
                "error[capability_worker.sandbox]: environment key is not UTF-8".to_string()
            })?;
            let value = value.into_string().map_err(|_| {
                "error[capability_worker.sandbox]: environment value is not UTF-8".to_string()
            })?;
            Ok((key, value))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let expected = BTreeSet::from([
        ("HOME".to_string(), SANDBOX_WORK_DIR.to_string()),
        ("LANG".to_string(), SANDBOX_LOCALE.to_string()),
        ("PWD".to_string(), SANDBOX_WORK_DIR.to_string()),
        ("TMPDIR".to_string(), SANDBOX_TEMP_DIR.to_string()),
    ]);
    let observed = observed.into_iter().collect::<BTreeSet<_>>();
    if observed == expected {
        Ok(())
    } else {
        Err(
            "error[capability_worker.sandbox]: environment is not the closed worker allowlist"
                .to_string(),
        )
    }
}

/// Requires every hard kernel limit installed by the launcher profile.
#[cfg(target_os = "linux")]
fn verify_resource_limits(limits: CapabilitySandboxLimits) -> Result<(), String> {
    let contents = std::fs::read_to_string("/proc/self/limits").map_err(|error| {
        format!("error[capability_worker.sandbox]: cannot inspect process limits: {error}")
    })?;
    require_limit(&contents, "Max cpu time", limits.cpu_seconds)?;
    require_limit(&contents, "Max file size", limits.file_bytes)?;
    require_limit(&contents, "Max processes", limits.processes)?;
    require_limit(&contents, "Max open files", limits.open_files)?;
    require_limit(&contents, "Max address space", limits.address_space_bytes)
}

/// Requires one named soft and hard limit to equal the profile value.
#[cfg(target_os = "linux")]
fn require_limit(contents: &str, name: &str, expected: u64) -> Result<(), String> {
    let line = contents
        .lines()
        .find(|line| line.starts_with(name))
        .ok_or_else(|| format!("error[capability_worker.sandbox]: missing `{name}` limit"))?;
    let values = line[name.len()..]
        .split_whitespace()
        .take(2)
        .map(|value| value.parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("error[capability_worker.sandbox]: invalid `{name}` limit"))?;
    if values.as_slice() == [expected, expected] {
        Ok(())
    } else {
        Err(format!(
            "error[capability_worker.sandbox]: `{name}` limit does not match {expected}"
        ))
    }
}

/// Requires only stdio plus the transient `/proc/self/fd` iterator descriptor.
#[cfg(target_os = "linux")]
fn verify_file_descriptors() -> Result<(), String> {
    let entries = std::fs::read_dir("/proc/self/fd").map_err(|error| {
        format!("error[capability_worker.sandbox]: cannot inspect descriptors: {error}")
    })?;
    let mut descriptors = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("error[capability_worker.sandbox]: cannot inspect descriptor: {error}")
        })?;
        let descriptor = entry
            .file_name()
            .to_string_lossy()
            .parse::<u32>()
            .map_err(|_| {
                "error[capability_worker.sandbox]: invalid descriptor identity".to_string()
            })?;
        descriptors.insert(descriptor);
    }
    validate_file_descriptors(&descriptors)
}

/// Requires stdio plus at most the transient `/proc/self/fd` iterator descriptor.
#[cfg(target_os = "linux")]
fn validate_file_descriptors(descriptors: &BTreeSet<u32>) -> Result<(), String> {
    if descriptors.contains(&0)
        && descriptors.contains(&1)
        && descriptors.contains(&2)
        && descriptors.iter().all(|descriptor| *descriptor <= 3)
    {
        Ok(())
    } else {
        Err("error[capability_worker.sandbox]: inherited file descriptor detected".to_string())
    }
}

#[cfg(test)]
#[path = "sandbox_test.rs"]
mod sandbox_test;
