use super::*;

pub(super) fn validate_execution_lane(lane: &VmAcmeWorkerExecutionLane) -> Result<(), String> {
    match lane {
        #[cfg(test)]
        VmAcmeWorkerExecutionLane::DeterministicFixture { fixture_id } => {
            if fixture_id.trim().is_empty() {
                Err("ACME deterministic fixture id must not be empty".to_string())
            } else {
                Ok(())
            }
        }
        VmAcmeWorkerExecutionLane::Live { directory_url } => {
            if directory_url.trim().is_empty() {
                return Err("ACME live directory URL must not be empty".to_string());
            }
            if !directory_url.starts_with("https://") {
                return Err("ACME live directory URL must use https".to_string());
            }
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(test, feature = "acme-live"))]
pub(super) enum VmAcmeWorkerStateName {
    Requested,
    ChallengeReady,
    Issuing,
    CacheWriting,
    Completed,
}

#[cfg(any(test, feature = "acme-live"))]
pub(super) fn state_name(state: &VmAcmeWorkerState) -> VmAcmeWorkerStateName {
    match state {
        VmAcmeWorkerState::Requested => VmAcmeWorkerStateName::Requested,
        VmAcmeWorkerState::ChallengeReady(_) => VmAcmeWorkerStateName::ChallengeReady,
        VmAcmeWorkerState::Issuing => VmAcmeWorkerStateName::Issuing,
        VmAcmeWorkerState::CacheWriting { .. } => VmAcmeWorkerStateName::CacheWriting,
        #[cfg(test)]
        VmAcmeWorkerState::RenewalScheduled { .. } => VmAcmeWorkerStateName::Completed,
        VmAcmeWorkerState::Completed => VmAcmeWorkerStateName::Completed,
        #[cfg(test)]
        VmAcmeWorkerState::Cancelled { .. } => VmAcmeWorkerStateName::Completed,
        #[cfg(test)]
        VmAcmeWorkerState::Shutdown => VmAcmeWorkerStateName::Completed,
    }
}

#[cfg(any(test, feature = "acme-live"))]
pub(super) fn ensure_state(
    state: &VmAcmeWorkerState,
    allowed: &[VmAcmeWorkerStateName],
) -> Result<(), String> {
    let actual = state_name(state);
    if allowed.contains(&actual) {
        Ok(())
    } else {
        Err(format!(
            "invalid ACME worker state transition from {actual:?}"
        ))
    }
}

pub(super) fn validate_request(request: &VmAcmeWorkerRequest) -> Result<(), String> {
    if request.domain.trim().is_empty() {
        return Err("ACME worker domain must not be empty".to_string());
    }
    if request.account_id.trim().is_empty() {
        return Err("ACME worker account id must not be empty".to_string());
    }
    if request.cache_key.trim().is_empty() {
        return Err("ACME worker cache key must not be empty".to_string());
    }
    if request.domain.contains("://") || request.domain.contains('/') {
        return Err("ACME worker domain must be a host name, not a URL".to_string());
    }
    Ok(())
}

#[cfg(any(test, feature = "acme-live"))]
pub(super) fn validate_http01_token(token: &str) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("ACME HTTP-01 token must not be empty".to_string());
    }
    if !token
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("ACME HTTP-01 token contains invalid characters".to_string());
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn mode_support_bundle_label(mode: VmAcmeMode) -> &'static str {
    match mode {
        VmAcmeMode::Staging => "staging",
        VmAcmeMode::Live => "live",
    }
}

#[cfg(test)]
pub(super) fn execution_lane_support_bundle_label(
    lane: &VmAcmeWorkerExecutionLane,
) -> &'static str {
    match lane {
        VmAcmeWorkerExecutionLane::DeterministicFixture { .. } => "deterministic-fixture",
        VmAcmeWorkerExecutionLane::Live { .. } => "live",
    }
}

#[cfg(test)]
pub(super) fn redact_acme_support_bundle_value(value: &str) -> &'static str {
    if value.is_empty() {
        "empty"
    } else {
        "<redacted>"
    }
}

#[cfg(test)]
pub(super) fn state_support_bundle_operation(state: &VmAcmeWorkerState) -> &'static str {
    match state {
        VmAcmeWorkerState::Requested => "acme.worker.requested",
        VmAcmeWorkerState::ChallengeReady(_) => "acme.worker.challenge_ready",
        VmAcmeWorkerState::Issuing => "acme.worker.issuing",
        VmAcmeWorkerState::CacheWriting { .. } => "acme.worker.cache_writing",
        VmAcmeWorkerState::RenewalScheduled { .. } => "acme.worker.renewal_scheduled",
        VmAcmeWorkerState::Completed => "acme.worker.completed",
        VmAcmeWorkerState::Cancelled { .. } => "acme.worker.cancelled",
        VmAcmeWorkerState::Shutdown => "acme.worker.shutdown",
    }
}

#[cfg(test)]
pub(super) fn renewal_actor_resource_handle(handle: VmAcmeWorkerHandle) -> String {
    format!("acme-renewal-actor:{}", handle.as_u64())
}
