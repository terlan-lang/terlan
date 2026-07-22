//! Android shell project layout generation for the mobile profile.
#![allow(dead_code)]
//!
//! Inputs:
//! - Typed Android shell configuration.
//!
//! Outputs:
//! - A deterministic list of generated project files for the Android shell.
//!
//! Transformation:
//! - Templates the first Android shell structure without invoking Android
//!   tooling or writing files directly.

use serde_json::json;
use serde_json::Value;
use std::collections::BTreeSet;

use super::mobile_angular_bridge::{
    mobile_angular_platform_environment_css_variables, MobileAngularPlatformEnvironment,
};
use super::mobile_route::MobileRouteConfiguration;
use super::mobile_widget::MobileWidgetMetadata;

/// Android shell generation input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidShellConfig {
    pub(crate) app_name: String,
    pub(crate) application_id: String,
    pub(crate) kotlin_package: String,
}

/// One generated Android shell file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidShellFile {
    pub(crate) path: String,
    pub(crate) kind: AndroidShellFileKind,
    pub(crate) contents: String,
}

/// Generated Android shell file kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AndroidShellFileKind {
    GradleSettings,
    GradleBuild,
    Manifest,
    KotlinSource,
    ResourceXml,
    PathConfig,
    AssetPlaceholder,
}

impl AndroidShellFileKind {
    /// Returns the stable file kind spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::GradleSettings => "gradle_settings",
            Self::GradleBuild => "gradle_build",
            Self::Manifest => "manifest",
            Self::KotlinSource => "kotlin_source",
            Self::ResourceXml => "resource_xml",
            Self::PathConfig => "path_config",
            Self::AssetPlaceholder => "asset_placeholder",
        }
    }
}

/// Generated Android shell layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidShellLayout {
    pub(crate) schema_version: u32,
    pub(crate) modules: Vec<&'static str>,
    pub(crate) files: Vec<AndroidShellFile>,
}

/// Android shell layout diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidShellDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Android native fragment route upgrade plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidNativeFragmentRouteUpgradePlan {
    pub(crate) schema_version: u32,
    pub(crate) routes: Vec<AndroidNativeFragmentRouteUpgrade>,
}

/// One Android native fragment route upgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidNativeFragmentRouteUpgrade {
    pub(crate) route: String,
    pub(crate) handler: String,
    pub(crate) native_fragment: String,
    pub(crate) presentation: String,
    pub(crate) presentation_hints: Vec<String>,
}

/// Android modal/bottom-sheet route presentation plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidRoutePresentationPlan {
    pub(crate) schema_version: u32,
    pub(crate) routes: Vec<AndroidRoutePresentation>,
}

/// One Android native route presentation entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidRoutePresentation {
    pub(crate) route: String,
    pub(crate) handler: String,
    pub(crate) native_component: String,
    pub(crate) presentation_mode: String,
    pub(crate) presentation_hints: Vec<String>,
}

/// Android native bridge component plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidBridgeComponentPlan {
    pub(crate) schema_version: u32,
    pub(crate) components: Vec<AndroidBridgeComponent>,
}

/// One Android native bridge component entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidBridgeComponent {
    pub(crate) widget: String,
    pub(crate) selector: String,
    pub(crate) native_component: String,
    pub(crate) upgrade: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) props: Vec<AndroidBridgeComponentField>,
    pub(crate) events: Vec<AndroidBridgeComponentEvent>,
}

/// One Android bridge component prop field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidBridgeComponentField {
    pub(crate) name: String,
    pub(crate) field_type: String,
    pub(crate) required: bool,
}

/// One Android bridge component event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidBridgeComponentEvent {
    pub(crate) name: String,
    pub(crate) payload: Vec<AndroidBridgeComponentField>,
}

/// Android WebView platform/theme environment injection plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidWebViewEnvironmentInjection {
    pub(crate) schema_version: u32,
    pub(crate) platform: String,
    pub(crate) theme: String,
    pub(crate) css_variables: Vec<AndroidWebViewCssVariable>,
    pub(crate) javascript: String,
}

/// One Android WebView CSS variable produced from native environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidWebViewCssVariable {
    pub(crate) name: String,
    pub(crate) value: String,
}

/// Android bridge protocol validation state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AndroidBridgeProtocolState {
    mounted_components: BTreeSet<String>,
    shell_generation: u64,
}

