//! iOS Swift shell project layout generation for the mobile profile.
#![allow(dead_code)]
//!
//! Inputs:
//! - Typed iOS shell configuration.
//!
//! Outputs:
//! - A deterministic list of generated Swift package files for the iOS shell.
//!
//! Transformation:
//! - Templates the first iOS shell structure matching the Android
//!   core/navigation/demo/app split without invoking Apple tooling or writing
//!   files directly.

use super::mobile_bridge::MobileBridgeCapability;
use super::mobile_route::MobileRouteConfiguration;
use super::mobile_widget::MobileWidgetMetadata;

/// iOS shell generation input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosShellConfig {
    pub(crate) app_name: String,
    pub(crate) bundle_id: String,
    pub(crate) swift_module_prefix: String,
}

/// One generated iOS shell file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosShellFile {
    pub(crate) path: String,
    pub(crate) kind: IosShellFileKind,
    pub(crate) contents: String,
}

/// Generated iOS shell file kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IosShellFileKind {
    PackageManifest,
    SwiftSource,
    ResourcePlaceholder,
}

impl IosShellFileKind {
    /// Returns the stable file kind spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::PackageManifest => "package_manifest",
            Self::SwiftSource => "swift_source",
            Self::ResourcePlaceholder => "resource_placeholder",
        }
    }
}

/// Generated iOS shell layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosShellLayout {
    pub(crate) schema_version: u32,
    pub(crate) modules: Vec<&'static str>,
    pub(crate) files: Vec<IosShellFile>,
}

/// iOS shell layout diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosShellDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// iOS native screen route upgrade plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosNativeScreenRoutePlan {
    pub(crate) schema_version: u32,
    pub(crate) routes: Vec<IosNativeScreenRoute>,
}

/// One iOS native screen route upgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosNativeScreenRoute {
    pub(crate) route: String,
    pub(crate) handler: String,
    pub(crate) native_screen: String,
    pub(crate) presentation: String,
    pub(crate) presentation_hints: Vec<String>,
}

/// iOS modal/bottom-sheet route presentation plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosRoutePresentationPlan {
    pub(crate) schema_version: u32,
    pub(crate) routes: Vec<IosRoutePresentation>,
}

/// One iOS native route presentation entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosRoutePresentation {
    pub(crate) route: String,
    pub(crate) handler: String,
    pub(crate) native_component: String,
    pub(crate) presentation_mode: String,
    pub(crate) presentation_hints: Vec<String>,
}

/// iOS native bridge component plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosBridgeComponentPlan {
    pub(crate) schema_version: u32,
    pub(crate) components: Vec<IosBridgeComponent>,
}

/// One iOS native bridge component entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IosBridgeComponent {
    pub(crate) widget: String,
    pub(crate) selector: String,
    pub(crate) native_component: String,
    pub(crate) upgrade: String,
    pub(crate) capabilities: Vec<String>,
}

/// iOS-specific platform behavior requiring explicit mobile capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IosPlatformBehavior {
    NativeScreenUpgrade,
    ModalPresentation,
    BottomSheetPresentation,
    WebViewEnvironment,
    CameraCapture,
    FilePicker,
    Geolocation,
    PushNotifications,
}

impl IosPlatformBehavior {
    /// Returns the stable behavior spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NativeScreenUpgrade => "native_screen_upgrade",
            Self::ModalPresentation => "modal_presentation",
            Self::BottomSheetPresentation => "bottom_sheet_presentation",
            Self::WebViewEnvironment => "webview_environment",
            Self::CameraCapture => "camera_capture",
            Self::FilePicker => "file_picker",
            Self::Geolocation => "geolocation",
            Self::PushNotifications => "push_notifications",
        }
    }
}

