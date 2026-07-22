//! Typed mobile route configuration for native shell presentation.
#![allow(dead_code)]
//!
//! Inputs:
//! - Route declarations collected from Terlan modules or AngularTS route
//!   metadata.
//!
//! Outputs:
//! - Validated native-presentation configuration that mobile shell generators
//!   can consume.
//!
//! Transformation:
//! - Keeps route presentation explicit and typed before mobile shell project
//!   generation exists.

use std::collections::BTreeSet;

use crate::mobile::mobile_debug_identity::{
    generate_mobile_debug_identity_metadata, validate_mobile_source_identity,
    MobileDebugIdentityMetadata, MobileSourceIdentity,
};

/// One route declaration that may be presented by a mobile shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileRouteDeclaration {
    pub(crate) route: String,
    pub(crate) source: MobileRouteSource,
    pub(crate) handler: String,
    pub(crate) presentation: MobileRoutePresentation,
    pub(crate) presentation_hints: Vec<MobileRoutePresentationHint>,
    pub(crate) native_component: Option<String>,
    pub(crate) source_identity: Option<MobileSourceIdentity>,
}

/// Source system that produced a route declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileRouteSource {
    Terlan,
    AngularTs,
}

impl MobileRouteSource {
    /// Returns the stable metadata spelling for one route source.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Terlan => "terlan",
            Self::AngularTs => "angular_ts",
        }
    }
}

/// Native shell presentation mode for one route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileRoutePresentation {
    Web,
    NativeFragment,
    Modal,
    BottomSheet,
}

impl MobileRoutePresentation {
    /// Returns the stable metadata spelling for one presentation mode.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::NativeFragment => "native_fragment",
            Self::Modal => "modal",
            Self::BottomSheet => "bottom_sheet",
        }
    }

    /// Returns whether this mode requires a native component name.
    const fn requires_native_component(self) -> bool {
        matches!(self, Self::NativeFragment | Self::Modal | Self::BottomSheet)
    }
}

/// Native shell presentation/navigation hint for one route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MobileRoutePresentationHint {
    Default,
    Modal,
    BottomSheet,
    ClearAll,
    Replace,
    Restore,
    NativeFragmentUpgrade,
}

impl MobileRoutePresentationHint {
    /// Returns the stable metadata spelling for one presentation hint.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Modal => "modal",
            Self::BottomSheet => "bottom_sheet",
            Self::ClearAll => "clear_all",
            Self::Replace => "replace",
            Self::Restore => "restore",
            Self::NativeFragmentUpgrade => "native_fragment_upgrade",
        }
    }

    /// Returns whether this hint requires a native component name.
    const fn requires_native_component(self) -> bool {
        matches!(
            self,
            Self::Modal | Self::BottomSheet | Self::NativeFragmentUpgrade
        )
    }
}

/// Generated mobile route configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileRouteConfiguration {
    pub(crate) schema_version: u32,
    pub(crate) routes: Vec<MobileRouteConfigEntry>,
}

/// Generated config entry for one route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileRouteConfigEntry {
    pub(crate) route: String,
    pub(crate) source: &'static str,
    pub(crate) handler: String,
    pub(crate) presentation: &'static str,
    pub(crate) presentation_hints: Vec<&'static str>,
    pub(crate) native_component: Option<String>,
    pub(crate) source_identity: Option<MobileDebugIdentityMetadata>,
}

/// Validation diagnostic for mobile route declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileRouteDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Generates native-presentation route configuration.
///
/// Inputs:
/// - `routes`: typed route declarations collected from Terlan/AngularTS.
///
/// Output:
/// - Schema-versioned route configuration when route declarations are coherent.
/// - Stable diagnostics when routes are malformed or ambiguous.
///
/// Transformation:
/// - Validates route paths and native component requirements, then converts
///   typed enum values to stable metadata spellings.
pub(crate) fn generate_mobile_route_configuration(
    routes: &[MobileRouteDeclaration],
) -> Result<MobileRouteConfiguration, Vec<MobileRouteDiagnostic>> {
    validate_mobile_routes(routes)?;
    Ok(MobileRouteConfiguration {
        schema_version: 1,
        routes: routes.iter().map(mobile_route_config_entry).collect(),
    })
}

