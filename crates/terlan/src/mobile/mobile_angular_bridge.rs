//! AngularTS mobile bridge runtime helper generation.
#![allow(dead_code)]
//!
//! Inputs:
//! - Validated mobile bridge metadata.
//!
//! Outputs:
//! - Typed command/event envelopes and a small JavaScript runtime helper source
//!   for AngularTS/native shell communication.
//!
//! Transformation:
//! - Converts compiler-owned bridge metadata into deterministic runtime helper
//!   shapes without executing JavaScript or depending on a platform shell.

use std::collections::BTreeSet;

use super::mobile_bridge::{
    MobileBridgeMetadata, MobileBridgeMetadataDeclaration, MobileBridgeMetadataField,
};
use super::mobile_widget::{MobileWidgetMetadata, MobileWidgetMetadataEntry};

const MISSING_NATIVE_SHELL_FALLBACK_MESSAGE: &str = "terlan native bridge unavailable";

/// Runtime helper diagnostic for AngularTS mobile bridge generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileAngularBridgeDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Typed value accepted by the AngularTS bridge envelope model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MobileAngularBridgeValue {
    Unit,
    Bool(bool),
    Int(i64),
    Float(String),
    String(String),
    Json(String),
}

impl MobileAngularBridgeValue {
    /// Returns the stable bridge type spelling for this value.
    pub(crate) const fn type_name(&self) -> &'static str {
        match self {
            Self::Unit => "Unit",
            Self::Bool(_) => "Bool",
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::String(_) => "String",
            Self::Json(_) => "Json",
        }
    }
}

/// One named payload entry in a bridge command/event envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileAngularBridgePayloadField {
    pub(crate) name: String,
    pub(crate) value: MobileAngularBridgeValue,
}

/// Typed command envelope sent from AngularTS/Terlan UI code to native shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileAngularBridgeCommandEnvelope {
    pub(crate) request_id: String,
    pub(crate) bridge: String,
    pub(crate) command: String,
    pub(crate) capability: String,
    pub(crate) payload: Vec<MobileAngularBridgePayloadField>,
}

/// Typed event envelope received from native shell by AngularTS/Terlan UI code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileAngularBridgeEventEnvelope {
    pub(crate) bridge: String,
    pub(crate) event: String,
    pub(crate) payload: Vec<MobileAngularBridgePayloadField>,
}

/// Native shell platform family exposed to AngularTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileAngularPlatform {
    Android,
    Ios,
    Web,
    Unknown,
}

impl MobileAngularPlatform {
    /// Returns the stable environment spelling for one platform.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
            Self::Web => "web",
            Self::Unknown => "unknown",
        }
    }
}

/// Native shell theme exposed to AngularTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileAngularTheme {
    Light,
    Dark,
    System,
}

impl MobileAngularTheme {
    /// Returns the stable environment spelling for one theme.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

/// Native safe-area insets exposed to AngularTS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileAngularSafeArea {
    pub(crate) top_px: u32,
    pub(crate) right_px: u32,
    pub(crate) bottom_px: u32,
    pub(crate) left_px: u32,
}

/// Platform/theme environment exposed by the native shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileAngularPlatformEnvironment {
    pub(crate) platform: MobileAngularPlatform,
    pub(crate) theme: MobileAngularTheme,
    pub(crate) locale: String,
    pub(crate) density_scale: String,
    pub(crate) safe_area: MobileAngularSafeArea,
}

/// Typed platform environment data visible to AngularTS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileAngularPlatformEnvironmentData {
    pub(crate) fields: Vec<MobileAngularBridgePayloadField>,
}

/// One CSS variable generated from platform environment data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileAngularCssVariable {
    pub(crate) name: String,
    pub(crate) value: String,
}

/// Native component lifecycle operation sent by AngularTS to a mobile shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileAngularComponentOperation {
    Mount,
    Update,
    Unmount,
}

impl MobileAngularComponentOperation {
    /// Returns the stable lifecycle operation spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Mount => "mount",
            Self::Update => "update",
            Self::Unmount => "unmount",
        }
    }
}

/// Typed native component lifecycle envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileAngularComponentLifecycleEnvelope {
    pub(crate) component_id: String,
    pub(crate) widget: String,
    pub(crate) selector: String,
    pub(crate) native_component: String,
    pub(crate) operation: &'static str,
    pub(crate) capability: &'static str,
    pub(crate) props: Vec<MobileAngularBridgePayloadField>,
}

