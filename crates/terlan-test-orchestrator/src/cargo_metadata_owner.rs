//! One bounded metadata producer; observations are not reusable build receipts.

use crate::executable_binding::{resolve_program, ExecutableBinding};
use crate::execution_environment::{identity, ExecutionEnvironment};
use crate::report_file::ReportFile;
use crate::source_inventory::{capture_with_git, SourceBinding};
use crate::tool_configuration::ToolConfiguration;
use crate::{process_failure, PhaseFailure};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;
use terlan_process_owner::ProcessControl;

const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_OBSERVATION_BYTES: u64 = 32 * 1024 * 1024;

mod handoff;
mod package_sources;
mod resolver_cache;
pub(super) use handoff::Handoff;

fn command(environment: &ExecutionEnvironment, cargo: &Path) -> std::process::Command {
    let mut command = environment.command(cargo);
    for key in [
        "MAKEFLAGS",
        "MFLAGS",
        "CARGO_MAKEFLAGS",
        "TERLAN_RUST_SUITE_REPORT",
        "TERLAN_RELEASE_COVERAGE_OWNS_TERLC_TESTS",
        "TERLAN_TEST_THREADS",
        "TERLAN_TEST_PHASE_TIMEOUT_SECONDS",
        "SHLVL",
        "_",
    ] {
        command.env_remove(key);
    }
    command
}

/// Runs only metadata production, never the correctness suite or a build.
pub(super) fn main(mut arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let result = (|| {
        let output = arguments
            .next()
            .ok_or_else(|| failure("missing metadata output path"))?;
        if arguments.next().as_deref() != Some(OsStr::new("--")) {
            return Err(failure("expected -- before the Cargo invocation"));
        }
        let cargo = arguments.collect::<Vec<_>>();
        let environment = ExecutionEnvironment::capture()?;
        let shutdown = crate::shutdown::Shutdown::install().map_err(failure)?;
        produce(
            Path::new(&output),
            &cargo,
            &environment,
            ProcessControl::new(Duration::from_secs(120)).with_cancellation(shutdown.flag()),
        )
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[cargo-metadata] {}: {}", error.outcome, error.detail);
            ExitCode::FAILURE
        }
    }
}

/// Publishes only a successful, shape-checked, source/configuration-stable query.
pub(super) fn produce(
    output: &Path,
    cargo: &[OsString],
    environment: &ExecutionEnvironment,
    control: ProcessControl<'_>,
) -> Result<(), PhaseFailure> {
    let (program, arguments) = cargo
        .split_first()
        .ok_or_else(|| failure("missing Cargo executable"))?;
    if arguments.iter().any(|argument| argument != "--locked") {
        return Err(failure(
            "metadata owner accepts only the Cargo executable and --locked prefix",
        ));
    }
    let root = home::env::Env::current_dir(environment).map_err(failure)?;
    let root = std::fs::canonicalize(root).map_err(failure)?;
    let mut report = ReportFile::open_bounded(output, MAX_OBSERVATION_BYTES).map_err(failure)?;
    let mut attempt = MetadataAttempt::open(output, report.run_id()).map_err(failure)?;
    let mut published_sha256 = None;
    let result = query(
        program,
        arguments,
        environment,
        control,
        &root,
        &mut attempt,
    )
    .and_then(|document| {
        let mut digest = Sha256::new();
        digest.update(&document);
        report.publish(&document).map_err(failure)?;
        published_sha256 = Some(crate::file_identity::hex(digest));
        Ok(())
    });
    attempt
        .finish(&result, published_sha256.as_deref())
        .map_err(failure)?;
    result
}

