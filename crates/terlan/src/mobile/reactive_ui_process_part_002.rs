
/// Converts one effect to generated metadata.
fn metadata_effect(effect: &ReactiveUiEffect) -> ReactiveUiMetadataEffect {
    ReactiveUiMetadataEffect {
        name: effect.name.clone(),
        effect_type: effect.effect_type.as_str(),
        required_capability: effect
            .required_capability
            .map(|capability| capability.as_str()),
        payload: effect
            .payload
            .iter()
            .map(|field| metadata_field(&field.name, field.field_type))
            .collect(),
    }
}

/// Converts one reply to generated metadata.
fn metadata_reply(reply: &ReactiveUiReply) -> ReactiveUiMetadataReply {
    ReactiveUiMetadataReply {
        name: reply.name.clone(),
        source_effect: reply.source_effect.clone(),
        payload: reply
            .payload
            .iter()
            .map(|field| metadata_field(&field.name, field.field_type))
            .collect(),
    }
}

/// Builds one generated metadata field.
fn metadata_field(name: &str, field_type: MobileBridgeType) -> ReactiveUiMetadataField {
    ReactiveUiMetadataField {
        name: name.to_string(),
        field_type: field_type.as_str(),
    }
}

/// Builds one mobile bridge field for process helper payloads.
fn bridge_field(name: &str, field_type: MobileBridgeType) -> MobileBridgeField {
    MobileBridgeField {
        name: name.to_string(),
        field_type,
    }
}

/// Returns whether a capability represents a native resource surface.
fn is_native_resource_capability(capability: MobileBridgeCapability) -> bool {
    matches!(
        capability,
        MobileBridgeCapability::Files
            | MobileBridgeCapability::Camera
            | MobileBridgeCapability::Geolocation
            | MobileBridgeCapability::Storage
            | MobileBridgeCapability::PushNotifications
    )
}

/// Validates one native bridge reply binding's directly owned names.
fn validate_native_reply_binding_shape(
    binding: &ReactiveUiNativeReplyBinding,
) -> Vec<ReactiveUiProcessDiagnostic> {
    let mut diagnostics = Vec::new();
    if is_blank(&binding.process) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_native_reply_binding_process",
            "reactive UI native reply binding process must not be empty",
        ));
    }
    if is_blank(&binding.reply) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_native_reply_binding_reply",
            "reactive UI native reply binding reply must not be empty",
        ));
    }
    if is_blank(&binding.bridge) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_native_reply_binding_bridge",
            "reactive UI native reply binding bridge must not be empty",
        ));
    }
    if is_blank(&binding.command) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_native_reply_binding_command",
            "reactive UI native reply binding command must not be empty",
        ));
    }
    diagnostics
}

/// Validates one event binding's directly owned names.
fn validate_event_binding_shape(
    binding: &ReactiveUiEventBinding,
) -> Vec<ReactiveUiProcessDiagnostic> {
    let mut diagnostics = Vec::new();
    if is_blank(&binding.process) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_event_binding_process",
            "reactive UI event binding process must not be empty",
        ));
    }
    if is_blank(&binding.event) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_event_binding_event",
            "reactive UI event binding event must not be empty",
        ));
    }
    if is_blank(&binding.component_id) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_event_binding_component_id",
            "reactive UI event binding component id must not be empty",
        ));
    }
    if is_blank(&binding.component_event) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_event_binding_component_event",
            "reactive UI event binding component event must not be empty",
        ));
    }
    diagnostics
}

