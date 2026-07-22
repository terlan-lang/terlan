use std::collections::BTreeSet;

use super::mobile_angular_bridge::MobileAngularBridgePayloadField;
use super::mobile_bridge::{MobileBridgeCapability, MobileBridgeField, MobileBridgeType};

/// One reactive UI process contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiProcessContract {
    pub(crate) name: String,
    pub(crate) state: Vec<ReactiveUiStateField>,
    pub(crate) events: Vec<ReactiveUiEvent>,
    pub(crate) effects: Vec<ReactiveUiEffect>,
    pub(crate) replies: Vec<ReactiveUiReply>,
}

/// One typed state field owned by a reactive UI process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiStateField {
    pub(crate) name: String,
    pub(crate) field_type: MobileBridgeType,
}

/// One input event accepted by a reactive UI process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiEvent {
    pub(crate) name: String,
    pub(crate) payload: Vec<MobileBridgeField>,
}

/// One effect emitted by a reactive UI process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiEffect {
    pub(crate) name: String,
    pub(crate) effect_type: ReactiveUiEffectType,
    pub(crate) required_capability: Option<MobileBridgeCapability>,
    pub(crate) payload: Vec<MobileBridgeField>,
}

/// Canonical effect category emitted by a reactive UI process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReactiveUiEffectType {
    StatePatch,
    Navigation,
    NativeCommand,
    PlatformPermission,
    NativeResource,
}

impl ReactiveUiEffectType {
    /// Returns the stable effect type spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::StatePatch => "state_patch",
            Self::Navigation => "navigation",
            Self::NativeCommand => "native_command",
            Self::PlatformPermission => "platform_permission",
            Self::NativeResource => "native_resource",
        }
    }

    /// Returns whether this effect kind must declare a native bridge capability.
    const fn requires_capability(self) -> bool {
        !matches!(self, Self::StatePatch)
    }
}

/// One native/async reply accepted by a reactive UI process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiReply {
    pub(crate) name: String,
    pub(crate) source_effect: Option<String>,
    pub(crate) payload: Vec<MobileBridgeField>,
}

/// Reactive UI process validation diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiProcessDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Generated metadata for reactive UI processes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiProcessMetadata {
    pub(crate) schema_version: u32,
    pub(crate) processes: Vec<ReactiveUiProcessMetadataEntry>,
}

/// Generated metadata for one reactive UI process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiProcessMetadataEntry {
    pub(crate) name: String,
    pub(crate) state: Vec<ReactiveUiMetadataField>,
    pub(crate) events: Vec<ReactiveUiMetadataEvent>,
    pub(crate) effects: Vec<ReactiveUiMetadataEffect>,
    pub(crate) replies: Vec<ReactiveUiMetadataReply>,
}

/// Generated metadata for one typed field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiMetadataField {
    pub(crate) name: String,
    pub(crate) field_type: &'static str,
}

/// Generated metadata for one input event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiMetadataEvent {
    pub(crate) name: String,
    pub(crate) payload: Vec<ReactiveUiMetadataField>,
}

/// Generated metadata for one effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiMetadataEffect {
    pub(crate) name: String,
    pub(crate) effect_type: &'static str,
    pub(crate) required_capability: Option<&'static str>,
    pub(crate) payload: Vec<ReactiveUiMetadataField>,
}

/// Generated metadata for one native/async reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiMetadataReply {
    pub(crate) name: String,
    pub(crate) source_effect: Option<String>,
    pub(crate) payload: Vec<ReactiveUiMetadataField>,
}

/// One binding from process state to an AngularTS component target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiStateBinding {
    pub(crate) process: String,
    pub(crate) state_field: String,
    pub(crate) component_id: String,
    pub(crate) component_prop: String,
    pub(crate) binding_kind: ReactiveUiBindingKind,
}

/// AngularTS binding target kind for one state binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReactiveUiBindingKind {
    Text,
    Prop,
    Attribute,
    Class,
    Style,
}

impl ReactiveUiBindingKind {
    /// Returns the stable binding kind spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Prop => "prop",
            Self::Attribute => "attribute",
            Self::Class => "class",
            Self::Style => "style",
        }
    }
}

/// Generated metadata for state-to-component bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiStateBindingMetadata {
    pub(crate) schema_version: u32,
    pub(crate) bindings: Vec<ReactiveUiStateBindingMetadataEntry>,
}

