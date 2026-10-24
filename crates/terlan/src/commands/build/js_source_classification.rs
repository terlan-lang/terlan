use std::path::Path;

use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxImportKind,
};

use crate::commands::build::js_browser::WebRouteSourceArtifact;
use crate::support::read_file;
use crate::validation::target_profile::TargetProfile;

/// Decides whether a source file should be skipped by browser JS emission.
///
/// Inputs:
/// - `file`: candidate Terlan source file.
/// - `profile`: selected JS target profile.
/// - `has_manifest_assets`: whether browser static assets are present.
///
/// Output:
/// - `true` for non-browser helper modules in browser projects.
///
/// Transformation:
/// - Parses the file and classifies imports so backend-only sources can remain
///   in a browser package without being lowered to JavaScript.
pub(super) fn should_skip_browser_backend_source(
    file: &Path,
    profile: TargetProfile,
    has_manifest_assets: bool,
) -> Result<bool, String> {
    if profile != TargetProfile::JsBrowser || !has_manifest_assets {
        return Ok(false);
    }
    let path = file.to_string_lossy();
    let source = read_file(&path)?;
    let syntax = parse_module_as_syntax_output(&source).map_err(|err| {
        format!(
            "cannot parse source {} for browser source classification: {err:?}",
            path
        )
    })?;
    Ok(!is_browser_js_source_module(&syntax))
}

/// Classifies one Terlan source file as a browser route-source module.
///
/// Inputs:
/// - `file`: candidate `.terl` source path discovered during JS builds.
///
/// Output:
/// - `Ok(Some(route_source))` for modules that declare HTTP router metadata or
///   WebSocket route metadata.
/// - `Ok(None)` for regular browser modules.
/// - `Err(message)` when the source cannot be read or parsed.
///
/// Transformation:
/// - Converts server-side HTTP/WebSocket modules into manifest-only route
///   inputs so `js.browser` projects can package routes without forcing handler
///   bodies through the JavaScript backend.
pub(super) fn web_route_source_artifact_from_file(
    file: &Path,
) -> Result<Option<WebRouteSourceArtifact>, String> {
    let path = file.to_string_lossy();
    let source = read_file(&path)?;
    let syntax = parse_module_as_syntax_output(&source).map_err(|err| {
        format!(
            "cannot parse source {} for web route source scan: {err:?}",
            path
        )
    })?;
    if !is_web_route_source_module(&syntax) {
        return Ok(None);
    }
    Ok(Some(WebRouteSourceArtifact {
        module: syntax.module_name.clone(),
        source_path: path.to_string(),
        manifest_path: None,
    }))
}

/// Returns whether a module is a browser-side JavaScript source module.
///
/// Inputs:
/// - `syntax`: parsed syntax-output module.
///
/// Output:
/// - `true` when imports mark the module as browser JS or asset-aware.
///
/// Transformation:
/// - Treats `std.js` and asset imports as the browser source boundary.
fn is_browser_js_source_module(syntax: &crate::terlan_syntax::SyntaxModuleOutput) -> bool {
    syntax.declarations.iter().any(|declaration| {
        matches!(
            &declaration.payload,
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::File | SyntaxImportKind::Css,
                ..
            }
        ) || matches!(
            &declaration.payload,
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                ..
            } if module_name == "std.js" || module_name.starts_with("std.js.")
        )
    })
}

/// Returns whether a source module contributes web route metadata.
///
/// Inputs:
/// - `syntax`: parsed syntax-output module.
///
/// Output:
/// - `true` for router, WebSocket, or room-protocol metadata modules.
///
/// Transformation:
/// - Supplies ordinary public export metadata to the HTTP package. Imported
///   callback providers are resolved by the application closure, not this scan.
fn is_web_route_source_module(syntax: &crate::terlan_syntax::SyntaxModuleOutput) -> bool {
    let exports = syntax
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                is_public: true,
                params,
                ..
            } => Some((name.as_str(), params.len())),
            _ => None,
        });
    terlan_http_native::manifest::has_route_entrypoints(&syntax.module_name, exports)
}

#[cfg(test)]
#[path = "js_source_classification_test.rs"]
mod tests;
