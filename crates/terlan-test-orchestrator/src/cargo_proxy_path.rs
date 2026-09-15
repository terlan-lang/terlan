//! Rustup proxy PATH policy used before Cargo's own subprocess env settings.

use crate::execution_environment::ExecutionEnvironment;
use crate::PhaseFailure;
use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::path::Path;

/// Mirrors Rustup's unique insertion, including Windows' toolchain-bin policy.
pub(super) fn capture(
    environment: &ExecutionEnvironment,
    toolchain: &Path,
) -> Result<OsString, PhaseFailure> {
    let cargo_home = home::env::cargo_home_with_env(environment).map_err(failure)?;
    let windows_policy = environment.value("RUSTUP_WINDOWS_PATH_ADD_BIN");
    merge(
        environment.test_path(),
        &cargo_home,
        toolchain,
        cfg!(windows),
        windows_policy.as_deref(),
    )
}

fn merge(
    path: &OsStr,
    cargo_home: &Path,
    toolchain: &Path,
    windows: bool,
    windows_policy: Option<&OsStr>,
) -> Result<OsString, PhaseFailure> {
    let mut parts = std::env::split_paths(path).collect::<VecDeque<_>>();
    let mut prepend = vec![cargo_home.join("bin")];
    let append = if windows {
        match windows_policy.and_then(OsStr::to_str) {
            Some("0") => None,
            Some("1") => {
                prepend.push(toolchain.join("bin"));
                None
            }
            _ => Some(toolchain.join("bin")),
        }
    } else {
        None
    };
    for entry in prepend.into_iter().rev() {
        // Existing entries retain their original position, even when not first.
        if !parts.contains(&entry) {
            parts.push_front(entry);
        }
    }
    if let Some(entry) = append {
        if !parts.contains(&entry) {
            parts.push_back(entry);
        }
    }
    std::env::join_paths(parts).map_err(failure)
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "cargo-proxy-path-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "cargo_proxy_path_test.rs"]
mod tests;