/// Generated metadata for one state-to-component binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiStateBindingMetadataEntry {
    pub(crate) process: String,
    pub(crate) state_field: String,
    pub(crate) state_type: &'static str,
    pub(crate) component_id: String,
    pub(crate) component_prop: String,
    pub(crate) binding_kind: &'static str,
}

/// One binding from an AngularTS component event to a process event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiEventBinding {
    pub(crate) process: String,
    pub(crate) event: String,
    pub(crate) component_id: String,
    pub(crate) component_event: String,
}

/// Generated metadata for component-event to process-event bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiEventBindingMetadata {
    pub(crate) schema_version: u32,
    pub(crate) bindings: Vec<ReactiveUiEventBindingMetadataEntry>,
}

/// Generated metadata for one component-event to process-event binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiEventBindingMetadataEntry {
    pub(crate) process: String,
    pub(crate) event: String,
    pub(crate) component_id: String,
    pub(crate) component_event: String,
    pub(crate) payload: Vec<ReactiveUiMetadataField>,
}

/// Typed process message produced by an AngularTS event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiProcessMessage {
    pub(crate) process: String,
    pub(crate) event: String,
    pub(crate) payload: Vec<MobileAngularBridgePayloadField>,
}

/// One binding from a native bridge command reply to a process reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiNativeReplyBinding {
    pub(crate) process: String,
    pub(crate) reply: String,
    pub(crate) bridge: String,
    pub(crate) command: String,
}

/// Generated metadata for native bridge reply to process reply bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiNativeReplyBindingMetadata {
    pub(crate) schema_version: u32,
    pub(crate) bindings: Vec<ReactiveUiNativeReplyBindingMetadataEntry>,
}

/// Generated metadata for one native bridge reply binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiNativeReplyBindingMetadataEntry {
    pub(crate) process: String,
    pub(crate) reply: String,
    pub(crate) source_effect: Option<String>,
    pub(crate) bridge: String,
    pub(crate) command: String,
    pub(crate) payload: Vec<ReactiveUiMetadataField>,
}

/// Typed process message produced by a native bridge reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiProcessReplyMessage {
    pub(crate) process: String,
    pub(crate) reply: String,
    pub(crate) source_effect: Option<String>,
    pub(crate) payload: Vec<MobileAngularBridgePayloadField>,
}

/// Typed state snapshot for one reactive UI process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiProcessState {
    pub(crate) process: String,
    pub(crate) fields: Vec<MobileAngularBridgePayloadField>,
}

/// One deterministic native bridge reply fixture for process-driven UI tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiNativeReplyFixture {
    pub(crate) fixture_id: String,
    pub(crate) process: String,
    pub(crate) reply: String,
    pub(crate) payload: Vec<MobileAngularBridgePayloadField>,
}

/// Deterministic replay result for native bridge reply fixtures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactiveUiNativeReplyFixtureReplay {
    pub(crate) schema_version: u32,
    pub(crate) messages: Vec<ReactiveUiProcessReplyMessage>,
}

/// Validates reactive UI process contracts.
pub(crate) fn validate_reactive_ui_process_contracts(
    contracts: &[ReactiveUiProcessContract],
) -> Result<(), Vec<ReactiveUiProcessDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut process_names = BTreeSet::new();

    for contract in contracts {
        if is_blank(&contract.name) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_empty_name",
                "reactive UI process name must not be empty",
            ));
        } else if !process_names.insert(contract.name.as_str()) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_name",
                format!(
                    "reactive UI process `{}` is declared more than once",
                    contract.name
                ),
            ));
        }
        diagnostics.extend(validate_state_fields(contract));
        diagnostics.extend(validate_events(contract));
        diagnostics.extend(validate_effects(contract));
        diagnostics.extend(validate_replies(contract));
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Generates reactive UI process metadata after validation.
pub(crate) fn generate_reactive_ui_process_metadata(
    contracts: &[ReactiveUiProcessContract],
) -> Result<ReactiveUiProcessMetadata, Vec<ReactiveUiProcessDiagnostic>> {
    validate_reactive_ui_process_contracts(contracts)?;
    Ok(ReactiveUiProcessMetadata {
        schema_version: 1,
        processes: contracts.iter().map(process_metadata_entry).collect(),
    })
}