/// Validates a named native reply payload against one process reply.
fn validate_reply_message_payload(
    process_name: &str,
    reply: &ReactiveUiReply,
    payload: &[MobileAngularBridgePayloadField],
) -> Result<(), ReactiveUiProcessDiagnostic> {
    let mut seen = BTreeSet::new();
    for actual in payload {
        if !seen.insert(actual.name.as_str()) {
            return Err(diagnostic(
                "reactive_ui_process_duplicate_reply_payload",
                format!(
                    "reactive UI process `{process_name}` reply `{}` repeats payload field `{}`",
                    reply.name, actual.name
                ),
            ));
        }
        let Some(expected) = reply
            .payload
            .iter()
            .find(|expected| expected.name == actual.name)
        else {
            return Err(diagnostic(
                "reactive_ui_process_unknown_reply_payload",
                format!(
                    "reactive UI process `{process_name}` reply `{}` has no payload field `{}`",
                    reply.name, actual.name
                ),
            ));
        };
        if expected.field_type.as_str() != actual.value.type_name() {
            return Err(diagnostic(
                "reactive_ui_process_reply_payload_type_mismatch",
                format!(
                    "reactive UI process `{process_name}` reply `{}` field `{}` expects {} but got {}",
                    reply.name,
                    actual.name,
                    expected.field_type.as_str(),
                    actual.value.type_name()
                ),
            ));
        }
    }
    for expected in &reply.payload {
        if !seen.contains(expected.name.as_str()) {
            return Err(diagnostic(
                "reactive_ui_process_missing_reply_payload",
                format!(
                    "reactive UI process `{process_name}` reply `{}` is missing payload field `{}`",
                    reply.name, expected.name
                ),
            ));
        }
    }
    Ok(())
}

/// Validates an existing process state snapshot.
fn validate_process_state_snapshot(
    process: &ReactiveUiProcessContract,
    state: &ReactiveUiProcessState,
) -> Result<(), ReactiveUiProcessDiagnostic> {
    let mut seen = BTreeSet::new();
    for field in &state.fields {
        if !seen.insert(field.name.as_str()) {
            return Err(diagnostic(
                "reactive_ui_process_duplicate_state_snapshot_field",
                format!(
                    "reactive UI process `{}` state snapshot repeats field `{}`",
                    process.name, field.name
                ),
            ));
        }
        let Some(expected) = find_state_field(process, &field.name) else {
            return Err(diagnostic(
                "reactive_ui_process_unknown_state_snapshot_field",
                format!(
                    "reactive UI process `{}` state snapshot has unknown field `{}`",
                    process.name, field.name
                ),
            ));
        };
        if expected.field_type.as_str() != field.value.type_name() {
            return Err(diagnostic(
                "reactive_ui_process_state_snapshot_type_mismatch",
                format!(
                    "reactive UI process `{}` state field `{}` expects {} but got {}",
                    process.name,
                    field.name,
                    expected.field_type.as_str(),
                    field.value.type_name()
                ),
            ));
        }
    }
    Ok(())
}

/// Validates a state patch payload against one state patch effect.
fn validate_state_patch_payload(
    process: &ReactiveUiProcessContract,
    effect: &ReactiveUiEffect,
    patch: &[MobileAngularBridgePayloadField],
) -> Result<(), ReactiveUiProcessDiagnostic> {
    let mut seen = BTreeSet::new();
    for field in patch {
        if !seen.insert(field.name.as_str()) {
            return Err(diagnostic(
                "reactive_ui_process_duplicate_state_patch_field",
                format!(
                    "reactive UI process `{}` state patch `{}` repeats field `{}`",
                    process.name, effect.name, field.name
                ),
            ));
        }
        let Some(expected_effect_field) = effect
            .payload
            .iter()
            .find(|expected| expected.name == field.name)
        else {
            return Err(diagnostic(
                "reactive_ui_process_unknown_state_patch_field",
                format!(
                    "reactive UI process `{}` state patch `{}` has unknown field `{}`",
                    process.name, effect.name, field.name
                ),
            ));
        };
        let Some(expected_state_field) = find_state_field(process, &field.name) else {
            return Err(diagnostic(
                "reactive_ui_process_state_patch_not_state_field",
                format!(
                    "reactive UI process `{}` state patch `{}` field `{}` is not process state",
                    process.name, effect.name, field.name
                ),
            ));
        };
        if expected_effect_field.field_type != expected_state_field.field_type
            || expected_state_field.field_type.as_str() != field.value.type_name()
        {
            return Err(diagnostic(
                "reactive_ui_process_state_patch_type_mismatch",
                format!(
                    "reactive UI process `{}` state patch `{}` field `{}` expects {} but got {}",
                    process.name,
                    effect.name,
                    field.name,
                    expected_state_field.field_type.as_str(),
                    field.value.type_name()
                ),
            ));
        }
    }
    Ok(())
}

