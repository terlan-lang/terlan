use super::super::mobile_bridge::{
    generate_mobile_bridge_metadata, MobileBridgeCapability, MobileBridgeCommand,
    MobileBridgeDeclaration, MobileBridgeEvent, MobileBridgeField, MobileBridgeType,
};
use super::super::mobile_widget::{
    generate_mobile_widget_metadata, standard_mobile_widget_declarations, MobileWidgetMetadata,
};
use super::*;

/// Builds sample bridge metadata for AngularTS helper tests.
fn bridge_metadata() -> MobileBridgeMetadata {
    generate_mobile_bridge_metadata(&[MobileBridgeDeclaration {
        name: "ShellBridge".to_string(),
        capabilities: vec![
            MobileBridgeCapability::Navigation,
            MobileBridgeCapability::PlatformEnvironment,
        ],
        commands: vec![MobileBridgeCommand {
            name: "openRoute".to_string(),
            required_capability: MobileBridgeCapability::Navigation,
            parameters: vec![MobileBridgeField {
                name: "route".to_string(),
                field_type: MobileBridgeType::String,
            }],
            result: MobileBridgeType::Unit,
            source_identity: None,
        }],
        events: vec![MobileBridgeEvent {
            name: "platformChanged".to_string(),
            payload: vec![
                MobileBridgeField {
                    name: "theme".to_string(),
                    field_type: MobileBridgeType::String,
                },
                MobileBridgeField {
                    name: "dark".to_string(),
                    field_type: MobileBridgeType::Bool,
                },
            ],
            source_identity: None,
        }],
    }])
    .expect("bridge metadata")
}

/// Builds sample platform environment data.
fn platform_environment() -> MobileAngularPlatformEnvironment {
    MobileAngularPlatformEnvironment {
        platform: MobileAngularPlatform::Android,
        theme: MobileAngularTheme::Dark,
        locale: "en-US".to_string(),
        density_scale: "2.0".to_string(),
        safe_area: MobileAngularSafeArea {
            top_px: 12,
            right_px: 3,
            bottom_px: 24,
            left_px: 3,
        },
    }
}

/// Builds standard mobile widget metadata for component lifecycle tests.
fn widget_metadata() -> MobileWidgetMetadata {
    generate_mobile_widget_metadata(&standard_mobile_widget_declarations())
        .expect("widget metadata")
}

/// Builds one named component prop.
fn component_prop(name: &str, value: MobileAngularBridgeValue) -> MobileAngularBridgePayloadField {
    MobileAngularBridgePayloadField {
        name: name.to_string(),
        value,
    }
}

/// Verifies typed command encoding uses declaration parameter names.
///
/// Inputs:
/// - Bridge metadata plus one command argument.
///
/// Output:
/// - Command envelope with request id, bridge, command, capability, and named
///   payload.
///
/// Transformation:
/// - Converts positional command values into a typed native-shell command
///   envelope.
#[test]
fn mobile_angular_bridge_encodes_typed_command() {
    let envelope = encode_mobile_angular_bridge_command(
        &bridge_metadata(),
        "ShellBridge",
        "openRoute",
        "request-1",
        &[MobileAngularBridgeValue::String("/home".to_string())],
    )
    .expect("command envelope");

    assert_eq!(envelope.request_id, "request-1");
    assert_eq!(envelope.bridge, "ShellBridge");
    assert_eq!(envelope.command, "openRoute");
    assert_eq!(envelope.capability, "navigation");
    assert_eq!(envelope.payload[0].name, "route");
    assert_eq!(
        envelope.payload[0].value,
        MobileAngularBridgeValue::String("/home".to_string())
    );
}

/// Verifies native event decoding uses declaration payload names.
///
/// Inputs:
/// - Bridge metadata plus native event payload values.
///
/// Output:
/// - Event envelope with named typed payload fields.
///
/// Transformation:
/// - Converts positional native payload values into a typed AngularTS event
///   envelope.
#[test]
fn mobile_angular_bridge_decodes_typed_event() {
    let envelope = decode_mobile_angular_bridge_event(
        &bridge_metadata(),
        "ShellBridge",
        "platformChanged",
        &[
            MobileAngularBridgeValue::String("dark".to_string()),
            MobileAngularBridgeValue::Bool(true),
        ],
    )
    .expect("event envelope");

    assert_eq!(envelope.bridge, "ShellBridge");
    assert_eq!(envelope.event, "platformChanged");
    assert_eq!(envelope.payload[0].name, "theme");
    assert_eq!(envelope.payload[1].name, "dark");
}

