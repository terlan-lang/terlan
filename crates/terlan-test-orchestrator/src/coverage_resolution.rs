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
    let coverage_owned = environment
        .value(crate::RELEASE_COVERAGE_OWNS_TERLC_ENV)
        .as_deref()
        == Some(std::ffi::OsStr::new("1"));
    let plan = crate::phase_plan::selected_test_phases(
        coverage_owned,
        environment
            .value(crate::phase_plan::COMPACT_PHASE_PLAN_ENV)
            .as_deref(),
    )?;
    let (base, phases) = phase_environments(environment, plan)?;
    Ok(move |request: &Value| {
        let actual_digest = request["environment"]
            .as_str()
            .ok_or_else(|| failure("missing request environment identity"))?;
        let actual = make_environment::TestIdentity::read(&request["environment_components"])?;
        if actual.digest != actual_digest {
            return Err(failure(
                "request environment evidence disagrees with its identity",
            ));
        }
        let arguments = &request["request"]["arguments"];
        let coverage =
            if arguments == &json!(["--whole-suite"]) || arguments == &json!(["--metadata"]) {
                if actual.digest != base.digest {
                    return Err(failure(format!(
                        "canonical suite request environment changed ({})",
                        base.difference(&actual)
                    )));
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
                    inventory.inspect_request(&args, &actual, &phases)?
                } else {
                    if actual.digest != base.digest {
                        return Err(failure(format!(
                            "native request environment changed ({})",
                            base.difference(&actual)
                        )));
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
    phases: Vec<crate::TestPhase>,
) -> Result<
    (
        make_environment::TestIdentity,
        BTreeMap<String, make_environment::TestIdentity>,
    ),
    PhaseFailure,
> {
    let program = Path::new("coverage-environment");
    let base = make_environment::test_observation(&environment.test_command(program))?;
    let phases = phases
        .into_iter()
        .map(|phase| {
            let mut command = environment.test_command(program);
            command.envs(phase.environment);
            Ok((
                phase.name.to_owned(),
                make_environment::test_observation(&command)?,
            ))
        })
        .collect::<Result<_, PhaseFailure>>()?;
    Ok((base, phases))
}