/// Validates a named AngularTS payload against one process event.
fn validate_event_message_payload(
    process_name: &str,
    event: &ReactiveUiEvent,
    payload: &[MobileAngularBridgePayloadField],
) -> Result<(), ReactiveUiProcessDiagnostic> {
    let mut seen = BTreeSet::new();
    for actual in payload {
        if !seen.insert(actual.name.as_str()) {
            return Err(diagnostic(
                "reactive_ui_process_duplicate_message_payload",
                format!(
                    "reactive UI process `{process_name}` event `{}` repeats payload field `{}`",
                    event.name, actual.name
                ),
            ));
        }
        let Some(expected) = event
            .payload
            .iter()
            .find(|expected| expected.name == actual.name)
        else {
            return Err(diagnostic(
                "reactive_ui_process_unknown_message_payload",
                format!(
                    "reactive UI process `{process_name}` event `{}` has no payload field `{}`",
                    event.name, actual.name
                ),
            ));
        };
        if expected.field_type.as_str() != actual.value.type_name() {
            return Err(diagnostic(
                "reactive_ui_process_message_payload_type_mismatch",
                format!(
                    "reactive UI process `{process_name}` event `{}` field `{}` expects {} but got {}",
                    event.name,
                    actual.name,
                    expected.field_type.as_str(),
                    actual.value.type_name()
                ),
            ));
        }
    }
    for expected in &event.payload {
        if !seen.contains(expected.name.as_str()) {
            return Err(diagnostic(
                "reactive_ui_process_missing_message_payload",
                format!(
                    "reactive UI process `{process_name}` event `{}` is missing payload field `{}`",
                    event.name, expected.name
                ),
            ));
        }
    }
    Ok(())
}

/// Validates one state binding's directly owned names.
fn validate_state_binding_shape(
    binding: &ReactiveUiStateBinding,
) -> Vec<ReactiveUiProcessDiagnostic> {
    let mut diagnostics = Vec::new();
    if is_blank(&binding.process) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_binding_process",
            "reactive UI state binding process must not be empty",
        ));
    }
    if is_blank(&binding.state_field) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_binding_state_field",
            "reactive UI state binding state field must not be empty",
        ));
    }
    if is_blank(&binding.component_id) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_binding_component_id",
            "reactive UI state binding component id must not be empty",
        ));
    }
    if is_blank(&binding.component_prop) {
        diagnostics.push(diagnostic(
            "reactive_ui_process_empty_binding_component_prop",
            "reactive UI state binding component prop must not be empty",
        ));
    }
    diagnostics
}

/// Finds one process contract by name.
fn find_process<'a>(
    contracts: &'a [ReactiveUiProcessContract],
    name: &str,
) -> Option<&'a ReactiveUiProcessContract> {
    contracts.iter().find(|contract| contract.name == name)
}

/// Finds one process state field by name.
fn find_state_field<'a>(
    contract: &'a ReactiveUiProcessContract,
    name: &str,
) -> Option<&'a ReactiveUiStateField> {
    contract.state.iter().find(|field| field.name == name)
}

/// Finds one process event by name.
fn find_event<'a>(
    contract: &'a ReactiveUiProcessContract,
    name: &str,
) -> Option<&'a ReactiveUiEvent> {
    contract.events.iter().find(|event| event.name == name)
}

/// Finds one process reply by name.
fn find_reply<'a>(
    contract: &'a ReactiveUiProcessContract,
    name: &str,
) -> Option<&'a ReactiveUiReply> {
    contract.replies.iter().find(|reply| reply.name == name)
}

/// Validates process state fields.
fn validate_state_fields(contract: &ReactiveUiProcessContract) -> Vec<ReactiveUiProcessDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for field in &contract.state {
        if is_blank(&field.name) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_empty_state_field",
                format!(
                    "reactive UI process `{}` has an empty state field",
                    contract.name
                ),
            ));
        } else if !seen.insert(field.name.as_str()) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_state_field",
                format!(
                    "reactive UI process `{}` repeats state field `{}`",
                    contract.name, field.name
                ),
            ));
        }
    }
    diagnostics
}