impl AndroidBridgeProtocolState {
    /// Returns the current native shell generation.
    ///
    /// Inputs:
    /// - Bridge protocol state.
    ///
    /// Output:
    /// - Monotonic generation value incremented whenever the native shell
    ///   restarts.
    ///
    /// Transformation:
    /// - Reads restart tracking state without mutating mounted components.
    pub(crate) const fn shell_generation(&self) -> u64 {
        self.shell_generation
    }

    /// Records a native shell restart.
    ///
    /// Inputs:
    /// - Mutable bridge protocol state.
    ///
    /// Output:
    /// - Updated state with mounted components cleared and generation advanced.
    ///
    /// Transformation:
    /// - Treats all pre-restart native component ids as stale so later update
    ///   or unmount messages must remount through the current shell generation.
    pub(crate) fn record_native_shell_restart(&mut self) {
        self.mounted_components.clear();
        self.shell_generation = self.shell_generation.saturating_add(1);
    }
}

/// Android bridge protocol delivery accepted by validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidBridgeProtocolDelivery {
    pub(crate) id: String,
    pub(crate) target: String,
    pub(crate) method: String,
    pub(crate) component_id: Option<String>,
    pub(crate) reply_to: Option<String>,
}

/// Generates the minimal Android shell project layout.
pub(crate) fn generate_android_shell_layout(
    config: &AndroidShellConfig,
) -> Result<AndroidShellLayout, Vec<AndroidShellDiagnostic>> {
    validate_android_shell_config(config)?;
    let package_path = config.kotlin_package.replace('.', "/");
    Ok(AndroidShellLayout {
        schema_version: 1,
        modules: vec!["core", "navigation", "demo", "app"],
        files: vec![
            shell_file(
                "settings.gradle.kts",
                AndroidShellFileKind::GradleSettings,
                settings_gradle(config),
            ),
            shell_file(
                "build.gradle.kts",
                AndroidShellFileKind::GradleBuild,
                root_build_gradle(),
            ),
            shell_file(
                "core/build.gradle.kts",
                AndroidShellFileKind::GradleBuild,
                android_library_build_gradle("core"),
            ),
            shell_file(
                "navigation/build.gradle.kts",
                AndroidShellFileKind::GradleBuild,
                android_library_build_gradle("navigation"),
            ),
            shell_file(
                "demo/build.gradle.kts",
                AndroidShellFileKind::GradleBuild,
                android_library_build_gradle("demo"),
            ),
            shell_file(
                "app/build.gradle.kts",
                AndroidShellFileKind::GradleBuild,
                app_build_gradle(config),
            ),
            shell_file(
                "app/src/main/AndroidManifest.xml",
                AndroidShellFileKind::Manifest,
                android_manifest(config),
            ),
            shell_file(
                &format!("app/src/main/java/{package_path}/MainActivity.kt"),
                AndroidShellFileKind::KotlinSource,
                main_activity(config),
            ),
            shell_file(
                "app/src/main/res/values/strings.xml",
                AndroidShellFileKind::ResourceXml,
                strings_xml(config),
            ),
            shell_file(
                "app/src/main/assets/.keep",
                AndroidShellFileKind::AssetPlaceholder,
                String::new(),
            ),
        ],
    })
}