/// Generates the minimal iOS Swift shell package layout.
pub(crate) fn generate_ios_shell_layout(
    config: &IosShellConfig,
) -> Result<IosShellLayout, Vec<IosShellDiagnostic>> {
    validate_ios_shell_config(config)?;
    Ok(IosShellLayout {
        schema_version: 1,
        modules: vec!["Core", "Navigation", "Demo", "App"],
        files: vec![
            shell_file(
                "Package.swift",
                IosShellFileKind::PackageManifest,
                package_swift(config),
            ),
            shell_file(
                "Sources/TerlanMobileCore/TerlanMobileBridge.swift",
                IosShellFileKind::SwiftSource,
                core_bridge_swift(config),
            ),
            shell_file(
                "Sources/TerlanMobileNavigation/TerlanMobileRouter.swift",
                IosShellFileKind::SwiftSource,
                navigation_router_swift(config),
            ),
            shell_file(
                "Sources/TerlanMobileDemo/TerlanMobileDemo.swift",
                IosShellFileKind::SwiftSource,
                demo_swift(config),
            ),
            shell_file(
                "Sources/TerlanMobileApp/TerlanMobileApp.swift",
                IosShellFileKind::SwiftSource,
                app_swift(config),
            ),
            shell_file(
                "Sources/TerlanMobileApp/Resources/.keep",
                IosShellFileKind::ResourcePlaceholder,
                String::new(),
            ),
        ],
    })
}

/// Validates iOS shell generation input.
pub(crate) fn validate_ios_shell_config(
    config: &IosShellConfig,
) -> Result<(), Vec<IosShellDiagnostic>> {
    let mut diagnostics = Vec::new();
    if is_blank(&config.app_name) {
        diagnostics.push(diagnostic(
            "ios_shell_empty_app_name",
            "iOS shell app name must not be empty",
        ));
    }
    if !is_valid_dotted_identifier(&config.bundle_id) {
        diagnostics.push(diagnostic(
            "ios_shell_invalid_bundle_id",
            format!(
                "iOS shell bundle id `{}` must be a dotted identifier",
                config.bundle_id
            ),
        ));
    }
    if !is_valid_swift_identifier(&config.swift_module_prefix) {
        diagnostics.push(diagnostic(
            "ios_shell_invalid_swift_module_prefix",
            format!(
                "iOS shell Swift module prefix `{}` must be an identifier",
                config.swift_module_prefix
            ),
        ));
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Generates iOS native screen route upgrades from route config.
pub(crate) fn generate_ios_native_screen_route_plan(
    routes: &MobileRouteConfiguration,
) -> Result<IosNativeScreenRoutePlan, Vec<IosShellDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut upgrades = Vec::new();

    for route in &routes.routes {
        if route.presentation == "native_fragment"
            || route
                .presentation_hints
                .contains(&"native_fragment_upgrade")
        {
            let Some(native_screen) = route.native_component.as_ref() else {
                diagnostics.push(diagnostic(
                    "ios_shell_missing_native_screen",
                    format!(
                        "iOS route `{}` requires a native screen component",
                        route.route
                    ),
                ));
                continue;
            };
            upgrades.push(IosNativeScreenRoute {
                route: route.route.clone(),
                handler: route.handler.clone(),
                native_screen: native_screen.clone(),
                presentation: route.presentation.to_string(),
                presentation_hints: route
                    .presentation_hints
                    .iter()
                    .map(|hint| (*hint).to_string())
                    .collect(),
            });
        }
    }

    if diagnostics.is_empty() {
        Ok(IosNativeScreenRoutePlan {
            schema_version: 1,
            routes: upgrades,
        })
    } else {
        Err(diagnostics)
    }
}

/// Generates iOS modal and bottom-sheet route presentation metadata.
pub(crate) fn generate_ios_route_presentation_plan(
    routes: &MobileRouteConfiguration,
) -> Result<IosRoutePresentationPlan, Vec<IosShellDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut presentations = Vec::new();

    for route in &routes.routes {
        let Some(presentation_mode) =
            ios_route_presentation_mode(route.presentation, &route.presentation_hints)
        else {
            continue;
        };
        let Some(native_component) = route.native_component.as_ref() else {
            diagnostics.push(diagnostic(
                "ios_shell_missing_route_presentation_component",
                format!(
                    "iOS route `{}` presentation `{presentation_mode}` requires a native component",
                    route.route
                ),
            ));
            continue;
        };
        presentations.push(IosRoutePresentation {
            route: route.route.clone(),
            handler: route.handler.clone(),
            native_component: native_component.clone(),
            presentation_mode: presentation_mode.to_string(),
            presentation_hints: route
                .presentation_hints
                .iter()
                .map(|hint| (*hint).to_string())
                .collect(),
        });
    }

    if diagnostics.is_empty() {
        Ok(IosRoutePresentationPlan {
            schema_version: 1,
            routes: presentations,
        })
    } else {
        Err(diagnostics)
    }
}

