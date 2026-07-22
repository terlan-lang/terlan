use super::super::mobile_debug_identity::{MobileSourceIdentity, MobileSourceSpan};
use super::*;

/// Builds a representative source identity.
fn source_identity(function_name: &str) -> MobileSourceIdentity {
    MobileSourceIdentity {
        module_path: "app.Mobile".to_string(),
        function_name: function_name.to_string(),
        span: MobileSourceSpan {
            file: "src/app/Mobile.terl".to_string(),
            start_line: 10,
            start_column: 3,
            end_line: 12,
            end_column: 4,
        },
    }
}

/// Builds a representative mobile bridge declaration.
fn sample_bridge() -> MobileBridgeDeclaration {
    MobileBridgeDeclaration {
        name: "ShellBridge".to_string(),
        capabilities: vec![
            MobileBridgeCapability::Navigation,
            MobileBridgeCapability::Camera,
            MobileBridgeCapability::PlatformEnvironment,
        ],
        commands: vec![
            MobileBridgeCommand {
                name: "openRoute".to_string(),
                required_capability: MobileBridgeCapability::Navigation,
                parameters: vec![MobileBridgeField {
                    name: "route".to_string(),
                    field_type: MobileBridgeType::String,
                }],
                result: MobileBridgeType::Unit,
                source_identity: Some(source_identity("openRoute")),
            },
            MobileBridgeCommand {
                name: "takePhoto".to_string(),
                required_capability: MobileBridgeCapability::Camera,
                parameters: vec![],
                result: MobileBridgeType::Json,
                source_identity: None,
            },
        ],
        events: vec![MobileBridgeEvent {
            name: "platformChanged".to_string(),
            payload: vec![MobileBridgeField {
                name: "theme".to_string(),
                field_type: MobileBridgeType::String,
            }],
            source_identity: Some(source_identity("platformChanged")),
        }],
    }
}

/// Verifies a coherent typed bridge declaration validates.
///
/// Inputs:
/// - One bridge with declared capabilities, typed commands, and typed events.
///
/// Output:
/// - Successful validation.
///
/// Transformation:
/// - Exercises the first mobile bridge declaration model without source syntax
///   or metadata emission.
#[test]
fn mobile_bridge_declaration_accepts_typed_commands_and_events() {
    let declarations = vec![sample_bridge()];

    assert_eq!(validate_mobile_bridge_declarations(&declarations), Ok(()));
}

/// Verifies typed bridge declarations generate stable metadata.
///
/// Inputs:
/// - One bridge with navigation, camera, and platform environment capability
///   declarations.
///
/// Output:
/// - Metadata with schema version, sorted capability spellings, command
///   parameters/results, and event payloads.
///
/// Transformation:
/// - Exercises metadata generation without writing files or choosing a mobile
///   platform backend.
#[test]
fn mobile_bridge_declaration_generates_metadata() {
    let metadata = generate_mobile_bridge_metadata(&[sample_bridge()]).expect("generate metadata");

    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.declarations.len(), 1);
    let declaration = &metadata.declarations[0];
    assert_eq!(declaration.name, "ShellBridge");
    assert_eq!(
        declaration.capabilities,
        vec!["camera", "navigation", "platform_environment"]
    );
    assert_eq!(declaration.commands[0].name, "openRoute");
    assert_eq!(declaration.commands[0].required_capability, "navigation");
    assert_eq!(declaration.commands[0].parameters[0].name, "route");
    assert_eq!(declaration.commands[0].parameters[0].field_type, "String");
    assert_eq!(declaration.commands[0].result, "Unit");
    assert_eq!(
        declaration.commands[0]
            .source_identity
            .as_ref()
            .expect("command source identity")
            .debug_key,
        "app.Mobile.openRoute@src/app/Mobile.terl:10:3-12:4"
    );
    assert_eq!(declaration.commands[1].required_capability, "camera");
    assert_eq!(declaration.commands[1].result, "Json");
    assert_eq!(declaration.commands[1].source_identity, None);
    assert_eq!(declaration.events[0].name, "platformChanged");
    assert_eq!(declaration.events[0].payload[0].field_type, "String");
    assert_eq!(
        declaration.events[0]
            .source_identity
            .as_ref()
            .expect("event source identity")
            .debug_key,
        "app.Mobile.platformChanged@src/app/Mobile.terl:10:3-12:4"
    );
}

