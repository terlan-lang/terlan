use super::*;

pub(super) fn register_route(
    routes: &mut BTreeMap<std::num::NonZeroU64, VmProcessId>,
    route: VmFixedActorRoute,
    step: ScheduledInvocationStep,
) -> Result<ScheduledInvocationStep, String> {
    let owner = match &step {
        ScheduledInvocationStep::Waiting { owner, .. }
        | ScheduledInvocationStep::TimerWaiting { owner, .. }
        | ScheduledInvocationStep::CapabilityWaiting { owner, .. }
        | ScheduledInvocationStep::Runnable { owner, .. } => Some(*owner),
        ScheduledInvocationStep::Complete(_) => None,
    };
    if let Some(owner) = owner {
        if routes.insert(route.actor_id(), owner).is_some() {
            return Err("error[vm.actor_route]: duplicate live actor route".to_string());
        }
    }
    Ok(step)
}

/// Converts telemetry corruption into the scheduler's fail-stop panic path.
pub(super) fn record_or_panic(
    telemetry: &VmFixedSchedulerTelemetry,
    kind: VmFixedSchedulerEventKind,
    route: Option<VmFixedActorRoute>,
) {
    if let Err(error) = telemetry.record(kind, route) {
        panic!("fixed scheduler telemetry corruption: {error}");
    }
}

/// Rejects duplicate shard-global identities before actor state allocation.
pub(in crate::commands::serve::handler_cache::shard_owner) fn reject_duplicate_route(
    routes: &BTreeMap<std::num::NonZeroU64, VmProcessId>,
    route: VmFixedActorRoute,
) -> Result<(), String> {
    if routes.contains_key(&route.actor_id()) {
        Err(format!(
            "error[vm.actor_route]: route {} is already live",
            route.actor_id()
        ))
    } else {
        Ok(())
    }
}

/// Validates the shard-global route before touching mutable actor state.
pub(in crate::commands::serve::handler_cache::shard_owner) fn validate_live_route(
    routes: &BTreeMap<std::num::NonZeroU64, VmProcessId>,
    route: VmFixedActorRoute,
    owner: VmProcessId,
) -> Result<(), String> {
    match routes.get(&route.actor_id()) {
        Some(expected) if *expected == owner => Ok(()),
        Some(expected) => Err(format!(
            "error[vm.actor_route]: route {} owns process {}, not {}",
            route.actor_id(),
            expected.as_u64(),
            owner.as_u64()
        )),
        None => Err(format!(
            "error[vm.actor_route]: route {} is not live",
            route.actor_id()
        )),
    }
}

/// Rejects a command delivered to a scheduler other than its fixed home.
pub(in crate::commands::serve::handler_cache::shard_owner) fn validate_scheduler_route(
    route: VmFixedActorRoute,
    current: &thread::Thread,
) -> Result<(), String> {
    let expected = format!("terlan-aot-scheduler-{}", route.scheduler().index());
    if current.name() == Some(expected.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "error[vm.actor_route]: route {} reached the wrong scheduler",
            route.actor_id()
        ))
    }
}
