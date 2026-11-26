use serde::Serialize;
use std::path::Path;
use std::process::Command;

use super::resources::{self, ResourceEvidence};
use crate::Oracle;

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
    compiler: &Path,
    oracle: Oracle,
    source: &Path,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> OracleResult {
    let root = source
        .parent()
        .expect("generated case source has a parent directory");
    let source_name = source
        .file_name()
        .expect("generated case source has a file name");
    run_batch(
        compiler,
        oracle,
        root,
        &[source_name.into()],
        timeout_ms,
        rss_limit_bytes,
        cpu_limit_ms,
    )
}

pub(super) fn run_batch(
    compiler: &Path,
    oracle: Oracle,
    root: &Path,
    sources: &[std::path::PathBuf],
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> OracleResult {
    if oracle == Oracle::None {
        return OracleResult {
            schema: "terlan.type-fuzz-observation.v1",
            oracle,
            outcome: "not_run",
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            resources: ResourceEvidence::unmeasured(timeout_ms, rss_limit_bytes, cpu_limit_ms),
        };
    }
    let mut command = Command::new(compiler);
    command
        .arg("--diagnostic-format")
        .arg("json")
        .arg("--color")
        .arg("never")
        .arg("check")
        .args(sources)
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
            return OracleResult {
                schema: "terlan.type-fuzz-observation.v1",
                oracle,
                outcome,
                exit_code: None,
                stdout: String::new(),
                stderr: error,
                resources,
            };
        }
    };
    let stdout = normalize(&String::from_utf8_lossy(&output.stdout), root);
    let stderr = normalize(&String::from_utf8_lossy(&output.stderr), root);
    OracleResult {
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
    }
}

fn normalize(text: &str, root: &Path) -> String {
    text.replace(root.to_string_lossy().as_ref(), "$CASE")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_oracle_has_stable_observation() {
        let result = run(
            Path::new("missing-compiler"),
            Oracle::None,
            Path::new("case.terl"),
            1,
            1024,
            1,
        );
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

    #[test]
    fn missing_compiler_is_structured_infrastructure_evidence() {
        let root = tempfile::tempdir().expect("case root");
        let source = root.path().join("case.terl");
        std::fs::write(&source, "module case.").expect("source");
        let result = run(
            Path::new("missing-type-fuzz-compiler"),
            Oracle::Check,
            &source,
            100,
            1024,
            100,
        );
        assert_eq!(result.outcome, "infrastructure_error");
        assert_eq!(result.exit_code, None);
    }

    #[cfg(unix)]
    #[test]
    fn successful_failed_and_resource_exhausted_compilers_are_distinct() {
        let root = tempfile::tempdir().expect("case root");
        let source = root.path().join("case.terl");
        std::fs::write(&source, "module case.").expect("source");
        let success = run(
            Path::new("/bin/true"),
            Oracle::Check,
            &source,
            1_000,
            1024 * 1024 * 1024,
            1_000,
        );
        assert_eq!(success.outcome, "passed");
        assert_eq!(success.exit_code, Some(0));

        let failure = run(
            Path::new("/bin/false"),
            Oracle::Check,
            &source,
            1_000,
            1024 * 1024 * 1024,
            1_000,
        );
        assert_eq!(failure.outcome, "failed");
        assert_ne!(failure.exit_code, Some(0));

        use std::os::unix::fs::PermissionsExt;
        let compiler = root.path().join("busy-compiler");
        std::fs::write(&compiler, "#!/bin/sh\nwhile :; do :; done\n").expect("compiler");
        std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o700))
            .expect("permissions");
        let exhausted = run(
            &compiler,
            Oracle::Check,
            &source,
            1_000,
            1024 * 1024 * 1024,
            10,
        );
        assert_eq!(exhausted.outcome, "resource_exhausted");
        assert_eq!(exhausted.exit_code, None);
    }
}
