//! Validation and diagnostic helpers for pure-native execution transitions.

use crate::runtime::native_image::control::{
    tvm_fixed_capability_frame_words, TvmTransitionOperation, TVM_SQL_CAPABILITY_PREFIX_WORDS,
    TVM_SQL_CAPABILITY_TAG,
};
use crate::runtime::native_image::{TvmBoundaryType, TvmContinuationDescriptor};
use crate::runtime::vm::process::VmExitReason;
use crate::runtime::vm::ReplValue;
use crate::runtime::vm::VmRuntimeResult;
use crate::terlan_native_boundary::term::NativeBoundaryTerm;

use super::super::validate_continuation_captures;

/// Converts a descriptor-checked VM value into the recursively owned term
/// accepted by the capability boundary.
pub(super) fn repl_value_to_boundary_term(value: ReplValue) -> VmRuntimeResult<NativeBoundaryTerm> {
    match value {
        ReplValue::Unit => Ok(NativeBoundaryTerm::Unit),
        ReplValue::Int(value) => Ok(NativeBoundaryTerm::Int(value)),
        ReplValue::Float(value) => Ok(value
            .parse::<f64>()
            .map(NativeBoundaryTerm::Float)
            .map_err(|error| format!("error[pure_native_capability_argument]: {error}"))?),
        ReplValue::Bool(value) => Ok(NativeBoundaryTerm::Bool(value)),
        ReplValue::Atom(value) => Ok(NativeBoundaryTerm::Atom(value)),
        ReplValue::String(value) => Ok(NativeBoundaryTerm::Text(value)),
        ReplValue::StringBytes(value) => Ok(std::str::from_utf8(&value)
            .map(|value| NativeBoundaryTerm::Text(value.to_string()))
            .map_err(|error| format!("error[pure_native_capability_argument]: {error}"))?),
        ReplValue::Bytes(value) => Ok(NativeBoundaryTerm::Bytes(value.to_vec())),
        ReplValue::Record { name, fields } => Ok(NativeBoundaryTerm::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| repl_value_to_boundary_term(value).map(|value| (name, value)))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        ReplValue::List(values) => Ok(NativeBoundaryTerm::List(
            values
                .into_iter()
                .map(repl_value_to_boundary_term)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        unsupported => Err(format!(
            "error[pure_native_capability_argument]: unsupported managed value `{unsupported:?}`"
        )
        .into()),
    }
}

pub(super) fn native_actor_exit_error(owner_id: u64, reason: &VmExitReason) -> String {
    match reason {
        VmExitReason::Error(message) => {
            format!("error[pure_native_failure]: native actor {owner_id} failed: {message}")
        }
        other => format!(
            "error[pure_native_failure]: native actor {owner_id} exited before resume: {other:?}"
        ),
    }
}

pub(super) fn validate_transition_continuation(
    continuation: &TvmContinuationDescriptor,
    result_type: &TvmBoundaryType,
    operation: &TvmTransitionOperation,
    arguments: &[i64],
    values: &[i64],
) -> VmRuntimeResult<()> {
    if !matches!(
        operation,
        TvmTransitionOperation::Identity
            | TvmTransitionOperation::Receive
            | TvmTransitionOperation::Spawn
            | TvmTransitionOperation::Monitor
            | TvmTransitionOperation::Resource
            | TvmTransitionOperation::Capability
    ) {
        let _ = result_type;
        return Ok(validate_continuation_captures(continuation, values)?);
    }
    let injected_type =
        if matches!(operation, TvmTransitionOperation::Receive) && arguments.len() == 3 {
            TvmBoundaryType::from_transition_words(arguments)?
        } else if matches!(operation, TvmTransitionOperation::Capability) {
            TvmBoundaryType::from_transition_words(&arguments[1..4])?
        } else {
            TvmBoundaryType::Int
        };
    if continuation.parameters.first() != Some(&injected_type) {
        return Err(format!(
            "error[pure_native_continuation_type]: {operation:?} continuation {} must accept a {injected_type:?} result first", continuation.id
        ).into());
    }
    let mut captures = continuation.clone();
    captures.parameters.remove(0);
    let _ = result_type;
    Ok(validate_continuation_captures(&captures, values)?)
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn transition_capture_types(
    continuation: &TvmContinuationDescriptor,
    operation: &TvmTransitionOperation,
    arguments: &[i64],
) -> VmRuntimeResult<Vec<TvmBoundaryType>> {
    let injected = match operation {
        TvmTransitionOperation::Identity => Some(TvmBoundaryType::Int),
        TvmTransitionOperation::Receive if arguments.len() == 3 => {
            Some(TvmBoundaryType::from_transition_words(arguments)?)
        }
        TvmTransitionOperation::Receive
        | TvmTransitionOperation::Spawn
        | TvmTransitionOperation::Monitor
        | TvmTransitionOperation::Resource => Some(TvmBoundaryType::Int),
        TvmTransitionOperation::Capability => {
            Some(TvmBoundaryType::from_transition_words(&arguments[1..4])?)
        }
        _ => None,
    };
    let captures = if let Some(injected) = injected {
        if continuation.parameters.first() != Some(&injected) {
            return Err(format!(
                "error[pure_native_continuation_type]: {operation:?} continuation {} must accept a {injected:?} result first",
                continuation.id
            ).into());
        }
        &continuation.parameters[1..]
    } else {
        continuation.parameters.as_slice()
    };
    Ok(captures.to_vec())
}

pub(super) fn validate_capability_arguments(arguments: &[i64]) -> VmRuntimeResult<()> {
    if arguments.first() == Some(&7) {
        if arguments.len() < 6 {
            return Err(
                "error[pure_native_capability_arguments]: package capability frame is truncated"
                    .into(),
            );
        }
        let argument_count = usize::try_from(arguments[5]).map_err(|_| {
            "error[pure_native_capability_arguments]: package argument count must be nonnegative"
                .to_string()
        })?;
        let expected = 6usize
            .checked_add(argument_count.checked_mul(4).ok_or_else(|| {
                "error[pure_native_capability_arguments]: package argument count overflow"
                    .to_string()
            })?)
            .ok_or_else(|| {
                "error[pure_native_capability_arguments]: package frame length overflow".to_string()
            })?;
        return if arguments.len() == expected {
            Ok(())
        } else {
            Err(format!(
                "error[pure_native_capability_arguments]: package capability requires {expected} words, received {}",
                arguments.len()
            ).into())
        };
    }
    if arguments.first() == Some(&TVM_SQL_CAPABILITY_TAG) {
        if arguments.len() < TVM_SQL_CAPABILITY_PREFIX_WORDS {
            return Err(
                "error[pure_native_capability_arguments]: SQL capability frame is truncated".into(),
            );
        }
        let parameter_count = usize::try_from(arguments[TVM_SQL_CAPABILITY_PREFIX_WORDS - 1])
            .map_err(|_| {
                "error[pure_native_capability_arguments]: SQL parameter count must be nonnegative"
                    .to_string()
            })?;
        let expected = TVM_SQL_CAPABILITY_PREFIX_WORDS
            .checked_add(parameter_count.checked_mul(4).ok_or_else(|| {
                "error[pure_native_capability_arguments]: SQL parameter count overflow".to_string()
            })?)
            .ok_or_else(|| {
                "error[pure_native_capability_arguments]: SQL frame length overflow".to_string()
            })?;
        if arguments.len() != expected {
            return Err(format!(
                "error[pure_native_capability_arguments]: SQL capability requires {expected} words, received {}",
                arguments.len()
            ).into());
        }
        for index in 0..parameter_count {
            let offset = TVM_SQL_CAPABILITY_PREFIX_WORDS + index * 4;
            TvmBoundaryType::from_transition_words(&arguments[offset..offset + 3]).map_err(
                |error| {
                    format!(
                        "error[pure_native_capability_arguments]: SQL parameter {} type: {error}",
                        index + 1
                    )
                },
            )?;
        }
        return Ok(());
    }
    let expected = match arguments.first().copied() {
        Some(tag) if tvm_fixed_capability_frame_words(tag).is_some() => {
            tvm_fixed_capability_frame_words(tag).expect("fixed capability count was checked")
        }
        Some(tag) => {
            return Err(format!(
                "error[pure_native_capability_arguments]: unknown capability tag {tag}"
            )
            .into());
        }
        None => {
            return Err("error[pure_native_capability_arguments]: missing capability tag".into());
        }
    };
    if arguments.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "error[pure_native_capability_arguments]: capability tag {} requires {} payload words, received {}",
            arguments[0],
            expected - 4,
            arguments.len() - 1
        ).into())
    }
}

