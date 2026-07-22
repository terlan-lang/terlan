use super::*;
use crate::mobile::mobile_debug_identity::{MobileSourceIdentity, MobileSourceSpan};

/// Builds a representative route source identity.
fn source_identity(function_name: &str) -> MobileSourceIdentity {
    MobileSourceIdentity {
        module_path: "app.Http".to_string(),
        function_name: function_name.to_string(),
        span: MobileSourceSpan {
            file: "src/app/Http.terl".to_string(),
            start_line: 20,
            start_column: 1,
            end_line: 22,
            end_column: 2,
        },
    }
}

/// Builds a web route declaration.
fn web_route(route: &str) -> MobileRouteDeclaration {
    MobileRouteDeclaration {
        route: route.to_string(),
        source: MobileRouteSource::Terlan,
        handler: "App.Http.home".to_string(),
        presentation: MobileRoutePresentation::Web,
        presentation_hints: vec![MobileRoutePresentationHint::Default],
        native_component: None,
        source_identity: Some(source_identity("home")),
    }
}

/// Builds a native route declaration.
fn native_route(route: &str, presentation: MobileRoutePresentation) -> MobileRouteDeclaration {
    MobileRouteDeclaration {
        route: route.to_string(),
        source: MobileRouteSource::AngularTs,
        handler: "App.Mobile.show".to_string(),
        presentation,
        presentation_hints: vec![],
        native_component: Some("UserScreen".to_string()),
        source_identity: Some(source_identity("show")),
    }
}

/// Verifies route declarations generate native presentation configuration.
///
/// Inputs:
/// - One Terlan web route and one AngularTS native-fragment route.
///
/// Output:
/// - Schema-versioned route config with stable source and presentation names.
///
/// Transformation:
/// - Exercises route config generation before platform shell emitters exist.
#[test]
fn mobile_routes_generate_presentation_configuration() {
    let config = generate_mobile_route_configuration(&[
        web_route("/"),
        native_route("/users/:id", MobileRoutePresentation::NativeFragment),
    ])
    .expect("route config");

    assert_eq!(config.schema_version, 1);
    assert_eq!(config.routes.len(), 2);
    assert_eq!(config.routes[0].source, "terlan");
    assert_eq!(config.routes[0].presentation, "web");
    assert_eq!(config.routes[0].presentation_hints, vec!["default"]);
    assert_eq!(config.routes[0].native_component, None);
    assert_eq!(
        config.routes[0]
            .source_identity
            .as_ref()
            .expect("route source identity")
            .debug_key,
        "app.Http.home@src/app/Http.terl:20:1-22:2"
    );
    assert_eq!(config.routes[1].source, "angular_ts");
    assert_eq!(config.routes[1].presentation, "native_fragment");
    assert!(config.routes[1].presentation_hints.is_empty());
    assert_eq!(
        config.routes[1].native_component.as_deref(),
        Some("UserScreen")
    );
    assert_eq!(
        config.routes[1]
            .source_identity
            .as_ref()
            .expect("native route source identity")
            .debug_key,
        "app.Http.show@src/app/Http.terl:20:1-22:2"
    );
}

/// Verifies route presentation hints are emitted with stable names.
///
/// Inputs:
/// - One web route upgraded to a native fragment through presentation hints.
///
/// Output:
/// - Route config containing the native-fragment-upgrade and replace hints.
///
/// Transformation:
/// - Keeps native shell navigation policy explicit without changing the base
///   web presentation.
#[test]
fn mobile_routes_generate_presentation_hints() {
    let mut route = web_route("/settings");
    route.native_component = Some("SettingsScreen".to_string());
    route.presentation_hints = vec![
        MobileRoutePresentationHint::NativeFragmentUpgrade,
        MobileRoutePresentationHint::Replace,
    ];

    let config = generate_mobile_route_configuration(&[route]).expect("route config");

    assert_eq!(
        config.routes[0].presentation_hints,
        vec!["native_fragment_upgrade", "replace"]
    );
    assert_eq!(
        config.routes[0].native_component.as_deref(),
        Some("SettingsScreen")
    );
}

/// Verifies malformed route paths and empty handlers are rejected.
///
/// Inputs:
/// - One route without a leading slash and without a handler.
///
/// Output:
/// - Stable invalid-path and empty-handler diagnostics.
///
/// Transformation:
/// - Keeps generated mobile route configs addressable by real route paths.
#[test]
fn mobile_routes_reject_malformed_paths_and_empty_handlers() {
    let mut route = web_route("users");
    route.handler = String::new();

    let diagnostics = generate_mobile_route_configuration(&[route]).expect_err("invalid route");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_route_invalid_path"));
    assert!(codes.contains(&"mobile_route_empty_handler"));
}

/// Verifies duplicate route/source pairs are rejected.
///
/// Inputs:
/// - Two Terlan routes with the same path.
///
/// Output:
/// - Stable duplicate route diagnostic.
///
/// Transformation:
/// - Prevents ambiguous native presentation configuration for one source.
#[test]
fn mobile_routes_reject_duplicate_source_route_pairs() {
    let diagnostics = generate_mobile_route_configuration(&[web_route("/"), web_route("/")])
        .expect_err("duplicate route");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_route_duplicate"));
}

