//! Scalar scheduler services shared by native transition dispatchers.

use super::*;

pub(crate) fn dispatch_transition_operation(
    actors: &mut VmActorRuntime,
    owner_id: u64,
    request_id: u64,
    continuation_id: u64,
    operation: &TvmTransitionOperation,
    arguments: &[i64],
) -> Result<Option<Vec<i64>>, String> {
    validate_transition_arguments(operation, arguments)?;
    match operation {
        TvmTransitionOperation::FailureTyped => Err(
            "error[pure_native_effect_failure]: typed failure requires its owning native heap"
                .to_string(),
        ),
        TvmTransitionOperation::Debug => actors
            .resume_native_continuation(owner_id, request_id, continuation_id)
            .map(|()| Some(Vec::new())),
        TvmTransitionOperation::Identity => actors
            .resume_native_continuation(owner_id, request_id, continuation_id)
            .and_then(|()| {
                i64::try_from(owner_id)
                    .map(|identity| Some(vec![identity]))
                    .map_err(|_| {
                        "error[pure_native_identity_result]: process identity exceeds native Int"
                            .to_string()
                    })
            }),
        TvmTransitionOperation::Yield => actors
            .resume_native_continuation(owner_id, request_id, continuation_id)
            .map(|()| Some(Vec::new())),
        TvmTransitionOperation::Send => {
            let recipient = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Send recipient must be a positive process identity"
                    .to_string()
            })?;
            actors
                .service_native_send(
                    owner_id,
                    request_id,
                    continuation_id,
                    recipient,
                    ReplValue::Int(arguments[1]),
                )
                .map(|_| Some(Vec::new()))
        }
        TvmTransitionOperation::Receive => actors
            .service_native_receive_int(owner_id, request_id, continuation_id)
            .map(|payload| payload.map(|value| vec![value])),
        TvmTransitionOperation::Spawn => {
            let entry_id = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Spawn entry must be a positive native identity"
                    .to_string()
            })?;
            actors
                .service_native_spawn(owner_id, request_id, continuation_id, entry_id)
                .map(|child| Some(vec![child as i64]))
        }
        TvmTransitionOperation::Timer => {
            let delay_ticks = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Timer delay must be positive".to_string()
            })?;
            actors
                .service_native_timer(owner_id, request_id, continuation_id, delay_ticks)
                .map(|()| Some(Vec::new()))
        }
        TvmTransitionOperation::Link => {
            let peer_id = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Link peer must be a positive process identity"
                    .to_string()
            })?;
            actors
                .service_native_link(owner_id, request_id, continuation_id, peer_id)
                .map(|_| Some(Vec::new()))
        }
        TvmTransitionOperation::Monitor => {
            let target_id = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Monitor target must be a positive process identity"
                    .to_string()
            })?;
            actors
                .service_native_monitor(owner_id, request_id, continuation_id, target_id)
                .and_then(|monitor_ref| {
                    i64::try_from(monitor_ref)
                        .map(|value| Some(vec![value]))
                        .map_err(|_| {
                            "error[pure_native_monitor_result]: monitor reference exceeds native Int"
                                .to_string()
                        })
                })
        }
        TvmTransitionOperation::Resource => {
            let kind_tag = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Resource kind tag must be positive"
                    .to_string()
            })?;
            actors
                .service_native_resource(owner_id, request_id, continuation_id, kind_tag)
                .and_then(|resource_id| {
                    i64::try_from(resource_id)
                        .map(|value| Some(vec![value]))
                        .map_err(|_| {
                            "error[pure_native_resource_result]: resource identity exceeds native Int"
                                .to_string()
                        })
                })
        }
        TvmTransitionOperation::Cancellation => {
            let target_id = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Cancellation target must be a positive process identity"
                    .to_string()
            })?;
            actors
                .service_native_cancellation(owner_id, request_id, continuation_id, target_id)
                .map(|()| Some(Vec::new()))
        }
        TvmTransitionOperation::Failure => {
            let failure_code = u64::try_from(arguments[0]).map_err(|_| {
                "error[pure_native_transition_arguments]: Failure code must be positive".to_string()
            })?;
            actors
                .service_native_failure(owner_id, request_id, continuation_id, failure_code)
                .map(|_| Some(Vec::new()))
        }
        TvmTransitionOperation::Scheduling => {
            let class = match arguments[0] {
                1 => VmSchedulerClass::Priority,
                2 => VmSchedulerClass::Normal,
                3 => VmSchedulerClass::Background,
                _ => unreachable!("Scheduling arguments were validated before dispatch"),
            };
            actors
                .service_native_scheduling(owner_id, request_id, continuation_id, class)
                .map(|()| Some(Vec::new()))
        }
        TvmTransitionOperation::Capability => Ok(None),
    }
}
