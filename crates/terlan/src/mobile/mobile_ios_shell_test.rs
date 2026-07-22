use super::super::mobile_bridge::MobileBridgeCapability;
use super::super::mobile_route::{
    generate_mobile_route_configuration, MobileRouteDeclaration, MobileRoutePresentation,
    MobileRoutePresentationHint, MobileRouteSource,
};
use super::super::mobile_widget::{
    generate_mobile_widget_metadata, standard_mobile_widget_declarations,
};
use super::*;

/// Builds one representative iOS shell config.
fn ios_config() -> IosShellConfig {
    IosShellConfig {
        app_name: "TerlanDemo".to_string(),
        bundle_id: "io.terlan.demo".to_string(),
        swift_module_prefix: "TerlanDemo".to_string(),
    }
}

/// Verifies the minimal iOS shell layout is generated.
///
/// Inputs:
/// - One valid iOS shell config.
///
/// Output:
/// - Schema-versioned layout with Core, Navigation, Demo, and App modules.
///
/// Transformation:
/// - Templates a deterministic Swift package structure without invoking Apple
///   tooling or writing files.
#[test]
fn ios_shell_generates_minimal_module_layout() {
    let layout = generate_ios_shell_layout(&ios_config()).expect("ios layout");
    let paths = layout
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(layout.schema_version, 1);
    assert_eq!(layout.modules, vec!["Core", "Navigation", "Demo", "App"]);
    assert!(paths.contains(&"Package.swift"));
    assert!(paths.contains(&"Sources/TerlanMobileCore/TerlanMobileBridge.swift"));
    assert!(paths.contains(&"Sources/TerlanMobileNavigation/TerlanMobileRouter.swift"));
    assert!(paths.contains(&"Sources/TerlanMobileDemo/TerlanMobileDemo.swift"));
    assert!(paths.contains(&"Sources/TerlanMobileApp/TerlanMobileApp.swift"));
    assert!(paths.contains(&"Sources/TerlanMobileApp/Resources/.keep"));
}

/// Verifies generated iOS shell file contents contain stable anchors.
///
/// Inputs:
/// - One valid iOS shell config.
///
/// Output:
/// - Generated package, bridge, route, demo, and app source contents with
///   stable module and symbol anchors.
///
/// Transformation:
/// - Pins the first iOS shell template surface for later shell generation.
#[test]
fn ios_shell_generates_stable_file_contents() {
    let layout = generate_ios_shell_layout(&ios_config()).expect("ios layout");
    let package = file_contents(&layout, "Package.swift");
    let bridge = file_contents(&layout, "Sources/TerlanMobileCore/TerlanMobileBridge.swift");
    let router = file_contents(
        &layout,
        "Sources/TerlanMobileNavigation/TerlanMobileRouter.swift",
    );
    let app = file_contents(&layout, "Sources/TerlanMobileApp/TerlanMobileApp.swift");

    assert!(package.contains("name: \"TerlanDemo\""));
    assert!(package.contains(".target(name: \"TerlanMobileNavigation\""));
    assert!(bridge.contains("public struct TerlanDemoMobileBridgeMessage"));
    assert!(router.contains("public struct TerlanDemoMobileRoute"));
    assert!(app.contains("public struct TerlanDemoMobileAppRoot: View"));
}

/// Verifies iOS native screen route upgrades are generated.
///
/// Inputs:
/// - One explicit native-screen route and one route upgraded by hint.
///
/// Output:
/// - Schema-versioned iOS native screen route plan.
///
/// Transformation:
/// - Converts shared route presentation metadata into iOS native screen inputs.
#[test]
fn ios_shell_generates_native_screen_route_plan() {
    let route_config = generate_mobile_route_configuration(&[
        MobileRouteDeclaration {
            route: "/profile".to_string(),
            source: MobileRouteSource::Terlan,
            handler: "App.Http.profile".to_string(),
            presentation: MobileRoutePresentation::NativeFragment,
            presentation_hints: vec![],
            native_component: Some("ProfileScreen".to_string()),
            source_identity: None,
        },
        MobileRouteDeclaration {
            route: "/settings".to_string(),
            source: MobileRouteSource::Terlan,
            handler: "App.Http.settings".to_string(),
            presentation: MobileRoutePresentation::Web,
            presentation_hints: vec![MobileRoutePresentationHint::NativeFragmentUpgrade],
            native_component: Some("SettingsScreen".to_string()),
            source_identity: None,
        },
    ])
    .expect("route config");
    let plan = generate_ios_native_screen_route_plan(&route_config).expect("screen plan");

    assert_eq!(plan.schema_version, 1);
    assert_eq!(plan.routes.len(), 2);
    assert_eq!(plan.routes[0].native_screen, "ProfileScreen");
    assert_eq!(plan.routes[0].presentation, "native_fragment");
    assert_eq!(plan.routes[1].native_screen, "SettingsScreen");
    assert_eq!(
        plan.routes[1].presentation_hints,
        vec!["native_fragment_upgrade"]
    );
}

