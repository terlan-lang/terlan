//! Live publication source-test coverage; local artifact checks retain their own owners.

use crate::coverage_requests::failure;
use crate::execution_environment::ExecutionEnvironment;
use crate::launch_ledger::LaunchLedger;
use crate::{make_environment, PhaseFailure, ValidationTier};
use serde_json::json;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::ExitCode;
use terlan_process_owner::ProcessControl;

/// Executes literal Make goals with authenticated historical source-test coverage only.
pub(super) fn main(mut arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let result = (|| {
        if arguments.next().as_deref() != Some(OsStr::new("--")) {
            return Err(failure("expected -- before hosted Make invocation"));
        }
        let program = arguments
            .next()
            .ok_or_else(|| failure("missing Make executable"))?;
        if !matches!(
            Path::new(&program).file_name().and_then(OsStr::to_str),
            Some("make" | "gmake" | "make.exe")
        ) {
            return Err(failure("hosted coverage requires Make"));
        }
        let args = arguments.collect::<Vec<_>>();
        if args.is_empty()
            || args.len() > 64
            || args.iter().any(|arg| {
                arg.to_str().is_none_or(|arg| {
                    arg != "--no-print-directory"
                        && (arg.is_empty()
                            || arg.len() > 128
                            || arg.starts_with('-')
                            || !arg.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
                            }))
                })
            })
        {
            return Err(failure(
                "hosted coverage requires bounded literal Make goals",
            ));
        }
        let environment = ExecutionEnvironment::capture()?;
        let shutdown = crate::shutdown::Shutdown::install().map_err(failure)?;
        let control = ProcessControl::new(crate::phase_timeout(&environment))
            .with_cancellation(shutdown.flag());
        let make = crate::executable_binding::resolve_program(
            program
                .to_str()
                .ok_or_else(|| failure("invalid Make path"))?,
            environment.test_path(),
        )?;
        if make_environment::dry_run(&environment) {
            control
                .run(
                    environment
                        .test_command(&make)
                        .args(&args)
                        .env(make_environment::CONTEXT, "dry-run:no-live-owner")
                        .env(make_environment::SCOPE, "hosted-source"),
                    |_| Ok(()),
                )
                .map_err(crate::process_failure)?;
            println!("[hosted-coverage] dry-run plan only; no source coverage admitted");
            return Ok(());
        }
        let root = std::env::current_dir().map_err(failure)?;
        let mut ledger = LaunchLedger::new(
            &root.join("target/quality/hosted-source-coverage.json"),
            crate::DEFAULT_TEST_THREADS,
            crate::phase_timeout(&environment),
        )
        .map_err(failure)?;
        let operation = (|| {
            ledger
                .bind_environment(&environment, &crate::phase_plan::test_phases(false))
                .map_err(failure)?;
            ledger.admit_executables(&environment, control)?;
            ledger.bind_test_programs(control)?;
            let tools = ledger.executables().clone();
            let git = tools.verify_program("git", control)?;
            let source = ledger.execute(
                "hosted source admission",
                ValidationTier::FastUnit,
                "git-source-inventory",
                |launched| {
                    crate::source_inventory::capture_with_git(
                        &root,
                        &git,
                        &environment,
                        control,
                        launched,
                    )
                },
            )?;
            ledger.bind_source(source).map_err(failure)?;
            let expected_revision = revision(
                &mut ledger,
                &environment,
                &git,
                control,
                "admit candidate revision",
            )?;
            clean(
                &mut ledger,
                &environment,
                &git,
                control,
                "admit candidate source status",
            )?;
            let (bundle, provenance) =
                crate::hosted_checkpoint::admit(&root, &expected_revision, control)?;
            let hosted_source = bundle.suite["source_binding"].clone();
            ledger.compare_hosted_source(&hosted_source)?;
            let resolver = crate::coverage_resolution::resolver(
                bundle.inventory,
                bundle.native,
                &environment,
                crate::coverage_resolution::Scope::HostedSource,
            )?;
            let mut make_binding = crate::executable_binding::ExecutableBinding::capture(
                &[("make", make.clone())],
                control,
            )?;
            ledger.execute_test(
                "hosted source covered Make gates",
                ValidationTier::Integration,
                "hosted-source-covered-gates",
                |launched, partial| {
                    let bridge = crate::coverage_bridge::Bridge::start(resolver)?;
                    let mut command =
                        environment.test_command(&make_binding.verify_program("make", control)?);
                    command
                        .args(&args)
                        .env(make_environment::CONTEXT, bridge.endpoint())
                        .env(make_environment::SCOPE, "hosted-source");
                    let invocation = make_environment::invocation_identity(&command);
                    let result = control
                        .run(&mut command, launched)
                        .map_err(crate::process_failure);
                    let mut evidence = bridge.finish()?;
                    evidence["request_scope"] = evidence["scope"].clone();
                    evidence["scope"] = json!("hosted-canonical-source-test-coverage-v1");
                    evidence["local_artifact_test_equivalence"] = json!(false);
                    evidence["checkpoint"] = provenance.clone();
                    evidence["make_invocation_sha256"] = json!(invocation);
                    *partial = Some(crate::test_execution::TestEvidence(evidence.clone()));
                    result?;
                    if evidence["decision"] != "pass" {
                        return Err(failure("hosted Make requests failed or were absent"));
                    }
                    make_binding.verify(control)?;
                    evidence["make_executable_binding"] = make_binding.json();
                    Ok(crate::test_execution::TestEvidence(evidence))
                },
            )?;
            let git = tools.verify_program("git", control)?;
            if revision(
                &mut ledger,
                &environment,
                &git,
                control,
                "close candidate revision",
            )? != expected_revision
            {
                return Err(failure(
                    "candidate revision changed during hosted gate execution",
                ));
            }
            clean(
                &mut ledger,
                &environment,
                &git,
                control,
                "close candidate source status",
            )?;
            let source = ledger.execute(
                "hosted source closeout",
                ValidationTier::FastUnit,
                "git-source-inventory",
                |launched| {
                    crate::source_inventory::capture_with_git(
                        &root,
                        &git,
                        &environment,
                        control,
                        launched,
                    )
                },
            )?;
            ledger.verify_source(source).map_err(failure)?;
            ledger.verify_executables(control)?;
            ledger.finish_hosted_source(&hosted_source, shutdown.flag())
        })();
        if let Err(error) = operation {
            ledger.reject_input_verification()?;
            return Err(error);
        }
        println!("[hosted-coverage] canonical source tests covered; local artifact test equivalence not claimed");
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[hosted-coverage] {}: {}", error.outcome, error.detail);
            ExitCode::FAILURE
        }
    }
}

