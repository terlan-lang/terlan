use super::super::mobile_angular_bridge::{
    MobileAngularPlatform, MobileAngularPlatformEnvironment, MobileAngularSafeArea,
    MobileAngularTheme,
};
use super::super::mobile_route::{
    generate_mobile_route_configuration, MobileRouteConfigEntry, MobileRouteConfiguration,
    MobileRouteDeclaration, MobileRoutePresentation, MobileRoutePresentationHint,
    MobileRouteSource,
};
use super::super::mobile_widget::{
    generate_mobile_widget_metadata, standard_mobile_widget_declarations, MobileWidgetMetadata,
    MobileWidgetMetadataEntry,
};
use super::*;

/// Builds one representative Android shell config.
fn android_config() -> AndroidShellConfig {
    AndroidShellConfig {
        app_name: "TerlanDemo".to_string(),
        application_id: "io.terlan.demo".to_string(),
        kotlin_package: "io.terlan.demo".to_string(),
    }
}

/// Builds one Android platform environment.
fn android_environment() -> MobileAngularPlatformEnvironment {
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

/// Verifies the minimal Android shell layout is generated.
///
/// Inputs:
/// - One valid Android shell config.
///
/// Output:
/// - Schema-versioned layout with core, navigation, demo, and app modules.
///
/// Transformation:
/// - Templates a deterministic Android project structure without invoking
///   Android tooling or writing files.
#[test]
fn android_shell_generates_minimal_module_layout() {
    let layout = generate_android_shell_layout(&android_config()).expect("android layout");
    let paths = layout
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(layout.schema_version, 1);
    assert_eq!(layout.modules, vec!["core", "navigation", "demo", "app"]);
    assert!(paths.contains(&"settings.gradle.kts"));
    assert!(paths.contains(&"build.gradle.kts"));
    assert!(paths.contains(&"core/build.gradle.kts"));
    assert!(paths.contains(&"navigation/build.gradle.kts"));
    assert!(paths.contains(&"demo/build.gradle.kts"));
    assert!(paths.contains(&"app/build.gradle.kts"));
    assert!(paths.contains(&"app/src/main/AndroidManifest.xml"));
    assert!(paths.contains(&"app/src/main/java/io/terlan/demo/MainActivity.kt"));
    assert!(paths.contains(&"app/src/main/res/values/strings.xml"));
    assert!(paths.contains(&"app/src/main/assets/.keep"));
}

/// Verifies generated Android shell file contents contain stable anchors.
///
/// Inputs:
/// - One valid Android shell config.
///
/// Output:
/// - Generated settings, app build, manifest, and activity contents with stable
///   module/package/application-id anchors.
///
/// Transformation:
/// - Pins the first Android shell template surface for later shell generation.
#[test]
fn android_shell_generates_stable_file_contents() {
    let layout = generate_android_shell_layout(&android_config()).expect("android layout");
    let settings = file_contents(&layout, "settings.gradle.kts");
    let app_build = file_contents(&layout, "app/build.gradle.kts");
    let manifest = file_contents(&layout, "app/src/main/AndroidManifest.xml");
    let activity = file_contents(&layout, "app/src/main/java/io/terlan/demo/MainActivity.kt");

    assert!(settings.contains("rootProject.name = \"TerlanDemo\""));
    assert!(settings.contains("include(\":core\", \":navigation\", \":demo\", \":app\")"));
    assert!(app_build.contains("applicationId = \"io.terlan.demo\""));
    assert!(app_build.contains("implementation(project(\":navigation\"))"));
    assert!(manifest.contains("android:name=\"io.terlan.demo.MainActivity\""));
    assert!(activity.contains("package io.terlan.demo"));
    assert!(activity.contains("class MainActivity : Activity()"));
}

/// Verifies Android shell route path config is generated as a JSON asset.
///
/// Inputs:
/// - One validated mobile route configuration.
///
/// Output:
/// - `app/src/main/assets/terlan-paths.json` with route, presentation, and
///   presentation hint metadata.
///
/// Transformation:
/// - Converts compiler-owned mobile route configuration into the Android shell
///   path configuration asset without writing files.
#[test]
fn android_shell_generates_path_config_file() {
    let route_config = generate_mobile_route_configuration(&[MobileRouteDeclaration {
        route: "/settings".to_string(),
        source: MobileRouteSource::Terlan,
        handler: "App.Http.settings".to_string(),
        presentation: MobileRoutePresentation::Web,
        presentation_hints: vec![
            MobileRoutePresentationHint::NativeFragmentUpgrade,
            MobileRoutePresentationHint::Replace,
        ],
        native_component: Some("SettingsScreen".to_string()),
        source_identity: None,
    }])
    .expect("route config");
    let file = generate_android_shell_path_config_file(&route_config);
    let json = serde_json::from_str::<serde_json::Value>(&file.contents).expect("path config JSON");

    assert_eq!(file.path, "app/src/main/assets/terlan-paths.json");
    assert_eq!(file.kind, AndroidShellFileKind::PathConfig);
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["routes"][0]["route"], "/settings");
    assert_eq!(json["routes"][0]["handler"], "App.Http.settings");
    assert_eq!(json["routes"][0]["presentation"], "web");
    assert_eq!(
        json["routes"][0]["presentation_hints"][0],
        "native_fragment_upgrade"
    );
    assert_eq!(json["routes"][0]["native_component"], "SettingsScreen");
}