/// Validates process input events.
fn validate_events(contract: &ReactiveUiProcessContract) -> Vec<ReactiveUiProcessDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for event in &contract.events {
        if is_blank(&event.name) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_empty_event",
                format!("reactive UI process `{}` has an empty event", contract.name),
            ));
        } else if !seen.insert(event.name.as_str()) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_event",
                format!(
                    "reactive UI process `{}` repeats event `{}`",
                    contract.name, event.name
                ),
            ));
        }
        diagnostics.extend(validate_payload_fields(
            "event",
            &contract.name,
            &event.name,
            &event.payload,
        ));
    }
    diagnostics
}

/// Validates process effects.
fn validate_effects(contract: &ReactiveUiProcessContract) -> Vec<ReactiveUiProcessDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for effect in &contract.effects {
        if is_blank(&effect.name) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_empty_effect",
                format!(
                    "reactive UI process `{}` has an empty effect",
                    contract.name
                ),
            ));
        } else if !seen.insert(effect.name.as_str()) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_effect",
                format!(
                    "reactive UI process `{}` repeats effect `{}`",
                    contract.name, effect.name
                ),
            ));
        }
        if effect.effect_type.requires_capability() && effect.required_capability.is_none() {
            diagnostics.push(diagnostic(
                "reactive_ui_process_missing_effect_capability",
                format!(
                    "reactive UI process `{}` effect `{}` requires a capability",
                    contract.name, effect.name
                ),
            ));
        }
        diagnostics.extend(validate_payload_fields(
            "effect",
            &contract.name,
            &effect.name,
            &effect.payload,
        ));
    }
    diagnostics
}

/// Validates native/async replies accepted by a process.
fn validate_replies(contract: &ReactiveUiProcessContract) -> Vec<ReactiveUiProcessDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    let effects = contract
        .effects
        .iter()
        .map(|effect| effect.name.as_str())
        .collect::<BTreeSet<_>>();
    for reply in &contract.replies {
        if is_blank(&reply.name) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_empty_reply",
                format!("reactive UI process `{}` has an empty reply", contract.name),
            ));
        } else if !seen.insert(reply.name.as_str()) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_reply",
                format!(
                    "reactive UI process `{}` repeats reply `{}`",
                    contract.name, reply.name
                ),
            ));
        }
        if let Some(source_effect) = reply.source_effect.as_ref() {
            if !effects.contains(source_effect.as_str()) {
                diagnostics.push(diagnostic(
                    "reactive_ui_process_unknown_reply_effect",
                    format!(
                        "reactive UI process `{}` reply `{}` references unknown effect `{}`",
                        contract.name, reply.name, source_effect
                    ),
                ));
            }
        }
        diagnostics.extend(validate_payload_fields(
            "reply",
            &contract.name,
            &reply.name,
            &reply.payload,
        ));
    }
    diagnostics
}

/// Validates repeated or empty payload field names.
fn validate_payload_fields(
    owner_kind: &str,
    process_name: &str,
    owner_name: &str,
    fields: &[MobileBridgeField],
) -> Vec<ReactiveUiProcessDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for field in fields {
        if is_blank(&field.name) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_empty_payload_field",
                format!(
                    "reactive UI process `{process_name}` {owner_kind} `{owner_name}` has an empty payload field"
                ),
            ));
        } else if !seen.insert(field.name.as_str()) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_payload_field",
                format!(
                    "reactive UI process `{process_name}` {owner_kind} `{owner_name}` repeats payload field `{}`",
                    field.name
                ),
            ));
        }
    }
    diagnostics
}

/// Returns whether a name-like value is blank.
fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

/// Builds one reactive UI process diagnostic.
fn diagnostic(code: &'static str, message: impl Into<String>) -> ReactiveUiProcessDiagnostic {
    ReactiveUiProcessDiagnostic {
        code,
        message: message.into(),
    }
}
