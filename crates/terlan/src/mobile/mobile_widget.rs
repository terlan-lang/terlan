//! Standard mobile widget declarations for AngularTS/native shell integration.
#![allow(dead_code)]
//!
//! Inputs:
//! - Compiler-owned widget declarations for HTML/AngularTS components that may
//!   upgrade to native shell controls.
//!
//! Outputs:
//! - Validated widget declarations and metadata with stable selector, prop,
//!   event, capability, and native component spellings.
//!
//! Transformation:
//! - Keeps the first mobile widget surface typed and explicit before AngularTS
//!   runtime helpers and platform shell code are generated.

use std::collections::BTreeSet;

use super::mobile_bridge::{MobileBridgeCapability, MobileBridgeField, MobileBridgeType};

/// One mobile widget declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileWidgetDeclaration {
    pub(crate) name: String,
    pub(crate) selector: String,
    pub(crate) native_component: Option<String>,
    pub(crate) upgrade: MobileWidgetUpgrade,
    pub(crate) capabilities: Vec<MobileBridgeCapability>,
    pub(crate) props: Vec<MobileWidgetProp>,
    pub(crate) events: Vec<MobileWidgetEvent>,
}

/// Native upgrade behavior for one widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileWidgetUpgrade {
    WebOnly,
    NativeOptional,
    NativeRequired,
}

impl MobileWidgetUpgrade {
    /// Returns the stable metadata spelling for one upgrade behavior.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::WebOnly => "web_only",
            Self::NativeOptional => "native_optional",
            Self::NativeRequired => "native_required",
        }
    }
}

/// One typed widget property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileWidgetProp {
    pub(crate) name: String,
    pub(crate) prop_type: MobileBridgeType,
    pub(crate) required: bool,
}

/// One typed widget event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileWidgetEvent {
    pub(crate) name: String,
    pub(crate) payload: Vec<MobileBridgeField>,
}

/// Validation diagnostic for mobile widget declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileWidgetDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Generated metadata for standard mobile widgets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileWidgetMetadata {
    pub(crate) schema_version: u32,
    pub(crate) widgets: Vec<MobileWidgetMetadataEntry>,
}

/// Generated metadata for one mobile widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileWidgetMetadataEntry {
    pub(crate) name: String,
    pub(crate) selector: String,
    pub(crate) native_component: Option<String>,
    pub(crate) upgrade: &'static str,
    pub(crate) capabilities: Vec<&'static str>,
    pub(crate) props: Vec<MobileWidgetMetadataProp>,
    pub(crate) events: Vec<MobileWidgetMetadataEvent>,
}

/// Generated metadata for one widget property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileWidgetMetadataProp {
    pub(crate) name: String,
    pub(crate) prop_type: &'static str,
    pub(crate) required: bool,
}

/// Generated metadata for one widget event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileWidgetMetadataEvent {
    pub(crate) name: String,
    pub(crate) payload: Vec<MobileWidgetMetadataField>,
}

/// Generated metadata for one widget event field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileWidgetMetadataField {
    pub(crate) name: String,
    pub(crate) field_type: &'static str,
}