/// Encodes one typed command envelope from bridge metadata.
///
/// Inputs:
/// - `metadata`: validated mobile bridge metadata.
/// - `bridge_name`: bridge declaration name.
/// - `command_name`: command name inside the bridge.
/// - `request_id`: caller-generated request id.
/// - `args`: positional values matching command parameters.
///
/// Output:
/// - Typed command envelope when arity and value types match metadata.
/// - Stable diagnostics for unknown bridge/command, empty id, arity mismatch,
///   or type mismatch.
///
/// Transformation:
/// - Converts positional values into named payload fields using metadata
///   parameter order.
pub(crate) fn encode_mobile_angular_bridge_command(
    metadata: &MobileBridgeMetadata,
    bridge_name: &str,
    command_name: &str,
    request_id: &str,
    args: &[MobileAngularBridgeValue],
) -> Result<MobileAngularBridgeCommandEnvelope, MobileAngularBridgeDiagnostic> {
    if request_id.trim().is_empty() {
        return Err(diagnostic(
            "mobile_angular_bridge_empty_request_id",
            "mobile bridge request id must not be empty",
        ));
    }
    let declaration = find_declaration(metadata, bridge_name)?;
    let command = declaration
        .commands
        .iter()
        .find(|command| command.name == command_name)
        .ok_or_else(|| {
            diagnostic(
                "mobile_angular_bridge_unknown_command",
                format!("mobile bridge `{bridge_name}` has no command `{command_name}`"),
            )
        })?;
    let payload = encode_payload_fields(
        bridge_name,
        "command",
        command_name,
        &command.parameters,
        args,
    )?;
    Ok(MobileAngularBridgeCommandEnvelope {
        request_id: request_id.to_string(),
        bridge: bridge_name.to_string(),
        command: command_name.to_string(),
        capability: command.required_capability.to_string(),
        payload,
    })
}

/// Decodes one typed native event envelope from bridge metadata.
///
/// Inputs:
/// - `metadata`: validated mobile bridge metadata.
/// - `bridge_name`: bridge declaration name.
/// - `event_name`: native event name inside the bridge.
/// - `payload`: positional values matching event payload metadata.
///
/// Output:
/// - Typed event envelope when arity and value types match metadata.
/// - Stable diagnostics for unknown bridge/event, arity mismatch, or type
///   mismatch.
///
/// Transformation:
/// - Converts positional native payload values into named event payload fields
///   using metadata payload order.
pub(crate) fn decode_mobile_angular_bridge_event(
    metadata: &MobileBridgeMetadata,
    bridge_name: &str,
    event_name: &str,
    payload: &[MobileAngularBridgeValue],
) -> Result<MobileAngularBridgeEventEnvelope, MobileAngularBridgeDiagnostic> {
    let declaration = find_declaration(metadata, bridge_name)?;
    let event = declaration
        .events
        .iter()
        .find(|event| event.name == event_name)
        .ok_or_else(|| {
            diagnostic(
                "mobile_angular_bridge_unknown_event",
                format!("mobile bridge `{bridge_name}` has no event `{event_name}`"),
            )
        })?;
    Ok(MobileAngularBridgeEventEnvelope {
        bridge: bridge_name.to_string(),
        event: event_name.to_string(),
        payload: encode_payload_fields(bridge_name, "event", event_name, &event.payload, payload)?,
    })
}

/// Encodes native platform/theme environment as typed AngularTS data.
///
/// Inputs:
/// - `environment`: native shell environment values.
///
/// Output:
/// - Named typed fields for platform, theme, locale, density scale, and
///   safe-area insets.
///
/// Transformation:
/// - Converts native shell values into the same typed payload field model used
///   by bridge command/event helpers.
pub(crate) fn encode_mobile_angular_platform_environment(
    environment: &MobileAngularPlatformEnvironment,
) -> MobileAngularPlatformEnvironmentData {
    MobileAngularPlatformEnvironmentData {
        fields: vec![
            payload_field(
                "platform",
                MobileAngularBridgeValue::String(environment.platform.as_str().to_string()),
            ),
            payload_field(
                "theme",
                MobileAngularBridgeValue::String(environment.theme.as_str().to_string()),
            ),
            payload_field(
                "locale",
                MobileAngularBridgeValue::String(environment.locale.clone()),
            ),
            payload_field(
                "density_scale",
                MobileAngularBridgeValue::Float(environment.density_scale.clone()),
            ),
            payload_field(
                "safe_area_top",
                MobileAngularBridgeValue::Int(i64::from(environment.safe_area.top_px)),
            ),
            payload_field(
                "safe_area_right",
                MobileAngularBridgeValue::Int(i64::from(environment.safe_area.right_px)),
            ),
            payload_field(
                "safe_area_bottom",
                MobileAngularBridgeValue::Int(i64::from(environment.safe_area.bottom_px)),
            ),
            payload_field(
                "safe_area_left",
                MobileAngularBridgeValue::Int(i64::from(environment.safe_area.left_px)),
            ),
        ],
    }
}