/// Verifies Android native fragment route upgrades are generated.
///
/// Inputs:
/// - One explicit native-fragment route and one web route upgraded by a
///   native-fragment presentation hint.
///
/// Output:
/// - Schema-versioned Android native fragment upgrade plan.
///
/// Transformation:
/// - Converts route presentation metadata into Android shell native fragment
///   routing inputs.
#[test]
fn android_shell_generates_native_fragment_route_upgrades() {
    let route_config = generate_mobile_route_configuration(&[
        MobileRouteDeclaration {
            route: "/profile".to_string(),
            source: MobileRouteSource::Terlan,
            handler: "App.Http.profile".to_string(),
            presentation: MobileRoutePresentation::NativeFragment,
            presentation_hints: vec![],
            native_component: Some("ProfileFragment".to_string()),
            source_identity: None,
        },
        MobileRouteDeclaration {
            route: "/settings".to_string(),
            source: MobileRouteSource::Terlan,
            handler: "App.Http.settings".to_string(),
            presentation: MobileRoutePresentation::Web,
            presentation_hints: vec![MobileRoutePresentationHint::NativeFragmentUpgrade],
            native_component: Some("SettingsFragment".to_string()),
            source_identity: None,
        },
    ])
    .expect("route config");
    let plan =
        generate_android_native_fragment_route_upgrade_plan(&route_config).expect("upgrade plan");

    assert_eq!(plan.schema_version, 1);
    assert_eq!(plan.routes.len(), 2);
    assert_eq!(plan.routes[0].route, "/profile");
    assert_eq!(plan.routes[0].native_fragment, "ProfileFragment");
    assert_eq!(plan.routes[0].presentation, "native_fragment");
    assert_eq!(plan.routes[1].route, "/settings");
    assert_eq!(plan.routes[1].native_fragment, "SettingsFragment");
    assert_eq!(
        plan.routes[1].presentation_hints,
        vec!["native_fragment_upgrade"]
    );
}

/// Verifies stale native fragment route metadata is rejected.
///
/// Inputs:
/// - Route config with native-fragment presentation but no native component.
///
/// Output:
/// - Stable missing-native-fragment diagnostic.
///
/// Transformation:
/// - Keeps Android shell native fragment generation from consuming stale route
///   metadata.
#[test]
fn android_shell_rejects_native_fragment_upgrade_without_component() {
    let diagnostics =
        generate_android_native_fragment_route_upgrade_plan(&MobileRouteConfiguration {
            schema_version: 1,
            routes: vec![MobileRouteConfigEntry {
                route: "/profile".to_string(),
                source: "terlan",
                handler: "App.Http.profile".to_string(),
                presentation: "native_fragment",
                presentation_hints: vec![],
                native_component: None,
                source_identity: None,
            }],
        })
        .expect_err("missing native fragment");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "android_shell_missing_native_fragment"));
}