pub(super) fn capability_identity(tag: i64) -> VmRuntimeResult<(String, String)> {
    match tag {
        1 => Ok(("stdio".to_string(), "std.io.console.println".to_string())),
        35 => Ok(("stdio".to_string(), "std.io.console.eprintln".to_string())),
        36 => Ok((
            "clock".to_string(),
            "std.time.clock.unix_time_ns".to_string(),
        )),
        37 => Ok((
            "clock".to_string(),
            "std.time.clock.monotonic_time_ns".to_string(),
        )),
        2 => Ok(("filesystem".to_string(), "std.io.file.exists".to_string())),
        3 => Ok((
            "filesystem".to_string(),
            "std.io.file.read_text".to_string(),
        )),
        4 => Ok((
            "filesystem".to_string(),
            "std.io.file.write_text".to_string(),
        )),
        5 => Ok((
            "filesystem".to_string(),
            "std.io.file.append_text".to_string(),
        )),
        6 => Ok(("filesystem".to_string(), "std.io.file.delete".to_string())),
        8 => Ok((
            "system.arguments".to_string(),
            "std.system.arguments.count".to_string(),
        )),
        9 => Ok((
            "system.arguments".to_string(),
            "std.system.arguments.get".to_string(),
        )),
        10 => Ok((
            "system.environment".to_string(),
            "std.system.environment.contains".to_string(),
        )),
        11 => Ok((
            "system.environment".to_string(),
            "std.system.environment.get".to_string(),
        )),
        12 => Ok((
            "system.environment".to_string(),
            "std.system.environment.current_directory".to_string(),
        )),
        59 => Ok((
            "system.platform".to_string(),
            "std.system.platform.current_metrics".to_string(),
        )),
        13 => Ok((
            "filesystem".to_string(),
            "std.io.directory.entries".to_string(),
        )),
        14 => Ok((
            "filesystem".to_string(),
            "std.io.directory.files_recursive".to_string(),
        )),
        15 => Ok((
            "filesystem".to_string(),
            "std.io.directory.create_all".to_string(),
        )),
        16 => Ok((
            "filesystem".to_string(),
            "std.io.directory.remove_all".to_string(),
        )),
        17 => Ok((
            "filesystem".to_string(),
            "std.io.directory.files_recursive_excluding".to_string(),
        )),
        18 => Ok((
            "filesystem".to_string(),
            "std.io.file.read_text_many".to_string(),
        )),
        19 => Ok((
            "filesystem".to_string(),
            "std.io.file.read_text_directory".to_string(),
        )),
        20 => Ok((
            "filesystem".to_string(),
            "std.io.file.read_text_tree_excluding".to_string(),
        )),
        21 => Ok((
            "filesystem".to_string(),
            "std.io.file.read_text_tree_matching".to_string(),
        )),
        22 => Ok(("process".to_string(), "std.system.process.run".to_string())),
        33 => Ok((
            "process".to_string(),
            "std.system.process.limits".to_string(),
        )),
        31 => Ok((
            "process".to_string(),
            "std.system.process.run_many".to_string(),
        )),
        23 => Ok((
            "filesystem".to_string(),
            "std.io.directory.create_temporary".to_string(),
        )),
        24 => Ok((
            "filesystem".to_string(),
            "std.io.directory.copy_tree_excluding".to_string(),
        )),
        34 => Ok((
            "filesystem".to_string(),
            "std.io.directory.create_symbolic_link".to_string(),
        )),
        32 => Ok((
            "filesystem".to_string(),
            "std.io.directory.tree_usage".to_string(),
        )),
        30 => Ok((
            "filesystem".to_string(),
            "std.io.file.read_bytes".to_string(),
        )),
        38 => Ok(("filesystem".to_string(), "std.io.file.size".to_string())),
        40 => Ok((
            "filesystem".to_string(),
            "std.io.file.timestamps".to_string(),
        )),
        41 => Ok((
            "filesystem".to_string(),
            "std.io.file.set_timestamps".to_string(),
        )),
        49 => Ok((
            "filesystem".to_string(),
            "std.io.file.is_executable".to_string(),
        )),
        50 => Ok((
            "filesystem".to_string(),
            "std.io.file.set_executable".to_string(),
        )),
        52 => Ok(("filesystem".to_string(), "std.io.file.copy".to_string())),
        54 => Ok((
            "filesystem".to_string(),
            "std.io.file.copy_many".to_string(),
        )),
        39 => Ok((
            "filesystem".to_string(),
            "std.io.directory.find_named_recursive_excluding".to_string(),
        )),
        43 => Ok((
            "filesystem".to_string(),
            "std.io.archive.extract".to_string(),
        )),
        51 => Ok((
            "filesystem".to_string(),
            "std.io.archive.create".to_string(),
        )),
        44 => Ok((
            "filesystem".to_string(),
            "std.crypto.hash.sha256_file".to_string(),
        )),
        45 => Ok((
            "filesystem".to_string(),
            "std.crypto.hash.verify_sha256_manifest".to_string(),
        )),
        46 => Ok((
            "filesystem".to_string(),
            "std.crypto.hash.sha256_tree".to_string(),
        )),
        47 => Ok((
            "filesystem".to_string(),
            "std.crypto.hash.sha256_selected_files".to_string(),
        )),
        48 => Ok((
            "process".to_string(),
            "std.system.process.run_length_framed".to_string(),
        )),
        53 => Ok((
            "vcs".to_string(),
            "std.vcs.git.source_tree_identity".to_string(),
        )),
        55 => Ok((
            "filesystem".to_string(),
            "std.crypto.hash.sha256_labeled_file_digests".to_string(),
        )),
        58 => Ok((
            "filesystem".to_string(),
            "std.crypto.hash.sha256_labeled_file_contents".to_string(),
        )),
        56 => Ok((
            "filesystem".to_string(),
            "std.crypto.hash.audit_labeled_files".to_string(),
        )),
        57 => Ok((
            "filesystem".to_string(),
            "std.crypto.hash.audit_labeled_file_patterns".to_string(),
        )),
        TVM_SQL_CAPABILITY_TAG => Ok(("postgres".to_string(), "std.db.sql.query".to_string())),
        _ => Err(
            format!("error[pure_native_capability_arguments]: unknown capability tag {tag}").into(),
        ),
    }
}