/// Verifies command encoding rejects arity mismatches.
///
/// Inputs:
/// - Command metadata expecting one argument but zero values.
///
/// Output:
/// - Stable arity mismatch diagnostic.
///
/// Transformation:
/// - Prevents AngularTS helpers from constructing malformed native command
///   envelopes.
#[test]
fn mobile_angular_bridge_rejects_command_arity_mismatch() {
    let diagnostic = encode_mobile_angular_bridge_command(
        &bridge_metadata(),
        "ShellBridge",
        "openRoute",
        "request-1",
        &[],
    )
    .expect_err("arity mismatch");

    assert_eq!(diagnostic.code, "mobile_angular_bridge_arity_mismatch");
}

/// Verifies event decoding rejects type mismatches.
///
/// Inputs:
/// - Event metadata expecting `String, Bool` but receiving `String, String`.
///
/// Output:
/// - Stable type mismatch diagnostic.
///
/// Transformation:
/// - Prevents native event payloads with stale/wrong types from entering the
///   AngularTS event stream.
#[test]
fn mobile_angular_bridge_rejects_event_type_mismatch() {
    let diagnostic = decode_mobile_angular_bridge_event(
        &bridge_metadata(),
        "ShellBridge",
        "platformChanged",
        &[
            MobileAngularBridgeValue::String("dark".to_string()),
            MobileAngularBridgeValue::String("true".to_string()),
        ],
    )
    .expect_err("type mismatch");

    assert_eq!(diagnostic.code, "mobile_angular_bridge_type_mismatch");
}

/// Verifies unknown bridge and command diagnostics are stable.
///
/// Inputs:
/// - Missing bridge and missing command references.
///
/// Output:
/// - Stable unknown bridge/command diagnostics.
///
/// Transformation:
/// - Keeps runtime helper errors deterministic for invalid generated metadata
///   or stale AngularTS calls.
#[test]
fn mobile_angular_bridge_rejects_unknown_bridge_and_command() {
    let unknown_bridge = encode_mobile_angular_bridge_command(
        &bridge_metadata(),
        "MissingBridge",
        "openRoute",
        "request-1",
        &[MobileAngularBridgeValue::String("/home".to_string())],
    )
    .expect_err("unknown bridge");
    let unknown_command = encode_mobile_angular_bridge_command(
        &bridge_metadata(),
        "ShellBridge",
        "missingCommand",
        "request-1",
        &[],
    )
    .expect_err("unknown command");

    assert_eq!(unknown_bridge.code, "mobile_angular_bridge_unknown_bridge");
    assert_eq!(
        unknown_command.code,
        "mobile_angular_bridge_unknown_command"
    );
}

/// Verifies JavaScript runtime helper source generation.
///
/// Inputs:
/// - Valid bridge metadata.
///
/// Output:
/// - JavaScript module source containing the generic bridge API and generated
///   command helper.
///
/// Transformation:
/// - Emits deterministic AngularTS-facing runtime helper source without
///   executing JavaScript.
#[test]
fn mobile_angular_bridge_generates_runtime_source() {
    let source =
        generate_mobile_angular_bridge_runtime_source(&bridge_metadata()).expect("runtime source");

    assert!(source.contains("export function createTerlanMobileBridge"));
    assert!(source.contains("target.postMessage(JSON.stringify(request))"));
    assert!(source.contains("function dispatchNativeEvent(message)"));
    assert!(source.contains("function platformEnvironment()"));
    assert!(source.contains("function applyPlatformEnvironment("));
    assert!(source.contains("'--terlan-platform'"));
    assert!(source.contains("commands[\"ShellBridge\"][\"openRoute\"]"));
    assert!(source.contains("send(\"ShellBridge\", \"openRoute\", \"navigation\", payload)"));
}

/// Verifies runtime source exposes native event dispatch.
///
/// Inputs:
/// - Valid bridge metadata.
///
/// Output:
/// - Runtime helper source with deterministic listener lookup and payload
///   dispatch code.
///
/// Transformation:
/// - Pins the generated AngularTS/native-shell event dispatch surface before a
///   real platform shell consumes it.
#[test]
fn mobile_angular_bridge_generates_native_event_dispatch_runtime() {
    let source =
        generate_mobile_angular_bridge_runtime_source(&bridge_metadata()).expect("runtime source");

    assert!(source.contains("function dispatchNativeEvent(message)"));
    assert!(source.contains("listeners.get(listenerKey(event.bridge, event.event))"));
    assert!(source.contains("handler(event.payload || {})"));
}

/// Verifies runtime source exposes the missing-native-shell fallback.
///
/// Inputs:
/// - Valid bridge metadata.
///
/// Output:
/// - Runtime helper source with the stable missing native bridge error message.
///
/// Transformation:
/// - Keeps browser-only and missing-shell execution deterministic instead of
///   failing with a platform-specific JavaScript error.
#[test]
fn mobile_angular_bridge_generates_missing_native_shell_fallback() {
    let source =
        generate_mobile_angular_bridge_runtime_source(&bridge_metadata()).expect("runtime source");

    assert_eq!(
        mobile_angular_missing_native_shell_fallback_message(),
        "terlan native bridge unavailable"
    );
    assert!(source.contains("Promise.reject(new Error('terlan native bridge unavailable'))"));
}

