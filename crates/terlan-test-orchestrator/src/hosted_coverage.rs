//! Historical CI record integrity; GitHub authentication and current-input reuse are separate.

use crate::coverage_requests::failure;
use crate::test_selections::{read_completed, read_document};
use crate::PhaseFailure;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Duration;
use terlan_process_owner::ProcessControl;

const REPORT: &str = "rust-test-suite-report.json";
const SELECTIONS: &str = "rust-test-suite-report.json.selections.json";
const MAKE: &str = "rust-test-suite-report.json.make-coverage.json";

/// Immutable historical records retained by a live consumer after one admission.
pub(super) struct Bundle {
    /// Original executed suite and its scoped input observations.
    pub(super) suite: Value,
    /// Already verified main-harness names and execution ownership.
    pub(super) inventory: crate::test_selections::TestSelections,
    /// Already verified native target completions.
    pub(super) native: Vec<crate::native_coverage::Target>,
    /// Exact checked subjects, without authentication or local-artifact equivalence claims.
    pub(super) summary: Value,
}

/// Checks downloaded records against caller-supplied coordinates, never granting test reuse.
pub(super) fn main(mut arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let result = (|| {
        let directory = arguments
            .next()
            .ok_or_else(|| failure("missing bundle directory"))?;
        let context = arguments
            .next()
            .ok_or_else(|| failure("missing expected context file"))?;
        if arguments.next().is_some() {
            return Err(failure("unexpected hosted coverage arguments"));
        }
        let shutdown = crate::shutdown::Shutdown::install().map_err(failure)?;
        let control =
            ProcessControl::new(Duration::from_secs(30)).with_cancellation(shutdown.flag());
        let (context, _) = read_document(Path::new(&context), 16 * 1024, control)?;
        let checked = read(Path::new(&directory), &context, control)?;
        serde_json::to_writer(std::io::stdout().lock(), &checked.summary).map_err(failure)?;
        println!();
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

/// Shares bounded historical admission with the live source-coverage owner.
pub(super) fn read(
    directory: &Path,
    context: &Value,
    control: ProcessControl<'_>,
) -> Result<Bundle, PhaseFailure> {
    if !std::fs::symlink_metadata(directory)
        .map_err(failure)?
        .is_dir()
    {
        return Err(failure("hosted bundle must be a regular directory"));
    }
    let mut names = BTreeSet::new();
    for entry in std::fs::read_dir(directory).map_err(failure)? {
        let entry = entry.map_err(failure)?;
        if names.len() == 3 || !entry.file_type().map_err(failure)?.is_file() {
            return Err(failure("unexpected hosted coverage entry"));
        }
        names.insert(entry.file_name());
    }
    if names
        != [REPORT, SELECTIONS, MAKE]
            .into_iter()
            .map(OsString::from)
            .collect()
    {
        return Err(failure("hosted coverage bundle is incomplete"));
    }
    let (suite, inventory, suite_digest) = read_completed(&directory.join(REPORT), control)?;
    let native = crate::native_coverage::inventory(&suite)?;
    let (make, make_digest) = read_document(&directory.join(MAKE), 1024 * 1024, control)?;
    let evidence = validate(&suite, &suite_digest, &make, context)?;
    requests(evidence, &inventory, &native)?;
    let summary = json!({"schema":"terlan.hosted-coverage-records.v1","decision":"records-consistent",
        "authentication_required":true,"current_inputs_verified":false,"reusable":false,
        "producer_context":evidence["producer_context"],"suite_run_id":suite["run_id"],
        "native_target_count":native.len(),"requester_process_count":evidence["requester_process_count"],
        "files":{REPORT:suite_digest,SELECTIONS:suite["test_selection_binding"]["sha256"],MAKE:make_digest}});
    Ok(Bundle {
        suite,
        inventory,
        native,
        summary,
    })
}

fn validate<'a>(
    suite: &Value,
    digest: &str,
    make: &'a Value,
    context: &Value,
) -> Result<&'a Value, PhaseFailure> {
    let phases = passed_phases(make)?;
    let suite_phases = passed_phases(suite)?;
    let owners = phases
        .iter()
        .filter(|phase| phase["executor"] == "make-covered-gates")
        .collect::<Vec<_>>();
    if make["schema"] != "terlan.rust-test-suite.v4"
        || make["decision"] != "gates-covered"
        || make["direct_cargo_launch_count"] != 0
        || suite["direct_cargo_launch_count"] != 3
        || suite_phases
            .iter()
            .filter(|phase| {
                matches!(
                    phase["executor"].as_str(),
                    Some("cargo-build" | "cargo" | "cargo-native-harnesses")
                )
            })
            .count()
            != 3
        || owners.len() != 1
        || phases.iter().any(|phase| {
            !matches!(
                phase["executor"].as_str(),
                Some(
                    "git-source-inventory"
                        | "rustup-tool-resolution"
                        | "rust-selected-version"
                        | "rust-selected-sysroot"
                        | "make-covered-gates"
                )
            )
        })
    {
        return Err(failure(
            "hosted Make record is not a completed no-replay gate owner",
        ));
    }
    let evidence = &owners[0]["test_execution"];
    if evidence["scope"] != "live-make-test-coverage-v1"
        || evidence["decision"] != "pass"
        || evidence["reusable"] != false
        || evidence["completed_suite_binding"]
            != json!({
            "sha256":digest,"run_id":suite["run_id"],"selections_sha256":suite["test_selection_binding"]["sha256"]})
    {
        return Err(failure(
            "hosted Make evidence is not bound to these exact suite records",
        ));
    }
    producer(&evidence["producer_context"], context)?;
    inputs(suite, make)?;
    invocation(evidence)?;
    Ok(evidence)
}

fn passed_phases(report: &Value) -> Result<&Vec<Value>, PhaseFailure> {
    let phases = report["phases"]
        .as_array()
        .filter(|rows| !rows.is_empty() && rows.len() <= 256)
        .ok_or_else(|| failure("missing or excessive completed phases"))?;
    if phases
        .iter()
        .any(|phase| phase["outcome"] != "passed" || !positive_pid(&phase["child_pid"]))
    {
        return Err(failure(
            "hosted phase lacks successful observed child completion",
        ));
    }
    Ok(phases)
}

fn producer(actual: &Value, expected: &Value) -> Result<(), PhaseFailure> {
    let repository = expected["repository"]
        .as_str()
        .filter(|value| value.len() <= 256)
        .ok_or_else(|| failure("missing expected repository"))?;
    let parts = repository.split('/').collect::<Vec<_>>();
    let revision = expected["revision"].as_str().unwrap_or("");
    let compiler = &expected["compiler"];
    if expected["schema"] != "terlan.hosted-download-cache.v1"
        || parts.len() != 2
        || parts.iter().any(|value| {
            matches!(*value, "" | "." | "..")
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        })
        || revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || compiler["path"] != ".github/workflows/ci.yml"
        || compiler["id"].as_u64().is_none_or(|value| value == 0)
        || compiler["attempt"].as_u64().is_none_or(|value| value == 0)
        || actual
            != &json!({"provider":"github-actions","repository":repository,"source_revision":revision,
            "run_id":compiler["id"],"run_attempt":compiler["attempt"],
            "workflow_ref":format!("{repository}/.github/workflows/ci.yml@refs/heads/main"),
            "job":"check","host_os":"linux","host_arch":"x86_64","authentication_required":true})
    {
        return Err(failure(
            "hosted coverage producer differs from expected compiler run, attempt or target",
        ));
    }
    Ok(())
}

fn inputs(suite: &Value, make: &Value) -> Result<(), PhaseFailure> {
    let mut bindings = crate::validation_inputs::ValidationInputs::default().snapshot();
    for (key, value) in bindings
        .as_object_mut()
        .expect("fixed input binding object")
    {
        *value = make[key.as_str()].clone();
        if !value.is_object()
            || (key != "environment_binding"
                && key != "cargo_metadata_binding"
                && value["verified"] != true)
        {
            return Err(failure(format!("hosted input is not closed: {key}")));
        }
    }
    for key in [
        "source_binding",
        "executable_binding",
        "tool_configuration_binding",
        "rust_toolchain_binding",
    ] {
        if bindings[key]["before"].is_null() || bindings[key]["before"] != bindings[key]["after"] {
            return Err(failure(format!(
                "hosted inputs changed during execution: {key}"
            )));
        }
    }
    for key in ["resolver_cache", "package_sources"] {
        if bindings["cargo_metadata_binding"][key]["verified"] != true {
            return Err(failure("hosted metadata lacks closed input observations"));
        }
    }
    crate::suite_inputs::compare(&bindings, suite)
}

fn invocation(evidence: &Value) -> Result<(), PhaseFailure> {
    let binding = &evidence["make_executable_binding"];
    let rows = binding["before"]
        .as_array()
        .filter(|rows| rows.len() == 1)
        .ok_or_else(|| failure("missing exact Make executable"))?;
    let path = rows[0]["path"]
        .as_str()
        .filter(|path| path.starts_with('/') && path.len() <= 4096)
        .ok_or_else(|| failure("invalid hosted Make path"))?;
    let mut command = Command::new(path);
    command.args(["--no-print-directory", "check-gates"]);
    let base = crate::make_environment::invocation_identity(&command);
    command.arg("release-evidence-compose");
    let release = crate::make_environment::invocation_identity(&command);
    if binding["verified"] != true
        || binding["before"] != binding["after"]
        || rows[0]["role"] != "make"
        || !sha256(&rows[0]["identity_sha256"])
        || (evidence["make_invocation_sha256"] != base
            && evidence["make_invocation_sha256"] != release)
    {
        return Err(failure(
            "hosted Make invocation is not the canonical complete gate graph",
        ));
    }
    Ok(())
}

fn requests(
    evidence: &Value,
    inventory: &crate::test_selections::TestSelections,
    native: &[crate::native_coverage::Target],
) -> Result<(), PhaseFailure> {
    let requests = evidence["requests"]
        .as_array()
        .filter(|rows| !rows.is_empty() && rows.len() <= 4096)
        .ok_or_else(|| failure("missing or excessive hosted gate requests"))?;
    let mut pids = BTreeSet::new();
    if evidence["requester_process_count"].as_u64() != Some(requests.len() as u64) {
        return Err(failure("hosted request count mismatch"));
    }
    for request in requests {
        if request["decision"] != "pass"
            || !positive_pid(&request["pid"])
            || !pids.insert(request["pid"].as_u64())
            || !sha256(&request["environment"])
        {
            return Err(failure("failed, duplicate or malformed hosted request"));
        }
        let args = &request["request"]["arguments"];
        if args != &json!(["--whole-suite"]) && args != &json!(["--metadata"]) {
            let parsed = crate::coverage_requests::parse_batch(
                &serde_json::to_vec(&json!([request["request"]])).map_err(failure)?,
            )?;
            let selection = &parsed[0].1;
            let coverage = if selection.package == "terlan" && selection.kind == "lib" {
                let selectors = selection
                    .selectors
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                inventory.inspect_coverage(&selectors)?
            } else {
                native
                    .iter()
                    .find(|target| target.matches(selection))
                    .ok_or_else(|| failure("hosted request has no completed native target"))?
                    .coverage(selection)?
            };
            if coverage["covered"] != true {
                return Err(failure("hosted request names tests that did not complete"));
            }
        }
    }
    Ok(())
}

fn positive_pid(value: &Value) -> bool {
    value
        .as_u64()
        .is_some_and(|value| value > 0 && value <= u64::from(u32::MAX))
}

fn sha256(value: &Value) -> bool {
    value.as_str().is_some_and(|value| {
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}