/// Returns the first standard native-upgradable mobile widgets.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Stable widget declarations for shell/navigation/media/file/location
///   surfaces used by mobile profile generation.
///
/// Transformation:
/// - Produces typed declarations only; no AngularTS code or native platform code
///   is emitted here.
pub(crate) fn standard_mobile_widget_declarations() -> Vec<MobileWidgetDeclaration> {
    vec![
        widget(
            "ToolbarAction",
            "terlan-toolbar-action",
            "ToolbarAction",
            &[MobileBridgeCapability::NativeComponents],
            &[
                required_prop("id", MobileBridgeType::String),
                required_prop("label", MobileBridgeType::String),
                optional_prop("icon", MobileBridgeType::String),
                optional_prop("enabled", MobileBridgeType::Bool),
            ],
            &[event("press", &[field("id", MobileBridgeType::String)])],
        ),
        widget(
            "BottomSheetMenu",
            "terlan-bottom-sheet-menu",
            "BottomSheetMenu",
            &[MobileBridgeCapability::NativeComponents],
            &[
                required_prop("id", MobileBridgeType::String),
                required_prop("title", MobileBridgeType::String),
                required_prop("items", MobileBridgeType::Json),
            ],
            &[event("select", &[field("item", MobileBridgeType::Json)])],
        ),
        widget(
            "Drawer",
            "terlan-drawer",
            "Drawer",
            &[MobileBridgeCapability::NativeComponents],
            &[
                required_prop("id", MobileBridgeType::String),
                optional_prop("open", MobileBridgeType::Bool),
            ],
            &[event("open", &[]), event("close", &[])],
        ),
        widget(
            "Card",
            "terlan-card",
            "Card",
            &[MobileBridgeCapability::NativeComponents],
            &[
                required_prop("id", MobileBridgeType::String),
                optional_prop("title", MobileBridgeType::String),
                optional_prop("subtitle", MobileBridgeType::String),
                optional_prop("body", MobileBridgeType::String),
            ],
            &[event("press", &[field("id", MobileBridgeType::String)])],
        ),
        widget(
            "Image",
            "terlan-image",
            "Image",
            &[MobileBridgeCapability::NativeComponents],
            &[
                required_prop("id", MobileBridgeType::String),
                required_prop("src", MobileBridgeType::String),
                optional_prop("alt", MobileBridgeType::String),
                optional_prop("width", MobileBridgeType::Int),
                optional_prop("height", MobileBridgeType::Int),
            ],
            &[
                event("load", &[]),
                event("error", &[field("message", MobileBridgeType::String)]),
            ],
        ),
        widget(
            "FilePicker",
            "terlan-file-picker",
            "FilePicker",
            &[
                MobileBridgeCapability::NativeComponents,
                MobileBridgeCapability::Files,
                MobileBridgeCapability::Permissions,
            ],
            &[
                required_prop("id", MobileBridgeType::String),
                optional_prop("accept", MobileBridgeType::String),
                optional_prop("multiple", MobileBridgeType::Bool),
            ],
            &[event("files", &[field("files", MobileBridgeType::Json)])],
        ),
        widget(
            "CameraCapture",
            "terlan-camera-capture",
            "CameraCapture",
            &[
                MobileBridgeCapability::NativeComponents,
                MobileBridgeCapability::Camera,
                MobileBridgeCapability::Permissions,
            ],
            &[
                required_prop("id", MobileBridgeType::String),
                optional_prop("quality", MobileBridgeType::Int),
            ],
            &[event("photo", &[field("photo", MobileBridgeType::Json)])],
        ),
        widget(
            "GeolocationPermission",
            "terlan-geolocation-permission",
            "GeolocationPermission",
            &[
                MobileBridgeCapability::NativeComponents,
                MobileBridgeCapability::Geolocation,
                MobileBridgeCapability::Permissions,
            ],
            &[
                required_prop("id", MobileBridgeType::String),
                optional_prop("reason", MobileBridgeType::String),
            ],
            &[
                event("location", &[field("location", MobileBridgeType::Json)]),
                event("denied", &[field("reason", MobileBridgeType::String)]),
            ],
        ),
    ]
}