fn query(
    program: &OsStr,
    arguments: &[OsString],
    environment: &ExecutionEnvironment,
    control: ProcessControl<'_>,
    root: &Path,
    attempt: &mut MetadataAttempt,
) -> Result<Vec<u8>, PhaseFailure> {
    let search = environment.value("PATH").unwrap_or_default();
    let program = program
        .to_str()
        .ok_or_else(|| failure("Cargo path must be UTF-8"))?;
    let cargo = resolve_program(program, &search)?;
    let git = resolve_program("git", &search)?;
    let resolved = std::fs::canonicalize(&cargo).map_err(failure)?;
    let rustup_name = format!("rustup{}", std::env::consts::EXE_SUFFIX);
    let rustup = if resolved.file_name() == Some(OsStr::new(&rustup_name)) {
        Some(resolved)
    } else {
        resolve_program("rustup", &search).ok()
    };
    let mut paths = vec![("cargo", cargo.clone()), ("git", git.clone())];
    if let Some(path) = &rustup {
        paths.push(("rustup", path.clone()));
    }
    let mut tools = ExecutableBinding::capture(&paths, control)?;
    let mut configuration = ToolConfiguration::capture(environment, control)?;
    let mut resolver_cache = resolver_cache::Binding::capture(environment, control)?;
    let mut selected_cargo = if tools.same_identity("cargo", "rustup") {
        let rustup = rustup.ok_or_else(|| failure("Rustup proxy has no admitted resolver"))?;
        let selected = crate::rust_toolchain::resolve_installed_tool(
            &mut environment.command(&rustup),
            "cargo",
            control,
            &mut |pid| attempt.launched("rustup-cargo-resolution", pid),
        )?;
        Some(ExecutableBinding::capture(
            &[("native-cargo", selected)],
            control,
        )?)
    } else {
        None
    };
    let mut before = |pid| attempt.launched("source-before", pid);
    let mut source = SourceBinding {
        before: Some(capture_with_git(
            root,
            &git,
            environment,
            control,
            &mut before,
        )?),
        after: None,
    };
    let mut command = command(environment, &cargo);
    command.args(arguments).args([
        "metadata",
        "--locked",
        "--all-features",
        "--format-version",
        "1",
    ]);
    let bytes = control
        .capture_stdout(&mut command, MAX_METADATA_BYTES, |pid| {
            attempt.launched("cargo-metadata", pid)
        })
        .map_err(process_failure)?;
    let mut document = admit(&bytes, root)?;
    source.after = Some(capture_with_git(
        root,
        &git,
        environment,
        control,
        &mut |pid| attempt.launched("source-after", pid),
    )?);
    if !source.verified() {
        return Err(failure("source inputs changed during Cargo metadata"));
    }
    configuration.verify(control)?;
    resolver_cache.verify(control)?;
    tools.verify(control)?;
    if let Some(native) = &mut selected_cargo {
        native.verify(control)?;
    }
    document["terlan_preparation"] = json!({
        "schema":"terlan.cargo-metadata-observation.v1", "run_id":attempt.run_id,
        "reusable":false, "source":source.json(), "configuration":configuration.json(),
        "executables":tools.json(), "resolver_cache":resolver_cache.json(),
        "environment_sha256":identity(&command), "launches":attempt.launches,
        "cargo_dispatch":{
            "kind":if selected_cargo.is_some() { "rustup-proxy" } else { "supplied-entry-point" },
            "native_cargo":selected_cargo.as_ref().map(ExecutableBinding::json),
            "probe_environment_overrides":{"RUSTUP_AUTO_INSTALL":"0"}
        },
        "query":["metadata", "--locked", "--all-features", "--format-version", "1"],
        "scope":"working-tree-selected-cargo-and-cargo-home-cache-observation; workspace discovery, vendored paths and wrapper inputs not yet completely bound"
    });
    serde_json::to_vec(&document).map_err(failure)
}

/// Latest attempt is separate from the last successful document. A running
/// observation never authorizes reuse, including after abrupt process death.
struct MetadataAttempt {
    report: ReportFile,
    run_id: String,
    launches: Vec<Value>,
}

impl MetadataAttempt {
    fn open(output: &Path, run_id: &str) -> Result<Self, String> {
        let mut path = output.as_os_str().to_os_string();
        path.push(".attempt.json");
        let mut attempt = Self {
            report: ReportFile::open(Path::new(&path))?,
            run_id: run_id.into(),
            launches: Vec::new(),
        };
        attempt.publish("running", None, None)?;
        Ok(attempt)
    }

    fn launched(&mut self, role: &str, pid: u32) -> Result<(), String> {
        self.launches.push(json!({"role":role,"pid":pid}));
        self.publish("running", None, None)
    }

    fn finish(
        &mut self,
        result: &Result<(), PhaseFailure>,
        output_sha256: Option<&str>,
    ) -> Result<(), String> {
        match result {
            Ok(()) => self.publish(
                "succeeded",
                None,
                Some(output_sha256.ok_or("successful metadata attempt has no output digest")?),
            ),
            Err(error) => self.publish(
                "failed",
                Some(json!({"outcome":error.outcome,"detail":error.detail})),
                None,
            ),
        }
    }

    fn publish(
        &mut self,
        state: &str,
        error: Option<Value>,
        output_sha256: Option<&str>,
    ) -> Result<(), String> {
        let bytes = serde_json::to_vec(&json!({
            "schema":"terlan.cargo-metadata-attempt.v1", "run_id":self.run_id,
            "state":state, "reusable":false, "launches":self.launches, "error":error,
            "metadata_sha256":output_sha256,
            "scope":"direct launches only; not a reusable preparation receipt"
        }))
        .map_err(|error| error.to_string())?;
        self.report.publish(&bytes)
    }
}

fn admit(bytes: &[u8], root: &Path) -> Result<Value, PhaseFailure> {
    let value: Value = serde_json::from_slice(bytes).map_err(failure)?;
    if value["version"] != 1
        || value.get("terlan_preparation").is_some()
        || value["workspace_root"].as_str().map(Path::new) != Some(root)
    {
        return Err(failure(
            "Cargo metadata has a wrong version, workspace, or reserved observation field",
        ));
    }
    validate_packages(&value)?;
    Ok(value)
}

fn validate_packages(value: &Value) -> Result<(), PhaseFailure> {
    let members = value["workspace_members"]
        .as_array()
        .ok_or_else(|| failure("missing workspace members"))?;
    let members = members
        .iter()
        .map(|member| {
            member
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| failure("invalid workspace member ID"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expected = members.iter().copied().collect::<BTreeSet<_>>();
    if members.is_empty() || members.len() > 256 || expected.len() != members.len() {
        return Err(failure("empty, repeated, or excessive workspace members"));
    }
    let packages = value["packages"]
        .as_array()
        .filter(|items| items.len() <= 16_384)
        .ok_or_else(|| failure("missing or excessive Cargo packages"))?;
    let mut ids = BTreeSet::new();
    for package in packages {
        let id = package["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| failure("invalid Cargo package ID"))?;
        if !ids.insert(id) {
            return Err(failure("duplicate Cargo package ID"));
        }
    }
    if !expected.is_subset(&ids) {
        return Err(failure("Cargo metadata omitted a workspace member"));
    }
    Ok(())
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "cargo-metadata-owner-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "cargo_metadata_owner_test.rs"]
mod tests;