/// Builds a typed navigation effect helper.
pub(crate) fn reactive_ui_navigation_effect(name: &str) -> ReactiveUiEffect {
    ReactiveUiEffect {
        name: name.to_string(),
        effect_type: ReactiveUiEffectType::Navigation,
        required_capability: Some(MobileBridgeCapability::Navigation),
        payload: vec![bridge_field("route", MobileBridgeType::String)],
    }
}

/// Builds a typed native component command effect helper.
pub(crate) fn reactive_ui_native_component_command_effect(name: &str) -> ReactiveUiEffect {
    ReactiveUiEffect {
        name: name.to_string(),
        effect_type: ReactiveUiEffectType::NativeCommand,
        required_capability: Some(MobileBridgeCapability::NativeComponents),
        payload: vec![
            bridge_field("component_id", MobileBridgeType::String),
            bridge_field("command", MobileBridgeType::String),
            bridge_field("payload", MobileBridgeType::Json),
        ],
    }
}

/// Builds a typed platform permission effect helper.
pub(crate) fn reactive_ui_platform_permission_effect(name: &str) -> ReactiveUiEffect {
    ReactiveUiEffect {
        name: name.to_string(),
        effect_type: ReactiveUiEffectType::PlatformPermission,
        required_capability: Some(MobileBridgeCapability::Permissions),
        payload: vec![
            bridge_field("permission", MobileBridgeType::String),
            bridge_field("reason", MobileBridgeType::String),
        ],
    }
}

/// Builds a typed native resource effect helper.
pub(crate) fn reactive_ui_native_resource_effect(
    name: &str,
    capability: MobileBridgeCapability,
) -> Result<ReactiveUiEffect, ReactiveUiProcessDiagnostic> {
    if !is_native_resource_capability(capability) {
        return Err(diagnostic(
            "reactive_ui_process_invalid_native_resource_capability",
            format!(
                "reactive UI native resource effect `{name}` cannot use capability `{}`",
                capability.as_str()
            ),
        ));
    }
    Ok(ReactiveUiEffect {
        name: name.to_string(),
        effect_type: ReactiveUiEffectType::NativeResource,
        required_capability: Some(capability),
        payload: vec![
            bridge_field("resource", MobileBridgeType::String),
            bridge_field("action", MobileBridgeType::String),
            bridge_field("request", MobileBridgeType::Json),
        ],
    })
}

/// Generates AngularTS component binding metadata from validated process state.
pub(crate) fn generate_reactive_ui_state_binding_metadata(
    contracts: &[ReactiveUiProcessContract],
    bindings: &[ReactiveUiStateBinding],
) -> Result<ReactiveUiStateBindingMetadata, Vec<ReactiveUiProcessDiagnostic>> {
    validate_reactive_ui_process_contracts(contracts)?;
    validate_reactive_ui_state_bindings(contracts, bindings)?;
    Ok(ReactiveUiStateBindingMetadata {
        schema_version: 1,
        bindings: bindings
            .iter()
            .map(|binding| state_binding_metadata_entry(contracts, binding))
            .collect(),
    })
}

/// Validates state-to-AngularTS component bindings.
pub(crate) fn validate_reactive_ui_state_bindings(
    contracts: &[ReactiveUiProcessContract],
    bindings: &[ReactiveUiStateBinding],
) -> Result<(), Vec<ReactiveUiProcessDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut targets = BTreeSet::new();

    for binding in bindings {
        diagnostics.extend(validate_state_binding_shape(binding));
        if !targets.insert((
            binding.component_id.as_str(),
            binding.component_prop.as_str(),
        )) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_state_binding_target",
                format!(
                    "reactive UI component `{}` prop `{}` has more than one state binding",
                    binding.component_id, binding.component_prop
                ),
            ));
        }
        let Some(process) = find_process(contracts, &binding.process) else {
            diagnostics.push(diagnostic(
                "reactive_ui_process_unknown_binding_process",
                format!(
                    "reactive UI state binding references unknown process `{}`",
                    binding.process
                ),
            ));
            continue;
        };
        if find_state_field(process, &binding.state_field).is_none() {
            diagnostics.push(diagnostic(
                "reactive_ui_process_unknown_binding_state_field",
                format!(
                    "reactive UI state binding references unknown state field `{}` on process `{}`",
                    binding.state_field, binding.process
                ),
            ));
        }
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Generates AngularTS event binding metadata from validated process events.
pub(crate) fn generate_reactive_ui_event_binding_metadata(
    contracts: &[ReactiveUiProcessContract],
    bindings: &[ReactiveUiEventBinding],
) -> Result<ReactiveUiEventBindingMetadata, Vec<ReactiveUiProcessDiagnostic>> {
    validate_reactive_ui_process_contracts(contracts)?;
    validate_reactive_ui_event_bindings(contracts, bindings)?;
    Ok(ReactiveUiEventBindingMetadata {
        schema_version: 1,
        bindings: bindings
            .iter()
            .map(|binding| event_binding_metadata_entry(contracts, binding))
            .collect(),
    })
}

