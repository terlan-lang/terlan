use serde::Serialize;
use std::path::{Path, PathBuf};
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

pub(super) struct FormattedBatchInputs<'a> {
    pub(super) root: &'a Path,
    pub(super) semantic_sources: &'a [PathBuf],
    pub(super) formatting_sources: &'a [PathBuf],
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

pub(super) fn run_formatted_batch_inputs(
    compiler: &Path,
    oracle: Oracle,
    inputs: FormattedBatchInputs<'_>,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> OracleResult {
    if oracle == Oracle::None {
        return run_batch(
            compiler,
            oracle,
            inputs.root,
            inputs.semantic_sources,
            timeout_ms,
            rss_limit_bytes,
            cpu_limit_ms,
        );
    }
    let mut command = base_command(compiler);
    command
        .arg("fmt")
        .arg("--check")
        .args(inputs.formatting_sources)
        .current_dir(inputs.root);
    let formatting = capture_result(
        command,
        oracle,
        inputs.root,
        timeout_ms,
        rss_limit_bytes,
        cpu_limit_ms,
    );
    if formatting.outcome != "passed" {
        return formatting;
    }
    finish_phase(
        formatting,
        timeout_ms,
        rss_limit_bytes,
        cpu_limit_ms,
        |remaining_wall, remaining_cpu| {
            run_batch(
                compiler,
                oracle,
                inputs.root,
                inputs.semantic_sources,
                remaining_wall,
                rss_limit_bytes,
                remaining_cpu,
            )
        },
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
    let operation = match oracle {
        Oracle::Check => Some("check"),
        Oracle::Native => Some("test"),
        Oracle::None => None,
        Oracle::JavaScript => {
            return super::javascript::run(
                compiler,
                root,
                sources,
                timeout_ms,
                rss_limit_bytes,
                cpu_limit_ms,
            );
        }
    };
    let Some(operation) = operation else {
        return OracleResult {
            schema: "terlan.type-fuzz-observation.v1",
            oracle,
            outcome: "not_run",
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            resources: ResourceEvidence::unmeasured(timeout_ms, rss_limit_bytes, cpu_limit_ms),
        };
    };
    let mut command = base_command(compiler);
    if oracle == Oracle::Native {
        command.arg("--timings");
    }
    command.arg(operation).args(sources).current_dir(root);
    capture_result(
        command,
        oracle,
        root,
        timeout_ms,
        rss_limit_bytes,
        cpu_limit_ms,
    )
}

pub(super) fn base_command(compiler: &Path) -> Command {
    let mut command = Command::new(compiler);
    command
        .arg("--diagnostic-format")
        .arg("json")
        .arg("--color")
        .arg("never");
    command
}

pub(super) fn capture_result(
    mut command: Command,
    oracle: Oracle,
    root: &Path,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> OracleResult {
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

pub(super) fn infrastructure_result(
    oracle: Oracle,
    message: impl Into<String>,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> OracleResult {
    OracleResult {
        schema: "terlan.type-fuzz-observation.v1",
        oracle,
        outcome: "infrastructure_error",
        exit_code: None,
        stdout: String::new(),
        stderr: message.into(),
        resources: ResourceEvidence::unmeasured(timeout_ms, rss_limit_bytes, cpu_limit_ms),
    }
}

pub(super) fn finish_phase(
    formatting: OracleResult,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
    execute: impl FnOnce(u64, u64) -> OracleResult,
) -> OracleResult {
    let elapsed = formatting.resources.wall_elapsed_ms.unwrap_or(0);
    let cpu = formatting.resources.peak_cpu_ms.unwrap_or(0);
    let remaining_wall = timeout_ms.saturating_sub(elapsed);
    let remaining_cpu = cpu_limit_ms.saturating_sub(cpu);
    if remaining_wall == 0 || remaining_cpu == 0 {
        return exhausted_after_phase(
            formatting,
            timeout_ms,
            rss_limit_bytes,
            cpu_limit_ms,
            if remaining_wall == 0 { "wall" } else { "cpu" },
        );
    }
    let execution = execute(remaining_wall, remaining_cpu);
    combine_sequential(
        formatting,
        execution,
        timeout_ms,
        rss_limit_bytes,
        cpu_limit_ms,
    )
}

fn exhausted_after_phase(
    mut formatting: OracleResult,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
    limit: &'static str,
) -> OracleResult {
    formatting.outcome = "resource_exhausted";
    formatting.exit_code = None;
    formatting.resources.wall_timeout_ms = timeout_ms;
    formatting.resources.rss_limit_bytes = rss_limit_bytes;
    formatting.resources.cpu_limit_ms = cpu_limit_ms;
    formatting.resources.limit_exceeded = Some(limit);
    formatting
}

fn combine_sequential(
    formatting: OracleResult,
    execution: OracleResult,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> OracleResult {
    let mut resources = execution.resources.clone();
    resources.wall_timeout_ms = timeout_ms;
    resources.rss_limit_bytes = rss_limit_bytes;
    resources.cpu_limit_ms = cpu_limit_ms;
    resources.wall_elapsed_ms = sum_optional(
        formatting.resources.wall_elapsed_ms,
        execution.resources.wall_elapsed_ms,
    );
    resources.peak_cpu_ms = sum_optional(
        formatting.resources.peak_cpu_ms,
        execution.resources.peak_cpu_ms,
    );
    resources.peak_rss_bytes = max_optional(
        formatting.resources.peak_rss_bytes,
        execution.resources.peak_rss_bytes,
    );
    resources.limit_exceeded = formatting
        .resources
        .limit_exceeded
        .or(execution.resources.limit_exceeded);
    resources.process_group_cleanup =
        formatting.resources.process_group_cleanup && execution.resources.process_group_cleanup;
    let stdout = join_output(&formatting.stdout, &execution.stdout);
    let stderr = join_output(&formatting.stderr, &execution.stderr);
    let (stdout, stderr, output_exceeded) = bounded_output(stdout, stderr);
    if output_exceeded {
        resources.limit_exceeded = Some("output");
    }
    OracleResult {
        schema: execution.schema,
        oracle: execution.oracle,
        outcome: if output_exceeded {
            "resource_exhausted"
        } else {
            execution.outcome
        },
        exit_code: if output_exceeded {
            None
        } else {
            execution.exit_code
        },
        stdout,
        stderr,
        resources,
    }
}

fn sum_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn max_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn join_output(left: &str, right: &str) -> String {
    match (left.is_empty(), right.is_empty()) {
        (true, _) => right.to_string(),
        (_, true) => left.to_string(),
        (false, false) => format!("{left}\n{right}"),
    }
}

fn bounded_output(mut stdout: String, mut stderr: String) -> (String, String, bool) {
    let exceeded = stdout.len().saturating_add(stderr.len()) > resources::OUTPUT_LIMIT;
    if exceeded {
        truncate_utf8(&mut stderr, resources::OUTPUT_LIMIT);
        truncate_utf8(
            &mut stdout,
            resources::OUTPUT_LIMIT.saturating_sub(stderr.len()),
        );
    }
    (stdout, stderr, exceeded)
}

fn truncate_utf8(text: &mut String, limit: usize) {
    if text.len() <= limit {
        return;
    }
    let mut boundary = limit;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
}

fn normalize(text: &str, root: &Path) -> String {
    let case_normalized = text.replace(root.to_string_lossy().as_ref(), "$CASE");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("type-fuzz crate belongs to the compiler workspace");
    case_normalized.replace(workspace.to_string_lossy().as_ref(), "$TOOLCHAIN")
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
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace");
        assert_eq!(
            normalize(
                &workspace.join("std/core/Result.terl").to_string_lossy(),
                Path::new("/tmp/case"),
            ),
            "$TOOLCHAIN/std/core/Result.terl"
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
    fn native_oracle_selects_the_test_operation() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("case root");
        let source = root.path().join("case.terl");
        std::fs::write(&source, "module case.").expect("source");
        let compiler = root.path().join("compiler");
        std::fs::write(
            &compiler,
            "#!/bin/sh\nfor argument in \"$@\"; do [ \"$argument\" = test ] && exit 0; done\nexit 9\n",
        )
        .expect("compiler");
        std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o700))
            .expect("permissions");
        let result = run(
            &compiler,
            Oracle::Native,
            &source,
            1_000,
            1024 * 1024 * 1024,
            1_000,
        );
        assert_eq!(result.outcome, "passed");
        assert_eq!(result.exit_code, Some(0));
    }

    #[cfg(unix)]
    #[test]
    fn generated_cases_format_before_the_semantic_oracle() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("case root");
        let source = root.path().join("case.terl");
        std::fs::write(&source, "module case.\n").expect("source");
        let support = root.path().join("support.terl");
        std::fs::write(&support, "module support.\n").expect("support source");
        let launches = root.path().join("launches");
        let compiler = root.path().join("compiler");
        std::fs::write(
            &compiler,
            format!(
                concat!(
                    "#!/bin/sh\n",
                    "printf '%s\\n' \"$*\" >> {}\n",
                    "case \" $* \" in\n",
                    "  *' fmt '*) printf 'format-out'; printf 'format-err' >&2 ;;\n",
                    "  *' check '*) printf 'check-out'; printf 'check-err' >&2 ;;\n",
                    "  *) exit 9 ;;\n",
                    "esac\n",
                ),
                launches.display(),
            ),
        )
        .expect("compiler");
        std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o700))
            .expect("permissions");
        let result = run_formatted_batch_inputs(
            &compiler,
            Oracle::Check,
            FormattedBatchInputs {
                root: root.path(),
                semantic_sources: &["case.terl".into()],
                formatting_sources: &["case.terl".into(), "support.terl".into()],
            },
            1_000,
            1024 * 1024 * 1024,
            1_000,
        );
        assert_eq!(result.outcome, "passed");
        assert_eq!(result.stdout, "format-out\ncheck-out");
        assert_eq!(result.stderr, "format-err\ncheck-err");
        let launches = std::fs::read_to_string(launches).expect("launch ledger");
        let launches = launches.lines().collect::<Vec<_>>();
        assert_eq!(launches.len(), 2);
        assert!(launches[0].contains(" fmt --check case.terl support.terl"));
        assert!(launches[1].contains(" check case.terl"));
    }

    #[cfg(unix)]
    #[test]
    fn formatter_failure_stops_before_semantic_execution() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("case root");
        let source = root.path().join("case.terl");
        std::fs::write(&source, "module case.\n").expect("source");
        let launches = root.path().join("launches");
        let compiler = root.path().join("compiler");
        std::fs::write(
            &compiler,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nexit 7\n",
                launches.display(),
            ),
        )
        .expect("compiler");
        std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o700))
            .expect("permissions");
        let result = run_formatted_batch_inputs(
            &compiler,
            Oracle::Check,
            FormattedBatchInputs {
                root: root.path(),
                semantic_sources: &["case.terl".into()],
                formatting_sources: &["case.terl".into()],
            },
            1_000,
            1024 * 1024 * 1024,
            1_000,
        );
        assert_eq!(result.outcome, "failed");
        assert_eq!(result.exit_code, Some(7));
        assert_eq!(
            std::fs::read_to_string(launches)
                .expect("launch ledger")
                .lines()
                .count(),
            1
        );
    }

    #[test]
    fn sequential_phase_budgets_and_evidence_are_complete() {
        fn execution(wall: u64, cpu: u64) -> OracleResult {
            assert_eq!((wall, cpu), (90, 80));
            OracleResult {
                schema: "terlan.type-fuzz-observation.v1",
                oracle: Oracle::Check,
                outcome: "passed",
                exit_code: Some(0),
                stdout: "right".into(),
                stderr: "second".into(),
                resources: ResourceEvidence {
                    wall_elapsed_ms: Some(5),
                    peak_cpu_ms: Some(7),
                    peak_rss_bytes: Some(40),
                    ..ResourceEvidence::unmeasured(100, 1_000, 100)
                },
            }
        }
        let result = |wall, cpu, rss, stdout: String, stderr: String| OracleResult {
            schema: "terlan.type-fuzz-observation.v1",
            oracle: Oracle::Check,
            outcome: "passed",
            exit_code: Some(0),
            stdout,
            stderr,
            resources: ResourceEvidence {
                wall_elapsed_ms: wall,
                peak_cpu_ms: cpu,
                peak_rss_bytes: rss,
                ..ResourceEvidence::unmeasured(100, 1_000, 100)
            },
        };
        let formatting = result(Some(10), Some(20), Some(30), "left".into(), "first".into());
        let combined = finish_phase(formatting, 100, 1_000, 100, execution);
        assert_eq!(combined.resources.wall_elapsed_ms, Some(15));
        assert_eq!(combined.resources.peak_cpu_ms, Some(27));
        assert_eq!(combined.resources.peak_rss_bytes, Some(40));
        assert_eq!(combined.stdout, "left\nright");
        assert_eq!(combined.stderr, "first\nsecond");

        let wall_exhausted = finish_phase(
            result(Some(100), Some(1), None, String::new(), String::new()),
            100,
            1_000,
            100,
            execution,
        );
        assert_eq!(wall_exhausted.outcome, "resource_exhausted");
        assert_eq!(wall_exhausted.resources.limit_exceeded, Some("wall"));
        let cpu_exhausted = finish_phase(
            result(Some(1), Some(100), None, String::new(), String::new()),
            100,
            1_000,
            100,
            execution,
        );
        assert_eq!(cpu_exhausted.resources.limit_exceeded, Some("cpu"));

        assert_eq!(sum_optional(Some(1), None), Some(1));
        assert_eq!(sum_optional(None, Some(2)), Some(2));
        assert_eq!(sum_optional(None, None), None);
        assert_eq!(max_optional(Some(1), Some(2)), Some(2));
        assert_eq!(max_optional(Some(1), None), Some(1));
        assert_eq!(max_optional(None, Some(2)), Some(2));
        assert_eq!(max_optional(None, None), None);
        assert_eq!(join_output("", "right"), "right");
        assert_eq!(join_output("left", ""), "left");
    }

    #[test]
    fn sequential_output_uses_one_aggregate_limit() {
        let result = |stdout: String| OracleResult {
            schema: "terlan.type-fuzz-observation.v1",
            oracle: Oracle::Check,
            outcome: "passed",
            exit_code: Some(0),
            stdout,
            stderr: String::new(),
            resources: ResourceEvidence::unmeasured(100, 1_000, 100),
        };
        let combined = combine_sequential(
            result("x".repeat(resources::OUTPUT_LIMIT)),
            result("y".into()),
            100,
            1_000,
            100,
        );
        assert_eq!(combined.outcome, "resource_exhausted");
        assert_eq!(combined.exit_code, None);
        assert_eq!(combined.resources.limit_exceeded, Some("output"));
        assert_eq!(combined.stdout.len(), resources::OUTPUT_LIMIT);
        assert!(combined.stderr.is_empty());
        drop(combined);

        let oversized_stderr = format!("{}x", "d".repeat(resources::OUTPUT_LIMIT));
        let (stdout, stderr, exceeded) = bounded_output("unused".into(), oversized_stderr);
        assert!(exceeded);
        assert!(stdout.is_empty());
        assert_eq!(stderr.len(), resources::OUTPUT_LIMIT);
        let (stdout, stderr, exceeded) = bounded_output("short".into(), "diagnostic".into());
        assert!(!exceeded);
        assert_eq!((stdout.as_str(), stderr.as_str()), ("short", "diagnostic"));
        let mut unicode = "λλ".to_string();
        truncate_utf8(&mut unicode, 3);
        assert_eq!(unicode, "λ");
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
