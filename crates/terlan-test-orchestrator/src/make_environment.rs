//! Classifies Make transport bookkeeping separately from test-affecting inputs.

use crate::coverage_requests::failure;
use crate::execution_environment::{identity, ExecutionEnvironment};
use crate::PhaseFailure;
use std::path::Path;
use std::process::Command;

/// The live coverage endpoint is routing, never a reusable test-success receipt.
pub(super) const CONTEXT: &str = "TERLAN_RUST_COVERAGE_CONTEXT";

/// Selects fresh metadata production for hosted-source coverage, never test success.
pub(super) const SCOPE: &str = "TERLAN_RUST_COVERAGE_SCOPE";

/// Binds the selected Make program and exact argument boundaries without leaking values.
pub(super) fn invocation_identity(command: &Command) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    crate::file_identity::field(&mut digest, b"terlan.make-invocation.v1");
    crate::file_identity::field(&mut digest, command.get_program().as_encoded_bytes());
    for argument in command.get_args() {
        crate::file_identity::field(&mut digest, argument.as_encoded_bytes());
    }
    crate::file_identity::hex(digest)
}

/// GNU Make executes recursive recipes even under -n; preserve planning without certifying it.
pub(super) fn dry_run(environment: &ExecutionEnvironment) -> bool {
    let flags = environment.value("MAKEFLAGS").unwrap_or_default();
    let Some(flags) = flags.to_str() else {
        return false;
    };
    flags
        .split_whitespace()
        .any(|flag| matches!(flag, "--dry-run" | "--just-print" | "--recon"))
        || flags.split_whitespace().next().is_some_and(|flags| {
            !flags.starts_with("--") && !flags.contains('=') && flags.contains('n')
        })
}

/// Projects only reviewed Make/shell transport keys; all test inputs remain byte-bound.
pub(super) fn test_identity(command: &Command) -> Result<String, PhaseFailure> {
    let directory = command
        .get_current_dir()
        .ok_or_else(|| failure("coverage request has no directory"))?;
    let mut entries = Vec::new();
    for (key, value) in command.get_envs() {
        let Some(value) = value else {
            continue;
        };
        match key.to_str() {
            Some("MAKEFLAGS" | "MAKEOVERRIDES" | "MFLAGS" | "_") => continue,
            Some("MAKELEVEL" | "SHLVL") => {
                if value
                    .to_str()
                    .and_then(|value| value.parse::<u32>().ok())
                    .is_none_or(|level| level > 64)
                {
                    return Err(failure("invalid Make/shell nesting level"));
                }
                continue;
            }
            Some(CONTEXT) => continue,
            Some("TERLAN_SEMANTIC_KERNEL_ROOT" | "TERLAN_LEAN_PROOF_ROOT") => {
                if Path::new(value) != directory {
                    return Err(failure(
                        "proof-script routing root differs from the coverage checkout",
                    ));
                }
                continue;
            }
            Some(SCOPE) => {
                if !matches!(value.to_str(), Some("current-inputs" | "hosted-source")) {
                    return Err(failure("invalid live coverage scope"));
                }
                continue;
            }
            Some("TERLAN_VALIDATION_BOOTSTRAPPED" | "TERLAN_BUILD_ARTIFACTS_PREBUILT") => {
                if !matches!(value.to_str(), Some("0" | "1")) {
                    return Err(failure("invalid validation routing value"));
                }
                continue;
            }
            _ => entries.push((key.to_owned(), value.to_owned())),
        }
    }
    let environment = ExecutionEnvironment::from_entries(entries, directory)?;
    Ok(identity(
        &environment.command(Path::new("coverage-test-environment")),
    ))
}

#[cfg(test)]
#[path = "make_environment_test.rs"]
mod tests;