/// Generates iOS native bridge component metadata from widget metadata.
pub(crate) fn generate_ios_bridge_component_plan(
    widgets: &MobileWidgetMetadata,
) -> Result<IosBridgeComponentPlan, Vec<IosShellDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut components = Vec::new();

    for widget in &widgets.widgets {
        if widget.upgrade != "web_only" {
            let Some(native_component) = widget.native_component.as_ref() else {
                diagnostics.push(diagnostic(
                    "ios_shell_missing_bridge_component",
                    format!("iOS widget `{}` requires a native component", widget.name),
                ));
                continue;
            };
            components.push(IosBridgeComponent {
                widget: widget.name.clone(),
                selector: widget.selector.clone(),
                native_component: native_component.clone(),
                upgrade: widget.upgrade.to_string(),
                capabilities: widget
                    .capabilities
                    .iter()
                    .map(|capability| (*capability).to_string())
                    .collect(),
            });
        }
    }

    if diagnostics.is_empty() {
        Ok(IosBridgeComponentPlan {
            schema_version: widgets.schema_version,
            components,
        })
    } else {
        Err(diagnostics)
    }
}

/// Returns required mobile capabilities for one iOS platform behavior.
pub(crate) fn ios_platform_behavior_required_capabilities(
    behavior: IosPlatformBehavior,
) -> Vec<MobileBridgeCapability> {
    match behavior {
        IosPlatformBehavior::NativeScreenUpgrade
        | IosPlatformBehavior::ModalPresentation
        | IosPlatformBehavior::BottomSheetPresentation => {
            vec![MobileBridgeCapability::NativeComponents]
        }
        IosPlatformBehavior::WebViewEnvironment => {
            vec![MobileBridgeCapability::PlatformEnvironment]
        }
        IosPlatformBehavior::CameraCapture => {
            vec![
                MobileBridgeCapability::Camera,
                MobileBridgeCapability::Permissions,
            ]
        }
        IosPlatformBehavior::FilePicker => {
            vec![
                MobileBridgeCapability::Files,
                MobileBridgeCapability::Permissions,
            ]
        }
        IosPlatformBehavior::Geolocation => {
            vec![
                MobileBridgeCapability::Geolocation,
                MobileBridgeCapability::Permissions,
            ]
        }
        IosPlatformBehavior::PushNotifications => {
            vec![
                MobileBridgeCapability::PushNotifications,
                MobileBridgeCapability::Permissions,
            ]
        }
    }
}

/// Validates declared capabilities for one iOS platform behavior.
pub(crate) fn validate_ios_platform_behavior_capabilities(
    behavior: IosPlatformBehavior,
    declared: &[MobileBridgeCapability],
) -> Result<(), Vec<IosShellDiagnostic>> {
    let missing = ios_platform_behavior_required_capabilities(behavior)
        .into_iter()
        .filter(|required| !declared.contains(required))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing
            .into_iter()
            .map(|capability| {
                diagnostic(
                    "ios_shell_missing_platform_capability",
                    format!(
                        "iOS behavior `{}` requires capability `{}`",
                        behavior.as_str(),
                        capability.as_str()
                    ),
                )
            })
            .collect())
    }
}

