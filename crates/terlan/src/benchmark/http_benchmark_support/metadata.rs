//! Host and build metadata helpers for reproducible HTTP benchmark reports.

use std::env;
use std::fs;
use std::process::Command;

use sha2::{Digest, Sha256};

pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn command_line(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            stdout
                .lines()
                .chain(stderr.lines())
                .find(|line| !line.trim().is_empty())
                .unwrap_or("unknown")
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| "unavailable".to_string())
}

pub(super) fn cpu_governor() -> String {
    fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

pub(super) fn rustflag_value(name: &str) -> Option<String> {
    let flags = env::var("RUSTFLAGS").ok()?;
    flags.split_whitespace().find_map(|flag| {
        flag.strip_prefix(&format!("-C{name}="))
            .or_else(|| flag.strip_prefix(&format!("{name}=")))
            .map(str::to_string)
    })
}