/// Validates AngularTS event to process event bindings.
pub(crate) fn validate_reactive_ui_event_bindings(
    contracts: &[ReactiveUiProcessContract],
    bindings: &[ReactiveUiEventBinding],
) -> Result<(), Vec<ReactiveUiProcessDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut targets = BTreeSet::new();

    for binding in bindings {
        diagnostics.extend(validate_event_binding_shape(binding));
        if !targets.insert((
            binding.component_id.as_str(),
            binding.component_event.as_str(),
            binding.process.as_str(),
        )) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_event_binding_target",
                format!(
                    "reactive UI component `{}` event `{}` is bound more than once to process `{}`",
                    binding.component_id, binding.component_event, binding.process
                ),
            ));
        }
        let Some(process) = find_process(contracts, &binding.process) else {
            diagnostics.push(diagnostic(
                "reactive_ui_process_unknown_event_binding_process",
                format!(
                    "reactive UI event binding references unknown process `{}`",
                    binding.process
                ),
            ));
            continue;
        };
        if find_event(process, &binding.event).is_none() {
            diagnostics.push(diagnostic(
                "reactive_ui_process_unknown_event_binding_event",
                format!(
                    "reactive UI event binding references unknown event `{}` on process `{}`",
                    binding.event, binding.process
                ),
            ));
        }
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Encodes an AngularTS event payload as a typed Terlan process message.
pub(crate) fn encode_reactive_ui_event_message(
    contracts: &[ReactiveUiProcessContract],
    process_name: &str,
    event_name: &str,
    payload: &[MobileAngularBridgePayloadField],
) -> Result<ReactiveUiProcessMessage, ReactiveUiProcessDiagnostic> {
    let process = find_process(contracts, process_name).ok_or_else(|| {
        diagnostic(
            "reactive_ui_process_unknown_message_process",
            format!("reactive UI message references unknown process `{process_name}`"),
        )
    })?;
    let event = find_event(process, event_name).ok_or_else(|| {
        diagnostic(
            "reactive_ui_process_unknown_message_event",
            format!(
                "reactive UI message references unknown event `{event_name}` on process `{process_name}`"
            ),
        )
    })?;
    validate_event_message_payload(process_name, event, payload)?;
    Ok(ReactiveUiProcessMessage {
        process: process_name.to_string(),
        event: event_name.to_string(),
        payload: payload.to_vec(),
    })
}

/// Generates native bridge reply binding metadata from validated process replies.
pub(crate) fn generate_reactive_ui_native_reply_binding_metadata(
    contracts: &[ReactiveUiProcessContract],
    bindings: &[ReactiveUiNativeReplyBinding],
) -> Result<ReactiveUiNativeReplyBindingMetadata, Vec<ReactiveUiProcessDiagnostic>> {
    validate_reactive_ui_process_contracts(contracts)?;
    validate_reactive_ui_native_reply_bindings(contracts, bindings)?;
    Ok(ReactiveUiNativeReplyBindingMetadata {
        schema_version: 1,
        bindings: bindings
            .iter()
            .map(|binding| native_reply_binding_metadata_entry(contracts, binding))
            .collect(),
    })
}