/// Verifies component mount/update/unmount lifecycle envelopes are typed.
///
/// Inputs:
/// - Standard toolbar widget metadata and lifecycle prop values.
///
/// Output:
/// - Native component lifecycle envelopes for mount, update, and unmount.
///
/// Transformation:
/// - Converts AngularTS component state changes into deterministic native shell
///   lifecycle envelopes without executing a shell.
#[test]
fn mobile_angular_bridge_encodes_component_lifecycle() {
    let metadata = widget_metadata();
    let mount = encode_mobile_angular_component_lifecycle(
        &metadata,
        "ToolbarAction",
        MobileAngularComponentOperation::Mount,
        "toolbar-save",
        &[
            component_prop(
                "id",
                MobileAngularBridgeValue::String("toolbar-save".to_string()),
            ),
            component_prop(
                "label",
                MobileAngularBridgeValue::String("Save".to_string()),
            ),
        ],
    )
    .expect("mount envelope");
    let update = encode_mobile_angular_component_lifecycle(
        &metadata,
        "ToolbarAction",
        MobileAngularComponentOperation::Update,
        "toolbar-save",
        &[component_prop(
            "enabled",
            MobileAngularBridgeValue::Bool(false),
        )],
    )
    .expect("update envelope");
    let unmount = encode_mobile_angular_component_lifecycle(
        &metadata,
        "ToolbarAction",
        MobileAngularComponentOperation::Unmount,
        "toolbar-save",
        &[],
    )
    .expect("unmount envelope");

    assert_eq!(mount.widget, "ToolbarAction");
    assert_eq!(mount.selector, "terlan-toolbar-action");
    assert_eq!(mount.native_component, "ToolbarAction");
    assert_eq!(mount.operation, "mount");
    assert_eq!(mount.capability, "native_components");
    assert_eq!(mount.props.len(), 2);
    assert_eq!(update.operation, "update");
    assert_eq!(update.props[0].name, "enabled");
    assert_eq!(unmount.operation, "unmount");
    assert!(unmount.props.is_empty());
}

/// Verifies component lifecycle envelopes reject stale or malformed props.
///
/// Inputs:
/// - Standard toolbar widget metadata and bad lifecycle prop values.
///
/// Output:
/// - Stable diagnostics for missing required props, type mismatch, duplicate
///   props, and unmount payloads.
///
/// Transformation:
/// - Prevents stale AngularTS component bindings from reaching native shell
///   lifecycle handling.
#[test]
fn mobile_angular_bridge_rejects_component_lifecycle_mismatches() {
    let metadata = widget_metadata();
    let missing_required = encode_mobile_angular_component_lifecycle(
        &metadata,
        "ToolbarAction",
        MobileAngularComponentOperation::Mount,
        "toolbar-save",
        &[component_prop(
            "id",
            MobileAngularBridgeValue::String("toolbar-save".to_string()),
        )],
    )
    .expect_err("missing required prop");
    let type_mismatch = encode_mobile_angular_component_lifecycle(
        &metadata,
        "ToolbarAction",
        MobileAngularComponentOperation::Update,
        "toolbar-save",
        &[component_prop("enabled", MobileAngularBridgeValue::Int(1))],
    )
    .expect_err("type mismatch");
    let duplicate = encode_mobile_angular_component_lifecycle(
        &metadata,
        "ToolbarAction",
        MobileAngularComponentOperation::Update,
        "toolbar-save",
        &[
            component_prop("icon", MobileAngularBridgeValue::String("save".to_string())),
            component_prop("icon", MobileAngularBridgeValue::String("save".to_string())),
        ],
    )
    .expect_err("duplicate prop");
    let unmount_props = encode_mobile_angular_component_lifecycle(
        &metadata,
        "ToolbarAction",
        MobileAngularComponentOperation::Unmount,
        "toolbar-save",
        &[component_prop(
            "icon",
            MobileAngularBridgeValue::String("x".to_string()),
        )],
    )
    .expect_err("unmount props");

    assert_eq!(
        missing_required.code,
        "mobile_angular_bridge_missing_required_component_prop"
    );
    assert_eq!(
        type_mismatch.code,
        "mobile_angular_bridge_component_prop_type_mismatch"
    );
    assert_eq!(
        duplicate.code,
        "mobile_angular_bridge_duplicate_component_prop"
    );
    assert_eq!(unmount_props.code, "mobile_angular_bridge_unmount_props");
}

