//! One exact-name and requester-environment policy for live coverage owners.

use crate::coverage_requests::{failure, parse_batch};
use crate::execution_environment::ExecutionEnvironment;
use crate::{make_environment, native_coverage, PhaseFailure};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// Distinguishes current-input equivalence from historical canonical source validation.
#[derive(Clone, Copy)]
pub(super) enum Scope {
    /// The parent already compared every scoped local input with the completed suite.
    CurrentInputs,
    /// Only the authenticated suite's source tree and canonical selections are reused.
    HostedSource,
}

/// Captures the local request contract once; never recaptures environment per request.
pub(super) fn resolver(
    inventory: crate::test_selections::TestSelections,
    native: Vec<native_coverage::Target>,
    environment: &ExecutionEnvironment,
    scope: Scope,
) -> Result<impl FnMut(&Value) -> Result<Value, PhaseFailure> + Send + 'static, PhaseFailure> {
    let (base, phases) = phase_environments(environment)?;
    Ok(move |request: &Value| {
        let actual = request["environment"]
            .as_str()
            .ok_or_else(|| failure("missing request environment identity"))?;
        let arguments = &request["request"]["arguments"];
        let coverage =
            if arguments == &json!(["--whole-suite"]) || arguments == &json!(["--metadata"]) {
                if actual != base {
                    return Err(failure("canonical suite request environment changed"));
                }
                if arguments == &json!(["--metadata"]) {
                    if matches!(scope, Scope::HostedSource) {
                        return Err(failure("hosted tests cannot supply current Cargo metadata"));
                    }
                    json!({"scope":"same-input-admitted-metadata","covered":true})
                } else {
                    json!({"scope":"completed-canonical-suite","covered":true})
                }
            } else {
                let batch = serde_json::to_vec(&json!([request["request"]])).map_err(failure)?;
                let (_, selection) = parse_batch(&batch)?
                    .pop()
                    .ok_or_else(|| failure("empty live request"))?;
                let args = selection
                    .selectors
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                if selection.package == "terlan" && selection.kind == "lib" {
                    inventory.inspect_request(&args, actual, &phases)?
                } else {
                    if actual != base {
                        return Err(failure("native request environment changed"));
                    }
                    let target = native
                        .iter()
                        .find(|target| target.matches(&selection))
                        .ok_or_else(|| failure("no completed native target"))?;
                    let coverage = target.coverage(&selection)?;
                    if coverage["covered"] != true {
                        return Err(failure("native request matches no completed tests"));
                    }
                    coverage
                }
            };
        Ok(match scope {
            Scope::CurrentInputs => coverage,
            Scope::HostedSource => json!({"scope":"hosted-canonical-source-test-coverage-v1",
                "covered":true,"local_artifact_test_equivalence":false,"reusable":false,
                "historical_coverage":coverage}),
        })
    })
}

fn phase_environments(
    environment: &ExecutionEnvironment,
) -> Result<(String, BTreeMap<String, String>), PhaseFailure> {
    let program = Path::new("coverage-environment");
    let base = make_environment::test_identity(&environment.test_command(program))?;
    let phases = crate::phase_plan::test_phases(false)
        .into_iter()
        .map(|phase| {
            let mut command = environment.test_command(program);
            command.envs(phase.environment);
            Ok((
                phase.name.to_owned(),
                make_environment::test_identity(&command)?,
            ))
        })
        .collect::<Result<_, PhaseFailure>>()?;
    Ok((base, phases))
}