/// Validates mobile widget declarations.
pub(crate) fn validate_mobile_widget_declarations(
    declarations: &[MobileWidgetDeclaration],
) -> Result<(), Vec<MobileWidgetDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut names = BTreeSet::new();
    let mut selectors = BTreeSet::new();

    for declaration in declarations {
        if is_blank(&declaration.name) {
            diagnostics.push(diagnostic(
                "mobile_widget_empty_name",
                "mobile widget name must not be empty",
            ));
        } else if !names.insert(declaration.name.as_str()) {
            diagnostics.push(diagnostic(
                "mobile_widget_duplicate_name",
                format!(
                    "mobile widget `{}` is declared more than once",
                    declaration.name
                ),
            ));
        }
        if is_blank(&declaration.selector) {
            diagnostics.push(diagnostic(
                "mobile_widget_empty_selector",
                format!(
                    "mobile widget `{}` selector must not be empty",
                    declaration.name
                ),
            ));
        } else if !selectors.insert(declaration.selector.as_str()) {
            diagnostics.push(diagnostic(
                "mobile_widget_duplicate_selector",
                format!(
                    "mobile widget selector `{}` is declared more than once",
                    declaration.selector
                ),
            ));
        }
        diagnostics.extend(validate_widget_upgrade(declaration));
        diagnostics.extend(validate_widget_capabilities(declaration));
        diagnostics.extend(validate_widget_props(declaration));
        diagnostics.extend(validate_widget_events(declaration));
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Generates typed widget metadata from declarations.
pub(crate) fn generate_mobile_widget_metadata(
    declarations: &[MobileWidgetDeclaration],
) -> Result<MobileWidgetMetadata, Vec<MobileWidgetDiagnostic>> {
    validate_mobile_widget_declarations(declarations)?;
    Ok(MobileWidgetMetadata {
        schema_version: 1,
        widgets: declarations.iter().map(widget_metadata_entry).collect(),
    })
}

/// Converts one widget declaration into metadata.
fn widget_metadata_entry(declaration: &MobileWidgetDeclaration) -> MobileWidgetMetadataEntry {
    let mut capabilities = declaration
        .capabilities
        .iter()
        .map(|capability| capability.as_str())
        .collect::<Vec<_>>();
    capabilities.sort_unstable();
    MobileWidgetMetadataEntry {
        name: declaration.name.clone(),
        selector: declaration.selector.clone(),
        native_component: declaration.native_component.clone(),
        upgrade: declaration.upgrade.as_str(),
        capabilities,
        props: declaration.props.iter().map(widget_metadata_prop).collect(),
        events: declaration
            .events
            .iter()
            .map(widget_metadata_event)
            .collect(),
    }
}

/// Converts one widget property into metadata.
fn widget_metadata_prop(prop: &MobileWidgetProp) -> MobileWidgetMetadataProp {
    MobileWidgetMetadataProp {
        name: prop.name.clone(),
        prop_type: prop.prop_type.as_str(),
        required: prop.required,
    }
}

/// Converts one widget event into metadata.
fn widget_metadata_event(event: &MobileWidgetEvent) -> MobileWidgetMetadataEvent {
    MobileWidgetMetadataEvent {
        name: event.name.clone(),
        payload: event.payload.iter().map(widget_metadata_field).collect(),
    }
}

/// Converts one widget event field into metadata.
fn widget_metadata_field(field: &MobileBridgeField) -> MobileWidgetMetadataField {
    MobileWidgetMetadataField {
        name: field.name.clone(),
        field_type: field.field_type.as_str(),
    }
}

/// Validates native upgrade metadata for one widget.
fn validate_widget_upgrade(declaration: &MobileWidgetDeclaration) -> Vec<MobileWidgetDiagnostic> {
    match (declaration.upgrade, declaration.native_component.as_ref()) {
        (MobileWidgetUpgrade::WebOnly, Some(_)) => vec![diagnostic(
            "mobile_widget_web_only_native_component",
            format!(
                "mobile widget `{}` is web-only but declares a native component",
                declaration.name
            ),
        )],
        (MobileWidgetUpgrade::NativeOptional | MobileWidgetUpgrade::NativeRequired, None) => {
            vec![diagnostic(
                "mobile_widget_missing_native_component",
                format!(
                    "mobile widget `{}` requires a native component name",
                    declaration.name
                ),
            )]
        }
        _ => Vec::new(),
    }
}

/// Validates capability metadata for one widget.
fn validate_widget_capabilities(
    declaration: &MobileWidgetDeclaration,
) -> Vec<MobileWidgetDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for capability in &declaration.capabilities {
        if !seen.insert(*capability) {
            diagnostics.push(diagnostic(
                "mobile_widget_duplicate_capability",
                format!(
                    "mobile widget `{}` repeats capability `{}`",
                    declaration.name,
                    capability.as_str()
                ),
            ));
        }
    }
    if matches!(
        declaration.upgrade,
        MobileWidgetUpgrade::NativeOptional | MobileWidgetUpgrade::NativeRequired
    ) && !seen.contains(&MobileBridgeCapability::NativeComponents)
    {
        diagnostics.push(diagnostic(
            "mobile_widget_missing_native_components_capability",
            format!(
                "mobile widget `{}` requires native_components capability",
                declaration.name
            ),
        ));
    }
    diagnostics
}