/// Verifies Android modal and bottom-sheet route presentations are generated.
///
/// Inputs:
/// - Routes with explicit modal presentation, explicit bottom-sheet
///   presentation, and hint-driven modal presentation.
///
/// Output:
/// - Schema-versioned Android presentation plan.
///
/// Transformation:
/// - Converts route presentation metadata into Android shell modal and
///   bottom-sheet routing inputs.
#[test]
fn android_shell_generates_modal_and_bottom_sheet_presentations() {
    let route_config = generate_mobile_route_configuration(&[
        MobileRouteDeclaration {
            route: "/login".to_string(),
            source: MobileRouteSource::Terlan,
            handler: "App.Http.login".to_string(),
            presentation: MobileRoutePresentation::Modal,
            presentation_hints: vec![],
            native_component: Some("LoginDialog".to_string()),
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
        MobileRouteDeclaration {
            route: "/help".to_string(),
            source: MobileRouteSource::Terlan,
            handler: "App.Http.help".to_string(),
            presentation: MobileRoutePresentation::Web,
            presentation_hints: vec![MobileRoutePresentationHint::Modal],
            native_component: Some("HelpDialog".to_string()),
            source_identity: None,
        },
    ])
    .expect("route config");
    let plan = generate_android_route_presentation_plan(&route_config).expect("presentation plan");

    assert_eq!(plan.schema_version, 1);
    assert_eq!(plan.routes.len(), 3);
    assert_eq!(plan.routes[0].route, "/login");
    assert_eq!(plan.routes[0].presentation_mode, "modal");
    assert_eq!(plan.routes[0].native_component, "LoginDialog");
    assert_eq!(plan.routes[1].route, "/actions");
    assert_eq!(plan.routes[1].presentation_mode, "bottom_sheet");
    assert_eq!(plan.routes[1].native_component, "ActionsSheet");
    assert_eq!(plan.routes[2].route, "/help");
    assert_eq!(plan.routes[2].presentation_mode, "modal");
    assert_eq!(plan.routes[2].presentation_hints, vec!["modal"]);
}

/// Verifies stale modal/bottom-sheet route metadata is rejected.
///
/// Inputs:
/// - Route config with modal presentation but no native component.
///
/// Output:
/// - Stable missing-route-presentation-component diagnostic.
///
/// Transformation:
/// - Keeps Android shell modal and bottom-sheet generation from consuming stale
///   route metadata.
#[test]
fn android_shell_rejects_route_presentation_without_component() {
    let diagnostics = generate_android_route_presentation_plan(&MobileRouteConfiguration {
        schema_version: 1,
        routes: vec![MobileRouteConfigEntry {
            route: "/login".to_string(),
            source: "terlan",
            handler: "App.Http.login".to_string(),
            presentation: "modal",
            presentation_hints: vec![],
            native_component: None,
            source_identity: None,
        }],
    })
    .expect_err("missing presentation component");

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "android_shell_missing_route_presentation_component"
    }));
}

/// Verifies Android bridge component plan includes the standard widget set.
///
/// Inputs:
/// - Standard mobile widget metadata.
///
/// Output:
/// - Android bridge component plan with toolbar, bottom sheet, drawer, card,
///   image, file picker, camera, and geolocation components.
///
/// Transformation:
/// - Converts compiler-owned widget metadata into Android shell native bridge
///   component inputs.
#[test]
fn android_shell_generates_standard_bridge_component_plan() {
    let widget_metadata = generate_mobile_widget_metadata(&standard_mobile_widget_declarations())
        .expect("widget metadata");
    let plan = generate_android_bridge_component_plan(&widget_metadata).expect("component plan");
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
    assert_eq!(plan.components[0].props[0].name, "id");
    assert_eq!(plan.components[0].props[0].field_type, "String");
    assert!(plan.components[0].props[0].required);
    assert_eq!(plan.components[0].events[0].name, "press");
    assert_eq!(plan.components[0].events[0].payload[0].name, "id");
}

/// Verifies stale bridge component metadata is rejected.
///
/// Inputs:
/// - Widget metadata that requires native upgrade but has no native component.
///
/// Output:
/// - Stable missing-bridge-component diagnostic.
///
/// Transformation:
/// - Keeps Android shell component planning from consuming stale widget
///   metadata.
#[test]
fn android_shell_rejects_bridge_component_without_native_component() {
    let mut metadata = generate_mobile_widget_metadata(&standard_mobile_widget_declarations())
        .expect("widget metadata");
    metadata.widgets[0].native_component = None;

    let diagnostics = generate_android_bridge_component_plan(&MobileWidgetMetadata {
        schema_version: metadata.schema_version,
        widgets: vec![MobileWidgetMetadataEntry {
            name: metadata.widgets[0].name.clone(),
            selector: metadata.widgets[0].selector.clone(),
            native_component: None,
            upgrade: metadata.widgets[0].upgrade,
            capabilities: metadata.widgets[0].capabilities.clone(),
            props: metadata.widgets[0].props.clone(),
            events: metadata.widgets[0].events.clone(),
        }],
    })
    .expect_err("missing bridge component");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "android_shell_missing_bridge_component"));
}

