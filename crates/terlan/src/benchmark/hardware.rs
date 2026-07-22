//! Shared host and toolchain identity for comparable benchmark reports.

use std::env;
use std::fs;
use std::process::Command;
use std::thread;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Stable machine identity attached to same-host benchmark reports.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct HardwareFingerprint {
    /// Versioned schema for the fingerprint fields.
    pub(crate) schema: String,
    /// Host operating-system family reported by Rust.
    pub(crate) operating_system: String,
    /// Host architecture reported by Rust.
    pub(crate) architecture: String,
    /// Processor model from the host operating system when available.
    pub(crate) cpu_model: String,
    /// Logical processors available to the benchmark process.
    pub(crate) logical_cpu_count: usize,
    /// Active Rust compiler version.
    pub(crate) rustc_version: String,
    /// SHA-256 of the canonical comparable host fields.
    pub(crate) sha256: String,
}

impl HardwareFingerprint {
    /// Captures and hashes the host fields that affect benchmark comparability.
    pub(crate) fn current() -> Self {
        let operating_system = env::consts::OS.to_string();
        let architecture = env::consts::ARCH.to_string();
        let cpu_model = cpu_model();
        let logical_cpu_count = thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        let rustc_version = command_version("rustc", &["--version"]);
        let canonical = format!(
            "{operating_system}|{architecture}|{cpu_model}|{logical_cpu_count}|{rustc_version}"
        );
        Self {
            schema: "terlan-benchmark-hardware-v1".to_string(),
            operating_system,
            architecture,
            cpu_model,
            logical_cpu_count,
            rustc_version,
            sha256: sha256(canonical.as_bytes()),
        }
    }
}

/// Returns one command's first output line or `unknown` when unavailable.
pub(crate) fn command_version(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.lines().next().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Computes one SHA-256 digest in lowercase hexadecimal form.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Returns the procfs or macOS CPU model with an architecture fallback.
fn cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|rest| rest.split_once(':'))
                    .map(|(_, value)| value.trim().to_string())
            })
        })
        .or_else(|| {
            Command::new("sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| env::consts::ARCH.to_string())
}
