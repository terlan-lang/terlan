use super::*;

/// Resolves a benchmark binary path.
///
/// Inputs:
/// - `binary`: Cargo binary name.
/// - `explicit`: optional caller-supplied path.
///
/// Output:
/// - Path to an executable local binary.
///
/// Transformation:
/// - Prefers explicit configuration, otherwise builds the requested binary
///   through Cargo before returning the sibling path.
pub(super) fn resolve_benchmark_binary(
    binary: &str,
    explicit: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return require_existing_binary(path);
    }
    let current = env::current_exe()
        .map_err(|error| format!("failed to resolve current benchmark binary: {error}"))?;
    let parent = current.parent().ok_or_else(|| {
        format!(
            "failed to resolve parent directory for benchmark binary `{}`",
            current.display()
        )
    })?;
    let sibling = parent.join(platform_binary_name(binary));
    build_cargo_binary(binary)?;
    require_existing_binary(&sibling)
}

/// Returns the platform-specific binary filename.
pub(super) fn platform_binary_name(binary: &str) -> String {
    if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_string()
    }
}

/// Ensures a configured binary path exists.
pub(super) fn require_existing_binary(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        Ok(path.to_path_buf())
    } else {
        Err(format!(
            "benchmark binary `{}` does not exist",
            path.display()
        ))
    }
}

/// Builds one local Cargo binary needed by the benchmark.
pub(super) fn build_cargo_binary(binary: &str) -> Result<(), String> {
    let output = Command::new("cargo")
        .args(["build", "-p", "terlan", "--bin", binary, "--quiet"])
        .output()
        .map_err(|error| format!("failed to start cargo build for `{binary}`: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format_command_failure("cargo build", &output))
    }
}

/// Creates a unique temporary workspace for VM benchmark sources.
pub(super) fn create_vm_benchmark_workspace() -> Result<PathBuf, String> {
    let path = env::temp_dir().join(format!(
        "terlan-vm-performance-baseline-{}-{}",
        std::process::id(),
        unix_timestamp_nanos()
    ));
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create VM benchmark workspace: {error}"))?;
    Ok(path)
}

/// Writes one Terlan benchmark source into the workspace.
pub(super) fn write_vm_benchmark_source(
    workspace: &Path,
    name: &str,
    source: &str,
) -> Result<PathBuf, String> {
    let path = workspace.join(name);
    fs::write(&path, source).map_err(|error| {
        format!(
            "failed to write VM benchmark source `{}`: {error}",
            path.display()
        )
    })?;
    Ok(path)
}

/// Measures single-file VM artifact build output.
///
/// Inputs:
/// - `compiler_binary`: resolved local `terlc` binary.
/// - `source`: Terlan source file to build.
///
/// Output:
/// - Success when one non-empty native `.tvm` application image is emitted.
///
/// Transformation:
/// - Runs `terlc build --target terlan-vm` in an isolated output directory and
///   validates the produced artifact envelope without loading it.
pub(super) fn measure_vm_artifact_build_single_file(
    compiler_binary: &Path,
    source: &Path,
) -> Result<(), String> {
    build_single_file_vm_artifact(compiler_binary, source).map(|_| ())
}

/// Measures single-file VM artifact loading.
///
/// Inputs:
/// - `vm_binary`: resolved local `terlan-vm` binary.
/// - `compiler_binary`: resolved local `terlc` binary.
/// - `source`: Terlan source file to build before loading.
///
/// Output:
/// - Success when `terlan-vm load` admits the compiler-emitted native image.
///
/// Transformation:
/// - Builds a fresh artifact, validates its envelope, and then routes it
///   through the standalone VM loader command.
pub(super) fn measure_vm_artifact_load_single_file(
    vm_binary: &Path,
    compiler_binary: &Path,
    source: &Path,
) -> Result<(), String> {
    let artifact = build_single_file_vm_artifact(compiler_binary, source)?;
    let artifact_arg = artifact.to_string_lossy().to_string();
    let output = run_required_command(vm_binary, &["load", &artifact_arg])?;
    require_stdout_contains(
        "vm_artifact_load_single_file",
        &output,
        "loaded native TVM image",
    )
}

/// Builds and validates one single-file VM artifact.
///
/// Inputs:
/// - `compiler_binary`: resolved local `terlc` binary.
/// - `source`: Terlan source file to build.
///
/// Output:
/// - Path to the emitted native `.tvm` application image.
///
/// Transformation:
/// - Runs `terlc build --target terlan-vm` in an isolated output directory and
///   validates the native image's expected path and non-empty publication.
pub(super) fn build_single_file_vm_artifact(
    compiler_binary: &Path,
    source: &Path,
) -> Result<PathBuf, String> {
    let out_dir = env::temp_dir().join(format!(
        "terlan-vm-artifact-build-{}-{}",
        std::process::id(),
        unix_timestamp_nanos()
    ));
    fs::create_dir_all(&out_dir)
        .map_err(|error| format!("failed to create VM artifact build output: {error}"))?;
    let out_dir_arg = out_dir.to_string_lossy().to_string();
    let source_arg = source.to_string_lossy().to_string();
    run_required_command(
        compiler_binary,
        &[
            "--out-dir",
            &out_dir_arg,
            "build",
            &source_arg,
            "--target",
            "terlan-vm",
        ],
    )?;

    let vm_dir = out_dir.join("vm");
    let artifacts = fs::read_dir(&vm_dir)
        .map_err(|error| format!("failed to read VM artifact directory: {error}"))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("failed to read VM artifact entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let artifacts = artifacts
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".tvm"))
        })
        .collect::<Vec<_>>();
    let [artifact_path] = artifacts.as_slice() else {
        return Err(format!(
            "expected one VM artifact under `{}`, found {}",
            vm_dir.display(),
            artifacts.len()
        ));
    };
    let metadata = fs::metadata(artifact_path).map_err(|error| {
        format!(
            "failed to inspect native TVM image `{}`: {error}",
            artifact_path.display()
        )
    })?;
    if metadata.len() == 0 {
        return Err(format!(
            "native TVM image `{}` is empty",
            artifact_path.display()
        ));
    }
    Ok(artifact_path.to_path_buf())
}