/// Verifies Android WebView environment injection is generated.
///
/// Inputs:
/// - Android platform, theme, locale, density, and safe-area environment data.
///
/// Output:
/// - Schema-versioned injection metadata with CSS variables and JavaScript
///   source for the WebView.
///
/// Transformation:
/// - Converts native shell environment data into deterministic WebView
///   injection source.
#[test]
fn android_shell_generates_webview_environment_injection() {
    let injection = generate_android_webview_environment_injection(&android_environment());

    assert_eq!(injection.schema_version, 1);
    assert_eq!(injection.platform, "android");
    assert_eq!(injection.theme, "dark");
    assert_eq!(injection.css_variables[0].name, "--terlan-platform");
    assert_eq!(injection.css_variables[0].value, "android");
    assert_eq!(injection.css_variables[4].name, "--terlan-safe-area-top");
    assert_eq!(injection.css_variables[4].value, "12px");
    assert!(injection
        .javascript
        .contains("window.TerlanNativeEnvironment = environment"));
    assert!(injection.javascript.contains("globalThis.document"));
    assert!(injection.javascript.contains("root.style.setProperty"));
    assert!(injection.javascript.contains("\"platform\":\"android\""));
    assert!(injection.javascript.contains("\"theme\":\"dark\""));
}

/// Verifies Android bridge protocol rejects invalid JSON.
///
/// Inputs:
/// - One malformed JSON message.
///
/// Output:
/// - Stable invalid-json diagnostic.
///
/// Transformation:
/// - Keeps native bridge protocol parsing failures deterministic.
#[test]
fn android_bridge_protocol_rejects_invalid_json() {
    let diagnostic =
        validate_android_bridge_protocol_message(&mut AndroidBridgeProtocolState::default(), "{")
            .expect_err("invalid JSON");

    assert_eq!(diagnostic.code, "android_bridge_invalid_json");
}

/// Verifies Android bridge protocol rejects unknown target and method values.
///
/// Inputs:
/// - One unknown target message and one unknown component method message.
///
/// Output:
/// - Stable unknown-target and unknown-method diagnostics.
///
/// Transformation:
/// - Keeps bridge dispatch closed over supported target/method pairs.
#[test]
fn android_bridge_protocol_rejects_unknown_target_and_method() {
    let unknown_target = validate_android_bridge_protocol_message(
        &mut AndroidBridgeProtocolState::default(),
        r#"{"id":"1","target":"bad","method":"mount"}"#,
    )
    .expect_err("unknown target");
    let unknown_method = validate_android_bridge_protocol_message(
        &mut AndroidBridgeProtocolState::default(),
        r#"{"id":"1","target":"component","method":"bad"}"#,
    )
    .expect_err("unknown method");

    assert_eq!(unknown_target.code, "android_bridge_unknown_target");
    assert_eq!(unknown_method.code, "android_bridge_unknown_method");
}

/// Verifies Android bridge protocol rejects missing ids.
///
/// Inputs:
/// - One message without a request id.
///
/// Output:
/// - Stable missing-id diagnostic.
///
/// Transformation:
/// - Ensures bridge messages are always correlatable.
#[test]
fn android_bridge_protocol_rejects_missing_ids() {
    let diagnostic = validate_android_bridge_protocol_message(
        &mut AndroidBridgeProtocolState::default(),
        r#"{"target":"component","method":"mount","component_id":"toolbar"}"#,
    )
    .expect_err("missing id");

    assert_eq!(diagnostic.code, "android_bridge_missing_id");
}

/// Verifies Android bridge protocol tracks mounted component identity.
///
/// Inputs:
/// - Mount, duplicate mount, stale update, and unmount messages.
///
/// Output:
/// - Accepted mount/unmount deliveries plus duplicate/stale diagnostics.
///
/// Transformation:
/// - Keeps native component lifecycle state deterministic for bridge tests.
#[test]
fn android_bridge_protocol_tracks_mounted_components() {
    let mut state = AndroidBridgeProtocolState::default();
    let mounted = validate_android_bridge_protocol_message(
        &mut state,
        r#"{"id":"1","target":"component","method":"mount","component_id":"toolbar"}"#,
    )
    .expect("mount");
    let duplicate = validate_android_bridge_protocol_message(
        &mut state,
        r#"{"id":"2","target":"component","method":"mount","component_id":"toolbar"}"#,
    )
    .expect_err("duplicate component");
    let stale = validate_android_bridge_protocol_message(
        &mut state,
        r#"{"id":"3","target":"component","method":"update","component_id":"missing"}"#,
    )
    .expect_err("stale component");
    let unmounted = validate_android_bridge_protocol_message(
        &mut state,
        r#"{"id":"4","target":"component","method":"unmount","component_id":"toolbar"}"#,
    )
    .expect("unmount");

    assert_eq!(mounted.component_id.as_deref(), Some("toolbar"));
    assert_eq!(duplicate.code, "android_bridge_duplicate_component_id");
    assert_eq!(stale.code, "android_bridge_stale_mounted_component");
    assert_eq!(unmounted.method, "unmount");
}