/// Verifies iOS modal and bottom-sheet route presentations are generated.
///
/// Inputs:
/// - Routes with explicit modal and bottom-sheet presentations.
///
/// Output:
/// - Schema-versioned iOS presentation plan.
///
/// Transformation:
/// - Converts shared route presentation metadata into iOS presentation inputs.
#[test]
fn ios_shell_generates_route_presentation_plan() {
    let route_config = generate_mobile_route_configuration(&[
        MobileRouteDeclaration {
            route: "/login".to_string(),
            source: MobileRouteSource::Terlan,
            handler: "App.Http.login".to_string(),
            presentation: MobileRoutePresentation::Modal,
            presentation_hints: vec![],
            native_component: Some("LoginSheet".to_string()),
            source_identity: None,
        },
        MobileRouteDeclaration {
            route: "/actions".to_string(),
            source: MobileRouteSource::Terlan,
            handler: "App.Http.actions".to_string(),
            presentation: MobileRoutePresentation::BottomSheet,
            presentation_hints: vec![],
            native_component: Some("ActionsSheet".to_string()),
            source_identity: None,
        },
    ])
    .expect("route config");
    let plan = generate_ios_route_presentation_plan(&route_config).expect("presentation plan");

    assert_eq!(plan.schema_version, 1);
    assert_eq!(plan.routes.len(), 2);
    assert_eq!(plan.routes[0].presentation_mode, "modal");
    assert_eq!(plan.routes[0].native_component, "LoginSheet");
    assert_eq!(plan.routes[1].presentation_mode, "bottom_sheet");
    assert_eq!(plan.routes[1].native_component, "ActionsSheet");
}

/// Verifies iOS bridge component plan includes the standard widget set.
///
/// Inputs:
/// - Standard mobile widget metadata.
///
/// Output:
/// - iOS bridge component plan preserving widget names, selectors,
///   capabilities, and native component names.
///
/// Transformation:
/// - Converts shared widget metadata into iOS native bridge component inputs.
#[test]
fn ios_shell_generates_bridge_component_plan() {
    let widget_metadata = generate_mobile_widget_metadata(&standard_mobile_widget_declarations())
        .expect("widget metadata");
    let plan = generate_ios_bridge_component_plan(&widget_metadata).expect("component plan");
    let names = plan
        .components
        .iter()
        .map(|component| component.widget.as_str())
        .collect::<Vec<_>>();

    assert_eq!(plan.schema_version, 1);
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
    assert_eq!(plan.components[0].selector, "terlan-toolbar-action");
    assert_eq!(plan.components[0].native_component, "ToolbarAction");
    assert_eq!(plan.components[0].capabilities, vec!["native_components"]);
}

/// Verifies iOS platform behavior capability requirements are typed.
///
/// Inputs:
/// - iOS-only native screen, WebView environment, camera, file, geolocation,
///   and push behavior declarations.
///
/// Output:
/// - Required mobile capabilities for each behavior.
///
/// Transformation:
/// - Keeps iOS-specific shell behavior behind explicit mobile capability
///   declarations.
#[test]
fn ios_shell_platform_behaviors_declare_required_capabilities() {
    assert_eq!(
        ios_platform_behavior_required_capabilities(IosPlatformBehavior::NativeScreenUpgrade),
        vec![MobileBridgeCapability::NativeComponents]
    );
    assert_eq!(
        ios_platform_behavior_required_capabilities(IosPlatformBehavior::WebViewEnvironment),
        vec![MobileBridgeCapability::PlatformEnvironment]
    );
    assert_eq!(
        ios_platform_behavior_required_capabilities(IosPlatformBehavior::CameraCapture),
        vec![
            MobileBridgeCapability::Camera,
            MobileBridgeCapability::Permissions
        ]
    );
    assert_eq!(
        ios_platform_behavior_required_capabilities(IosPlatformBehavior::FilePicker),
        vec![
            MobileBridgeCapability::Files,
            MobileBridgeCapability::Permissions
        ]
    );
    assert_eq!(
        ios_platform_behavior_required_capabilities(IosPlatformBehavior::Geolocation),
        vec![
            MobileBridgeCapability::Geolocation,
            MobileBridgeCapability::Permissions
        ]
    );
    assert_eq!(
        ios_platform_behavior_required_capabilities(IosPlatformBehavior::PushNotifications),
        vec![
            MobileBridgeCapability::PushNotifications,
            MobileBridgeCapability::Permissions
        ]
    );
}