/// Generates CSS variables from native platform/theme environment values.
///
/// Inputs:
/// - `environment`: native shell environment values.
///
/// Output:
/// - Stable `--terlan-*` CSS variable names and CSS-ready values.
///
/// Transformation:
/// - Converts typed environment fields into browser styling values without
///   requiring AngularTS to understand native platform-specific structures.
pub(crate) fn mobile_angular_platform_environment_css_variables(
    environment: &MobileAngularPlatformEnvironment,
) -> Vec<MobileAngularCssVariable> {
    vec![
        css_variable("--terlan-platform", environment.platform.as_str()),
        css_variable("--terlan-theme", environment.theme.as_str()),
        css_variable("--terlan-locale", &environment.locale),
        css_variable("--terlan-density-scale", &environment.density_scale),
        css_variable(
            "--terlan-safe-area-top",
            &format!("{}px", environment.safe_area.top_px),
        ),
        css_variable(
            "--terlan-safe-area-right",
            &format!("{}px", environment.safe_area.right_px),
        ),
        css_variable(
            "--terlan-safe-area-bottom",
            &format!("{}px", environment.safe_area.bottom_px),
        ),
        css_variable(
            "--terlan-safe-area-left",
            &format!("{}px", environment.safe_area.left_px),
        ),
    ]
}

/// Encodes one native component lifecycle envelope from widget metadata.
///
/// Inputs:
/// - `metadata`: validated standard or generated widget metadata.
/// - `widget_name`: widget declaration name.
/// - `operation`: mount/update/unmount operation.
/// - `component_id`: stable component instance id.
/// - `props`: named property values supplied by AngularTS bindings.
///
/// Output:
/// - Typed lifecycle envelope when widget, operation, component id, required
///   props, and prop value types are coherent.
/// - Stable diagnostics for stale metadata, unknown widgets, missing ids,
///   missing native components, duplicate/unknown props, type mismatches, and
///   unmount payloads.
///
/// Transformation:
/// - Converts AngularTS component state into a deterministic native-shell
///   lifecycle envelope without executing shell code.
pub(crate) fn encode_mobile_angular_component_lifecycle(
    metadata: &MobileWidgetMetadata,
    widget_name: &str,
    operation: MobileAngularComponentOperation,
    component_id: &str,
    props: &[MobileAngularBridgePayloadField],
) -> Result<MobileAngularComponentLifecycleEnvelope, MobileAngularBridgeDiagnostic> {
    if metadata.schema_version != 1 {
        return Err(diagnostic(
            "mobile_angular_bridge_unsupported_widget_schema",
            format!(
                "mobile widget metadata schema version {} is not supported",
                metadata.schema_version
            ),
        ));
    }
    if component_id.trim().is_empty() {
        return Err(diagnostic(
            "mobile_angular_bridge_empty_component_id",
            "mobile component id must not be empty",
        ));
    }
    let widget = metadata
        .widgets
        .iter()
        .find(|widget| widget.name == widget_name)
        .ok_or_else(|| {
            diagnostic(
                "mobile_angular_bridge_unknown_widget",
                format!("unknown mobile widget `{widget_name}`"),
            )
        })?;
    let native_component = widget.native_component.clone().ok_or_else(|| {
        diagnostic(
            "mobile_angular_bridge_missing_native_component",
            format!(
                "mobile widget `{}` cannot be sent to native shell without a native component",
                widget.name
            ),
        )
    })?;
    validate_component_lifecycle_props(widget, operation, props)?;
    Ok(MobileAngularComponentLifecycleEnvelope {
        component_id: component_id.to_string(),
        widget: widget.name.clone(),
        selector: widget.selector.clone(),
        native_component,
        operation: operation.as_str(),
        capability: "native_components",
        props: props.to_vec(),
    })
}

/// Returns the stable missing-native-shell fallback message.
pub(crate) const fn mobile_angular_missing_native_shell_fallback_message() -> &'static str {
    MISSING_NATIVE_SHELL_FALLBACK_MESSAGE
}