/// Builds one generated iOS shell file.
fn shell_file(path: &str, kind: IosShellFileKind, contents: String) -> IosShellFile {
    IosShellFile {
        path: path.to_string(),
        kind,
        contents,
    }
}

/// Generates the Swift package manifest.
fn package_swift(config: &IosShellConfig) -> String {
    format!(
        "// swift-tools-version: 5.10\n\
         import PackageDescription\n\n\
         let package = Package(\n\
         name: \"{}\",\n\
         platforms: [.iOS(.v16)],\n\
         products: [.library(name: \"{}MobileApp\", targets: [\"TerlanMobileApp\"])],\n\
         targets: [\n\
         .target(name: \"TerlanMobileCore\"),\n\
         .target(name: \"TerlanMobileNavigation\", dependencies: [\"TerlanMobileCore\"]),\n\
         .target(name: \"TerlanMobileDemo\", dependencies: [\"TerlanMobileCore\", \"TerlanMobileNavigation\"]),\n\
         .target(name: \"TerlanMobileApp\", dependencies: [\"TerlanMobileCore\", \"TerlanMobileNavigation\", \"TerlanMobileDemo\"], resources: [.process(\"Resources\")])\n\
         ]\n\
         )\n",
        config.app_name, config.swift_module_prefix
    )
}

/// Generates the core bridge Swift source.
fn core_bridge_swift(config: &IosShellConfig) -> String {
    format!(
        "import Foundation\n\n\
         public struct {}MobileBridgeMessage: Equatable {{\n\
         public let id: String\n\
         public let target: String\n\
         public let method: String\n\
         }}\n",
        config.swift_module_prefix
    )
}

/// Generates the navigation Swift source.
fn navigation_router_swift(config: &IosShellConfig) -> String {
    format!(
        "import Foundation\n\
         import TerlanMobileCore\n\n\
         public struct {}MobileRoute: Equatable {{\n\
         public let path: String\n\
         public let handler: String\n\
         public let nativeComponent: String?\n\
         }}\n",
        config.swift_module_prefix
    )
}

/// Generates the demo Swift source.
fn demo_swift(config: &IosShellConfig) -> String {
    format!(
        "import Foundation\n\
         import TerlanMobileNavigation\n\n\
         public enum {}MobileDemo {{\n\
         public static let appName = \"{}\"\n\
         }}\n",
        config.swift_module_prefix, config.app_name
    )
}

/// Generates the app Swift source.
fn app_swift(config: &IosShellConfig) -> String {
    format!(
        "import SwiftUI\n\
         import TerlanMobileDemo\n\n\
         public struct {}MobileAppRoot: View {{\n\
         public init() {{}}\n\
         public var body: some View {{ Text({}MobileDemo.appName) }}\n\
         }}\n",
        config.swift_module_prefix, config.swift_module_prefix
    )
}

/// Resolves the iOS native presentation mode for a route.
fn ios_route_presentation_mode(presentation: &str, hints: &[&'static str]) -> Option<&'static str> {
    match presentation {
        "modal" => Some("modal"),
        "bottom_sheet" => Some("bottom_sheet"),
        _ if hints.contains(&"modal") => Some("modal"),
        _ if hints.contains(&"bottom_sheet") => Some("bottom_sheet"),
        _ => None,
    }
}

/// Returns whether one dotted identifier is valid enough for shell metadata.
fn is_valid_dotted_identifier(value: &str) -> bool {
    let mut segments = value.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    !is_blank(first)
        && value.contains('.')
        && std::iter::once(first)
            .chain(segments)
            .all(is_valid_swift_identifier)
}

/// Returns whether one Swift identifier segment is valid enough for metadata.
fn is_valid_swift_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Returns whether a string is blank.
fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

/// Builds one iOS shell diagnostic.
fn diagnostic(code: &'static str, message: impl Into<String>) -> IosShellDiagnostic {
    IosShellDiagnostic {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "mobile_ios_shell_test.rs"]
mod mobile_ios_shell_test;