/// Verifies mobile bridge payload names remain stable string metadata.
///
/// Inputs:
/// - One bridge event whose payload field names look like Vm atom builder
///   functions.
///
/// Output:
/// - Metadata preserves the field names exactly.
///
/// Transformation:
/// - Exercises the native/mobile bridge declaration path without converting
///   payload keys into runtime atoms.
#[test]
fn mobile_bridge_payload_names_that_look_like_atom_builders_remain_strings() {
    let mut bridge = sample_bridge();
    bridge.events.push(MobileBridgeEvent {
        name: "nativePayload".to_string(),
        payload: vec![
            MobileBridgeField {
                name: "binary_to_atom".to_string(),
                field_type: MobileBridgeType::String,
            },
            MobileBridgeField {
                name: "list_to_atom".to_string(),
                field_type: MobileBridgeType::Json,
            },
        ],
        source_identity: Some(source_identity("nativePayload")),
    });

    let metadata = generate_mobile_bridge_metadata(&[bridge]).expect("generate metadata");
    let payload = &metadata.declarations[0].events[1].payload;

    assert_eq!(payload[0].name, "binary_to_atom");
    assert_eq!(payload[0].field_type, "String");
    assert_eq!(payload[1].name, "list_to_atom");
    assert_eq!(payload[1].field_type, "Json");
}

/// Verifies metadata generation rejects invalid declarations.
///
/// Inputs:
/// - One bridge command with an undeclared capability.
///
/// Output:
/// - Stable validation diagnostic instead of metadata.
///
/// Transformation:
/// - Ensures metadata generation cannot bypass declaration validation.
#[test]
fn mobile_bridge_metadata_generation_rejects_invalid_declarations() {
    let mut declaration = sample_bridge();
    declaration.capabilities = vec![MobileBridgeCapability::Navigation];

    let diagnostics =
        generate_mobile_bridge_metadata(&[declaration]).expect_err("invalid metadata");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_bridge_missing_capability"));
}

/// Verifies invalid command source identity is rejected by bridge validation.
///
/// Inputs:
/// - One bridge command whose source identity has an empty file.
///
/// Output:
/// - Stable source-identity diagnostic propagated through bridge validation.
///
/// Transformation:
/// - Prevents bridge metadata generation from emitting unusable source mapping.
#[test]
fn mobile_bridge_declaration_rejects_invalid_source_identity() {
    let mut declaration = sample_bridge();
    declaration.commands[0]
        .source_identity
        .as_mut()
        .expect("source identity")
        .span
        .file = String::new();

    let diagnostics =
        validate_mobile_bridge_declarations(&[declaration]).expect_err("invalid source identity");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_debug_identity_empty_file"));
}

/// Verifies duplicate declaration names are rejected.
///
/// Inputs:
/// - Two bridge declarations with the same name.
///
/// Output:
/// - Stable duplicate-name diagnostic.
///
/// Transformation:
/// - Keeps generated bridge metadata addressable by one unique declaration
///   name.
#[test]
fn mobile_bridge_declaration_rejects_duplicate_names() {
    let declarations = vec![sample_bridge(), sample_bridge()];

    let diagnostics =
        validate_mobile_bridge_declarations(&declarations).expect_err("duplicate name");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_bridge_duplicate_name"));
}

/// Verifies command capability references must be declared by the bridge.
///
/// Inputs:
/// - One command requiring a capability omitted from the bridge declaration.
///
/// Output:
/// - Stable missing-capability diagnostic.
///
/// Transformation:
/// - Prevents native capability use from entering bridge metadata implicitly.
#[test]
fn mobile_bridge_declaration_rejects_undeclared_command_capability() {
    let mut declaration = sample_bridge();
    declaration.capabilities = vec![MobileBridgeCapability::Navigation];

    let diagnostics =
        validate_mobile_bridge_declarations(&[declaration]).expect_err("missing capability");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_bridge_missing_capability"));
}