/// Generates JavaScript helper source for AngularTS/mobile bridge communication.
///
/// Inputs:
/// - `metadata`: validated bridge metadata.
///
/// Output:
/// - Deterministic JavaScript module source exposing `createTerlanMobileBridge`.
///
/// Transformation:
/// - Emits a tiny runtime wrapper around a host `TerlanNativeBridge.postMessage`
///   object plus generated per-command helper functions.
pub(crate) fn generate_mobile_angular_bridge_runtime_source(
    metadata: &MobileBridgeMetadata,
) -> Result<String, MobileAngularBridgeDiagnostic> {
    if metadata.schema_version != 1 {
        return Err(diagnostic(
            "mobile_angular_bridge_unsupported_schema",
            format!(
                "mobile bridge metadata schema version {} is not supported",
                metadata.schema_version
            ),
        ));
    }
    let mut source = String::from(
        "export function createTerlanMobileBridge(host = globalThis) {\n\
         const pending = new Map();\n\
         const listeners = new Map();\n\
         let nextId = 1;\n\
         const commands = {};\n\
         function nextRequestId() { return String(nextId++); }\n\
         function listenerKey(bridge, event) { return bridge + ':' + event; }\n\
         function post(request) {\n\
         const target = host.TerlanNativeBridge;\n\
         if (!target || typeof target.postMessage !== 'function') {\n\
         return Promise.reject(new Error('terlan native bridge unavailable'));\n\
         }\n\
         target.postMessage(JSON.stringify(request));\n\
         return new Promise((resolve, reject) => pending.set(request.id, { resolve, reject }));\n\
         }\n\
         function send(bridge, command, capability, payload = {}) {\n\
         return post({ id: nextRequestId(), bridge, command, capability, payload });\n\
         }\n\
         function on(bridge, event, handler) {\n\
         const key = listenerKey(bridge, event);\n\
         const bucket = listeners.get(key) || [];\n\
         bucket.push(handler);\n\
         listeners.set(key, bucket);\n\
         return () => listeners.set(key, (listeners.get(key) || []).filter((item) => item !== handler));\n\
         }\n\
         function dispatchNativeEvent(message) {\n\
         const event = typeof message === 'string' ? JSON.parse(message) : message;\n\
         const bucket = listeners.get(listenerKey(event.bridge, event.event)) || [];\n\
         for (const handler of bucket) { handler(event.payload || {}); }\n\
         }\n",
    );
    source.push_str(
        "function platformEnvironment() {\n\
         return host.TerlanNativeEnvironment || { platform: 'unknown', theme: 'system' };\n\
         }\n\
         function applyPlatformEnvironment(environment = platformEnvironment(), root = host.document && host.document.documentElement) {\n\
         if (!root || !root.style) { return environment; }\n\
         const vars = {\n\
         '--terlan-platform': environment.platform || 'unknown',\n\
         '--terlan-theme': environment.theme || 'system',\n\
         '--terlan-locale': environment.locale || '',\n\
         '--terlan-density-scale': environment.density_scale || '1.0',\n\
         '--terlan-safe-area-top': String(environment.safe_area_top || 0) + 'px',\n\
         '--terlan-safe-area-right': String(environment.safe_area_right || 0) + 'px',\n\
         '--terlan-safe-area-bottom': String(environment.safe_area_bottom || 0) + 'px',\n\
         '--terlan-safe-area-left': String(environment.safe_area_left || 0) + 'px'\n\
         };\n\
         for (const [name, value] of Object.entries(vars)) { root.style.setProperty(name, value); }\n\
         return environment;\n\
         }\n",
    );
    for declaration in &metadata.declarations {
        source.push_str(&format!(
            "commands[{}] = commands[{}] || {{}};\n",
            js_string(&declaration.name),
            js_string(&declaration.name)
        ));
        for command in &declaration.commands {
            source.push_str(&format!(
                "commands[{}][{}] = (payload = {{}}) => send({}, {}, {}, payload);\n",
                js_string(&declaration.name),
                js_string(&command.name),
                js_string(&declaration.name),
                js_string(&command.name),
                js_string(command.required_capability),
            ));
        }
    }
    source.push_str(
        "return { send, on, dispatchNativeEvent, platformEnvironment, applyPlatformEnvironment, commands };\n}\n",
    );
    Ok(source)
}