/// Verifies iOS platform behavior capability validation.
///
/// Inputs:
/// - Matching and missing capability declarations for iOS-only behaviors.
///
/// Output:
/// - Successful validation for complete declarations and stable diagnostics for
///   missing capabilities.
///
/// Transformation:
/// - Prevents iOS-specific behavior from being planned without explicit
///   capability declarations.
#[test]
fn ios_shell_validates_platform_behavior_capabilities() {
    assert_eq!(
        validate_ios_platform_behavior_capabilities(
            IosPlatformBehavior::CameraCapture,
            &[
                MobileBridgeCapability::Camera,
                MobileBridgeCapability::Permissions
            ],
        ),
        Ok(())
    );

    let diagnostics = validate_ios_platform_behavior_capabilities(
        IosPlatformBehavior::CameraCapture,
        &[MobileBridgeCapability::Camera],
    )
    .expect_err("missing permissions capability");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "ios_shell_missing_platform_capability");
    assert!(diagnostics[0].message.contains("permissions"));
}

/// Verifies invalid iOS shell config is rejected.
///
/// Inputs:
/// - Empty app name plus invalid bundle id and Swift module prefix.
///
/// Output:
/// - Stable diagnostics for each invalid field.
///
/// Transformation:
/// - Keeps generated iOS shell layouts addressable by valid Swift/package
///   names.
#[test]
fn ios_shell_rejects_invalid_config() {
    let diagnostics = validate_ios_shell_config(&IosShellConfig {
        app_name: String::new(),
        bundle_id: "bad id".to_string(),
        swift_module_prefix: "bad-prefix".to_string(),
    })
    .expect_err("invalid config");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"ios_shell_empty_app_name"));
    assert!(codes.contains(&"ios_shell_invalid_bundle_id"));
    assert!(codes.contains(&"ios_shell_invalid_swift_module_prefix"));
}

/// Verifies iOS shell file kind names are stable.
///
/// Inputs:
/// - Every first-slice iOS shell file kind.
///
/// Output:
/// - Stable file kind spellings.
///
/// Transformation:
/// - Keeps generated shell layout metadata stable for build planning.
#[test]
fn ios_shell_file_kind_names_are_stable() {
    assert_eq!(
        IosShellFileKind::PackageManifest.as_str(),
        "package_manifest"
    );
    assert_eq!(IosShellFileKind::SwiftSource.as_str(), "swift_source");
    assert_eq!(
        IosShellFileKind::ResourcePlaceholder.as_str(),
        "resource_placeholder"
    );
    assert_eq!(
        IosPlatformBehavior::NativeScreenUpgrade.as_str(),
        "native_screen_upgrade"
    );
    assert_eq!(
        IosPlatformBehavior::ModalPresentation.as_str(),
        "modal_presentation"
    );
    assert_eq!(
        IosPlatformBehavior::BottomSheetPresentation.as_str(),
        "bottom_sheet_presentation"
    );
    assert_eq!(
        IosPlatformBehavior::WebViewEnvironment.as_str(),
        "webview_environment"
    );
    assert_eq!(
        IosPlatformBehavior::CameraCapture.as_str(),
        "camera_capture"
    );
    assert_eq!(IosPlatformBehavior::FilePicker.as_str(), "file_picker");
    assert_eq!(IosPlatformBehavior::Geolocation.as_str(), "geolocation");
    assert_eq!(
        IosPlatformBehavior::PushNotifications.as_str(),
        "push_notifications"
    );
}

/// Reads one generated file's contents.
fn file_contents<'a>(layout: &'a IosShellLayout, path: &str) -> &'a str {
    layout
        .files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.contents.as_str())
        .expect("generated file")
}
