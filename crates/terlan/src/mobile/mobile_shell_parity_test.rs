use super::super::mobile_route::{
    generate_mobile_route_configuration, MobileRouteDeclaration, MobileRoutePresentation,
    MobileRoutePresentationHint, MobileRouteSource,
};
use super::super::mobile_widget::{
    generate_mobile_widget_metadata, standard_mobile_widget_declarations,
};
use super::*;

/// Builds shared mobile route configuration for parity tests.
fn route_config() -> super::super::mobile_route::MobileRouteConfiguration {
    generate_mobile_route_configuration(&[
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
            presentation: MobileRoutePresentation::Web,
            presentation_hints: vec![MobileRoutePresentationHint::BottomSheet],
            native_component: Some("ActionsSheet".to_string()),
            source_identity: None,
        },
    ])
    .expect("route config")
}

/// Verifies Android and iOS consume the same route/widget metadata.
///
/// Inputs:
/// - Shared mobile route configuration and standard widget metadata.
///
/// Output:
/// - Cross-platform parity fixture with matching normalized native routes,
///   presentations, and bridge component summaries.
///
/// Transformation:
/// - Runs Android and iOS planners from the same compiler-owned metadata and
///   compares their normalized shell surfaces.
#[test]
fn mobile_shell_parity_fixture_matches_android_and_ios_surfaces() {
    let widgets = generate_mobile_widget_metadata(&standard_mobile_widget_declarations())
        .expect("widget metadata");
    let fixture =
        generate_mobile_shell_parity_fixture(&route_config(), &widgets).expect("parity fixture");

    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.android.native_routes, fixture.ios.native_routes);
    assert_eq!(fixture.android.presentations, fixture.ios.presentations);
    assert_eq!(fixture.android.components, fixture.ios.components);
    assert_eq!(fixture.android.native_routes[0].route, "/profile");
    assert_eq!(
        fixture.android.native_routes[0].native_component,
        "ProfileScreen"
    );
    assert_eq!(fixture.android.presentations[0].route, "/login");
    assert_eq!(fixture.android.presentations[0].presentation_mode, "modal");
    assert_eq!(fixture.android.presentations[1].route, "/actions");
    assert_eq!(
        fixture.android.presentations[1].presentation_mode,
        "bottom_sheet"
    );
    assert_eq!(fixture.android.components[0].widget, "ToolbarAction");
    assert_eq!(
        fixture.android.components[0].selector,
        "terlan-toolbar-action"
    );
}
