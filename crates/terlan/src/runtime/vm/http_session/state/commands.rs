use super::*;

/// Writes one string session value through the VM session runtime.
pub fn set(
    runtime: &mut VmHttpSessionRuntime,
    session: &VmHttpSession,
    key: &str,
    value: &str,
) -> Result<(), terlan_runtime_abi::BoundaryError> {
    runtime
        .write(session, key, ReplValue::String(value.to_string()))
        .map_err(vm_session_error)
}

/// Deletes one string session value through the VM session runtime.
pub fn delete(
    runtime: &mut VmHttpSessionRuntime,
    session: &VmHttpSession,
    key: &str,
) -> Result<(), terlan_runtime_abi::BoundaryError> {
    runtime
        .delete(session, key)
        .map(|_| ())
        .map_err(vm_session_error)
}

/// Rotates one VM session id while preserving actor-owned state.
pub fn rotate(
    runtime: &mut VmHttpSessionRuntime,
    session: &VmHttpSession,
) -> Result<VmHttpSessionLookup, terlan_runtime_abi::BoundaryError> {
    runtime.rotate(session).map_err(vm_session_error)
}

/// Expires one VM session actor and its owned table state.
pub fn expire(
    runtime: &mut VmHttpSessionRuntime,
    session: &VmHttpSession,
) -> Result<(), terlan_runtime_abi::BoundaryError> {
    runtime.expire(session).map_err(vm_session_error)
}

fn vm_session_error(error: String) -> terlan_runtime_abi::BoundaryError {
    terlan_runtime_abi::BoundaryError::message(
        terlan_runtime_abi::ErrorDomain::VmRuntime,
        "mutate HTTP session",
        error,
    )
}

/// Threads session response metadata onto a response value.
#[cfg(test)]
pub fn with_response<T>(response: T, _session: &VmHttpSession) -> T {
    response
}

pub(crate) fn normalize_cookie_value(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(crate) fn cookie_header_for(session_id: &str) -> String {
    format!("{SESSION_COOKIE_NAME}={session_id}; Path=/; HttpOnly; SameSite=Lax")
}

pub(crate) fn created_session_table_id(event: VmTableEvent) -> VmTableId {
    match event {
        VmTableEvent::Created { id, .. } => id,
        other => panic!("unexpected HTTP session table creation event: {other:?}"),
    }
}

pub(crate) fn deleted_session_value(event: Option<VmTableEvent>) -> Option<ReplValue> {
    match event {
        Some(VmTableEvent::Deleted { old_value, .. }) => Some(old_value),
        Some(_) | None => None,
    }
}

#[cfg(test)]
pub(crate) fn live_template_state_patch_event_id(
    session_id: &str,
    state_version: u64,
    subscriber_id: &str,
) -> String {
    format!("{session_id}:{state_version}:{subscriber_id}")
}

#[cfg(test)]
pub(crate) fn live_template_subscriber_authorized_diagnostic(
    subscriber_id: &str,
    required_capability: &str,
) -> String {
    format!(
        "HTTP live-template subscriber `{subscriber_id}` authorized with capability `{required_capability}`"
    )
}

#[cfg(test)]
pub(crate) fn live_template_subscriber_capability_diagnostic(
    subscriber_id: &str,
    required_capability: &str,
) -> String {
    format!(
        "HTTP live-template subscriber `{subscriber_id}` missing capability `{required_capability}`"
    )
}

#[cfg(test)]
pub(crate) fn live_template_actor_binding_diagnostic(
    session_id: &str,
    template_id: &str,
    state_key: &str,
    actor_pid: u64,
) -> String {
    format!(
        "HTTP live-template `{template_id}` bound to session `{session_id}` actor {actor_pid} state `{state_key}`"
    )
}

#[cfg(test)]
pub(crate) fn live_template_source_map_trace_diagnostic(
    session_id: &str,
    subscriber_id: &str,
    template_id: &str,
    source_module: &str,
    source_line: u32,
    source_column: u32,
) -> String {
    format!(
        "HTTP live-template `{template_id}` subscriber `{subscriber_id}` on session `{session_id}` traced to {source_module}:{source_line}:{source_column}"
    )
}

#[cfg(test)]
pub(crate) fn live_template_missing_subscriber_trace_diagnostic(
    subscriber_id: &str,
    template_id: &str,
) -> String {
    format!("HTTP live-template `{template_id}` cannot trace missing subscriber `{subscriber_id}`")
}

#[cfg(test)]
pub(crate) fn validate_live_template_source_location(
    source_line: u32,
    source_column: u32,
) -> Result<(), String> {
    if source_line == 0 {
        return Err("HTTP live-template source line must be greater than 0".to_string());
    }
    if source_column == 0 {
        return Err("HTTP live-template source column must be greater than 0".to_string());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn normalize_command_id(command_id: &str) -> Result<&str, String> {
    let trimmed = command_id.trim();
    if trimmed.is_empty() {
        Err("HTTP session command id cannot be empty".to_string())
    } else {
        Ok(trimmed)
    }
}

#[cfg(test)]
pub(crate) fn normalize_command_name(name: &str) -> Result<&str, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        Err("HTTP live-template command name cannot be empty".to_string())
    } else if trimmed.chars().any(char::is_control) {
        Err("HTTP live-template command name cannot contain control characters".to_string())
    } else {
        Ok(trimmed)
    }
}

#[cfg(test)]
pub(crate) fn http_message_id_to_int(message_id: u64) -> Result<i64, String> {
    i64::try_from(message_id)
        .map_err(|_| "HTTP live-template command message id overflowed Int".to_string())
}

#[cfg(test)]
pub(crate) fn normalize_persistence_session_id(session_id: &str) -> Result<&str, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        Err("HTTP session persistence snapshot id cannot be empty".to_string())
    } else {
        Ok(trimmed)
    }
}