/// Validates widget properties.
fn validate_widget_props(declaration: &MobileWidgetDeclaration) -> Vec<MobileWidgetDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut names = BTreeSet::new();
    for prop in &declaration.props {
        if is_blank(&prop.name) {
            diagnostics.push(diagnostic(
                "mobile_widget_empty_prop_name",
                format!(
                    "mobile widget `{}` has an empty prop name",
                    declaration.name
                ),
            ));
            continue;
        }
        if !names.insert(prop.name.as_str()) {
            diagnostics.push(diagnostic(
                "mobile_widget_duplicate_prop",
                format!(
                    "mobile widget `{}` repeats prop `{}`",
                    declaration.name, prop.name
                ),
            ));
        }
    }
    diagnostics
}

/// Validates widget events.
fn validate_widget_events(declaration: &MobileWidgetDeclaration) -> Vec<MobileWidgetDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut names = BTreeSet::new();
    for event in &declaration.events {
        if is_blank(&event.name) {
            diagnostics.push(diagnostic(
                "mobile_widget_empty_event_name",
                format!(
                    "mobile widget `{}` has an empty event name",
                    declaration.name
                ),
            ));
        } else if !names.insert(event.name.as_str()) {
            diagnostics.push(diagnostic(
                "mobile_widget_duplicate_event",
                format!(
                    "mobile widget `{}` repeats event `{}`",
                    declaration.name, event.name
                ),
            ));
        }
        diagnostics.extend(validate_event_payload_fields(declaration, event));
    }
    diagnostics
}

/// Validates event payload fields.
fn validate_event_payload_fields(
    declaration: &MobileWidgetDeclaration,
    event: &MobileWidgetEvent,
) -> Vec<MobileWidgetDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut names = BTreeSet::new();
    for field in &event.payload {
        if is_blank(&field.name) {
            diagnostics.push(diagnostic(
                "mobile_widget_empty_event_field_name",
                format!(
                    "mobile widget `{}` event `{}` has an empty payload field name",
                    declaration.name, event.name
                ),
            ));
            continue;
        }
        if !names.insert(field.name.as_str()) {
            diagnostics.push(diagnostic(
                "mobile_widget_duplicate_event_field",
                format!(
                    "mobile widget `{}` event `{}` repeats payload field `{}`",
                    declaration.name, event.name, field.name
                ),
            ));
        }
    }
    diagnostics
}

/// Builds one standard native-optional widget declaration.
fn widget(
    name: &str,
    selector: &str,
    native_component: &str,
    capabilities: &[MobileBridgeCapability],
    props: &[MobileWidgetProp],
    events: &[MobileWidgetEvent],
) -> MobileWidgetDeclaration {
    MobileWidgetDeclaration {
        name: name.to_string(),
        selector: selector.to_string(),
        native_component: Some(native_component.to_string()),
        upgrade: MobileWidgetUpgrade::NativeOptional,
        capabilities: capabilities.to_vec(),
        props: props.to_vec(),
        events: events.to_vec(),
    }
}

/// Builds one required widget property.
fn required_prop(name: &str, prop_type: MobileBridgeType) -> MobileWidgetProp {
    MobileWidgetProp {
        name: name.to_string(),
        prop_type,
        required: true,
    }
}

/// Builds one optional widget property.
fn optional_prop(name: &str, prop_type: MobileBridgeType) -> MobileWidgetProp {
    MobileWidgetProp {
        name: name.to_string(),
        prop_type,
        required: false,
    }
}

/// Builds one widget event.
fn event(name: &str, payload: &[MobileBridgeField]) -> MobileWidgetEvent {
    MobileWidgetEvent {
        name: name.to_string(),
        payload: payload.to_vec(),
    }
}

/// Builds one typed event payload field.
fn field(name: &str, field_type: MobileBridgeType) -> MobileBridgeField {
    MobileBridgeField {
        name: name.to_string(),
        field_type,
    }
}

/// Returns whether a widget identifier is blank after trimming whitespace.
fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

/// Builds a stable mobile widget diagnostic.
fn diagnostic(code: &'static str, message: impl Into<String>) -> MobileWidgetDiagnostic {
    MobileWidgetDiagnostic {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "mobile_widget_test.rs"]
mod mobile_widget_test;
