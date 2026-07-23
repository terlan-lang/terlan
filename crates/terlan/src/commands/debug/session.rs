//! Read-only debugger admission for compiler-generated native images.

use std::fs;
use std::path::Path;

use crate::runtime::native_image::debug::{inspect_tvm_native_debug, TvmNativeDebugRecord};
use crate::runtime::native_image::{host_tvm_target, inspect_tvm_image};
use crate::runtime::vm::fixed_scheduler_telemetry::VM_FIXED_SCHEDULER_TRACE_CAPACITY;
use crate::runtime::vm::multicore_replay::{
    VmMulticoreEventContext, VmMulticoreEventKind, VmMulticoreReplayEvidence,
    VmMulticoreReplayRecorder,
};
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::scheduler_topology::VmSchedulerId;

use super::{DebugArgs, DebugCliError};

/// One command-line breakpoint resolved against embedded compiler metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DebugBreakpointResolution {
    /// Original user-provided breakpoint expression.
    pub(super) spec: String,
    /// Native functions whose source identity matches the expression.
    pub(super) functions: Vec<String>,
}

/// Admitted native-image state exposed by the read-only debugger command.
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
    /// Whether machine-readable debugger event output was requested.
    pub(super) json_events: bool,
    /// Bounded scheduler evidence for the admitted debugger generation.
    pub(super) multicore_replay: VmMulticoreReplayEvidence,
}

/// Admits one `.tvm` image and resolves its debugger metadata without executing code.
pub(super) fn open_native_debug_session(
    args: &DebugArgs,
    script_commands: Option<usize>,
) -> Result<NativeDebugSessionReport, DebugCliError> {
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
        script_commands,
        json_events: args.json_events,
        multicore_replay,
    })
}

/// Converts a native-image admission failure into a stable debugger diagnostic.
fn native_admission_error(message: String) -> DebugCliError {
    DebugCliError {
        code: "debug_native_image_rejected",
        message,
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
fn resolve_breakpoint(
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
fn function_identity(record: &TvmNativeDebugRecord) -> String {
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
mod session_test;
