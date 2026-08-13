//! Versioned bounded protocol for the external capability worker.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{BufRead, Write};

use crate::terlan_native_boundary::capability_sandbox::CapabilitySandboxProfile;
use crate::terlan_native_boundary::metadata::{
    NativeBoundaryExecutionProfile, NativeBoundaryWorkerClass,
};

#[path = "protocol/execution.rs"]
mod execution;

const DEFAULT_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const HARD_MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_MAX_REQUESTS: u64 = 1024;
const HARD_MAX_REQUESTS: u64 = 1_000_000;
const DEFAULT_CREDIT_LIMIT: u64 = 64;

/// Closed startup policy for one capability-worker process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CapabilityWorkerConfig {
    /// Explicit worker-only execution profile.
    execution_profile: NativeBoundaryExecutionProfile,
    /// Attested operating-system sandbox profile.
    sandbox_profile: CapabilitySandboxProfile,
    /// Capabilities granted to requests admitted by this process.
    capabilities: BTreeSet<String>,
    /// Scheduler classes this process may execute.
    worker_classes: BTreeSet<String>,
    /// Maximum bytes in one request or response frame.
    max_payload_bytes: usize,
    /// Maximum requests accepted during the process lifetime.
    max_requests: u64,
    /// Maximum concurrently reserved requests.
    credit_limit: u64,
}

impl CapabilityWorkerConfig {
    /// Parses a closed worker policy from command-line arguments.
    pub(crate) fn parse(args: &[OsString]) -> Result<Self, String> {
        let mut sandbox_profile = None;
        let mut execution_profile = None;
        let mut config = Self {
            execution_profile: NativeBoundaryExecutionProfile::ExternalAdapter,
            sandbox_profile: CapabilitySandboxProfile::LinuxBwrapV1,
            capabilities: BTreeSet::new(),
            worker_classes: BTreeSet::new(),
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            max_requests: DEFAULT_MAX_REQUESTS,
            credit_limit: DEFAULT_CREDIT_LIMIT,
        };
        let mut index = 0;
        while index < args.len() {
            let flag = args[index].to_str().ok_or_else(|| {
                "error[capability_worker.args]: arguments must be UTF-8".to_string()
            })?;
            if !matches!(
                flag,
                "--execution-profile"
                    | "--sandbox-profile"
                    | "--allow"
                    | "--worker-class"
                    | "--max-payload-bytes"
                    | "--max-requests"
                    | "--credit-limit"
            ) {
                return Err(format!(
                    "error[capability_worker.args]: unsupported argument `{flag}`"
                ));
            }
            let value = args.get(index + 1).ok_or_else(|| {
                format!("error[capability_worker.args]: `{flag}` requires a value")
            })?;
            let value = value.to_str().ok_or_else(|| {
                format!("error[capability_worker.args]: `{flag}` value must be UTF-8")
            })?;
            match flag {
                "--execution-profile" => {
                    let profile = match value {
                        "external-adapter" => NativeBoundaryExecutionProfile::ExternalAdapter,
                        "crash-isolated" => NativeBoundaryExecutionProfile::CrashIsolated,
                        "cross-boundary" => NativeBoundaryExecutionProfile::CrossBoundary,
                        _ => {
                            return Err(format!(
                                "error[capability_worker.profile]: unsupported execution profile `{value}`"
                            ))
                        }
                    };
                    if execution_profile.replace(profile).is_some() {
                        return Err(
                            "error[capability_worker.profile]: execution profile may be declared only once"
                                .to_string(),
                        );
                    }
                }
                "--sandbox-profile" => {
                    let profile = CapabilitySandboxProfile::current()?;
                    if value != profile.name() {
                        return Err(format!(
                            "error[capability_worker.sandbox]: unsupported sandbox profile `{value}`"
                        ));
                    }
                    if sandbox_profile.replace(profile).is_some() {
                        return Err(
                            "error[capability_worker.sandbox]: sandbox profile may be declared only once"
                                .to_string(),
                        );
                    }
                }
                "--allow" => {
                    validate_name("capability", value)?;
                    config.capabilities.insert(value.to_owned());
                }
                "--worker-class" => {
                    worker_class(value)?;
                    config.worker_classes.insert(value.to_owned());
                }
                "--max-payload-bytes" => {
                    config.max_payload_bytes =
                        bounded_usize(flag, value, 1, HARD_MAX_PAYLOAD_BYTES)?;
                }
                "--max-requests" => {
                    config.max_requests = bounded_u64(flag, value, 1, HARD_MAX_REQUESTS)?;
                }
                "--credit-limit" => {
                    config.credit_limit = bounded_u64(flag, value, 1, HARD_MAX_REQUESTS)?;
                }
                _ => {
                    return Err(format!(
                        "error[capability_worker.args]: unsupported argument `{flag}`"
                    ));
                }
            }
            index += 2;
        }
        config.sandbox_profile = sandbox_profile.ok_or_else(|| {
            "error[capability_worker.sandbox]: a sandbox profile is required".to_string()
        })?;
        config.execution_profile = execution_profile.ok_or_else(|| {
            "error[capability_worker.profile]: an external-adapter, crash-isolated, or cross-boundary execution profile is required"
                .to_string()
        })?;
        Ok(config)
    }

