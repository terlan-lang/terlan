//! Live debugger admission for compiler-generated native images.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::native_image::debug::{
    inspect_tvm_native_debug, tvm_debug_source_sha256, TvmNativeDebugRecord,
};
use crate::runtime::native_image::{host_tvm_target, inspect_tvm_image};
use crate::runtime::vm::fixed_scheduler_telemetry::VM_FIXED_SCHEDULER_TRACE_CAPACITY;
use crate::runtime::vm::multicore_replay::{
    VmMulticoreEventContext, VmMulticoreEventKind, VmMulticoreReplayEvidence,
    VmMulticoreReplayRecorder,
};
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::scheduler_topology::VmSchedulerId;

use super::execution::execute_debug_script;
use super::interactive_session::execute_interactive_debug_session;
use super::script::DebugScriptCommand;
use super::{DebugArgs, DebugCliError};

/// One command-line breakpoint resolved against embedded compiler metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DebugBreakpointResolution {
    /// Original user-provided breakpoint expression.
    pub(super) spec: String,
    /// Native functions whose source identity matches the expression.
    pub(super) functions: Vec<String>,
}

/// Admitted native-image state exposed by the VM debugger command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeDebugSessionReport {
    /// Native image path admitted by the execution shard.
    pub(super) target: String,
    /// Parsed executable object format.
    pub(super) format: String,
    /// Parsed executable architecture.
    pub(super) architecture: String,
    /// Descriptor target triple admitted for this host.
    pub(super) target_triple: String,
    /// Compiler identity embedded in the descriptor.
    pub(super) compiler: String,
    /// Deterministic build identity embedded in the descriptor.
    pub(super) build: String,
    /// Package identity embedded in the descriptor.
    pub(super) package: String,
    /// Application-image module identity embedded in the descriptor.
    pub(super) module: String,
    /// Hexadecimal digest of the exact admitted descriptor.
    pub(super) descriptor_digest: String,
    /// Native export names available to debugger launch selection.
    pub(super) exports: Vec<String>,
    /// Compiler-generated continuation identities available for stack recovery.
    pub(super) continuation_ids: Vec<u64>,
    /// Number of decoded native source records.
    pub(super) source_record_count: usize,
    /// Command-line breakpoints resolved against native source records.
    pub(super) breakpoints: Vec<DebugBreakpointResolution>,
    /// Number of syntactically validated debugger script commands.
    pub(super) script_commands: Option<usize>,
    /// Scheduler-owned execution mode after applying scripted control commands.
    pub(super) execution_state: String,
    /// Ordered scheduler-control events accepted by the VM debugger.
    pub(super) control_events: Vec<String>,
    /// Whether a script entered the live AOT execution shard.
    pub(super) live_execution: bool,
    /// Stable source-style rendering of the completed entry value.
    pub(super) result: Option<String>,
    /// VM-owned process rows captured by the script.
    pub(super) process_snapshots: Vec<String>,
    /// VM-owned resource rows captured by the script.
    pub(super) resource_snapshots: Vec<String>,
    /// VM-owned timer rows captured by the script.
    pub(super) timer_snapshots: Vec<String>,
    /// Bounded VM mailbox rows captured without consuming messages.
    pub(super) mailbox_snapshots: Vec<String>,
    /// Whether machine-readable debugger event output was requested.
    pub(super) json_events: bool,
    /// Bounded scheduler evidence for the admitted debugger generation.
    pub(super) multicore_replay: VmMulticoreReplayEvidence,
}

/// Admits one `.tvm` image and resolves its debugger metadata without executing code.
pub(super) fn open_native_debug_session(
    args: &DebugArgs,
    script_commands: Option<&[DebugScriptCommand]>,
) -> Result<NativeDebugSessionReport, DebugCliError> {
    open_native_debug_session_mode(args, script_commands, false)
}