#[cfg(test)]
pub(crate) fn normalize_live_template_subscriber_field<'a>(
    value: &'a str,
    label: &str,
) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{label} cannot be empty"))
    } else {
        Ok(trimmed)
    }
}

#[cfg(test)]
pub(crate) fn resolve_http_session_affinity_key(
    affinity_keys: &[VmHttpSessionAffinityKey],
) -> Result<&VmHttpSessionAffinityKey, VmHttpSessionAffinityError> {
    let mut selected = None;
    for key in affinity_keys
        .iter()
        .filter(|key| !key.key.trim().is_empty())
    {
        match selected {
            None => selected = Some(key),
            Some(existing) if existing.key == key.key => {}
            Some(existing) => {
                return Err(VmHttpSessionAffinityError::ConflictingAffinityKeys {
                    existing_source: existing.source.clone(),
                    existing_key: existing.key.clone(),
                    incoming_source: key.source.clone(),
                    incoming_key: key.key.clone(),
                });
            }
        }
    }
    selected.ok_or(VmHttpSessionAffinityError::MissingAffinityKey)
}

#[cfg(test)]
impl VmHttpSessionAffinityError {
    #[cfg(test)]
    pub(crate) fn render(&self) -> String {
        match self {
            Self::MissingAffinityKey => "missing HTTP session affinity key".to_string(),
            Self::ConflictingAffinityKeys {
                existing_source,
                existing_key,
                incoming_source,
                incoming_key,
            } => format!(
                "conflicting HTTP session affinity keys: {existing_source} requested `{existing_key}`, {incoming_source} requested `{incoming_key}`"
            ),
        }
    }
}

pub(crate) fn stale_session_diagnostic(session_id: &str) -> String {
    format!("stale HTTP session `{session_id}`")
}

#[cfg(test)]
pub(crate) fn duplicate_persistence_snapshot_diagnostic(session_id: &str) -> String {
    format!("HTTP session persistence snapshot `{session_id}` would overwrite live session")
}

#[cfg(test)]
pub(crate) fn expired_persistence_snapshot_diagnostic(session_id: &str) -> String {
    format!("HTTP session persistence snapshot `{session_id}` is expired")
}

#[cfg(test)]
pub(crate) fn mailbox_backpressure_attribution(
    session_id: &str,
    mailbox_len: usize,
    threshold: usize,
    saturated: bool,
) -> String {
    if saturated {
        format!(
            "HTTP session `{session_id}` actor mailbox backpressure: {mailbox_len} queued messages >= threshold {threshold}"
        )
    } else {
        format!(
            "HTTP session `{session_id}` actor mailbox pressure is within threshold: {mailbox_len} queued messages < threshold {threshold}"
        )
    }
}

#[cfg(test)]
pub(crate) fn worker_migration_diagnostic(
    session_id: &str,
    source_node_id: &str,
    destination_node_id: &str,
    destination_actor_pid: u64,
) -> String {
    format!(
        "HTTP session `{session_id}` migrated from worker `{source_node_id}` to worker `{destination_node_id}` as actor {destination_actor_pid}"
    )
}

#[cfg(test)]
pub(crate) fn hot_reload_migration_compatibility_diagnostic(
    session_id: &str,
    previous_generation: u64,
    active_generation: u64,
    durable_table_entries: usize,
    durable_command_results: usize,
    transient_subscribers: usize,
) -> String {
    format!(
        "HTTP session `{session_id}` is compatible with hot reload generation {previous_generation}->{active_generation}: {durable_table_entries} table entries and {durable_command_results} command results remain durable; {transient_subscribers} live-template subscribers remain transient"
    )
}

#[cfg(test)]
pub(crate) fn state_version_conflict_diagnostic(
    session_id: &str,
    expected_version: u64,
    actual_version: u64,
) -> String {
    format!(
        "HTTP session `{session_id}` state version conflict: expected {expected_version}, actual {actual_version}"
    )
}