/// Verifies duplicate command, event, capability, and field names are rejected.
///
/// Inputs:
/// - One bridge declaration with repeated local names.
///
/// Output:
/// - Stable diagnostics for each duplicate class.
///
/// Transformation:
/// - Keeps bridge metadata deterministic before source-to-bridge generation.
#[test]
fn mobile_bridge_declaration_rejects_local_duplicates() {
    let mut declaration = sample_bridge();
    declaration
        .capabilities
        .push(MobileBridgeCapability::Camera);
    declaration.commands.push(declaration.commands[0].clone());
    declaration.events.push(declaration.events[0].clone());
    let duplicate_field = declaration.commands[0].parameters[0].clone();
    declaration.commands[0].parameters.push(duplicate_field);

    let diagnostics =
        validate_mobile_bridge_declarations(&[declaration]).expect_err("local duplicates");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_bridge_duplicate_capability"));
    assert!(codes.contains(&"mobile_bridge_duplicate_command"));
    assert!(codes.contains(&"mobile_bridge_duplicate_event"));
    assert!(codes.contains(&"mobile_bridge_duplicate_field"));
}

/// Verifies malformed bridge names are rejected.
///
/// Inputs:
/// - Bridge declaration with blank declaration, command, event, and field
///   names.
///
/// Output:
/// - Stable diagnostics for each malformed name class.
///
/// Transformation:
/// - Exercises declaration-shape validation before metadata generation.
#[test]
fn mobile_bridge_declaration_rejects_malformed_names() {
    let mut declaration = sample_bridge();
    declaration.name = " ".to_string();
    declaration.commands[0].name = String::new();
    declaration.events[0].name = "\t".to_string();
    declaration.commands[1].parameters.push(MobileBridgeField {
        name: String::new(),
        field_type: MobileBridgeType::String,
    });

    let diagnostics =
        validate_mobile_bridge_declarations(&[declaration]).expect_err("malformed names");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_bridge_empty_name"));
    assert!(codes.contains(&"mobile_bridge_empty_command_name"));
    assert!(codes.contains(&"mobile_bridge_empty_event_name"));
    assert!(codes.contains(&"mobile_bridge_empty_field_name"));
}

/// Verifies permission commands must declare the permissions capability.
///
/// Inputs:
/// - One command requiring `MobileBridgeCapability::Permissions` without the
///   bridge declaring that capability.
///
/// Output:
/// - Stable missing-capability diagnostic.
///
/// Transformation:
/// - Locks permission use into the same explicit capability contract as other
///   native bridge features.
#[test]
fn mobile_bridge_declaration_rejects_missing_permissions_capability() {
    let mut declaration = sample_bridge();
    declaration.commands.push(MobileBridgeCommand {
        name: "requestCameraPermission".to_string(),
        required_capability: MobileBridgeCapability::Permissions,
        parameters: vec![],
        result: MobileBridgeType::Bool,
        source_identity: Some(source_identity("requestCameraPermission")),
    });

    let diagnostics =
        validate_mobile_bridge_declarations(&[declaration]).expect_err("missing permissions");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_bridge_missing_capability"));
}

/// Verifies generated metadata can be checked for stale arity and field types.
///
/// Inputs:
/// - Valid declaration plus generated metadata with one removed parameter and
///   one stale event payload type.
///
/// Output:
/// - Stable stale-arity and stale-field-type diagnostics.
///
/// Transformation:
/// - Exercises release/build validation for metadata that no longer matches
///   the typed bridge declaration.
#[test]
fn mobile_bridge_metadata_validation_rejects_stale_arity_and_field_type() {
    let declaration = sample_bridge();
    let mut metadata =
        generate_mobile_bridge_metadata(std::slice::from_ref(&declaration)).expect("metadata");
    metadata.declarations[0].commands[0].parameters.clear();
    metadata.declarations[0].events[0].payload[0].field_type = "Int";

    let diagnostics =
        validate_mobile_bridge_metadata_matches_declarations(&[declaration], &metadata)
            .expect_err("stale metadata");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_bridge_stale_metadata_arity"));
    assert!(codes.contains(&"mobile_bridge_stale_metadata_field_type"));
}

