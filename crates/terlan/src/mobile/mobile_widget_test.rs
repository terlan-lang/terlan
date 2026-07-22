use super::super::mobile_bridge::{MobileBridgeCapability, MobileBridgeField, MobileBridgeType};
use super::*;

/// Finds one standard widget declaration by name.
fn standard_widget(name: &str) -> MobileWidgetDeclaration {
    standard_mobile_widget_declarations()
        .into_iter()
        .find(|widget| widget.name == name)
        .expect("standard widget")
}

/// Verifies the standard mobile widget surface is present.
///
/// Inputs:
/// - Compiler-owned standard widget declarations.
///
/// Output:
/// - Stable widget names and selectors for native-upgradable components.
///
/// Transformation:
/// - Locks the first AngularTS/mobile component vocabulary before runtime
///   helper generation.
#[test]
fn standard_mobile_widgets_define_expected_surface() {
    let widgets = standard_mobile_widget_declarations();
    let names = widgets
        .iter()
        .map(|widget| widget.name.as_str())
        .collect::<Vec<_>>();
    let selectors = widgets
        .iter()
        .map(|widget| widget.selector.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "ToolbarAction",
            "BottomSheetMenu",
            "Drawer",
            "Card",
            "Image",
            "FilePicker",
            "CameraCapture",
            "GeolocationPermission"
        ]
    );
    assert!(selectors.contains(&"terlan-toolbar-action"));
    assert!(selectors.contains(&"terlan-bottom-sheet-menu"));
    assert!(selectors.contains(&"terlan-camera-capture"));
    assert!(selectors.contains(&"terlan-geolocation-permission"));
}

/// Verifies standard widget declarations validate.
///
/// Inputs:
/// - Built-in mobile widget declarations.
///
/// Output:
/// - Successful validation.
///
/// Transformation:
/// - Ensures the compiler-owned default widget set satisfies the same rules as
///   any future generated declarations.
#[test]
fn standard_mobile_widgets_validate() {
    let widgets = standard_mobile_widget_declarations();

    assert_eq!(validate_mobile_widget_declarations(&widgets), Ok(()));
}

/// Verifies standard widget metadata is stable.
///
/// Inputs:
/// - Built-in mobile widget declarations.
///
/// Output:
/// - Schema-versioned metadata with sorted capabilities and typed properties.
///
/// Transformation:
/// - Exercises the metadata shape that AngularTS helpers and native shells will
///   consume.
#[test]
fn standard_mobile_widgets_generate_metadata() {
    let widgets = standard_mobile_widget_declarations();
    let metadata = generate_mobile_widget_metadata(&widgets).expect("metadata");

    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.widgets[0].name, "ToolbarAction");
    assert_eq!(metadata.widgets[0].selector, "terlan-toolbar-action");
    assert_eq!(metadata.widgets[0].upgrade, "native_optional");
    assert_eq!(metadata.widgets[0].capabilities, vec!["native_components"]);
    assert_eq!(metadata.widgets[0].props[0].name, "id");
    assert_eq!(metadata.widgets[0].props[0].prop_type, "String");
    assert!(metadata.widgets[0].props[0].required);
    assert_eq!(metadata.widgets[0].events[0].name, "press");
    assert_eq!(
        metadata.widgets[0].events[0].payload[0].field_type,
        "String"
    );
}

/// Verifies native media widgets declare the expected capabilities.
///
/// Inputs:
/// - Built-in file, camera, and geolocation widget declarations.
///
/// Output:
/// - Capability lists include the native resource and permissions capability.
///
/// Transformation:
/// - Keeps permission-sensitive mobile widgets explicit for bridge metadata and
///   native shell permission prompts.
#[test]
fn standard_mobile_widgets_declare_permission_capabilities() {
    let file_picker = standard_widget("FilePicker");
    let camera = standard_widget("CameraCapture");
    let geolocation = standard_widget("GeolocationPermission");

    assert!(file_picker
        .capabilities
        .contains(&MobileBridgeCapability::Files));
    assert!(file_picker
        .capabilities
        .contains(&MobileBridgeCapability::Permissions));
    assert!(camera
        .capabilities
        .contains(&MobileBridgeCapability::Camera));
    assert!(camera
        .capabilities
        .contains(&MobileBridgeCapability::Permissions));
    assert!(geolocation
        .capabilities
        .contains(&MobileBridgeCapability::Geolocation));
    assert!(geolocation
        .capabilities
        .contains(&MobileBridgeCapability::Permissions));
    assert!(geolocation.events.iter().any(|event| {
        event.name == "denied"
            && event.payload.len() == 1
            && event.payload[0].name == "reason"
            && event.payload[0].field_type == MobileBridgeType::String
    }));
}

