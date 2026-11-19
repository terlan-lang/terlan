//! Classifies Make transport bookkeeping separately from test-affecting inputs.

use crate::coverage_requests::failure;
use crate::execution_environment::{identity, ExecutionEnvironment};
use crate::PhaseFailure;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// The live coverage endpoint is routing, never a reusable test-success receipt.
pub(super) const CONTEXT: &str = "TERLAN_RUST_COVERAGE_CONTEXT";

/// Selects fresh metadata production for hosted-source coverage, never test success.
pub(super) const SCOPE: &str = "TERLAN_RUST_COVERAGE_SCOPE";

/// Executable routing is byte-bound by the orchestrator and metadata handoff.
const EXECUTABLE_ROUTES: [&str; 6] = [
    "TERLC",
    "TERLAN_RUST_ORCHESTRATOR",
    "TERLAN_CARGO_METADATA_OWNER",
    "TERLAN_RUST_SUITE_TERLC",
    "TERLAN_RUST_SUITE_VM",
    "TERLAN_RUST_SUITE_NATIVE_WORKER",
];

/// Digest-only effective environment evidence retained by live coverage requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TestIdentity {
    pub(super) digest: String,
    components: BTreeMap<String, String>,
}

impl TestIdentity {
    pub(super) fn json(&self) -> Value {
        json!({"scope":"test-environment-components-v1","identity_sha256":self.digest,
            "components":self.components})
    }

    pub(super) fn read(value: &Value) -> Result<Self, PhaseFailure> {
        if value["scope"] != "test-environment-components-v1" {
            return Err(failure("missing test environment components"));
        }
        let digest = value["identity_sha256"]
            .as_str()
            .filter(|value| valid_hash(value))
            .ok_or_else(|| failure("invalid test environment identity"))?
            .to_owned();
        let rows = value["components"]
            .as_object()
            .ok_or_else(|| failure("invalid test environment components"))?;
        if rows.len() > 4_096 {
            return Err(failure("excessive test environment components"));
        }
        let mut components = BTreeMap::new();
        for (key, value) in rows {
            let value = value
                .as_str()
                .filter(|value| valid_hash(value))
                .ok_or_else(|| failure("invalid test environment component"))?;
            if !valid_hash(key) || components.insert(key.clone(), value.to_owned()).is_some() {
                return Err(failure("invalid test environment component"));
            }
        }
        Ok(Self { digest, components })
    }

    pub(super) fn difference(&self, actual: &Self) -> String {
        let expected = self.components.keys().cloned().collect::<BTreeSet<_>>();
        let observed = actual.components.keys().cloned().collect::<BTreeSet<_>>();
        let missing = expected.difference(&observed).cloned().collect::<Vec<_>>();
        let extra = observed.difference(&expected).cloned().collect::<Vec<_>>();
        let changed = expected
            .intersection(&observed)
            .filter(|key| self.components.get(*key) != actual.components.get(*key))
            .cloned()
            .collect::<Vec<_>>();
        format!(
            "missing={}; extra={}; changed={}",
            missing.join(","),
            extra.join(","),
            changed.join(",")
        )
    }
}

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

/// Projects a request environment and retains only key/value digests for diagnostics.
pub(super) fn test_observation(command: &Command) -> Result<TestIdentity, PhaseFailure> {
    let directory = command
        .get_current_dir()
        .ok_or_else(|| failure("coverage request has no directory"))?;
    let mut entries = Vec::new();
    for (key, value) in command.get_envs() {
        let Some(value) = value else {
            continue;
        };
        match key.to_str() {
            Some(
                "MAKEFLAGS" | "MAKEOVERRIDES" | "MAKE_TERMERR" | "MAKE_TERMOUT" | "MFLAGS" | "_",
            ) => continue,
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
            Some(key) if EXECUTABLE_ROUTES.contains(&key) => continue,
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
            Some("TERLAN_PROOF_RUNTIME_RUN" | "TERLAN_PROOF_TRACK_RUN") => continue,
            _ => entries.push((key.to_owned(), value.to_owned())),
        }
    }
    let environment = ExecutionEnvironment::from_entries(entries, directory)?;
    let command = environment.command(Path::new("coverage-test-environment"));
    let components = command
        .get_envs()
        .filter_map(|(key, value)| {
            Some((
                hash(key.as_encoded_bytes()),
                hash(value?.as_encoded_bytes()),
            ))
        })
        .collect();
    Ok(TestIdentity {
        digest: identity(&command),
        components,
    })
}

fn hash(bytes: &[u8]) -> String {
    crate::file_identity::hex(Sha256::new_with_prefix(bytes))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "make_environment_test.rs"]
mod tests;