/// Verifies platform environment values encode as typed AngularTS data.
///
/// Inputs:
/// - Native shell platform, theme, locale, density, and safe-area values.
///
/// Output:
/// - Named typed fields with stable names.
///
/// Transformation:
/// - Converts native shell environment state into data AngularTS can bind
///   without platform-specific parsing.
#[test]
fn mobile_angular_bridge_encodes_platform_environment_data() {
    let data = encode_mobile_angular_platform_environment(&platform_environment());

    assert_eq!(data.fields[0].name, "platform");
    assert_eq!(
        data.fields[0].value,
        MobileAngularBridgeValue::String("android".to_string())
    );
    assert_eq!(data.fields[1].name, "theme");
    assert_eq!(
        data.fields[1].value,
        MobileAngularBridgeValue::String("dark".to_string())
    );
    assert_eq!(data.fields[2].name, "locale");
    assert_eq!(data.fields[4].name, "safe_area_top");
    assert_eq!(data.fields[4].value, MobileAngularBridgeValue::Int(12));
}

/// Verifies platform environment values generate CSS variables.
///
/// Inputs:
/// - Native shell platform, theme, locale, density, and safe-area values.
///
/// Output:
/// - Stable `--terlan-*` CSS variable names and CSS-ready values.
///
/// Transformation:
/// - Gives AngularTS and plain CSS a stable environment surface independent of
///   the platform shell implementation.
#[test]
fn mobile_angular_bridge_generates_platform_environment_css_variables() {
    let variables = mobile_angular_platform_environment_css_variables(&platform_environment());

    assert_eq!(variables[0].name, "--terlan-platform");
    assert_eq!(variables[0].value, "android");
    assert_eq!(variables[1].name, "--terlan-theme");
    assert_eq!(variables[1].value, "dark");
    assert_eq!(variables[4].name, "--terlan-safe-area-top");
    assert_eq!(variables[4].value, "12px");
    assert_eq!(variables[6].name, "--terlan-safe-area-bottom");
    assert_eq!(variables[6].value, "24px");
}

/// Verifies runtime source generation rejects unsupported metadata schemas.
///
/// Inputs:
/// - Bridge metadata with a stale schema version.
///
/// Output:
/// - Stable unsupported-schema diagnostic.
///
/// Transformation:
/// - Prevents AngularTS helper generation from consuming stale bridge metadata.
#[test]
fn mobile_angular_bridge_rejects_unsupported_schema() {
    let mut metadata = bridge_metadata();
    metadata.schema_version = 0;

    let diagnostic =
        generate_mobile_angular_bridge_runtime_source(&metadata).expect_err("unsupported schema");

    assert_eq!(diagnostic.code, "mobile_angular_bridge_unsupported_schema");
}

/// Verifies bridge value type names are stable.
///
/// Inputs:
/// - Every first-slice bridge value kind.
///
/// Output:
/// - Stable type names matching mobile bridge metadata.
///
/// Transformation:
/// - Keeps command/event helper validation aligned with bridge metadata type
///   spellings.
#[test]
fn mobile_angular_bridge_value_type_names_are_stable() {
    assert_eq!(MobileAngularBridgeValue::Unit.type_name(), "Unit");
    assert_eq!(MobileAngularBridgeValue::Bool(true).type_name(), "Bool");
    assert_eq!(MobileAngularBridgeValue::Int(1).type_name(), "Int");
    assert_eq!(
        MobileAngularBridgeValue::Float("1.0".to_string()).type_name(),
        "Float"
    );
    assert_eq!(
        MobileAngularBridgeValue::String("x".to_string()).type_name(),
        "String"
    );
    assert_eq!(
        MobileAngularBridgeValue::Json("{}".to_string()).type_name(),
        "Json"
    );
}

/// Verifies platform and theme spellings are stable.
///
/// Inputs:
/// - Every first-slice platform and theme value.
///
/// Output:
/// - Stable environment spellings.
///
/// Transformation:
/// - Keeps native shell, AngularTS, and CSS environment names aligned.
#[test]
fn mobile_angular_bridge_platform_environment_names_are_stable() {
    assert_eq!(MobileAngularPlatform::Android.as_str(), "android");
    assert_eq!(MobileAngularPlatform::Ios.as_str(), "ios");
    assert_eq!(MobileAngularPlatform::Web.as_str(), "web");
    assert_eq!(MobileAngularPlatform::Unknown.as_str(), "unknown");
    assert_eq!(MobileAngularTheme::Light.as_str(), "light");
    assert_eq!(MobileAngularTheme::Dark.as_str(), "dark");
    assert_eq!(MobileAngularTheme::System.as_str(), "system");
    assert_eq!(MobileAngularComponentOperation::Mount.as_str(), "mount");
    assert_eq!(MobileAngularComponentOperation::Update.as_str(), "update");
    assert_eq!(MobileAngularComponentOperation::Unmount.as_str(), "unmount");
}
