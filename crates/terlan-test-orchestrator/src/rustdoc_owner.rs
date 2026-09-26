//! Independent workspace Rustdoc ownership without changing its test runner.

use crate::executable_binding::ExecutableBinding;
use crate::execution_environment::ExecutionEnvironment;
use crate::test_execution::TestEvidence;
use crate::workspace_native_storage::{read_json, write_new, Registry};
use crate::{PhaseFailure, TestPhase};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use terlan_process_owner::ProcessControl;

mod targets;
pub(crate) use targets::{project, Target};
mod observer;
pub(crate) use observer::{is_observer, main};

fn observer_name() -> String {
    format!("terlan-rustdoc-observer{}", std::env::consts::EXE_SUFFIX)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    schema: String,
    rustdoc: PathBuf,
    rustdoc_binding: Value,
    timeout_seconds: u64,
    targets: Vec<Target>,
}

/// Selected tools and independently admitted package targets for one doctest phase.
pub(crate) struct Inputs {
    /// Original selected Rustdoc, retaining its invocation path and argv-zero behavior.
    pub(crate) rustdoc: PathBuf,
    /// Enabled workspace libraries which must complete exactly once.
    pub(crate) targets: Vec<Target>,
    /// Already-parsed Cargo settings used to preserve child environments.
    pub(crate) settings: crate::cargo_tool_settings::CargoToolSettings,
}

/// Runs Cargo once and reconciles every expected Rustdoc completion independently.
pub(crate) fn run(
    phase: &TestPhase,
    tools: (&Path, &Path, &Inputs),
    environment: &ExecutionEnvironment,
    threads: usize,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
    partial: &mut Option<TestEvidence>,
) -> Result<TestEvidence, PhaseFailure> {
    let mut registry = Registry::create(environment)?;
    let result = registered(
        phase,
        tools,
        environment,
        threads,
        control,
        launched,
        &mut registry,
    );
    match result {
        Ok(evidence) => {
            registry.close().inspect_err(|_| {
                *partial = Some(TestEvidence(evidence.json()));
            })?;
            Ok(evidence)
        }
        Err(error) => {
            *partial = Some(TestEvidence(registry.failure_evidence()));
            Err(error)
        }
    }
}

fn registered(
    phase: &TestPhase,
    tools: (&Path, &Path, &Inputs),
    environment: &ExecutionEnvironment,
    threads: usize,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
    registry: &mut Registry,
) -> Result<TestEvidence, PhaseFailure> {
    if phase.args.last() != Some(&"--")
        || !phase.args.contains(&"--doc")
        || tools.2.targets.is_empty()
    {
        return Err(failure("Rustdoc phase has no admitted target inventory"));
    }
    let observer = registry.path().join(observer_name());
    registry.register_observer(&observer_name())?;
    install_observer(tools.1, &observer)?;
    let mut observer_binding =
        ExecutableBinding::capture(&[("rustdoc-observer", observer.clone())], control)?;
    let mut rustdoc_binding =
        ExecutableBinding::capture(&[("rustdoc", tools.2.rustdoc.clone())], control)?;
    let context = Context {
        schema: "terlan.rustdoc-context.v1".into(),
        rustdoc: tools.2.rustdoc.clone(),
        rustdoc_binding: rustdoc_binding.json(),
        timeout_seconds: crate::phase_timeout(environment).as_secs(),
        targets: tools.2.targets.clone(),
    };
    let context = serde_json::to_value(context).map_err(failure)?;
    write_new(&registry.path().join("context.json"), &context)?;
    let (_, context_identity) = read_json(&registry.path().join("context.json"), control)?;
    for target in &tools.2.targets {
        registry.register_target(&target.key()?)?;
    }
    let mut command = environment.test_command(tools.0);
    command.args(&phase.args[..phase.args.len() - 1]);
    tools
        .2
        .settings
        .observe_rustdoc(&mut command, environment, &observer)?;
    command
        .args([
            "--",
            "--test-threads",
            &threads.to_string(),
            "--format",
            "pretty",
            "--color",
            "never",
        ])
        .envs(phase.environment.iter().map(|(key, value)| (key, value)));
    let captured = control
        .capture_stdout_observed(
            &mut command,
            16 * 1024 * 1024,
            |pid| {
                launched(pid)?;
                write_new(&registry.path().join("cargo.json"), &json!({"pid":pid}))
                    .map_err(|error| error.detail)
            },
            |bytes| {
                let mut output = std::io::stdout().lock();
                output
                    .write_all(bytes)
                    .and_then(|()| output.flush())
                    .map_err(|error| error.to_string())
            },
        )
        .map_err(crate::process_failure)?;
    captured.outcome.map_err(crate::process_failure)?;
    let mut records = Vec::new();
    let mut passed = 0_u64;
    for target in &tools.2.targets {
        let key = target.key()?;
        let (complete, _) = read_json(
            &registry.path().join(format!("{key}.complete.json")),
            control,
        )?;
        let (started, _) = read_json(
            &registry.path().join(format!("{key}.started.json")),
            control,
        )?;
        let (run, _) = read_json(&registry.path().join(format!("{key}.run.json")), control)?;
        passed = passed
            .checked_add(admit_completion(
                &complete,
                target,
                &context_identity,
                &started,
                &run,
            )?)
            .ok_or_else(|| failure("Rustdoc completed-test count overflow"))?;
        records.push(complete);
    }
    if read_json(&registry.path().join("context.json"), control)?.0 != context {
        return Err(failure("Rustdoc context changed during execution"));
    }
    observer_binding.verify(control)?;
    rustdoc_binding.verify(control)?;
    Ok(TestEvidence(
        json!({"scope":"admitted-rustdoc-package-records-v1", "independent_package_inventory":true,
        "independent_test_inventory":false, "passed":passed, "nested_process_launch_count":records.len()*2,
        "targets":records, "observer_binding":observer_binding.json(), "rustdoc_binding":rustdoc_binding.json(),
        "cargo_stdout_bytes":captured.stdout.len()}),
    ))
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "rustdoc-owner-failed",
        detail: detail.to_string(),
    }
}

