use serde::Serialize;
use std::path::Path;
use std::process::Command;

use super::resources::{self, ResourceEvidence};
use terlan_type_fuzz::Oracle;

#[derive(Debug, Serialize)]
pub(super) struct OracleResult {
    pub(super) schema: &'static str,
    pub(super) oracle: Oracle,
    pub(super) outcome: &'static str,
    pub(super) exit_code: Option<i32>,
    pub(super) stdout: String,
    pub(super) stderr: String,
    #[serde(skip)]
    pub(super) resources: ResourceEvidence,
}

pub(super) fn run(
    oracle: Oracle,
    source: &Path,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> Result<OracleResult, String> {
    if oracle == Oracle::None {
        return Ok(OracleResult {
            schema: "terlan.type-fuzz-observation.v1",
            oracle,
            outcome: "not_run",
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            resources: ResourceEvidence::unmeasured(timeout_ms, rss_limit_bytes, cpu_limit_ms),
        });
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate current compiler: {error}"))?;
    let root = source
        .parent()
        .ok_or_else(|| "generated source has no parent directory".to_string())?;
    let source_name = source
        .file_name()
        .ok_or_else(|| "generated source has no file name".to_string())?;
    let mut command = Command::new(executable);
    command
        .arg("--diagnostic-format")
        .arg("json")
        .arg("--color")
        .arg("never")
        .arg("check")
        .arg(source_name)
        .current_dir(root);
    let resources::Capture { output, resources } =
        resources::capture(&mut command, timeout_ms, rss_limit_bytes, cpu_limit_ms);
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            let outcome = if resources.limit_exceeded.is_some() {
                "resource_exhausted"
            } else {
                "infrastructure_error"
            };
            return Ok(OracleResult {
                schema: "terlan.type-fuzz-observation.v1",
                oracle,
                outcome,
                exit_code: None,
                stdout: String::new(),
                stderr: error,
                resources,
            });
        }
    };
    let stdout = normalize(&String::from_utf8_lossy(&output.stdout), root);
    let stderr = normalize(&String::from_utf8_lossy(&output.stderr), root);
    Ok(OracleResult {
        schema: "terlan.type-fuzz-observation.v1",
        oracle,
        outcome: if output.status.success() {
            "passed"
        } else {
            "failed"
        },
        exit_code: output.status.code(),
        stdout,
        stderr,
        resources,
    })
}

fn normalize(text: &str, root: &Path) -> String {
    text.replace(root.to_string_lossy().as_ref(), "$CASE")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_oracle_has_stable_observation() {
        let result = run(Oracle::None, Path::new("case.terl"), 1, 1024, 1).expect("observation");
        assert_eq!(result.outcome, "not_run");
        assert_eq!(result.exit_code, None);
    }

    #[test]
    fn diagnostics_replace_case_directory() {
        assert_eq!(
            normalize("/tmp/case/case.terl", Path::new("/tmp/case")),
            "$CASE/case.terl"
        );
    }
}