/// Validates lifecycle props for one widget lifecycle operation.
fn validate_component_lifecycle_props(
    widget: &MobileWidgetMetadataEntry,
    operation: MobileAngularComponentOperation,
    props: &[MobileAngularBridgePayloadField],
) -> Result<(), MobileAngularBridgeDiagnostic> {
    if operation == MobileAngularComponentOperation::Unmount && !props.is_empty() {
        return Err(diagnostic(
            "mobile_angular_bridge_unmount_props",
            format!(
                "mobile widget `{}` unmount operation must not include props",
                widget.name
            ),
        ));
    }
    let mut seen = BTreeSet::new();
    for prop in props {
        if !seen.insert(prop.name.as_str()) {
            return Err(diagnostic(
                "mobile_angular_bridge_duplicate_component_prop",
                format!(
                    "mobile widget `{}` repeats component prop `{}`",
                    widget.name, prop.name
                ),
            ));
        }
        let Some(expected) = widget
            .props
            .iter()
            .find(|expected| expected.name == prop.name)
        else {
            return Err(diagnostic(
                "mobile_angular_bridge_unknown_component_prop",
                format!(
                    "mobile widget `{}` has no component prop `{}`",
                    widget.name, prop.name
                ),
            ));
        };
        if expected.prop_type != prop.value.type_name() {
            return Err(diagnostic(
                "mobile_angular_bridge_component_prop_type_mismatch",
                format!(
                    "mobile widget `{}` prop `{}` expects {} but got {}",
                    widget.name,
                    prop.name,
                    expected.prop_type,
                    prop.value.type_name()
                ),
            ));
        }
    }
    if operation == MobileAngularComponentOperation::Mount {
        for required in widget.props.iter().filter(|prop| prop.required) {
            if !seen.contains(required.name.as_str()) {
                return Err(diagnostic(
                    "mobile_angular_bridge_missing_required_component_prop",
                    format!(
                        "mobile widget `{}` mount operation is missing required prop `{}`",
                        widget.name, required.name
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Finds one bridge declaration in metadata.
fn find_declaration<'a>(
    metadata: &'a MobileBridgeMetadata,
    bridge_name: &str,
) -> Result<&'a MobileBridgeMetadataDeclaration, MobileAngularBridgeDiagnostic> {
    metadata
        .declarations
        .iter()
        .find(|declaration| declaration.name == bridge_name)
        .ok_or_else(|| {
            diagnostic(
                "mobile_angular_bridge_unknown_bridge",
                format!("unknown mobile bridge `{bridge_name}`"),
            )
        })
}

/// Encodes ordered values into named payload fields.
fn encode_payload_fields(
    bridge_name: &str,
    owner_kind: &str,
    owner_name: &str,
    fields: &[MobileBridgeMetadataField],
    values: &[MobileAngularBridgeValue],
) -> Result<Vec<MobileAngularBridgePayloadField>, MobileAngularBridgeDiagnostic> {
    if fields.len() != values.len() {
        return Err(diagnostic(
            "mobile_angular_bridge_arity_mismatch",
            format!(
                "mobile bridge `{bridge_name}` {owner_kind} `{owner_name}` expects {} values but got {}",
                fields.len(),
                values.len()
            ),
        ));
    }
    fields
        .iter()
        .zip(values)
        .map(|(field, value)| {
            encode_payload_field(bridge_name, owner_kind, owner_name, field, value)
        })
        .collect()
}

/// Encodes one payload value after checking its metadata type.
fn encode_payload_field(
    bridge_name: &str,
    owner_kind: &str,
    owner_name: &str,
    field: &MobileBridgeMetadataField,
    value: &MobileAngularBridgeValue,
) -> Result<MobileAngularBridgePayloadField, MobileAngularBridgeDiagnostic> {
    if field.field_type != value.type_name() {
        return Err(diagnostic(
            "mobile_angular_bridge_type_mismatch",
            format!(
                "mobile bridge `{bridge_name}` {owner_kind} `{owner_name}` field `{}` expects {} but got {}",
                field.name,
                field.field_type,
                value.type_name()
            ),
        ));
    }
    Ok(MobileAngularBridgePayloadField {
        name: field.name.clone(),
        value: value.clone(),
    })
}

/// Builds one named payload field.
fn payload_field(name: &str, value: MobileAngularBridgeValue) -> MobileAngularBridgePayloadField {
    MobileAngularBridgePayloadField {
        name: name.to_string(),
        value,
    }
}

/// Builds one CSS variable.
fn css_variable(name: &str, value: &str) -> MobileAngularCssVariable {
    MobileAngularCssVariable {
        name: name.to_string(),
        value: value.to_string(),
    }
}

/// Quotes one JavaScript string literal with serde_json's string encoder.
fn js_string(value: &str) -> String {
    serde_json::to_string(value).expect("serialize JavaScript string")
}

/// Builds one stable AngularTS bridge diagnostic.
fn diagnostic(code: &'static str, message: impl Into<String>) -> MobileAngularBridgeDiagnostic {
    MobileAngularBridgeDiagnostic {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "mobile_angular_bridge_test.rs"]
mod mobile_angular_bridge_test;