/// Verifies native presentation modes require a component.
///
/// Inputs:
/// - One native-fragment route with no native component.
///
/// Output:
/// - Stable missing-native-component diagnostic.
///
/// Transformation:
/// - Keeps native shell generation from receiving incomplete upgrade routes.
#[test]
fn mobile_routes_reject_native_presentation_without_component() {
    let mut route = native_route("/users/:id", MobileRoutePresentation::NativeFragment);
    route.native_component = None;

    let diagnostics = generate_mobile_route_configuration(&[route]).expect_err("missing component");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_route_missing_native_component"));
}

/// Verifies web presentation cannot carry native component metadata.
///
/// Inputs:
/// - One web route with a native component name.
///
/// Output:
/// - Stable unexpected-native-component diagnostic.
///
/// Transformation:
/// - Keeps web-only routes distinct from native-upgraded routes.
#[test]
fn mobile_routes_reject_web_presentation_with_component() {
    let mut route = web_route("/");
    route.native_component = Some("HomeScreen".to_string());

    let diagnostics =
        generate_mobile_route_configuration(&[route]).expect_err("unexpected component");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_route_unexpected_native_component"));
}

/// Verifies duplicate route presentation hints are rejected.
///
/// Inputs:
/// - One route with the same navigation hint repeated.
///
/// Output:
/// - Stable duplicate-presentation-hint diagnostic.
///
/// Transformation:
/// - Prevents generated shell config from carrying ambiguous navigation
///   policy.
#[test]
fn mobile_routes_reject_duplicate_presentation_hints() {
    let mut route = web_route("/");
    route.presentation_hints = vec![
        MobileRoutePresentationHint::Replace,
        MobileRoutePresentationHint::Replace,
    ];

    let diagnostics = generate_mobile_route_configuration(&[route]).expect_err("duplicate hint");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_route_duplicate_presentation_hint"));
}

/// Verifies default route presentation cannot be combined with other hints.
///
/// Inputs:
/// - One route with both default and replace hints.
///
/// Output:
/// - Stable default-presentation-hint-conflict diagnostic.
///
/// Transformation:
/// - Keeps default navigation policy as the absence of special behavior.
#[test]
fn mobile_routes_reject_default_hint_combination() {
    let mut route = web_route("/");
    route.presentation_hints = vec![
        MobileRoutePresentationHint::Default,
        MobileRoutePresentationHint::Replace,
    ];

    let diagnostics = generate_mobile_route_configuration(&[route]).expect_err("default conflict");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_route_default_presentation_hint_conflict"));
}

/// Verifies native-upgrade route hints require a native component.
///
/// Inputs:
/// - One web route with a modal presentation hint but no native component.
///
/// Output:
/// - Stable missing-native-component diagnostic.
///
/// Transformation:
/// - Keeps native shell presentation hints from producing incomplete mobile
///   route manifests.
#[test]
fn mobile_routes_reject_native_hint_without_component() {
    let mut route = web_route("/");
    route.presentation_hints = vec![MobileRoutePresentationHint::Modal];

    let diagnostics = generate_mobile_route_configuration(&[route]).expect_err("missing component");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_route_missing_native_component"));
}

/// Verifies invalid route source identity is rejected by route validation.
///
/// Inputs:
/// - One route whose source identity has an empty function name.
///
/// Output:
/// - Stable source-identity diagnostic propagated through route validation.
///
/// Transformation:
/// - Prevents native route configuration from losing source correlation.
#[test]
fn mobile_routes_reject_invalid_source_identity() {
    let mut route = web_route("/");
    route
        .source_identity
        .as_mut()
        .expect("source identity")
        .function_name = String::new();

    let diagnostics = validate_mobile_routes(&[route]).expect_err("invalid source identity");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_debug_identity_empty_function"));
}

/// Verifies route source and presentation spellings are stable.
///
/// Inputs:
/// - Every first-slice route source and presentation variant.
///
/// Output:
/// - Stable metadata spelling for each value.
///
/// Transformation:
/// - Protects shell generators and native tests from spelling drift.
#[test]
fn mobile_route_metadata_names_are_stable() {
    assert_eq!(MobileRouteSource::Terlan.as_str(), "terlan");
    assert_eq!(MobileRouteSource::AngularTs.as_str(), "angular_ts");
    assert_eq!(MobileRoutePresentation::Web.as_str(), "web");
    assert_eq!(
        MobileRoutePresentation::NativeFragment.as_str(),
        "native_fragment"
    );
    assert_eq!(MobileRoutePresentation::Modal.as_str(), "modal");
    assert_eq!(
        MobileRoutePresentation::BottomSheet.as_str(),
        "bottom_sheet"
    );
    assert_eq!(MobileRoutePresentationHint::Default.as_str(), "default");
    assert_eq!(MobileRoutePresentationHint::Modal.as_str(), "modal");
    assert_eq!(
        MobileRoutePresentationHint::BottomSheet.as_str(),
        "bottom_sheet"
    );
    assert_eq!(MobileRoutePresentationHint::ClearAll.as_str(), "clear_all");
    assert_eq!(MobileRoutePresentationHint::Replace.as_str(), "replace");
    assert_eq!(MobileRoutePresentationHint::Restore.as_str(), "restore");
    assert_eq!(
        MobileRoutePresentationHint::NativeFragmentUpgrade.as_str(),
        "native_fragment_upgrade"
    );
}