/// Measures VM-owned process-table primitives directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when process spawn, ordered message delivery, selective receive,
///   mailbox accounting, and exit cleanup behave correctly.
///
/// Transformation:
/// - Exercises local process identity and mailbox semantics without routing
///   through actor convenience APIs or source-level process syntax.
pub(super) fn measure_vm_process_runtime_primitives() -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let parent = processes.spawn_root(VmProcessSource::new("bench.Process", "parent", 0));
    let child = processes.spawn_child(parent, VmProcessSource::new("bench.Process", "child", 0))?;
    let first_id = processes.send(parent, child, VmPrimitiveValue::Atom("first".to_string()))?;
    let second_id = processes.send(parent, child, VmPrimitiveValue::Atom("second".to_string()))?;
    if second_id <= first_id {
        return Err(format!(
            "message ids were not monotonic: first={first_id}, second={second_id}"
        ));
    }

    processes.with_process_control_mutator(child, |child_process| -> Result<(), String> {
        let selected = child_process
            .selective_receive(|message| {
                message.payload == VmPrimitiveValue::Atom("second".to_string())
            })
            .ok_or_else(|| "selective receive did not find second message".to_string())?;
        if selected.id != second_id {
            return Err(format!(
                "selective receive returned message {}, expected {second_id}",
                selected.id
            ));
        }
        if child_process.mailbox_len() != 1 {
            return Err(format!(
                "skipped mailbox length was {}, expected 1",
                child_process.mailbox_len()
            ));
        }
        let remaining = child_process
            .receive_next()
            .ok_or_else(|| "remaining message was not preserved".to_string())?;
        if remaining.id != first_id {
            return Err(format!(
                "remaining message id was {}, expected {first_id}",
                remaining.id
            ));
        }
        child_process.add_resource_handle("native:process-benchmark");
        Ok(())
    })??;
    let cleanup = processes.exit_process(child, process::VmExitReason::Normal)?;
    if cleanup == ["native:process-benchmark".to_string()] {
        Ok(())
    } else {
        Err(format!("unexpected process cleanup handles: {cleanup:?}"))
    }
}

/// Measures VM-owned process inspection directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when immutable process inspection exposes source identity,
///   process relation, state, mailbox depth, reductions, and resources.
///
/// Transformation:
/// - Exercises the committed runtime inspection data without depending on a
///   CLI inspection command or OTP process metadata.
pub(super) fn measure_vm_process_inspection_startup() -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let parent = processes.spawn_root(VmProcessSource::new("bench.Inspect", "parent", 0));
    let child = processes.spawn_child(parent, VmProcessSource::new("bench.Inspect", "child", 1))?;
    processes.send(parent, child, VmPrimitiveValue::String("ready".to_string()))?;

    processes.with_process_control_mutator(child, |child_process| {
        child_process.charge_reductions(11);
        child_process.add_resource_handle("native:inspect-benchmark");
        child_process.block();
    })?;

    let inspected = processes
        .get(child)
        .ok_or_else(|| "child process missing for inspection".to_string())?;
    if inspected.pid != child {
        return Err(format!("inspected pid was {:?}", inspected.pid));
    }
    if inspected.parent != Some(parent) {
        return Err(format!("inspected parent was {:?}", inspected.parent));
    }
    if inspected.source.module != "bench.Inspect"
        || inspected.source.function != "child"
        || inspected.source.arity != 1
    {
        return Err(format!(
            "unexpected source metadata: {:?}",
            inspected.source
        ));
    }
    if inspected.state != process::VmProcessState::Blocked {
        return Err(format!("unexpected inspected state: {:?}", inspected.state));
    }
    if inspected.mailbox_len() != 1 {
        return Err(format!(
            "inspected mailbox depth was {}, expected 1",
            inspected.mailbox_len()
        ));
    }
    if inspected.reductions != 11 {
        return Err(format!(
            "inspected reductions were {}, expected 11",
            inspected.reductions
        ));
    }
    if inspected.resource_handles != ["native:inspect-benchmark".to_string()] {
        return Err(format!(
            "unexpected inspected resources: {:?}",
            inspected.resource_handles
        ));
    }
    Ok(())
}