/// Validates Android shell generation input.
pub(crate) fn validate_android_shell_config(
    config: &AndroidShellConfig,
) -> Result<(), Vec<AndroidShellDiagnostic>> {
    let mut diagnostics = Vec::new();
    if is_blank(&config.app_name) {
        diagnostics.push(diagnostic(
            "android_shell_empty_app_name",
            "Android shell app name must not be empty",
        ));
    }
    if !is_valid_dotted_identifier(&config.application_id) {
        diagnostics.push(diagnostic(
            "android_shell_invalid_application_id",
            format!(
                "Android shell application id `{}` must be a dotted identifier",
                config.application_id
            ),
        ));
    }
    if !is_valid_dotted_identifier(&config.kotlin_package) {
        diagnostics.push(diagnostic(
            "android_shell_invalid_kotlin_package",
            format!(
                "Android shell Kotlin package `{}` must be a dotted identifier",
                config.kotlin_package
            ),
        ));
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Generates the Android shell route path configuration asset.
pub(crate) fn generate_android_shell_path_config_file(
    routes: &MobileRouteConfiguration,
) -> AndroidShellFile {
    let route_entries = routes
        .routes
        .iter()
        .map(|route| {
            json!({
                "route": &route.route,
                "source": route.source,
                "handler": &route.handler,
                "presentation": route.presentation,
                "presentation_hints": &route.presentation_hints,
                "native_component": &route.native_component,
                "source_identity": route.source_identity.as_ref().map(|identity| {
                    json!({
                        "debug_key": &identity.debug_key,
                        "module_path": &identity.module_path,
                        "function_name": &identity.function_name,
                        "file": &identity.file,
                        "start_line": identity.start_line,
                        "start_column": identity.start_column,
                        "end_line": identity.end_line,
                        "end_column": identity.end_column,
                    })
                }),
            })
        })
        .collect::<Vec<_>>();
    let contents = serde_json::to_string_pretty(&json!({
        "schema_version": routes.schema_version,
        "routes": route_entries,
    }))
    .expect("serialize Android route path config");
    shell_file(
        "app/src/main/assets/terlan-paths.json",
        AndroidShellFileKind::PathConfig,
        format!("{contents}\n"),
    )
}

/// Generates Android native fragment route upgrades from route config.
pub(crate) fn generate_android_native_fragment_route_upgrade_plan(
    routes: &MobileRouteConfiguration,
) -> Result<AndroidNativeFragmentRouteUpgradePlan, Vec<AndroidShellDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut upgrades = Vec::new();

    for route in &routes.routes {
        if route.presentation == "native_fragment"
            || route
                .presentation_hints
                .contains(&"native_fragment_upgrade")
        {
            let Some(native_fragment) = route.native_component.as_ref() else {
                diagnostics.push(diagnostic(
                    "android_shell_missing_native_fragment",
                    format!(
                        "Android route `{}` requires a native fragment component",
                        route.route
                    ),
                ));
                continue;
            };
            upgrades.push(AndroidNativeFragmentRouteUpgrade {
                route: route.route.clone(),
                handler: route.handler.clone(),
                native_fragment: native_fragment.clone(),
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
        Ok(AndroidNativeFragmentRouteUpgradePlan {
            schema_version: 1,
            routes: upgrades,
        })
    } else {
        Err(diagnostics)
    }
}

/// Generates Android modal and bottom-sheet route presentation metadata.
pub(crate) fn generate_android_route_presentation_plan(
    routes: &MobileRouteConfiguration,
) -> Result<AndroidRoutePresentationPlan, Vec<AndroidShellDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut presentations = Vec::new();

    for route in &routes.routes {
        let presentation_mode =
            android_route_presentation_mode(route.presentation, &route.presentation_hints);
        let Some(presentation_mode) = presentation_mode else {
            continue;
        };
        let Some(native_component) = route.native_component.as_ref() else {
            diagnostics.push(diagnostic(
                "android_shell_missing_route_presentation_component",
                format!(
                    "Android route `{}` presentation `{presentation_mode}` requires a native component",
                    route.route
                ),
            ));
            continue;
        };
        presentations.push(AndroidRoutePresentation {
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
        Ok(AndroidRoutePresentationPlan {
            schema_version: 1,
            routes: presentations,
        })
    } else {
        Err(diagnostics)
    }
}

/// Generates Android native bridge component metadata from widget metadata.
pub(crate) fn generate_android_bridge_component_plan(
    widgets: &MobileWidgetMetadata,
) -> Result<AndroidBridgeComponentPlan, Vec<AndroidShellDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut components = Vec::new();

    for widget in &widgets.widgets {
        if widget.upgrade != "web_only" {
            let Some(native_component) = widget.native_component.as_ref() else {
                diagnostics.push(diagnostic(
                    "android_shell_missing_bridge_component",
                    format!(
                        "Android widget `{}` requires a native component",
                        widget.name
                    ),
                ));
                continue;
            };
            components.push(AndroidBridgeComponent {
                widget: widget.name.clone(),
                selector: widget.selector.clone(),
                native_component: native_component.clone(),
                upgrade: widget.upgrade.to_string(),
                capabilities: widget
                    .capabilities
                    .iter()
                    .map(|capability| (*capability).to_string())
                    .collect(),
                props: widget
                    .props
                    .iter()
                    .map(|prop| AndroidBridgeComponentField {
                        name: prop.name.clone(),
                        field_type: prop.prop_type.to_string(),
                        required: prop.required,
                    })
                    .collect(),
                events: widget
                    .events
                    .iter()
                    .map(|event| AndroidBridgeComponentEvent {
                        name: event.name.clone(),
                        payload: event
                            .payload
                            .iter()
                            .map(|field| AndroidBridgeComponentField {
                                name: field.name.clone(),
                                field_type: field.field_type.to_string(),
                                required: true,
                            })
                            .collect(),
                    })
                    .collect(),
            });
        }
    }

    if diagnostics.is_empty() {
        Ok(AndroidBridgeComponentPlan {
            schema_version: widgets.schema_version,
            components,
        })
    } else {
        Err(diagnostics)
    }
}

/// Generates Android WebView platform/theme environment injection metadata.
pub(crate) fn generate_android_webview_environment_injection(
    environment: &MobileAngularPlatformEnvironment,
) -> AndroidWebViewEnvironmentInjection {
    let css_variables = mobile_angular_platform_environment_css_variables(environment)
        .into_iter()
        .map(|variable| AndroidWebViewCssVariable {
            name: variable.name,
            value: variable.value,
        })
        .collect::<Vec<_>>();
    let environment_json = json!({
        "platform": environment.platform.as_str(),
        "theme": environment.theme.as_str(),
        "locale": &environment.locale,
        "density_scale": &environment.density_scale,
        "safe_area_top": environment.safe_area.top_px,
        "safe_area_right": environment.safe_area.right_px,
        "safe_area_bottom": environment.safe_area.bottom_px,
        "safe_area_left": environment.safe_area.left_px,
    });
    let css_json = json!(css_variables
        .iter()
        .map(|variable| json!({ "name": &variable.name, "value": &variable.value }))
        .collect::<Vec<_>>());
    let javascript = format!(
        "(function() {{\n\
         const environment = {};\n\
         const cssVariables = {};\n\
         window.TerlanNativeEnvironment = environment;\n\
         const root = globalThis.document && globalThis.document.documentElement;\n\
         if (root && root.style) {{\n\
         for (const item of cssVariables) {{ root.style.setProperty(item.name, item.value); }}\n\
         }}\n\
         return environment;\n\
         }})();",
        serde_json::to_string(&environment_json).expect("serialize WebView environment"),
        serde_json::to_string(&css_json).expect("serialize WebView CSS variables")
    );
    AndroidWebViewEnvironmentInjection {
        schema_version: 1,
        platform: environment.platform.as_str().to_string(),
        theme: environment.theme.as_str().to_string(),
        css_variables,
        javascript,
    }
}

/// Validates one Android bridge protocol message and updates component state.
pub(crate) fn validate_android_bridge_protocol_message(
    state: &mut AndroidBridgeProtocolState,
    raw: &str,
) -> Result<AndroidBridgeProtocolDelivery, AndroidShellDiagnostic> {
    let value = serde_json::from_str::<Value>(raw).map_err(|error| {
        diagnostic(
            "android_bridge_invalid_json",
            format!("Android bridge protocol message is not valid JSON: {error}"),
        )
    })?;
    let id = json_string_field(&value, "id", "android_bridge_missing_id")?;
    let target = json_string_field(&value, "target", "android_bridge_unknown_target")?;
    let method = json_string_field(&value, "method", "android_bridge_unknown_method")?;

    match (target.as_str(), method.as_str()) {
        ("component", "mount") => {
            let component_id = json_string_field(
                &value,
                "component_id",
                "android_bridge_missing_component_id",
            )?;
            if !state.mounted_components.insert(component_id.clone()) {
                return Err(diagnostic(
                    "android_bridge_duplicate_component_id",
                    format!("Android bridge component `{component_id}` is already mounted"),
                ));
            }
            Ok(protocol_delivery(
                id,
                target,
                method,
                Some(component_id),
                None,
            ))
        }
        ("component", "update" | "unmount") => {
            let component_id = json_string_field(
                &value,
                "component_id",
                "android_bridge_missing_component_id",
            )?;
            if !state.mounted_components.contains(&component_id) {
                return Err(diagnostic(
                    "android_bridge_stale_mounted_component",
                    format!("Android bridge component `{component_id}` is not mounted"),
                ));
            }
            if method == "unmount" {
                state.mounted_components.remove(&component_id);
            }
            Ok(protocol_delivery(
                id,
                target,
                method,
                Some(component_id),
                None,
            ))
        }
        ("native_reply", "deliver") => {
            let reply_to =
                json_string_field(&value, "reply_to", "android_bridge_missing_reply_to")?;
            Ok(protocol_delivery(id, target, method, None, Some(reply_to)))
        }
        ("component", _) | ("native_reply", _) => Err(diagnostic(
            "android_bridge_unknown_method",
            format!("Android bridge target `{target}` has no method `{method}`"),
        )),
        _ => Err(diagnostic(
            "android_bridge_unknown_target",
            format!("Android bridge target `{target}` is not supported"),
        )),
    }
}

/// Builds one generated shell file.
fn shell_file(path: &str, kind: AndroidShellFileKind, contents: String) -> AndroidShellFile {
    AndroidShellFile {
        path: path.to_string(),
        kind,
        contents,
    }
}

/// Generates Gradle settings.
fn settings_gradle(config: &AndroidShellConfig) -> String {
    format!(
        "pluginManagement {{ repositories {{ google(); mavenCentral(); gradlePluginPortal() }} }}\n\
         dependencyResolutionManagement {{ repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS); repositories {{ google(); mavenCentral() }} }}\n\
         rootProject.name = \"{}\"\n\
         include(\":core\", \":navigation\", \":demo\", \":app\")\n",
        config.app_name
    )
}

/// Generates the root Gradle build file.
fn root_build_gradle() -> String {
    "plugins {\n\
     id(\"com.android.application\") version \"8.7.3\" apply false\n\
     id(\"com.android.library\") version \"8.7.3\" apply false\n\
     id(\"org.jetbrains.kotlin.android\") version \"2.0.21\" apply false\n\
     }\n"
    .to_string()
}

/// Generates a reusable Android library module build file.
fn android_library_build_gradle(namespace_suffix: &str) -> String {
    format!(
        "plugins {{ id(\"com.android.library\"); id(\"org.jetbrains.kotlin.android\") }}\n\
         android {{ namespace = \"terlan.mobile.{namespace_suffix}\"; compileSdk = 35 }}\n"
    )
}

/// Generates the app module build file.
fn app_build_gradle(config: &AndroidShellConfig) -> String {
    format!(
        "plugins {{ id(\"com.android.application\"); id(\"org.jetbrains.kotlin.android\") }}\n\
         android {{ namespace = \"{}\"; compileSdk = 35\n\
         defaultConfig {{ applicationId = \"{}\"; minSdk = 26; targetSdk = 35; versionCode = 1; versionName = \"0.0.1\" }} }}\n\
         dependencies {{ implementation(project(\":core\")); implementation(project(\":navigation\")); implementation(project(\":demo\")) }}\n",
        config.kotlin_package, config.application_id
    )
}

/// Generates the Android manifest.
fn android_manifest(config: &AndroidShellConfig) -> String {
    format!(
        "<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\">\n\
         <application android:theme=\"@style/AppTheme\" android:label=\"@string/app_name\">\n\
         <activity android:name=\"{}.MainActivity\" android:exported=\"true\">\n\
         <intent-filter>\n\
         <action android:name=\"android.intent.action.MAIN\" />\n\
         <category android:name=\"android.intent.category.LAUNCHER\" />\n\
         </intent-filter>\n\
         </activity>\n\
         </application>\n\
         </manifest>\n",
        config.kotlin_package
    )
}

/// Generates the minimal Android activity source.
fn main_activity(config: &AndroidShellConfig) -> String {
    format!(
        "package {}\n\n\
         import android.app.Activity\n\
         import android.os.Bundle\n\n\
         class MainActivity : Activity() {{\n\
         override fun onCreate(savedInstanceState: Bundle?) {{\n\
         super.onCreate(savedInstanceState)\n\
         }}\n\
         }}\n",
        config.kotlin_package
    )
}

/// Generates Android string resources.
fn strings_xml(config: &AndroidShellConfig) -> String {
    format!(
        "<resources>\n<string name=\"app_name\">{}</string>\n</resources>\n",
        config.app_name
    )
}

/// Reads one required string field from a JSON bridge message.
fn json_string_field(
    value: &Value,
    field: &str,
    code: &'static str,
) -> Result<String, AndroidShellDiagnostic> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            diagnostic(
                code,
                format!("Android bridge protocol field `{field}` must be a non-empty string"),
            )
        })
}

/// Builds one accepted Android bridge protocol delivery.
fn protocol_delivery(
    id: String,
    target: String,
    method: String,
    component_id: Option<String>,
    reply_to: Option<String>,
) -> AndroidBridgeProtocolDelivery {
    AndroidBridgeProtocolDelivery {
        id,
        target,
        method,
        component_id,
        reply_to,
    }
}

/// Resolves the Android native presentation mode for a route.
fn android_route_presentation_mode(
    presentation: &str,
    hints: &[&'static str],
) -> Option<&'static str> {
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
            .all(is_valid_identifier)
}

/// Returns whether one identifier segment is valid enough for shell metadata.
fn is_valid_identifier(value: &str) -> bool {
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

/// Builds one Android shell diagnostic.
fn diagnostic(code: &'static str, message: impl Into<String>) -> AndroidShellDiagnostic {
    AndroidShellDiagnostic {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "mobile_android_shell_test.rs"]
mod mobile_android_shell_test;
