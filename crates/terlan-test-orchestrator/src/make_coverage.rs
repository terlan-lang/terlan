//! Own Make gate execution between current-input admission and closeout.

use crate::coverage_bridge::{self, Bridge};
use crate::coverage_requests::failure;
use crate::execution_environment::ExecutionEnvironment;
use crate::make_environment::{self, CONTEXT};
use crate::{native_coverage, PhaseFailure, ValidationTier};
use serde_json::json;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use terlan_process_owner::ProcessControl;

/// Requests coverage from a live parent; a dead endpoint or changed test environment fails.
pub(super) fn request(arguments: impl Iterator<Item = OsString>) -> ExitCode {
    finish((|| {
        let arguments = arguments
            .map(|value| {
                value
                    .into_string()
                    .map_err(|_| failure("coverage arguments must be UTF-8"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let environment = ExecutionEnvironment::capture()?;
        let endpoint = environment
            .value(CONTEXT)
            .and_then(|value| value.into_string().ok())
            .ok_or_else(|| failure("test reuse requires a live coverage owner"))?;
        let environment =
            make_environment::test_identity(&environment.command(Path::new("coverage-request")))?;
        coverage_bridge::request(
            &endpoint,
            json!({"request":{"id":"make-request","arguments":arguments},"environment":environment}),
        )?;
        Ok(())
    })())
}

/// Runs the specified Make gate graph once under a live, input-bound coverage session.
pub(super) fn main(mut arguments: impl Iterator<Item = OsString>) -> ExitCode {
    finish((|| {
        let path = PathBuf::from(
            arguments
                .next()
                .ok_or_else(|| failure("missing suite report"))?,
        );
        if arguments.next().as_deref() != Some(OsStr::new("--")) {
            return Err(failure("expected -- before Make command"));
        }
        let program = arguments
            .next()
            .ok_or_else(|| failure("missing Make program"))?;
        if !matches!(
            Path::new(&program).file_name().and_then(OsStr::to_str),
            Some("make" | "gmake" | "make.exe")
        ) {
            return Err(failure(
                "coverage owner requires Make, not an arbitrary command",
            ));
        }
        let arguments = arguments.collect::<Vec<_>>();
        if arguments.is_empty()
            || arguments.len() > 256
            || arguments
                .iter()
                .map(|argument| argument.as_encoded_bytes().len())
                .sum::<usize>()
                > 64 * 1024
        {
            return Err(failure("empty or excessive Make arguments"));
        }
        let environment = ExecutionEnvironment::capture()?;
        let shutdown = crate::shutdown::Shutdown::install().map_err(failure)?;
        let control = ProcessControl::new(crate::phase_timeout(&environment))
            .with_cancellation(shutdown.flag());
        let make = crate::executable_binding::resolve_program(
            program
                .to_str()
                .ok_or_else(|| failure("Make path must be UTF-8"))?,
            environment.test_path(),
        )?;
        if make_environment::dry_run(&environment) {
            control
                .run(
                    environment
                        .test_command(&make)
                        .args(&arguments)
                        .env(CONTEXT, "dry-run:no-live-owner"),
                    |_| Ok(()),
                )
                .map_err(crate::process_failure)?;
            println!("[rust-coverage] dry-run plan only; no coverage verified");
            return Ok(());
        }
        let _suite_lease = crate::report_file::read_lease(&path)
            .map_err(failure)?
            .ok_or_else(|| failure("live Make coverage requires an owned suite publication"))?;
        let (report, inventory, report_digest) =
            crate::test_selections::read_completed(&path, control)?;
        let producer = crate::hosted_producer::context(&environment)?;
        let suite_binding = json!({"sha256":report_digest,"run_id":report["run_id"],
            "selections_sha256":report["test_selection_binding"]["sha256"]});
        let native = native_coverage::inventory(&report)?;
        let records = native
            .iter()
            .map(|target| target.record().clone())
            .collect::<Vec<_>>();
        let mut resolver = Some(crate::coverage_resolution::resolver(
            inventory,
            native,
            &environment,
            crate::coverage_resolution::Scope::CurrentInputs,
        )?);
        let mut make_binding = crate::executable_binding::ExecutableBinding::capture(
            &[("make", make.clone())],
            control,
        )?;
        crate::verify_coverage::verify_with_operation(
            &path,
            &report,
            &environment,
            &records,
            control,
            shutdown.flag(),
            Some(&mut |ledger| {
                let resolve = resolver
                    .take()
                    .ok_or_else(|| failure("Make coverage owner cannot repeat"))?;
                ledger.execute_test(
                    "owned Make gates",
                    ValidationTier::Integration,
                    "make-covered-gates",
                    |launched, partial| {
                        let bridge = Bridge::start(resolve)?;
                        let mut command = environment
                            .test_command(&make_binding.verify_program("make", control)?);
                        command.args(&arguments).env(CONTEXT, bridge.endpoint());
                        let invocation = make_environment::invocation_identity(&command);
                        let outcome = control
                            .run(&mut command, launched)
                            .map_err(crate::process_failure);
                        let mut evidence = bridge.finish()?;
                        evidence["make_invocation_sha256"] = json!(invocation);
                        evidence["completed_suite_binding"] = suite_binding.clone();
                        evidence["producer_context"] = producer.clone();
                        *partial = Some(crate::test_execution::TestEvidence(evidence.clone()));
                        outcome?;
                        if evidence["decision"] != "pass" {
                            return Err(failure("Make had failed or missing coverage requests"));
                        }
                        make_binding.verify(control)?;
                        evidence["make_executable_binding"] = make_binding.json();
                        Ok(crate::test_execution::TestEvidence(evidence))
                    },
                )?;
                Ok(())
            }),
        )?;
        println!(
            "[rust-coverage] Make gates passed with live request coverage and unchanged inputs"
        );
        Ok(())
    })())
}

fn finish(result: Result<(), PhaseFailure>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[rust-coverage] {}: {}", error.outcome, error.detail);
            ExitCode::FAILURE
        }
    }
}
