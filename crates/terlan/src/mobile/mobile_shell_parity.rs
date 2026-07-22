//! Cross-platform mobile shell parity fixtures.
#![allow(dead_code)]
//!
//! Inputs:
//! - Shared mobile route configuration and widget metadata.
//!
//! Outputs:
//! - Normalized Android/iOS shell summaries that can be compared in tests.
//!
//! Transformation:
//! - Runs both platform planners from the same compiler-owned metadata and
//!   records the native route, presentation, and component surfaces.

use super::mobile_android_shell::{
    generate_android_bridge_component_plan, generate_android_native_fragment_route_upgrade_plan,
    generate_android_route_presentation_plan,
};
use super::mobile_ios_shell::{
    generate_ios_bridge_component_plan, generate_ios_native_screen_route_plan,
    generate_ios_route_presentation_plan,
};
use super::mobile_route::MobileRouteConfiguration;
use super::mobile_widget::MobileWidgetMetadata;

/// Cross-platform mobile shell parity fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileShellParityFixture {
    pub(crate) schema_version: u32,
    pub(crate) android: MobileShellPlatformSummary,
    pub(crate) ios: MobileShellPlatformSummary,
}

/// Normalized platform shell summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileShellPlatformSummary {
    pub(crate) native_routes: Vec<MobileShellRouteSummary>,
    pub(crate) presentations: Vec<MobileShellPresentationSummary>,
    pub(crate) components: Vec<MobileShellComponentSummary>,
}

/// Normalized native route summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileShellRouteSummary {
    pub(crate) route: String,
    pub(crate) native_component: String,
}

/// Normalized route presentation summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileShellPresentationSummary {
    pub(crate) route: String,
    pub(crate) presentation_mode: String,
    pub(crate) native_component: String,
}

/// Normalized bridge component summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileShellComponentSummary {
    pub(crate) widget: String,
    pub(crate) selector: String,
    pub(crate) native_component: String,
}

/// Generates a cross-platform mobile shell parity fixture.
pub(crate) fn generate_mobile_shell_parity_fixture(
    routes: &MobileRouteConfiguration,
    widgets: &MobileWidgetMetadata,
) -> Result<MobileShellParityFixture, String> {
    let android_routes =
        generate_android_native_fragment_route_upgrade_plan(routes).map_err(debug_diagnostics)?;
    let android_presentations =
        generate_android_route_presentation_plan(routes).map_err(debug_diagnostics)?;
    let android_components =
        generate_android_bridge_component_plan(widgets).map_err(debug_diagnostics)?;
    let ios_routes = generate_ios_native_screen_route_plan(routes).map_err(debug_diagnostics)?;
    let ios_presentations =
        generate_ios_route_presentation_plan(routes).map_err(debug_diagnostics)?;
    let ios_components = generate_ios_bridge_component_plan(widgets).map_err(debug_diagnostics)?;

    Ok(MobileShellParityFixture {
        schema_version: 1,
        android: MobileShellPlatformSummary {
            native_routes: android_routes
                .routes
                .into_iter()
                .map(|route| MobileShellRouteSummary {
                    route: route.route,
                    native_component: route.native_fragment,
                })
                .collect(),
            presentations: android_presentations
                .routes
                .into_iter()
                .map(|route| MobileShellPresentationSummary {
                    route: route.route,
                    presentation_mode: route.presentation_mode,
                    native_component: route.native_component,
                })
                .collect(),
            components: android_components
                .components
                .into_iter()
                .map(|component| MobileShellComponentSummary {
                    widget: component.widget,
                    selector: component.selector,
                    native_component: component.native_component,
                })
                .collect(),
        },
        ios: MobileShellPlatformSummary {
            native_routes: ios_routes
                .routes
                .into_iter()
                .map(|route| MobileShellRouteSummary {
                    route: route.route,
                    native_component: route.native_screen,
                })
                .collect(),
            presentations: ios_presentations
                .routes
                .into_iter()
                .map(|route| MobileShellPresentationSummary {
                    route: route.route,
                    presentation_mode: route.presentation_mode,
                    native_component: route.native_component,
                })
                .collect(),
            components: ios_components
                .components
                .into_iter()
                .map(|component| MobileShellComponentSummary {
                    widget: component.widget,
                    selector: component.selector,
                    native_component: component.native_component,
                })
                .collect(),
        },
    })
}

/// Renders platform diagnostics as a compact test fixture error.
fn debug_diagnostics<T: std::fmt::Debug>(diagnostics: T) -> String {
    format!("{diagnostics:?}")
}

#[cfg(test)]
#[path = "mobile_shell_parity_test.rs"]
mod mobile_shell_parity_test;