/// Opens a live command-at-a-time break loop for the public CLI.
pub(super) fn open_interactive_native_debug_session(
    args: &DebugArgs,
) -> Result<NativeDebugSessionReport, DebugCliError> {
    open_native_debug_session_mode(args, None, true)
}

fn open_native_debug_session_mode(
    args: &DebugArgs,
    script_commands: Option<&[DebugScriptCommand]>,
    interactive: bool,
) -> Result<NativeDebugSessionReport, DebugCliError> {
    let prepared_target = prepare_debug_target(args)?;
    let args = &prepared_target.args;
    let target = args.target.as_ref().ok_or_else(|| DebugCliError {
        code: "debug_missing_native_image",
        message: "terlc debug requires a compiler-generated .tvm image".to_string(),
    })?;
    if target.extension().and_then(|value| value.to_str()) != Some("tvm") {
        return Err(DebugCliError {
            code: "debug_target_not_native_image",
            message: format!(
                "debug target `{}` is not a compiler-generated .tvm image",
                target.display()
            ),
        });
    }

    let bytes = fs::read(target).map_err(|error| DebugCliError {
        code: "debug_image_read_failed",
        message: format!(
            "failed to read native image `{}`: {error}",
            target.display()
        ),
    })?;
    let expected_target = host_tvm_target().map_err(native_admission_error)?;
    let inspection =
        inspect_tvm_image(&bytes, &expected_target.triple).map_err(native_admission_error)?;
    let source_records = inspect_tvm_native_debug(&bytes).map_err(native_admission_error)?;
    validate_source_records(&source_records)?;
    let breakpoints = resolve_breakpoints(&args.breakpoints, &source_records)?;

    let mut shard = PureNativeExecutionShard::load_image(target).map_err(native_admission_error)?;
    let runtime_generation = shard.generation().map_err(native_admission_error)?.as_u64();
    let mut replay = VmMulticoreReplayRecorder::recording(
        VmSchedulerId::primary(),
        VM_FIXED_SCHEDULER_TRACE_CAPACITY,
    )
    .map_err(|error| native_admission_error(error.to_string()))?;
    let context = VmMulticoreEventContext::scheduler()
        .with_shard_epoch(runtime_generation)
        .map_err(|error| native_admission_error(error.to_string()))?;
    replay
        .observe(VmMulticoreEventKind::ImageGeneration, context)
        .map_err(|error| native_admission_error(error.to_string()))?;
    let multicore_replay = VmMulticoreReplayEvidence::new(
        runtime_generation,
        1,
        VM_FIXED_SCHEDULER_TRACE_CAPACITY,
        vec![replay
            .capture()
            .map_err(|error| native_admission_error(error.to_string()))?],
    )
    .map_err(|error| native_admission_error(error.to_string()))?;
    let execution = match if interactive {
        execute_interactive_debug_session(
            &mut shard,
            &source_records,
            &breakpoints,
            args.json_events,
            None,
        )
    } else {
        execute_debug_script(
            &mut shard,
            &source_records,
            &breakpoints,
            script_commands,
            None,
        )
    } {
        Ok(execution) => execution,
        Err(error) => {
            let _ = shard.shutdown();
            return Err(error);
        }
    };
    shard.shutdown().map_err(native_admission_error)?;

    Ok(NativeDebugSessionReport {
        target: target.display().to_string(),
        format: inspection.format.to_string(),
        architecture: inspection.architecture,
        target_triple: inspection.descriptor.target.triple,
        compiler: inspection.descriptor.identity.compiler,
        build: inspection.descriptor.identity.build,
        package: inspection.descriptor.identity.package,
        module: inspection.descriptor.identity.module,
        descriptor_digest: hex_digest(&inspection.descriptor_digest),
        exports: inspection
            .descriptor
            .exports
            .into_iter()
            .map(|export| export.name)
            .collect(),
        continuation_ids: inspection
            .descriptor
            .continuations
            .into_iter()
            .map(|continuation| continuation.id)
            .collect(),
        source_record_count: source_records.len(),
        breakpoints,
        script_commands: script_commands.map(<[DebugScriptCommand]>::len),
        execution_state: execution.execution_state,
        control_events: execution.events,
        live_execution: execution.live_execution,
        result: execution.result,
        process_snapshots: execution.process_snapshots,
        resource_snapshots: execution.resource_snapshots,
        timer_snapshots: execution.timer_snapshots,
        mailbox_snapshots: execution.mailbox_snapshots,
        json_events: args.json_events,
        multicore_replay,
    })
}