/// Verifies duplicate widget names and selectors are rejected.
///
/// Inputs:
/// - Two widgets sharing the same declaration identity.
///
/// Output:
/// - Stable duplicate-name and duplicate-selector diagnostics.
///
/// Transformation:
/// - Keeps generated widget metadata addressable by one unique name and one
///   unique HTML selector.
#[test]
fn mobile_widget_validation_rejects_duplicate_names_and_selectors() {
    let widget = standard_widget("Card");

    let diagnostics =
        validate_mobile_widget_declarations(&[widget.clone(), widget]).expect_err("duplicates");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_widget_duplicate_name"));
    assert!(codes.contains(&"mobile_widget_duplicate_selector"));
}

/// Verifies native-upgradable widgets require native component metadata.
///
/// Inputs:
/// - One native-upgradable widget with no native component name.
///
/// Output:
/// - Stable missing-native-component diagnostic.
///
/// Transformation:
/// - Prevents AngularTS/runtime metadata from claiming a native upgrade path
///   without a concrete native shell component.
#[test]
fn mobile_widget_validation_rejects_missing_native_component() {
    let mut widget = standard_widget("Image");
    widget.native_component = None;

    let diagnostics =
        validate_mobile_widget_declarations(&[widget]).expect_err("missing native component");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_widget_missing_native_component"));
}

/// Verifies native-upgradable widgets require the native-components capability.
///
/// Inputs:
/// - One native-upgradable widget without `native_components`.
///
/// Output:
/// - Stable missing-capability diagnostic.
///
/// Transformation:
/// - Keeps native component upgrades tied to explicit bridge capabilities.
#[test]
fn mobile_widget_validation_rejects_missing_native_components_capability() {
    let mut widget = standard_widget("CameraCapture");
    widget.capabilities = vec![
        MobileBridgeCapability::Camera,
        MobileBridgeCapability::Permissions,
    ];

    let diagnostics =
        validate_mobile_widget_declarations(&[widget]).expect_err("missing native capability");

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "mobile_widget_missing_native_components_capability"
    }));
}

/// Verifies duplicate widget props and events are rejected.
///
/// Inputs:
/// - One widget with repeated prop/event/payload names and repeated
///   capabilities.
///
/// Output:
/// - Stable diagnostics for all repeated local names.
///
/// Transformation:
/// - Keeps widget metadata deterministic before AngularTS helper generation.
#[test]
fn mobile_widget_validation_rejects_duplicate_local_names() {
    let mut widget = standard_widget("ToolbarAction");
    widget
        .capabilities
        .push(MobileBridgeCapability::NativeComponents);
    widget.props.push(widget.props[0].clone());
    widget.events.push(widget.events[0].clone());
    let duplicate_payload = widget.events[0].payload[0].clone();
    widget.events[0].payload.push(duplicate_payload);

    let diagnostics =
        validate_mobile_widget_declarations(&[widget]).expect_err("duplicate local names");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_widget_duplicate_capability"));
    assert!(codes.contains(&"mobile_widget_duplicate_prop"));
    assert!(codes.contains(&"mobile_widget_duplicate_event"));
    assert!(codes.contains(&"mobile_widget_duplicate_event_field"));
}

/// Verifies malformed widget local names are rejected.
///
/// Inputs:
/// - One widget with blank name, selector, prop, event, and event payload field.
///
/// Output:
/// - Stable malformed-name diagnostics.
///
/// Transformation:
/// - Exercises basic shape validation for future source-generated widget
///   declarations.
#[test]
fn mobile_widget_validation_rejects_malformed_names() {
    let mut widget = standard_widget("ToolbarAction");
    widget.name = String::new();
    widget.selector = " ".to_string();
    widget.props.push(MobileWidgetProp {
        name: String::new(),
        prop_type: MobileBridgeType::String,
        required: true,
    });
    widget.events.push(MobileWidgetEvent {
        name: String::new(),
        payload: vec![MobileBridgeField {
            name: String::new(),
            field_type: MobileBridgeType::String,
        }],
    });

    let diagnostics = validate_mobile_widget_declarations(&[widget]).expect_err("malformed names");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_widget_empty_name"));
    assert!(codes.contains(&"mobile_widget_empty_selector"));
    assert!(codes.contains(&"mobile_widget_empty_prop_name"));
    assert!(codes.contains(&"mobile_widget_empty_event_name"));
    assert!(codes.contains(&"mobile_widget_empty_event_field_name"));
}

/// Verifies widget upgrade spellings are stable.
///
/// Inputs:
/// - Every widget upgrade mode.
///
/// Output:
/// - Stable metadata spellings.
///
/// Transformation:
/// - Prevents AngularTS/native shell metadata from inventing multiple upgrade
///   vocabularies.
#[test]
fn mobile_widget_upgrade_names_are_stable() {
    assert_eq!(MobileWidgetUpgrade::WebOnly.as_str(), "web_only");
    assert_eq!(
        MobileWidgetUpgrade::NativeOptional.as_str(),
        "native_optional"
    );
    assert_eq!(
        MobileWidgetUpgrade::NativeRequired.as_str(),
        "native_required"
    );
}
