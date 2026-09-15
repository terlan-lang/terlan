//! Point-in-time cache and filesystem admission before the compiler bootstrap.
use crate::{incremental, layout};
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::Path;
use std::time::SystemTime;

/// Parses the explicit floor before any filesystem mutation or cache retirement.
fn minimum(args: &[OsString]) -> io::Result<u64> {
    let [flag, value] = args else {
        return Err(io::Error::other(
            "usage: admit --minimum-free-bytes <positive integer>",
        ));
    };
    let value = value.to_str().unwrap_or("");
    if flag != "--minimum-free-bytes"
        || value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(io::Error::other("invalid build disk-space floor"));
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| io::Error::other("invalid build disk-space floor"))
}

/// Computes user-available bytes, excluding blocks reserved for the superuser.
fn available(blocks: u64, fragment_size: u64) -> io::Result<u64> {
    if fragment_size == 0 {
        return Err(io::Error::other("filesystem reports zero fragment size"));
    }
    blocks
        .checked_mul(fragment_size)
        .ok_or_else(|| io::Error::other("filesystem available-byte count overflow"))
}

/// Cleans only the known cache namespace, then measures the actual build filesystem.
pub(crate) fn run(root: &Path, args: &[OsString]) -> io::Result<bool> {
    let minimum = minimum(args)?;
    if !layout::regular(&root.join("Cargo.toml"))?.is_file() || !root.join(".git").try_exists()? {
        return Err(layout::invalid("expected a Git/Cargo repository root"));
    }
    layout::directory(&root.join("target"))?;
    let profile = root.join("target/debug");
    layout::directory(&profile)?;
    let directory = File::open(&profile)?;
    let cache = if layout::present(&profile.join("incremental"))? {
        Some(incremental::maintain(
            root,
            true,
            incremental::Policy::default(),
            SystemTime::now(),
        )?)
    } else {
        // A cold non-incremental bootstrap has nothing to prune.
        None
    };
    let stats = rustix::fs::fstatvfs(&directory)?;
    let free = available(stats.f_bavail, stats.f_frsize)?;
    let cache_passed = cache.as_ref().is_none_or(|report| report.budget_verified);
    let passed = cache_passed && free >= minimum;
    println!(
        "{}",
        serde_json::json!({
            "schema": "terlan.build-resource-admission.v1",
            "decision": if passed { "pass" } else { "fail" },
            "available_bytes": free, "minimum_free_bytes": minimum,
            "reservation": false, "cache": cache,
        })
    );
    if !passed {
        eprintln!("error[build.resources]: compiler bootstrap stopped before launch: available={free} required={minimum} cache_budget_passed={cache_passed}");
    }
    Ok(passed)
}

#[cfg(test)]
#[path = "admission_test.rs"]
mod tests;