/// Validates native bridge reply to process reply bindings.
pub(crate) fn validate_reactive_ui_native_reply_bindings(
    contracts: &[ReactiveUiProcessContract],
    bindings: &[ReactiveUiNativeReplyBinding],
) -> Result<(), Vec<ReactiveUiProcessDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut targets = BTreeSet::new();

    for binding in bindings {
        diagnostics.extend(validate_native_reply_binding_shape(binding));
        if !targets.insert((
            binding.bridge.as_str(),
            binding.command.as_str(),
            binding.process.as_str(),
            binding.reply.as_str(),
        )) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_native_reply_binding_target",
                format!(
                    "reactive UI native bridge `{}.{}` is bound more than once to process `{}` reply `{}`",
                    binding.bridge, binding.command, binding.process, binding.reply
                ),
            ));
        }
        let Some(process) = find_process(contracts, &binding.process) else {
            diagnostics.push(diagnostic(
                "reactive_ui_process_unknown_native_reply_binding_process",
                format!(
                    "reactive UI native reply binding references unknown process `{}`",
                    binding.process
                ),
            ));
            continue;
        };
        if find_reply(process, &binding.reply).is_none() {
            diagnostics.push(diagnostic(
                "reactive_ui_process_unknown_native_reply_binding_reply",
                format!(
                    "reactive UI native reply binding references unknown reply `{}` on process `{}`",
                    binding.reply, binding.process
                ),
            ));
        }
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Encodes a native bridge reply payload as a typed Terlan process reply message.
pub(crate) fn encode_reactive_ui_reply_message(
    contracts: &[ReactiveUiProcessContract],
    process_name: &str,
    reply_name: &str,
    payload: &[MobileAngularBridgePayloadField],
) -> Result<ReactiveUiProcessReplyMessage, ReactiveUiProcessDiagnostic> {
    let process = find_process(contracts, process_name).ok_or_else(|| {
        diagnostic(
            "reactive_ui_process_unknown_reply_message_process",
            format!("reactive UI reply message references unknown process `{process_name}`"),
        )
    })?;
    let reply = find_reply(process, reply_name).ok_or_else(|| {
        diagnostic(
            "reactive_ui_process_unknown_reply_message_reply",
            format!(
                "reactive UI reply message references unknown reply `{reply_name}` on process `{process_name}`"
            ),
        )
    })?;
    validate_reply_message_payload(process_name, reply, payload)?;
    Ok(ReactiveUiProcessReplyMessage {
        process: process_name.to_string(),
        reply: reply_name.to_string(),
        source_effect: reply.source_effect.clone(),
        payload: payload.to_vec(),
    })
}

/// Replays native bridge reply fixtures into deterministic process messages.
pub(crate) fn replay_reactive_ui_native_reply_fixtures(
    contracts: &[ReactiveUiProcessContract],
    fixtures: &[ReactiveUiNativeReplyFixture],
) -> Result<ReactiveUiNativeReplyFixtureReplay, Vec<ReactiveUiProcessDiagnostic>> {
    validate_reactive_ui_process_contracts(contracts)?;
    let mut diagnostics = Vec::new();
    let mut fixture_ids = BTreeSet::new();
    let mut messages = Vec::new();

    for fixture in fixtures {
        if is_blank(&fixture.fixture_id) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_empty_reply_fixture_id",
                "reactive UI native reply fixture id must not be empty",
            ));
        } else if !fixture_ids.insert(fixture.fixture_id.as_str()) {
            diagnostics.push(diagnostic(
                "reactive_ui_process_duplicate_reply_fixture_id",
                format!(
                    "reactive UI native reply fixture `{}` is declared more than once",
                    fixture.fixture_id
                ),
            ));
        }
        match encode_reactive_ui_reply_message(
            contracts,
            &fixture.process,
            &fixture.reply,
            &fixture.payload,
        ) {
            Ok(message) => messages.push(message),
            Err(reply_diagnostic) => diagnostics.push(diagnostic(
                reply_diagnostic.code,
                format!(
                    "reactive UI native reply fixture `{}` failed: {}",
                    fixture.fixture_id, reply_diagnostic.message
                ),
            )),
        }
    }

    if diagnostics.is_empty() {
        Ok(ReactiveUiNativeReplyFixtureReplay {
            schema_version: 1,
            messages,
        })
    } else {
        Err(diagnostics)
    }
}

