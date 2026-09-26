//! Fresh, scoped coverage verification without builds, discovery or test execution.

use crate::execution_environment::ExecutionEnvironment;
use crate::{phase_plan, suite_inputs, PhaseFailure};
use serde_json::json;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use terlan_process_owner::ProcessControl;

/// A single gate operation borrows the admitted ledger for its execution lifetime.
type CoverageOperation<'a> =
    dyn FnMut(&mut crate::launch_ledger::LaunchLedger) -> Result<(), PhaseFailure> + 'a;

/// Checks historical selections against freshly admitted and closed current inputs.
pub(super) fn main(mut arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let result = (|| {
        let path = PathBuf::from(
            arguments
                .next()
                .ok_or_else(|| failure("missing suite report"))?,
        );
        if arguments.next().as_deref() != Some(OsStr::new("--")) {
            return Err(failure("expected -- before libtest selectors"));
        }
        let mut selectors = Vec::new();
        let mut bytes = 0_usize;
        for argument in arguments {
            let argument = argument
                .into_string()
                .map_err(|_| failure("selectors must be UTF-8"))?;
            bytes = bytes.saturating_add(argument.len());
            if selectors.len() == 256 || bytes > 64 * 1024 {
                return Err(failure("excessive coverage selectors"));
            }
            selectors.push(argument);
        }
        let environment = ExecutionEnvironment::capture()?;
        let timeout = crate::phase_timeout(&environment);
        let shutdown = crate::shutdown::Shutdown::install().map_err(failure)?;
        let control = ProcessControl::new(timeout).with_cancellation(shutdown.flag());
        let (report, inventory, digest) = crate::test_selections::read_completed(&path, control)?;
        let selectors = selectors.iter().map(String::as_str).collect::<Vec<_>>();
        let mut coverage = inventory.inspect_coverage(&selectors)?;
        if coverage["covered"] != true {
            return Err(failure(
                "requested tests are absent or not fully covered by this suite",
            ));
        }
        verify_inputs(&path, &report, &environment, &[], control, shutdown.flag())?;
        coverage["scope"] = json!("current-main-harness-coverage-v1");
        coverage["current_inputs_verified"] = json!(true);
        coverage["suite_run_id"] = report["run_id"].clone();
        coverage["suite_sha256"] = json!(digest);
        coverage["decision"] = json!("pass");
        // This is a fresh decision for this invocation, not a transferable skip token.
        coverage["reusable"] = json!(false);
        serde_json::to_writer(std::io::stdout().lock(), &coverage).map_err(failure)?;
        println!();
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[rust-coverage] {}: {}", error.outcome, error.detail);
            ExitCode::FAILURE
        }
    }
}

/// Shares one full input admission/closeout across single-selector and batched requests.
pub(super) fn verify_inputs(
    path: &Path,
    report: &serde_json::Value,
    environment: &ExecutionEnvironment,
    native_records: &[serde_json::Value],
    control: ProcessControl<'_>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<(), PhaseFailure> {
    verify_with_operation(
        path,
        report,
        environment,
        native_records,
        control,
        cancelled,
        None,
    )
}

/// Keeps the same admitted inputs around a live gate owner, without repeating input scans.
pub(super) fn verify_with_operation(
    path: &Path,
    report: &serde_json::Value,
    environment: &ExecutionEnvironment,
    native_records: &[serde_json::Value],
    control: ProcessControl<'_>,
    cancelled: &std::sync::atomic::AtomicBool,
    mut operation: Option<&mut CoverageOperation<'_>>,
) -> Result<(), PhaseFailure> {
    let threads = environment
        .value("TERLAN_TEST_THREADS")
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(crate::DEFAULT_TEST_THREADS);
    let coverage_owned = environment
        .value(crate::RELEASE_COVERAGE_OWNS_TERLC_ENV)
        .as_deref()
        == Some(OsStr::new("1"));
    let phases = phase_plan::test_phases(coverage_owned);
    let timeout = crate::phase_timeout(environment);
    let mut observation = path.as_os_str().to_owned();
    observation.push(if operation.is_some() {
        ".make-coverage.json"
    } else {
        ".verification.json"
    });
    let mut ledger =
        crate::launch_ledger::LaunchLedger::new(Path::new(&observation), threads, timeout)
            .map_err(failure)?;
    let verification = (|| {
        suite_inputs::admit(&mut ledger, environment, &phases, control)?;
        let mut native = crate::native_coverage::Inputs::admit(
            native_records,
            &ledger.metadata()?.packages(),
            control,
        )?;
        let declaration = crate::cargo_harness_admission::DeclaredHarness::restore_library(
            &report["executable_binding"]["declaration"],
            Path::new("."),
            control,
        )?;
        crate::cargo_metadata_owner::Handoff::admit_harness(
            &ledger.metadata()?.packages(),
            &declaration.json(),
        )?;
        ledger.bind_test_programs(control)?;
        ledger.bind_declared_harness(declaration, control)?;
        if let Some(operation) = operation.as_mut() {
            ledger.compare_admission(report)?;
            operation(&mut ledger)?;
        }
        native.close(control)?;
        suite_inputs::close(&mut ledger, environment, control)?;
        if operation.is_some() {
            ledger.finish_owned_coverage(report, cancelled)
        } else {
            ledger.finish_input_verification(report, cancelled)
        }
    })();
    if let Err(error) = verification {
        ledger.reject_input_verification()?;
        return Err(error);
    }
    Ok(())
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "coverage-verification-failed",
        detail: detail.to_string(),
    }
}