struct PreparedDebugTarget {
    args: DebugArgs,
    temporary_build: Option<std::path::PathBuf>,
}

impl Drop for PreparedDebugTarget {
    fn drop(&mut self) {
        if let Some(path) = &self.temporary_build {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn prepare_debug_target(args: &DebugArgs) -> Result<PreparedDebugTarget, DebugCliError> {
    let Some(target) = args.target.as_ref() else {
        return Ok(PreparedDebugTarget {
            args: args.clone(),
            temporary_build: None,
        });
    };
    if target.extension().and_then(|value| value.to_str()) == Some("tvm") {
        return Ok(PreparedDebugTarget {
            args: args.clone(),
            temporary_build: None,
        });
    }

    static NEXT_DEBUG_BUILD: AtomicU64 = AtomicU64::new(1);
    let sequence = NEXT_DEBUG_BUILD.fetch_add(1, Ordering::Relaxed);
    let temporary_build = std::env::temp_dir().join(format!(
        "terlan-debug-build-{}-{sequence}",
        std::process::id()
    ));
    let state = crate::CliState {
        out_dir: temporary_build.clone(),
        ..crate::CliState::default()
    };
    let status = crate::commands::build::run(
        crate::CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                target.display().to_string(),
                "--target".to_string(),
                "terlan-vm".to_string(),
            ],
        },
        state,
    );
    if status != std::process::ExitCode::SUCCESS {
        let _ = fs::remove_dir_all(&temporary_build);
        return Err(DebugCliError {
            code: "debug_project_build_failed",
            message: format!(
                "failed to build debugger target `{}` as a Terlan VM image",
                target.display()
            ),
        });
    }
    let mut images = fs::read_dir(temporary_build.join("vm"))
        .map_err(|error| DebugCliError {
            code: "debug_project_image_missing",
            message: format!(
                "debugger build for `{}` produced no VM image directory: {error}",
                target.display()
            ),
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("tvm"))
        .collect::<Vec<_>>();
    images.sort();
    let [image] = images.as_slice() else {
        let count = images.len();
        let _ = fs::remove_dir_all(&temporary_build);
        return Err(DebugCliError {
            code: "debug_project_image_ambiguous",
            message: format!(
                "debugger build for `{}` produced {count} executable VM images; expected one application image",
                target.display()
            ),
        });
    };
    let mut prepared_args = args.clone();
    prepared_args.target = Some(image.clone());
    Ok(PreparedDebugTarget {
        args: prepared_args,
        temporary_build: Some(temporary_build),
    })
}

fn validate_source_records(records: &[TvmNativeDebugRecord]) -> Result<(), DebugCliError> {
    for record in records {
        let Ok(source) = fs::read_to_string(&record.source_file) else {
            continue;
        };
        if source.get(record.span_start..record.span_end).is_none() {
            return Err(stale_source_map(record));
        }
        if record.continuation_spans.iter().any(|continuation| {
            !record.continuation_ids.contains(&continuation.id)
                || source
                    .get(continuation.span_start..continuation.span_end)
                    .is_none()
        }) {
            return Err(stale_source_map(record));
        }
        if tvm_debug_source_sha256(source.as_bytes()) != record.source_sha256 {
            return Err(stale_source_map(record));
        }
    }
    Ok(())
}

fn stale_source_map(record: &TvmNativeDebugRecord) -> DebugCliError {
    DebugCliError {
        code: "debug_source_map_stale",
        message: format!(
            "embedded source map for `{}.{}/{}` no longer matches `{}`; rebuild the native image",
            record.module, record.function, record.arity, record.source_file
        ),
    }
}

/// Converts a native-image admission failure into a stable debugger diagnostic.
fn native_admission_error(message: impl ToString) -> DebugCliError {
    DebugCliError {
        code: "debug_native_image_rejected",
        message: message.to_string(),
    }
}

/// Resolves all requested breakpoint expressions against embedded source records.
fn resolve_breakpoints(
    specs: &[String],
    records: &[TvmNativeDebugRecord],
) -> Result<Vec<DebugBreakpointResolution>, DebugCliError> {
    specs
        .iter()
        .map(|spec| resolve_breakpoint(spec, records))
        .collect()
}

/// Resolves one module/function or file/line breakpoint.
pub(super) fn resolve_breakpoint(
    spec: &str,
    records: &[TvmNativeDebugRecord],
) -> Result<DebugBreakpointResolution, DebugCliError> {
    let base = spec
        .split_once(" where ")
        .map_or(spec, |(breakpoint, _)| breakpoint)
        .trim();
    let matching = if let Some((path, line)) = parse_file_line(base) {
        records
            .iter()
            .filter(|record| record_covers_line(record, Path::new(path), line))
            .collect::<Vec<_>>()
    } else {
        records
            .iter()
            .filter(|record| format!("{}.{}", record.module, record.function) == base)
            .collect::<Vec<_>>()
    };
    if matching.is_empty() {
        return Err(DebugCliError {
            code: "debug_breakpoint_unresolved",
            message: format!("breakpoint `{spec}` does not resolve in the admitted native image"),
        });
    }
    Ok(DebugBreakpointResolution {
        spec: spec.to_string(),
        functions: matching.into_iter().map(function_identity).collect(),
    })
}

/// Parses a validated breakpoint base as `file:line` when applicable.
fn parse_file_line(value: &str) -> Option<(&str, usize)> {
    let (path, line) = value.rsplit_once(':')?;
    line.parse::<usize>().ok().map(|line| (path, line))
}

/// Returns whether one source record covers a requested one-based source line.
fn record_covers_line(record: &TvmNativeDebugRecord, path: &Path, line: usize) -> bool {
    if !source_path_matches(Path::new(&record.source_file), path) {
        return false;
    }
    let Ok(source) = fs::read_to_string(&record.source_file) else {
        return false;
    };
    let Some((start, end)) = source_line_span(&source, record.span_start, record.span_end) else {
        return false;
    };
    (start..=end).contains(&line)
}

/// Compares exact or suffix-equivalent source paths for workspace-relative breakpoints.
fn source_path_matches(record: &Path, requested: &Path) -> bool {
    record == requested || record.ends_with(requested) || requested.ends_with(record)
}

/// Converts a checked byte span to an inclusive one-based source-line span.
fn source_line_span(source: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if start >= end
        || end > source.len()
        || !source.is_char_boundary(start)
        || !source.is_char_boundary(end)
    {
        return None;
    }
    let start_line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let end_prefix = &source[..end];
    let newline_count = end_prefix.bytes().filter(|byte| *byte == b'\n').count();
    let end_line = if end_prefix.ends_with('\n') {
        newline_count.max(1)
    } else {
        newline_count + 1
    };
    Some((start_line, end_line))
}

/// Renders one stable function/source identity for debugger output.
pub(super) fn function_identity(record: &TvmNativeDebugRecord) -> String {
    format!(
        "{}.{}/{}@{}:{}..{}",
        record.module,
        record.function,
        record.arity,
        record.source_file,
        record.span_start,
        record.span_end
    )
}

/// Renders a fixed-size binary digest as lowercase hexadecimal text.
fn hex_digest(digest: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
#[path = "session_test.rs"]
#[cfg(test)]
mod session_test;