fn revision(
    ledger: &mut LaunchLedger,
    environment: &ExecutionEnvironment,
    git: &Path,
    control: ProcessControl<'_>,
    phase: &'static str,
) -> Result<String, PhaseFailure> {
    ledger.execute(
        phase,
        ValidationTier::FastUnit,
        "git-source-revision",
        |launched| {
            let bytes = control
                .capture_stdout(
                    environment
                        .command(git)
                        .args(["rev-parse", "--verify", "HEAD"]),
                    128,
                    launched,
                )
                .map_err(crate::process_failure)?;
            let revision = std::str::from_utf8(&bytes).map_err(failure)?.trim();
            if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(failure("invalid candidate revision"));
            }
            Ok(revision.to_owned())
        },
    )
}

fn clean(
    ledger: &mut LaunchLedger,
    environment: &ExecutionEnvironment,
    git: &Path,
    control: ProcessControl<'_>,
    phase: &'static str,
) -> Result<(), PhaseFailure> {
    ledger.execute(
        phase,
        ValidationTier::FastUnit,
        "git-source-status",
        |launched| {
            let bytes = control
                .capture_stdout(
                    environment.command(git).args([
                        "status",
                        "--porcelain=v1",
                        "--untracked-files=all",
                        "-z",
                    ]),
                    1024 * 1024,
                    launched,
                )
                .map_err(crate::process_failure)?;
            if !bytes.is_empty() {
                return Err(failure("hosted source coverage requires a clean candidate"));
            }
            Ok(())
        },
    )
}