/// Verifies Android bridge protocol treats native shell restart as a state reset.
///
/// Inputs:
/// - Mounted component followed by native shell restart and a stale update.
///
/// Output:
/// - Generation is advanced and the stale update is rejected.
///
/// Transformation:
/// - Prevents post-restart native messages from reusing component state owned
///   by the previous shell lifetime.
#[test]
fn android_bridge_protocol_restart_clears_mounted_components() {
    let mut state = AndroidBridgeProtocolState::default();

    validate_android_bridge_protocol_message(
        &mut state,
        r#"{"id":"1","target":"component","method":"mount","component_id":"toolbar"}"#,
    )
    .expect("mount");
    state.record_native_shell_restart();
    let stale = validate_android_bridge_protocol_message(
        &mut state,
        r#"{"id":"2","target":"component","method":"update","component_id":"toolbar"}"#,
    )
    .expect_err("stale component after restart");

    assert_eq!(state.shell_generation(), 1);
    assert_eq!(stale.code, "android_bridge_stale_mounted_component");
}

/// Verifies Android bridge protocol accepts native reply delivery.
///
/// Inputs:
/// - One native reply delivery message.
///
/// Output:
/// - Accepted delivery preserving id, target, method, and reply correlation id.
///
/// Transformation:
/// - Pins the bridge surface used to deliver native replies back into Terlan
///   process messages.
#[test]
fn android_bridge_protocol_accepts_native_reply_delivery() {
    let delivery = validate_android_bridge_protocol_message(
        &mut AndroidBridgeProtocolState::default(),
        r#"{"id":"reply-1","target":"native_reply","method":"deliver","reply_to":"request-1"}"#,
    )
    .expect("native reply delivery");

    assert_eq!(delivery.id, "reply-1");
    assert_eq!(delivery.target, "native_reply");
    assert_eq!(delivery.method, "deliver");
    assert_eq!(delivery.reply_to.as_deref(), Some("request-1"));
}

/// Verifies invalid Android shell config is rejected.
///
/// Inputs:
/// - Empty app name plus invalid application id and Kotlin package.
///
/// Output:
/// - Stable diagnostics for each invalid field.
///
/// Transformation:
/// - Keeps generated Android shell layouts addressable by valid package names.
#[test]
fn android_shell_rejects_invalid_config() {
    let diagnostics = validate_android_shell_config(&AndroidShellConfig {
        app_name: String::new(),
        application_id: "bad id".to_string(),
        kotlin_package: "bad-package".to_string(),
    })
    .expect_err("invalid config");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"android_shell_empty_app_name"));
    assert!(codes.contains(&"android_shell_invalid_application_id"));
    assert!(codes.contains(&"android_shell_invalid_kotlin_package"));
}

/// Verifies Android shell file kind names are stable.
///
/// Inputs:
/// - Every first-slice Android shell file kind.
///
/// Output:
/// - Stable file kind spellings.
///
/// Transformation:
/// - Keeps generated shell layout metadata stable for build planning.
#[test]
fn android_shell_file_kind_names_are_stable() {
    assert_eq!(
        AndroidShellFileKind::GradleSettings.as_str(),
        "gradle_settings"
    );
    assert_eq!(AndroidShellFileKind::GradleBuild.as_str(), "gradle_build");
    assert_eq!(AndroidShellFileKind::Manifest.as_str(), "manifest");
    assert_eq!(AndroidShellFileKind::KotlinSource.as_str(), "kotlin_source");
    assert_eq!(AndroidShellFileKind::ResourceXml.as_str(), "resource_xml");
    assert_eq!(AndroidShellFileKind::PathConfig.as_str(), "path_config");
    assert_eq!(
        AndroidShellFileKind::AssetPlaceholder.as_str(),
        "asset_placeholder"
    );
}

/// Reads one generated file's contents.
fn file_contents<'a>(layout: &'a AndroidShellLayout, path: &str) -> &'a str {
    layout
        .files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.contents.as_str())
        .expect("generated file")
}
