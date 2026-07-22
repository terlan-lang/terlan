use super::*;
use crate::mobile::mobile_bridge::{
    MobileBridgeCapability, MobileBridgeCommand, MobileBridgeDeclaration, MobileBridgeEvent,
    MobileBridgeField, MobileBridgeType,
};
use crate::mobile::mobile_debug_identity::{MobileSourceIdentity, MobileSourceSpan};

/// Builds a representative source identity for bridge validation.
fn source_identity(function_name: &str) -> MobileSourceIdentity {
    MobileSourceIdentity {
        module_path: "app.Mobile".to_string(),
        function_name: function_name.to_string(),
        span: MobileSourceSpan {
            file: "src/app/Mobile.terl".to_string(),
            start_line: 4,
            start_column: 1,
            end_line: 6,
            end_column: 2,
        },
    }
}

/// Builds a representative typechecker bridge declaration.
fn bridge_declaration() -> MobileBridgeDeclaration {
    MobileBridgeDeclaration {
        name: "ShellBridge".to_string(),
        capabilities: vec![
            MobileBridgeCapability::Navigation,
            MobileBridgeCapability::Camera,
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
            name: "routeChanged".to_string(),
            payload: vec![MobileBridgeField {
                name: "route".to_string(),
                field_type: MobileBridgeType::String,
            }],
            source_identity: Some(source_identity("routeChanged")),
        }],
    }
}

/// Verifies valid bridge declarations pass through typechecking.
///
/// Inputs:
/// - One typed bridge declaration with declared command capabilities.
///
/// Output:
/// - No typechecker diagnostics.
///
/// Transformation:
/// - Exercises the typechecker validation adapter without source parsing.
#[test]
fn mobile_bridge_typecheck_accepts_valid_declarations() {
    let diagnostics = check_mobile_bridge_declarations(&[bridge_declaration()]);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Verifies typechecking gates bridge metadata generation.
///
/// Inputs:
/// - One valid bridge declaration.
///
/// Output:
/// - Schema-versioned bridge metadata with source debug identity.
///
/// Transformation:
/// - Ensures build-facing metadata generation cannot bypass typechecker bridge
///   validation.
#[test]
fn mobile_bridge_typecheck_generates_validated_metadata() {
    let metadata =
        typecheck_mobile_bridge_metadata(&[bridge_declaration()]).expect("bridge metadata");

    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.declarations[0].commands[0].name, "openRoute");
    assert_eq!(
        metadata.declarations[0].commands[0]
            .source_identity
            .as_ref()
            .expect("source identity")
            .debug_key,
        "app.Mobile.openRoute@src/app/Mobile.terl:4:1-6:2"
    );
}

/// Verifies bridge validation failures become typechecker diagnostics.
///
/// Inputs:
/// - One command that requires an undeclared capability.
///
/// Output:
/// - Typechecker error containing the stable bridge diagnostic code.
///
/// Transformation:
/// - Prevents native capability use from bypassing typechecking.
#[test]
fn mobile_bridge_typecheck_rejects_missing_capability() {
    let mut declaration = bridge_declaration();
    declaration.capabilities = vec![MobileBridgeCapability::Navigation];

    let diagnostics = check_mobile_bridge_declarations(&[declaration]);

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0]
        .message
        .contains("mobile_bridge_missing_capability"));
}

/// Verifies source identity failures are reported by typechecking.
///
/// Inputs:
/// - One bridge event with an incomplete source identity.
///
/// Output:
/// - Typechecker error containing the source identity diagnostic code.
///
/// Transformation:
/// - Locks source-to-bridge debug identity validation into the typechecker
///   bridge path.
#[test]
fn mobile_bridge_typecheck_rejects_invalid_source_identity() {
    let mut declaration = bridge_declaration();
    declaration.events[0]
        .source_identity
        .as_mut()
        .expect("source identity")
        .function_name = String::new();

    let diagnostics =
        typecheck_mobile_bridge_metadata(&[declaration]).expect_err("invalid source identity");

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0]
        .message
        .contains("mobile_debug_identity_empty_function"));
}
