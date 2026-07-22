use super::super::mobile_bridge::MobileBridgeCapability;
use super::*;

/// Verifies mobile capabilities become native-boundary resources.
///
/// Inputs:
/// - Navigation and camera mobile bridge capabilities.
///
/// Output:
/// - Resource manifest with stable native-boundary resource names.
///
/// Transformation:
/// - Converts typed capability enum values into metadata-ready resource rows.
#[test]
fn mobile_native_capabilities_generate_native_boundary_resources() {
    let manifest = generate_mobile_native_capability_resources(&[
        MobileBridgeCapability::Navigation,
        MobileBridgeCapability::Camera,
    ])
    .expect("capability resources");

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.resources.len(), 2);
    assert_eq!(manifest.resources[0].name, "mobile.navigation");
    assert_eq!(manifest.resources[0].capability, "navigation");
    assert_eq!(manifest.resources[0].boundary, "native_boundary");
    assert_eq!(manifest.resources[1].name, "mobile.camera");
    assert_eq!(manifest.resources[1].capability, "camera");
    assert_eq!(manifest.resources[1].boundary, "native_boundary");
}

/// Verifies duplicate mobile capabilities are rejected.
///
/// Inputs:
/// - Repeated geolocation capability.
///
/// Output:
/// - Stable duplicate-capability diagnostic.
///
/// Transformation:
/// - Prevents native-boundary resource manifests from carrying ambiguous
///   duplicate resource rows for the same mobile capability.
#[test]
fn mobile_native_capabilities_reject_duplicates() {
    let diagnostics = generate_mobile_native_capability_resources(&[
        MobileBridgeCapability::Geolocation,
        MobileBridgeCapability::Geolocation,
    ])
    .expect_err("duplicate capability");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "mobile_native_capability_duplicate");
    assert!(diagnostics[0]
        .message
        .contains("mobile native capability `geolocation`"));
}

/// Verifies empty mobile capability sets produce an empty resource manifest.
///
/// Inputs:
/// - Empty capability list.
///
/// Output:
/// - Schema-versioned manifest with no resources.
///
/// Transformation:
/// - Keeps first-slice mobile build metadata valid before source-level
///   capability collection exists.
#[test]
fn mobile_native_capabilities_accept_empty_set() {
    let manifest = generate_mobile_native_capability_resources(&[]).expect("empty manifest");

    assert_eq!(manifest.schema_version, 1);
    assert!(manifest.resources.is_empty());
}

/// Verifies explicit native service declarations generate service-scoped resources.
///
/// Inputs:
/// - A camera service with camera and files capabilities.
///
/// Output:
/// - Service-scoped native-boundary resource names.
///
/// Transformation:
/// - Converts explicit service declarations into metadata rows without
///   inferring capabilities from commands or implementation names.
#[test]
fn mobile_native_services_generate_service_scoped_resources() {
    let manifest =
        generate_mobile_native_service_capability_resources(&[MobileNativeServiceDeclaration {
            name: "camera".to_string(),
            capabilities: vec![
                MobileBridgeCapability::Camera,
                MobileBridgeCapability::Files,
            ],
        }])
        .expect("service resources");

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.resources.len(), 2);
    assert_eq!(manifest.resources[0].name, "mobile.camera.camera");
    assert_eq!(manifest.resources[0].capability, "camera");
    assert_eq!(manifest.resources[0].boundary, "native_boundary");
    assert_eq!(manifest.resources[1].name, "mobile.camera.files");
    assert_eq!(manifest.resources[1].capability, "files");
    assert_eq!(manifest.resources[1].boundary, "native_boundary");
}

/// Verifies native services must declare capabilities explicitly.
///
/// Inputs:
/// - One named service with no capabilities.
///
/// Output:
/// - Stable missing-capability diagnostic.
///
/// Transformation:
/// - Prevents native services from entering the manifest with implementation-
///   inferred or empty capability surfaces.
#[test]
fn mobile_native_services_reject_missing_capabilities() {
    let diagnostics =
        generate_mobile_native_service_capability_resources(&[MobileNativeServiceDeclaration {
            name: "camera".to_string(),
            capabilities: vec![],
        }])
        .expect_err("missing capability");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].code,
        "mobile_native_service_missing_capability"
    );
}

/// Verifies native service names are explicit and unique.
///
/// Inputs:
/// - One blank service name and two repeated storage services.
///
/// Output:
/// - Stable empty-name and duplicate-service diagnostics.
///
/// Transformation:
/// - Keeps mobile native service declarations addressable by one stable name.
#[test]
fn mobile_native_services_reject_blank_and_duplicate_names() {
    let diagnostics = generate_mobile_native_service_capability_resources(&[
        MobileNativeServiceDeclaration {
            name: String::new(),
            capabilities: vec![MobileBridgeCapability::Storage],
        },
        MobileNativeServiceDeclaration {
            name: "storage".to_string(),
            capabilities: vec![MobileBridgeCapability::Storage],
        },
        MobileNativeServiceDeclaration {
            name: "storage".to_string(),
            capabilities: vec![MobileBridgeCapability::Files],
        },
    ])
    .expect_err("invalid services");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_native_service_empty_name"));
    assert!(codes.contains(&"mobile_native_service_duplicate"));
}

/// Verifies one service cannot declare the same capability twice.
///
/// Inputs:
/// - One geolocation service with repeated geolocation capability.
///
/// Output:
/// - Stable duplicate service capability diagnostic.
///
/// Transformation:
/// - Prevents ambiguous duplicate resource rows inside one native service.
#[test]
fn mobile_native_services_reject_duplicate_service_capabilities() {
    let diagnostics =
        generate_mobile_native_service_capability_resources(&[MobileNativeServiceDeclaration {
            name: "location".to_string(),
            capabilities: vec![
                MobileBridgeCapability::Geolocation,
                MobileBridgeCapability::Geolocation,
            ],
        }])
        .expect_err("duplicate service capability");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].code,
        "mobile_native_service_duplicate_capability"
    );
}
