//! One current-input scan for a bounded batch of explicit Cargo test requests.

use crate::coverage_requests::{failure, parse_batch};
use crate::execution_environment::ExecutionEnvironment;
use crate::native_coverage;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;
use terlan_process_owner::ProcessControl;

/// Verifies a whole request document with no Cargo, discovery, or test launches.
pub(super) fn main(mut arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let result = (|| {
        let report_path = PathBuf::from(
            arguments
                .next()
                .ok_or_else(|| failure("missing suite report"))?,
        );
        let request_path = PathBuf::from(
            arguments
                .next()
                .ok_or_else(|| failure("missing coverage request document"))?,
        );
        if arguments.next().is_some() {
            return Err(failure("unexpected coverage batch arguments"));
        }
        let environment = ExecutionEnvironment::capture()?;
        let shutdown = crate::shutdown::Shutdown::install().map_err(failure)?;
        let control = ProcessControl::new(crate::phase_timeout(&environment))
            .with_cancellation(shutdown.flag());
        let bytes = crate::file_identity::read_hashed_file(
            &request_path,
            &mut Sha256::new(),
            1024 * 1024,
            control,
            Instant::now(),
        )?;
        let requests = parse_batch(&bytes)?;
        let (report, inventory, digest) =
            crate::test_selections::read_completed(&report_path, control)?;
        let needs_native = requests
            .iter()
            .any(|(_, selection)| selection.package != "terlan" || selection.kind != "lib");
        let native = if needs_native {
            native_coverage::inventory(&report)?
        } else {
            Vec::new()
        };
        let mut native_inputs = BTreeMap::new();
        let mut results = Vec::new();
        for (request, selection) in requests {
            let coverage = if selection.package == "terlan" && selection.kind == "lib" {
                let selectors = selection
                    .selectors
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                inventory.inspect_coverage(&selectors)?
            } else {
                let target = native
                    .iter()
                    .find(|target| target.matches(&selection))
                    .ok_or_else(|| {
                        failure(format!("no completed target for request {}", request.id))
                    })?;
                let key = target.record()["target"]["executable"]
                    .as_str()
                    .ok_or_else(|| failure("missing native executable"))?;
                native_inputs.insert(key.to_owned(), target.record().clone());
                target.coverage(&selection)?
            };
            if coverage["covered"] != true {
                return Err(failure(format!(
                    "request {} is empty or not completely covered",
                    request.id
                )));
            }
            results
                .push(json!({"id":request.id,"arguments":request.arguments,"coverage":coverage}));
        }
        let native_inputs = native_inputs.into_values().collect::<Vec<Value>>();
        crate::verify_coverage::verify_inputs(
            &report_path,
            &report,
            &environment,
            &native_inputs,
            control,
            shutdown.flag(),
        )?;
        let result = json!({"schema":"terlan.cargo-test-coverage.v1","scope":"current-canonical-test-request-coverage-v1",
            "decision":"pass","current_inputs_verified":true,"reusable":false,"suite_run_id":report["run_id"],
            "suite_sha256":digest,"requests_sha256":crate::file_identity::hex(Sha256::new_with_prefix(&bytes)),
            "native_targets_verified":native_inputs.len(),"requests":results});
        serde_json::to_writer(std::io::stdout().lock(), &result).map_err(failure)?;
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