/// Applies a typed state patch effect payload to one process state snapshot.
pub(crate) fn apply_reactive_ui_state_patch(
    contracts: &[ReactiveUiProcessContract],
    state: &ReactiveUiProcessState,
    effect_name: &str,
    patch: &[MobileAngularBridgePayloadField],
) -> Result<ReactiveUiProcessState, ReactiveUiProcessDiagnostic> {
    let process = find_process(contracts, &state.process).ok_or_else(|| {
        diagnostic(
            "reactive_ui_process_unknown_state_patch_process",
            format!(
                "reactive UI state patch references unknown process `{}`",
                state.process
            ),
        )
    })?;
    validate_process_state_snapshot(process, state)?;
    let effect = process
        .effects
        .iter()
        .find(|effect| effect.name == effect_name)
        .ok_or_else(|| {
            diagnostic(
                "reactive_ui_process_unknown_state_patch_effect",
                format!(
                    "reactive UI state patch references unknown effect `{effect_name}` on process `{}`",
                    state.process
                ),
            )
        })?;
    if effect.effect_type != ReactiveUiEffectType::StatePatch {
        return Err(diagnostic(
            "reactive_ui_process_non_state_patch_effect",
            format!(
                "reactive UI process `{}` effect `{effect_name}` is not a state patch",
                state.process
            ),
        ));
    }
    validate_state_patch_payload(process, effect, patch)?;
    let mut next = state.clone();
    for field in patch {
        if let Some(existing) = next
            .fields
            .iter_mut()
            .find(|existing| existing.name == field.name)
        {
            *existing = field.clone();
        } else {
            next.fields.push(field.clone());
        }
    }
    Ok(next)
}

/// Converts one process contract to generated metadata.
fn process_metadata_entry(contract: &ReactiveUiProcessContract) -> ReactiveUiProcessMetadataEntry {
    ReactiveUiProcessMetadataEntry {
        name: contract.name.clone(),
        state: contract
            .state
            .iter()
            .map(|field| metadata_field(&field.name, field.field_type))
            .collect(),
        events: contract.events.iter().map(metadata_event).collect(),
        effects: contract.effects.iter().map(metadata_effect).collect(),
        replies: contract.replies.iter().map(metadata_reply).collect(),
    }
}

/// Converts one state binding to generated metadata.
fn state_binding_metadata_entry(
    contracts: &[ReactiveUiProcessContract],
    binding: &ReactiveUiStateBinding,
) -> ReactiveUiStateBindingMetadataEntry {
    let process = find_process(contracts, &binding.process).expect("validated process binding");
    let state_field =
        find_state_field(process, &binding.state_field).expect("validated state binding");
    ReactiveUiStateBindingMetadataEntry {
        process: binding.process.clone(),
        state_field: binding.state_field.clone(),
        state_type: state_field.field_type.as_str(),
        component_id: binding.component_id.clone(),
        component_prop: binding.component_prop.clone(),
        binding_kind: binding.binding_kind.as_str(),
    }
}

/// Converts one native reply binding to generated metadata.
fn native_reply_binding_metadata_entry(
    contracts: &[ReactiveUiProcessContract],
    binding: &ReactiveUiNativeReplyBinding,
) -> ReactiveUiNativeReplyBindingMetadataEntry {
    let process = find_process(contracts, &binding.process).expect("validated reply binding");
    let reply = find_reply(process, &binding.reply).expect("validated reply binding");
    ReactiveUiNativeReplyBindingMetadataEntry {
        process: binding.process.clone(),
        reply: binding.reply.clone(),
        source_effect: reply.source_effect.clone(),
        bridge: binding.bridge.clone(),
        command: binding.command.clone(),
        payload: reply
            .payload
            .iter()
            .map(|field| metadata_field(&field.name, field.field_type))
            .collect(),
    }
}

/// Converts one event binding to generated metadata.
fn event_binding_metadata_entry(
    contracts: &[ReactiveUiProcessContract],
    binding: &ReactiveUiEventBinding,
) -> ReactiveUiEventBindingMetadataEntry {
    let process = find_process(contracts, &binding.process).expect("validated event binding");
    let event = find_event(process, &binding.event).expect("validated event binding");
    ReactiveUiEventBindingMetadataEntry {
        process: binding.process.clone(),
        event: binding.event.clone(),
        component_id: binding.component_id.clone(),
        component_event: binding.component_event.clone(),
        payload: event
            .payload
            .iter()
            .map(|field| metadata_field(&field.name, field.field_type))
            .collect(),
    }
}

/// Converts one event to generated metadata.
fn metadata_event(event: &ReactiveUiEvent) -> ReactiveUiMetadataEvent {
    ReactiveUiMetadataEvent {
        name: event.name.clone(),
        payload: event
            .payload
            .iter()
            .map(|field| metadata_field(&field.name, field.field_type))
            .collect(),
    }
}