/// Validates mobile route declarations.
pub(crate) fn validate_mobile_routes(
    routes: &[MobileRouteDeclaration],
) -> Result<(), Vec<MobileRouteDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut route_keys = BTreeSet::new();

    for route in routes {
        if !route.route.starts_with('/') {
            diagnostics.push(diagnostic(
                "mobile_route_invalid_path",
                format!("mobile route `{}` must start with `/`", route.route),
            ));
        }
        if route.route.trim().is_empty() || route.route.contains(char::is_whitespace) {
            diagnostics.push(diagnostic(
                "mobile_route_invalid_path",
                format!(
                    "mobile route `{}` must not be empty or contain whitespace",
                    route.route
                ),
            ));
        }
        if route.handler.trim().is_empty() {
            diagnostics.push(diagnostic(
                "mobile_route_empty_handler",
                format!("mobile route `{}` must name a handler", route.route),
            ));
        }
        if !route_keys.insert((route.source.as_str(), route.route.as_str())) {
            diagnostics.push(diagnostic(
                "mobile_route_duplicate",
                format!(
                    "mobile route `{}` is declared more than once for source `{}`",
                    route.route,
                    route.source.as_str()
                ),
            ));
        }
        diagnostics.extend(validate_native_component(route));
        diagnostics.extend(validate_presentation_hints(route));
        diagnostics.extend(validate_optional_source_identity(route));
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Converts one route declaration to generated metadata.
fn mobile_route_config_entry(route: &MobileRouteDeclaration) -> MobileRouteConfigEntry {
    MobileRouteConfigEntry {
        route: route.route.clone(),
        source: route.source.as_str(),
        handler: route.handler.clone(),
        presentation: route.presentation.as_str(),
        presentation_hints: route
            .presentation_hints
            .iter()
            .map(|hint| hint.as_str())
            .collect(),
        native_component: route.native_component.clone(),
        source_identity: route.source_identity.as_ref().map(|identity| {
            generate_mobile_debug_identity_metadata(identity)
                .expect("validated mobile route source identity")
        }),
    }
}

/// Validates native component requirements for one route.
fn validate_native_component(route: &MobileRouteDeclaration) -> Vec<MobileRouteDiagnostic> {
    let mut diagnostics = Vec::new();
    if route_requires_native_component(route)
        && route
            .native_component
            .as_ref()
            .is_none_or(|component| component.trim().is_empty())
    {
        diagnostics.push(diagnostic(
            "mobile_route_missing_native_component",
            format!(
                "mobile route `{}` presentation `{}` requires a native component",
                route.route,
                route.presentation.as_str()
            ),
        ));
    }
    if route.presentation == MobileRoutePresentation::Web
        && !route_hints_require_native_component(route)
        && route.native_component.is_some()
    {
        diagnostics.push(diagnostic(
            "mobile_route_unexpected_native_component",
            format!(
                "mobile route `{}` web presentation must not name a native component",
                route.route
            ),
        ));
    }
    diagnostics
}

/// Validates route presentation hints.
fn validate_presentation_hints(route: &MobileRouteDeclaration) -> Vec<MobileRouteDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for hint in &route.presentation_hints {
        if !seen.insert(*hint) {
            diagnostics.push(diagnostic(
                "mobile_route_duplicate_presentation_hint",
                format!(
                    "mobile route `{}` repeats presentation hint `{}`",
                    route.route,
                    hint.as_str()
                ),
            ));
        }
    }
    if route.presentation_hints.len() > 1
        && route
            .presentation_hints
            .contains(&MobileRoutePresentationHint::Default)
    {
        diagnostics.push(diagnostic(
            "mobile_route_default_presentation_hint_conflict",
            format!(
                "mobile route `{}` default presentation hint must not be combined with other hints",
                route.route
            ),
        ));
    }
    diagnostics
}

/// Returns whether one route requires a native component.
fn route_requires_native_component(route: &MobileRouteDeclaration) -> bool {
    route.presentation.requires_native_component() || route_hints_require_native_component(route)
}

/// Returns whether any route hint requires a native component.
fn route_hints_require_native_component(route: &MobileRouteDeclaration) -> bool {
    route
        .presentation_hints
        .iter()
        .any(|hint| hint.requires_native_component())
}

/// Validates optional route source identity.
fn validate_optional_source_identity(route: &MobileRouteDeclaration) -> Vec<MobileRouteDiagnostic> {
    let Some(identity) = route.source_identity.as_ref() else {
        return Vec::new();
    };
    validate_mobile_source_identity(identity)
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|source_diagnostic| {
            diagnostic(
                source_diagnostic.code,
                format!(
                    "mobile route `{}` has invalid source identity: {}",
                    route.route, source_diagnostic.message
                ),
            )
        })
        .collect()
}

/// Builds a stable mobile route diagnostic.
fn diagnostic(code: &'static str, message: impl Into<String>) -> MobileRouteDiagnostic {
    MobileRouteDiagnostic {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "mobile_route_test.rs"]
mod mobile_route_test;
