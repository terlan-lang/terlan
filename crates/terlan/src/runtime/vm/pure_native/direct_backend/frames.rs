//! Decodes the generated dispatch status and capability transition frames.

use crate::runtime::native_image::control::{
    tvm_fixed_capability_frame_words, TvmControlFrame, TvmTransitionOperation,
    TVM_SQL_CAPABILITY_PREFIX_WORDS, TVM_SQL_CAPABILITY_TAG,
};
use crate::runtime::native_image::TvmBoundaryType;

/// Separates transition arguments from the captured continuation values.
pub(super) fn frame_from_status(
    request_id: u64,
    owner_id: u64,
    status: i32,
    value: i64,
    mut transition_values: Vec<i64>,
) -> Result<TvmControlFrame, String> {
    if status == 0 {
        return Ok(TvmControlFrame::Success {
            request_id,
            owner_id,
            value,
        });
    }
    let (operation, argument_count) = match status {
        6 => (TvmTransitionOperation::Yield, 0),
        8 => (TvmTransitionOperation::Send, 2),
        9 => (TvmTransitionOperation::Receive, 0),
        10 => (TvmTransitionOperation::Spawn, 1),
        11 => (TvmTransitionOperation::Timer, 1),
        12 => (TvmTransitionOperation::Link, 1),
        13 => (TvmTransitionOperation::Monitor, 1),
        14 => (TvmTransitionOperation::Resource, 1),
        15 => (TvmTransitionOperation::Cancellation, 1),
        16 => (TvmTransitionOperation::Failure, 1),
        17 => (TvmTransitionOperation::Scheduling, 1),
        22 => (TvmTransitionOperation::Send, 5),
        23 => (TvmTransitionOperation::Receive, 3),
        25 => (TvmTransitionOperation::Debug, 0),
        26 => (TvmTransitionOperation::Identity, 0),
        27 => (TvmTransitionOperation::FailureTyped, 1),
        24 => {
            let tag = transition_values.first().copied().ok_or_else(|| {
                "error[execution_shard.capability_arguments]: missing capability tag".to_string()
            })?;
            let count = match tag {
                7 => {
                    let argument_count = transition_values
                        .get(5)
                        .copied()
                        .and_then(|count| usize::try_from(count).ok())
                        .ok_or_else(|| {
                            "error[execution_shard.capability_arguments]: package argument count is missing or invalid"
                                .to_string()
                        })?;
                    6usize
                        .checked_add(argument_count.checked_mul(4).ok_or_else(|| {
                            "error[execution_shard.capability_arguments]: package argument count overflow"
                                .to_string()
                        })?)
                        .ok_or_else(|| {
                            "error[execution_shard.capability_arguments]: package frame length overflow"
                                .to_string()
                        })?
                }
                TVM_SQL_CAPABILITY_TAG => {
                    let parameter_count = transition_values
                        .get(TVM_SQL_CAPABILITY_PREFIX_WORDS - 1)
                        .copied()
                        .and_then(|count| usize::try_from(count).ok())
                        .ok_or_else(|| {
                            "error[execution_shard.capability_arguments]: SQL parameter count is missing or invalid"
                                .to_string()
                        })?;
                    TVM_SQL_CAPABILITY_PREFIX_WORDS
                        .checked_add(parameter_count.checked_mul(4).ok_or_else(|| {
                            "error[execution_shard.capability_arguments]: SQL parameter count overflow"
                                .to_string()
                        })?)
                        .ok_or_else(|| {
                            "error[execution_shard.capability_arguments]: SQL frame length overflow"
                                .to_string()
                        })?
                }
                _ if tvm_fixed_capability_frame_words(tag).is_some() => {
                    tvm_fixed_capability_frame_words(tag)
                        .expect("fixed capability count was checked")
                }
                _ => {
                    return Err(format!(
                        "error[execution_shard.capability_arguments]: unknown capability tag {tag}"
                    ));
                }
            };
            (TvmTransitionOperation::Capability, count)
        }
        _ => {
            return Ok(TvmControlFrame::Failure {
                request_id,
                owner_id,
                status,
            });
        }
    };
    if transition_values.len() < argument_count {
        return Err(format!(
            "error[execution_shard.transition_arguments]: {operation:?} returned {} values, expected at least {argument_count}",
            transition_values.len()
        ));
    }
    let values = transition_values.split_off(argument_count);
    Ok(TvmControlFrame::Transition {
        request_id,
        owner_id,
        continuation_id: value as u64,
        operation,
        arguments: transition_values,
        values,
    })
}

/// Decodes the declared result type carried by a capability transition.
pub(super) fn capability_result_type(arguments: &[i64]) -> Result<TvmBoundaryType, String> {
    if arguments.len() < 4 {
        return Err(
            "error[execution_shard.capability_arguments]: result type metadata is missing".into(),
        );
    }
    Ok(TvmBoundaryType::from_transition_words(&arguments[1..4])?)
}
