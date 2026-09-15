//! Cargo package-context execution with one authorized owner per native test target.

use crate::cargo_artifact_stream::CargoArtifactStream;
use crate::executable_binding::ExecutableBinding;
use crate::execution_environment::ExecutionEnvironment;
use crate::test_execution::TestEvidence;
use crate::workspace_native_storage::{executable_key, failure, read_json, write_new, Registry};
use crate::{PhaseFailure, TestPhase};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use terlan_process_owner::ProcessControl;

struct ExpectedOwner {
    binding: ExecutableBinding,
    certificate_identity: String,
    delegated: bool,
}

/// Preserves Cargo's native execution context; doctests have a separate unmodified Cargo owner.
pub(super) fn run(
    phase: &TestPhase,
    tools: (&Path, &Path, &BTreeMap<String, PathBuf>, &Value),
    environment: &ExecutionEnvironment,
    threads: usize,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
    partial: &mut Option<TestEvidence>,
) -> Result<TestEvidence, PhaseFailure> {
    let mut registry = Registry::create(environment)?;
    let result = run_registered(
        phase,
        tools,
        environment,
        threads,
        control,
        launched,
        &mut registry,
    );
    if result.is_err() {
        *partial = Some(TestEvidence(registry.failure_evidence()));
        return result;
    }
    let evidence = result?;
    if let Err(error) = registry.close() {
        *partial = Some(evidence);
        return Err(error);
    }
    Ok(evidence)
}

fn run_registered(
    phase: &TestPhase,
    tools: (&Path, &Path, &BTreeMap<String, PathBuf>, &Value),
    environment: &ExecutionEnvironment,
    threads: usize,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
    registry: &mut Registry,
) -> Result<TestEvidence, PhaseFailure> {
    let root = home::env::Env::current_dir(environment).map_err(failure)?;
    let directory = registry.path().to_path_buf();
    let context = json!({"schema":"terlan.workspace-native-context.v1", "root":root,
        "threads":threads, "timeout_seconds":crate::phase_timeout(environment).as_secs(), "main_harness":tools.3});
    write_new(&directory.join("context.json"), &context)?;
    let (_, context_identity) = read_json(&directory.join("context.json"), control)?;
    let runner = [
        tools.1.as_os_str(),
        std::ffi::OsStr::new("--cargo-native-runner"),
        directory.as_os_str(),
    ]
    .into_iter()
    .map(|value| {
        value
            .to_str()
            .ok_or_else(|| failure("Cargo runner paths must be UTF-8"))
    })
    .collect::<Result<Vec<_>, _>>()?;
    let mut command = environment.test_command(tools.0);
    command
        .args(&phase.args)
        .arg("--config")
        .arg(format!(
            "target.'cfg(all())'.runner={}",
            serde_json::to_string(&runner).map_err(failure)?
        ))
        .args([
            "--",
            "--test-threads",
            &threads.to_string(),
            "--quiet",
            "--color",
            "never",
        ])
        .envs(phase.environment.iter().map(|(key, value)| (key, value)));
    let mut artifacts = CargoArtifactStream::new(&root, |message| {
        message["profile"]["test"] == true && message["executable"].is_string()
    })?;
    artifacts.require_fresh_main();
    let mut owners = BTreeMap::new();
    let mut executable_bytes = 0_u64;
    let mut delegated_count = 0;
    artifacts.capture(&mut command, control, &mut |pid| {
        launched(pid)?;
        write_new(&directory.join("cargo.json"), &json!({"pid":pid})).map_err(|error| error.detail)
    }, |declaration| {
        crate::cargo_metadata_owner::Handoff::admit_harness(tools.2, &declaration.json())?;
        let delegated = main_owner(&declaration.json(), tools.3)?;
        delegated_count += usize::from(delegated);
        let key = executable_key(&declaration.executable)?;
        registry.register_target(&key)?;
        declaration.verify(control)?;
        let binding = ExecutableBinding::capture(&[("workspace-harness", declaration.executable.clone())], control)?;
        executable_bytes += binding.json()["before"][0]["bytes"].as_u64().ok_or_else(|| failure("missing executable byte count"))?;
        if executable_bytes > 8 * 1024 * 1024 * 1024 {
            return Err(failure("workspace test executables exceed their aggregate byte budget"));
        }
        let certificate = directory.join(format!("{key}.certificate.json"));
        write_new(&certificate, &json!({"schema":"terlan.workspace-native-certificate.v1", "declaration":declaration, "binding":binding.json(), "delegated_main":delegated}))?;
        let (_, certificate_identity) = read_json(&certificate, control)?;
        if owners.insert(key, ExpectedOwner { binding, certificate_identity, delegated }).is_some() {
            return Err(failure("duplicate workspace executable authorization"));
        }
        Ok(())
    })?;
    let declarations = artifacts.finish()?;
    if declarations.is_empty() || declarations.len() != owners.len() || delegated_count != 1 {
        return Err(failure(
            "workspace build has missing or unmatched native targets",
        ));
    }
    if read_json(&directory.join("context.json"), control)?.0 != context {
        return Err(failure("workspace execution context changed"));
    }
    let mut records = Vec::new();
    let mut total_passed = 0_u64;
    let mut nested_process_launch_count = 0;
    for declaration in declarations {
        declaration.verify(control)?;
        let key = executable_key(&declaration.executable)?;
        let mut owner = owners
            .remove(&key)
            .ok_or_else(|| failure("unregistered workspace target"))?;
        owner.binding.verify(control)?;
        let (completion, _) = read_json(&directory.join(format!("{key}.complete.json")), control)?;
        let mut launches = Vec::new();
        let roles: &[&str] = if owner.delegated {
            &["started"]
        } else {
            &["started", "all", "ignored", "run"]
        };
        for role in roles {
            let (record, _) = read_json(&directory.join(format!("{key}.{role}.json")), control)?;
            if record["pid"]
                .as_u64()
                .is_none_or(|pid| pid == 0 || pid > u32::MAX.into())
            {
                return Err(failure("workspace completion has no valid launch record"));
            }
            launches.push(json!({"role":role, "pid":record["pid"]}));
        }
        if owner.delegated {
            validate_delegated_completion(
                &completion,
                &owner.certificate_identity,
                &context_identity,
                &launches,
            )?;
        } else {
            total_passed += validate_completion(
                &completion,
                &owner.certificate_identity,
                &context_identity,
                &launches,
            )?;
        }
        nested_process_launch_count += launches.len();
        records.push(json!({"target":declaration.json(), "executable_binding":owner.binding.json(), "completion":completion, "delegated_main":owner.delegated}));
    }
    if total_passed == 0 {
        return Err(failure("workspace native owner executed no tests"));
    }
    Ok(TestEvidence(
        json!({"scope":"cargo-native-libtest-records-v1", "passed":total_passed,
        "nested_process_launch_count":nested_process_launch_count, "delegated_main_harnesses":delegated_count, "targets":records}),
    ))
}

