//! Scheduler transition argument validation.

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::native_image::TvmBoundaryType;

use super::validate_capability_arguments;

pub(crate) fn validate_transition_arguments(
    operation: &TvmTransitionOperation,
    arguments: &[i64],
) -> Result<(), String> {
    match operation {
        TvmTransitionOperation::Debug if arguments.is_empty() => Ok(()),
        TvmTransitionOperation::Debug => Err(
            "error[pure_native_transition_arguments]: Debug expects no arguments".to_string(),
        ),
        TvmTransitionOperation::Identity if arguments.is_empty() => Ok(()),
        TvmTransitionOperation::Identity => Err(
            "error[pure_native_transition_arguments]: Identity expects no arguments".to_string(),
        ),
        TvmTransitionOperation::Yield if arguments.is_empty() => Ok(()),
        TvmTransitionOperation::Yield => Err(
            "error[pure_native_transition_arguments]: Yield transition must not carry operation arguments"
                .to_string(),
        ),
        TvmTransitionOperation::Send if !matches!(arguments.len(), 2 | 5) => Err(format!(
            "error[pure_native_transition_arguments]: Send transition requires 2 scalar or 5 typed arguments, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Send if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Send recipient must be a positive process identity"
                .to_string(),
        ),
        TvmTransitionOperation::Send if arguments.len() == 5 => {
            Ok(TvmBoundaryType::from_transition_words(&arguments[1..4]).map(|_| ())?)
        }
        TvmTransitionOperation::Send => Ok(()),
        TvmTransitionOperation::Receive if arguments.is_empty() => Ok(()),
        TvmTransitionOperation::Receive if arguments.len() == 3 => {
            Ok(TvmBoundaryType::from_transition_words(arguments).map(|_| ())?)
        }
        TvmTransitionOperation::Receive => Err(format!(
            "error[pure_native_transition_arguments]: Receive transition requires 0 scalar or 3 typed arguments, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Spawn if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Spawn transition requires one native entry identity, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Spawn if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Spawn entry must be a positive native identity"
                .to_string(),
        ),
        TvmTransitionOperation::Spawn => Ok(()),
        TvmTransitionOperation::Timer if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Timer transition requires one positive delay, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Timer if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Timer delay must be positive".to_string(),
        ),
        TvmTransitionOperation::Timer => Ok(()),
        TvmTransitionOperation::Link if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Link transition requires one positive peer identity, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Link if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Link peer must be a positive process identity"
                .to_string(),
        ),
        TvmTransitionOperation::Link => Ok(()),
        TvmTransitionOperation::Monitor if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Monitor transition requires one positive target identity, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Monitor if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Monitor target must be a positive process identity"
                .to_string(),
        ),
        TvmTransitionOperation::Monitor => Ok(()),
        TvmTransitionOperation::Resource if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Resource transition requires one positive kind tag, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Resource if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Resource kind tag must be positive"
                .to_string(),
        ),
        TvmTransitionOperation::Resource => Ok(()),
        TvmTransitionOperation::Cancellation if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Cancellation transition requires one positive target identity, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Cancellation if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Cancellation target must be a positive process identity"
                .to_string(),
        ),
        TvmTransitionOperation::Cancellation => Ok(()),
        TvmTransitionOperation::Failure if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Failure transition requires one positive failure code, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Failure if arguments[0] <= 0 => Err(
            "error[pure_native_transition_arguments]: Failure code must be positive".to_string(),
        ),
        TvmTransitionOperation::Failure => Ok(()),
        TvmTransitionOperation::Scheduling if arguments.len() != 1 => Err(format!(
            "error[pure_native_transition_arguments]: Scheduling transition requires one class tag, received {} arguments",
            arguments.len()
        )),
        TvmTransitionOperation::Scheduling if !(1..=3).contains(&arguments[0]) => Err(
            "error[pure_native_transition_arguments]: Scheduling class tag must be 1, 2, or 3"
                .to_string(),
        ),
        TvmTransitionOperation::Scheduling => Ok(()),
        TvmTransitionOperation::Capability => Ok(validate_capability_arguments(arguments)?),
    }
}