/// Verifies generated metadata can be checked for stale command contract data.
///
/// Inputs:
/// - Valid declaration plus generated metadata with a stale command capability
///   and result type.
///
/// Output:
/// - Stable stale-capability and stale-result diagnostics.
///
/// Transformation:
/// - Prevents command metadata from drifting from the typed declaration used by
///   typechecking.
#[test]
fn mobile_bridge_metadata_validation_rejects_stale_command_contract() {
    let declaration = sample_bridge();
    let mut metadata =
        generate_mobile_bridge_metadata(std::slice::from_ref(&declaration)).expect("metadata");
    metadata.declarations[0].commands[0].required_capability = "camera";
    metadata.declarations[0].commands[0].result = "String";

    let diagnostics =
        validate_mobile_bridge_metadata_matches_declarations(&[declaration], &metadata)
            .expect_err("stale metadata");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_bridge_stale_metadata_capability"));
    assert!(codes.contains(&"mobile_bridge_stale_metadata_result_type"));
}

/// Verifies generated metadata can be checked for stale source identity.
///
/// Inputs:
/// - Valid declaration plus generated metadata with a stale source debug key.
///
/// Output:
/// - Stable stale-source-identity diagnostic.
///
/// Transformation:
/// - Keeps native reply/error mapping tied to the current Terlan source span.
#[test]
fn mobile_bridge_metadata_validation_rejects_stale_source_identity() {
    let declaration = sample_bridge();
    let mut metadata =
        generate_mobile_bridge_metadata(std::slice::from_ref(&declaration)).expect("metadata");
    metadata.declarations[0].commands[0]
        .source_identity
        .as_mut()
        .expect("source identity")
        .debug_key = "app.Mobile.old@src/app/Mobile.terl:1:1-1:2".to_string();

    let diagnostics =
        validate_mobile_bridge_metadata_matches_declarations(&[declaration], &metadata)
            .expect_err("stale source identity");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_bridge_stale_metadata_source_identity"));
}

/// Verifies generated metadata can be checked for stale schema version.
///
/// Inputs:
/// - Valid declaration plus generated metadata with an unsupported schema
///   version.
///
/// Output:
/// - Stable stale-schema diagnostic.
///
/// Transformation:
/// - Gives release checks a direct way to reject old mobile bridge artifacts.
#[test]
fn mobile_bridge_metadata_validation_rejects_stale_schema() {
    let declaration = sample_bridge();
    let mut metadata =
        generate_mobile_bridge_metadata(std::slice::from_ref(&declaration)).expect("metadata");
    metadata.schema_version = 0;

    let diagnostics =
        validate_mobile_bridge_metadata_matches_declarations(&[declaration], &metadata)
            .expect_err("stale schema");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_bridge_stale_metadata_schema"));
}

/// Verifies bridge type names are stable for generated metadata.
///
/// Inputs:
/// - Every first-slice mobile bridge type.
///
/// Output:
/// - Stable manifest spelling for each type.
///
/// Transformation:
/// - Protects future metadata emitters from inventing a second type spelling.
#[test]
fn mobile_bridge_type_names_are_stable() {
    assert_eq!(MobileBridgeType::Unit.as_str(), "Unit");
    assert_eq!(MobileBridgeType::Bool.as_str(), "Bool");
    assert_eq!(MobileBridgeType::Int.as_str(), "Int");
    assert_eq!(MobileBridgeType::Float.as_str(), "Float");
    assert_eq!(MobileBridgeType::String.as_str(), "String");
    assert_eq!(MobileBridgeType::Json.as_str(), "Json");
}

/// Verifies bridge capability names are stable for generated metadata.
///
/// Inputs:
/// - Every first-slice mobile bridge capability.
///
/// Output:
/// - Stable manifest spelling for each capability.
///
/// Transformation:
/// - Protects future metadata emitters from inventing a second capability
///   spelling.
#[test]
fn mobile_bridge_capability_names_are_stable() {
    let capabilities = [
        (MobileBridgeCapability::Navigation, "navigation"),
        (
            MobileBridgeCapability::NativeComponents,
            "native_components",
        ),
        (MobileBridgeCapability::Permissions, "permissions"),
        (MobileBridgeCapability::Files, "files"),
        (MobileBridgeCapability::Camera, "camera"),
        (MobileBridgeCapability::Geolocation, "geolocation"),
        (MobileBridgeCapability::Storage, "storage"),
        (
            MobileBridgeCapability::PushNotifications,
            "push_notifications",
        ),
        (
            MobileBridgeCapability::PlatformEnvironment,
            "platform_environment",
        ),
    ];

    for (capability, expected) in capabilities {
        assert_eq!(capability.as_str(), expected);
    }
}