fn install_observer(source: &Path, destination: &Path) -> Result<(), PhaseFailure> {
    match std::fs::hard_link(source, destination) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => (),
        Err(error) => return Err(failure(error)),
    }
    // Fixtures and externally located target directories may span filesystems.
    // Reserve the destination exclusively and stream within the executable budget.
    use std::io::Read;
    let source = std::fs::File::open(source).map_err(failure)?;
    let metadata = source.metadata().map_err(failure)?;
    if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 * 1024 {
        return Err(failure(
            "Rustdoc observer source is not a bounded executable",
        ));
    }
    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(failure)?;
    let copied =
        std::io::copy(&mut source.take(metadata.len() + 1), &mut destination).map_err(failure)?;
    if copied != metadata.len() {
        return Err(failure("Rustdoc observer source changed while copying"));
    }
    destination
        .set_permissions(metadata.permissions())
        .map_err(failure)?;
    destination.sync_all().map_err(failure)
}

fn admit_completion(
    complete: &Value,
    target: &Target,
    context: &str,
    started: &Value,
    run: &Value,
) -> Result<u64, PhaseFailure> {
    let execution = &complete["test_execution"];
    if complete["schema"] != "terlan.rustdoc-completion.v1"
        || complete["context_identity"] != context
        || complete["target"] != serde_json::to_value(target).map_err(failure)?
        || complete["launches"] != json!([started, run])
        || complete["decision"] != "pass"
        || started["role"] != "rustdoc-observer"
        || run["role"] != "rustdoc"
        || started["pid"] == run["pid"]
        || ![started, run].iter().all(|row| {
            row["pid"]
                .as_u64()
                .is_some_and(|pid| pid > 0 && pid <= u32::MAX.into())
        })
        || execution["scope"] != "rustdoc-emitted-harness-records-v1"
        || execution["independent_inventory"] != false
    {
        return Err(failure(
            "Rustdoc completion differs from its admitted owner or evidence scope",
        ));
    }
    let harnesses = execution["harnesses"]
        .as_array()
        .filter(|rows| !rows.is_empty() && rows.len() <= 1024)
        .ok_or_else(|| failure("missing or excessive Rustdoc harness completions"))?;
    for key in ["passed", "ignored"] {
        let expected = execution[key]
            .as_u64()
            .filter(|count| *count <= 100_000)
            .ok_or_else(|| failure("invalid Rustdoc completed-test count"))?;
        let actual = harnesses
            .iter()
            .try_fold(0_u64, |sum, row| {
                row[key].as_u64().and_then(|count| sum.checked_add(count))
            })
            .ok_or_else(|| failure("invalid Rustdoc harness count"))?;
        if actual != expected {
            return Err(failure("Rustdoc harness counts disagree with completion"));
        }
    }
    Ok(execution["passed"]
        .as_u64()
        .expect("validated completed-test count"))
}

#[cfg(test)]
#[path = "rustdoc_owner_test.rs"]
mod tests;
