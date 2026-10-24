//! Web manifests project executed routers through the HTTP package contract.

use std::fs;

use crate::terlan_syntax::{parse_module_as_syntax_output, SyntaxDeclarationPayload};

use super::manifest::WebSocketArtifact;
use super::WebRouteSourceArtifact;

mod evaluated;
mod helpers;
mod source_values;
mod validation;

pub(super) use evaluated::discover_web_route_manifest_from_sources;
pub(super) use terlan_http_native::manifest::Routes as WebRouteManifestRows;

use helpers::{route_source_context, source_span_for_expr, WebRouteSourceContext};
use source_values::websocket_from_source;
use validation::validate_discovered_web_routes;

/// Projects handler rows through the same evaluated source path as packaging.
#[cfg(test)]
pub(super) fn discover_web_handlers_from_modules(
    modules: &[crate::commands::build::js::JsModuleArtifact],
) -> Result<Vec<super::manifest::WebHandlerArtifact>, String> {
    let sources = modules
        .iter()
        .map(WebRouteSourceArtifact::from_js_module)
        .collect::<Vec<_>>();
    Ok(discover_web_route_manifest_from_sources(&sources)?.handlers)
}