    /// Returns the operating-system profile that must be attested at startup.
    pub(crate) fn sandbox_profile(&self) -> CapabilitySandboxProfile {
        self.sandbox_profile
    }

    /// Resolves the configured worker classes into manifest policy values.
    fn admitted_worker_classes(&self) -> Result<Vec<NativeBoundaryWorkerClass>, String> {
        self.worker_classes
            .iter()
            .map(|name| worker_class(name))
            .collect()
    }
}

/// Runs bounded capability RPC until an authenticated shutdown frame or EOF.
pub(crate) fn run_capability_worker(
    config: CapabilityWorkerConfig,
    input: impl BufRead + Send,
    output: impl Write,
) -> Result<(), String> {
    execution::run(config, input, output)
}

/// Rejects identities that cannot belong to a live VM request.
fn validate_request_identity(request_id: u64, owner_id: u64) -> Result<(), String> {
    if request_id == 0 || owner_id == 0 {
        return Err(
            "error[capability_worker.identity]: request and owner identities must be nonzero"
                .to_string(),
        );
    }
    Ok(())
}

/// Advances the process lifetime request counter without overflow.
fn next_request_count(current: u64, maximum: u64) -> Result<u64, String> {
    let next = current.checked_add(1).ok_or_else(|| {
        "error[capability_worker.request_limit]: request counter overflow".to_string()
    })?;
    if next > maximum {
        return Err(format!(
            "error[capability_worker.request_limit]: worker accepts at most {maximum} requests"
        ));
    }
    Ok(next)
}

/// Parses one admitted scheduler class name.
fn worker_class(value: &str) -> Result<NativeBoundaryWorkerClass, String> {
    match value {
        "fast" => Ok(NativeBoundaryWorkerClass::Fast),
        "blocking" => Ok(NativeBoundaryWorkerClass::Blocking),
        "long-running-cancellable" => Ok(NativeBoundaryWorkerClass::LongRunningCancellable),
        "sandboxed" => Ok(NativeBoundaryWorkerClass::Sandboxed),
        "resource-owning" => Ok(NativeBoundaryWorkerClass::ResourceOwning),
        _ => Err(format!(
            "error[capability_worker.worker_class]: unsupported worker class `{value}`"
        )),
    }
}

/// Rejects empty or structurally ambiguous capability names.
fn validate_name(label: &str, value: &str) -> Result<(), String> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Ok(());
    }
    Err(format!(
        "error[capability_worker.args]: invalid {label} `{value}`"
    ))
}

/// Parses one bounded positive usize option.
fn bounded_usize(flag: &str, value: &str, minimum: usize, maximum: usize) -> Result<usize, String> {
    let value = value
        .parse::<usize>()
        .map_err(|_| format!("error[capability_worker.args]: `{flag}` requires an integer"))?;
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "error[capability_worker.args]: `{flag}` must be between {minimum} and {maximum}"
        ))
    }
}

/// Parses one bounded positive u64 option.
fn bounded_u64(flag: &str, value: &str, minimum: u64, maximum: u64) -> Result<u64, String> {
    let value = value
        .parse::<u64>()
        .map_err(|_| format!("error[capability_worker.args]: `{flag}` requires an integer"))?;
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "error[capability_worker.args]: `{flag}` must be between {minimum} and {maximum}"
        ))
    }
}

#[cfg(test)]
#[path = "protocol_test.rs"]
#[cfg(test)]
mod protocol_test;

#[cfg(all(test, target_os = "linux"))]
#[cfg(test)]
#[path = "efile_beam_suite_parity_test.rs"]
#[cfg(test)]
mod efile_beam_suite_parity_test;
