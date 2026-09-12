//! Compiler-probe environment after Rustup and Cargo's declared overrides.

use crate::cargo_tool_settings::CargoToolSettings;
use crate::execution_environment::ExecutionEnvironment;
use crate::rustup_selection::RustupSelection;
use crate::PhaseFailure;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Builds Cargo's version-query environment without changing process-global state.
pub(super) fn capture(
    settings: &CargoToolSettings,
    environment: &ExecutionEnvironment,
    proxy: Option<(&Path, &RustupSelection)>,
    native_cargo: &Path,
) -> Result<Command, PhaseFailure> {
    let mut command = environment.test_command(Path::new("compiler-environment"));
    if let Some((root, selection)) = proxy {
        let cargo_home = home::env::cargo_home_with_env(environment).map_err(failure)?;
        let rustup_home = home::env::rustup_home_with_env(environment).map_err(failure)?;
        let recursion = environment
            .value("RUST_RECURSION_COUNT")
            .and_then(|value| value.into_string().ok())
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let recursion = recursion
            .checked_add(1)
            .ok_or_else(|| failure("Rustup recursion count overflow"))?;
        command
            .env("PATH", crate::cargo_proxy_path::capture(environment, root)?)
            .env("CARGO_HOME", cargo_home)
            .env("RUSTUP_HOME", rustup_home)
            .env("RUSTUP_TOOLCHAIN", &selection.name)
            .env("RUSTUP_TOOLCHAIN_SOURCE", selection.source)
            .env("RUST_RECURSION_COUNT", recursion.to_string());
        loader_path(&mut command, environment, root)?;
    }
    let mut command = settings.subprocess_from_command(command)?;
    // Rustc::new sets CARGO after apply_env_config, so even a forced [env] value
    // cannot replace the actual Cargo executable in its compiler version query.
    command.env("CARGO", native_cargo);
    Ok(command)
}

fn loader_path(
    command: &mut Command,
    environment: &ExecutionEnvironment,
    root: &Path,
) -> Result<(), PhaseFailure> {
    let variable = if cfg!(target_os = "macos") {
        "DYLD_FALLBACK_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    let inherited = environment.value(variable);
    let mut prepend = vec![root.join("lib")];
    if cfg!(target_os = "macos") && inherited.as_ref().is_none_or(|value| value.is_empty()) {
        if let Some(home) = environment.value("HOME") {
            prepend.push(PathBuf::from(home).join("lib"));
        }
        prepend.extend([PathBuf::from("/usr/local/lib"), PathBuf::from("/usr/lib")]);
    }
    let mut parts = inherited
        .as_ref()
        .map(|value| std::env::split_paths(value).collect::<VecDeque<_>>())
        .unwrap_or_default();
    for path in prepend.into_iter().rev() {
        if !parts.contains(&path) {
            parts.push_front(path);
        }
    }
    command.env(variable, std::env::join_paths(parts).map_err(failure)?);
    Ok(())
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "cargo-compiler-environment-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "cargo_compiler_environment_test.rs"]
mod tests;