/// Only the exact previously admitted main library belongs to the direct phase owner.
pub(super) fn main_owner(declaration: &Value, main: &Value) -> Result<bool, PhaseFailure> {
    if main["scope"] != "declared-cargo-libtest-target-v1"
        || main["package"] != "terlan"
        || main["kind"] != "lib"
    {
        return Err(failure(
            "workspace requires the admitted main library owner",
        ));
    }
    if declaration["package"] == "terlan" && declaration["kind"] == "lib" {
        if declaration != main {
            return Err(failure(
                "workspace rebuilt a different main library harness",
            ));
        }
        return Ok(true);
    }
    if declaration["executable"] == main["executable"] {
        return Err(failure("non-library target aliases the main harness"));
    }
    Ok(false)
}

fn validate_delegated_completion(
    completion: &Value,
    certificate: &str,
    context: &str,
    launches: &[Value],
) -> Result<(), PhaseFailure> {
    if completion["schema"] != "terlan.workspace-native-delegation.v1"
        || completion["decision"] != "delegated"
        || completion["certificate_identity"] != certificate
        || completion["context_identity"] != context
        || completion["owner"] != "main-library-phases"
        || !completion["test_execution"].is_null()
        || completion["launches"].as_array().map(Vec::as_slice) != Some(launches)
        || launches.len() != 1
        || launches[0]["role"] != "started"
    {
        return Err(failure(
            "main library delegation does not match its admitted owner",
        ));
    }
    Ok(())
}

/// Reconciles retained names with the target's private completion and launch records.
pub(super) fn validate_completion(
    completion: &Value,
    certificate: &str,
    context: &str,
    launches: &[Value],
) -> Result<u64, PhaseFailure> {
    let expected = crate::test_execution::ExpectedTests::native_names(&completion["passed_names"])?;
    if completion["schema"] != "terlan.workspace-native-completion.v1"
        || completion["decision"] != "pass"
        || completion["certificate_identity"] != certificate
        || completion["context_identity"] != context
        || completion["test_execution"]["scope"] != "admitted-libtest-records-v1"
        || completion["test_execution"]["ignored"] != 0
        || completion["test_execution"]["filtered"] != 0
        || completion["test_execution"]["passed"] != expected.passed.len()
        || completion["test_execution"]["selection_identity_sha256"] != expected.identity()
        || completion["launches"].as_array().map(Vec::as_slice) != Some(launches)
    {
        return Err(failure(
            "workspace completion does not match admitted inputs and actual launches",
        ));
    }
    completion["test_execution"]["passed"]
        .as_u64()
        .ok_or_else(|| failure("workspace completion has no passed count"))
}

#[cfg(test)]
#[path = "workspace_native_test.rs"]
mod tests;
