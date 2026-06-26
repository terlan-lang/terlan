use std::path::Path;

use terlan_syntax::{parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxImportKind};

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
fn is_browser_js_source_module(syntax: &terlan_syntax::SyntaxModuleOutput) -> bool {
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
/// - Combines explicit router imports with naming conventions for generated
///   realtime route metadata.
fn is_web_route_source_module(syntax: &terlan_syntax::SyntaxModuleOutput) -> bool {
    (imports_std_http_router(syntax) && declares_router_function(syntax))
        || is_websocket_metadata_module(syntax)
        || is_room_protocol_metadata_module(syntax)
}

/// Returns whether a module declares WebSocket metadata.
///
/// Inputs:
/// - `syntax`: parsed syntax-output module.
///
/// Output:
/// - `true` when the module provides zero-argument `route` and `protocol`
///   functions under a `.WebSocket` module name.
///
/// Transformation:
/// - Recognizes declarative websocket modules without compiling handler bodies.
fn is_websocket_metadata_module(syntax: &terlan_syntax::SyntaxModuleOutput) -> bool {
    syntax.module_name.ends_with(".WebSocket")
        && declares_zero_arg_public_function(syntax, "route")
        && declares_zero_arg_public_function(syntax, "protocol")
}

/// Returns whether a module declares room-protocol metadata.
///
/// Inputs:
/// - `syntax`: parsed syntax-output module.
///
/// Output:
/// - `true` when the module provides zero-argument `route` and `protocol`
///   functions under a `.RoomProtocol` module name.
///
/// Transformation:
/// - Identifies generated realtime protocol descriptors as route metadata.
fn is_room_protocol_metadata_module(syntax: &terlan_syntax::SyntaxModuleOutput) -> bool {
    syntax.module_name.ends_with(".RoomProtocol")
        && declares_zero_arg_public_function(syntax, "route")
        && declares_zero_arg_public_function(syntax, "protocol")
}

/// Returns whether a source module imports `std.http.Router`.
///
/// Inputs:
/// - `syntax`: parsed syntax-output module.
///
/// Output:
/// - `true` when a module import references `std.http.Router`.
///
/// Transformation:
/// - Uses explicit imports as the web-route source marker, avoiding filename
///   conventions or backend-specific annotations.
fn imports_std_http_router(syntax: &terlan_syntax::SyntaxModuleOutput) -> bool {
    syntax.declarations.iter().any(|declaration| {
        matches!(
            &declaration.payload,
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                ..
            } if module_name == "std.http.Router"
        )
    })
}

/// Returns whether a source module declares a public router function.
///
/// Inputs:
/// - `syntax`: parsed syntax-output module.
///
/// Output:
/// - `true` when the module has `pub router(...)`.
///
/// Transformation:
/// - Keeps the route-source classifier narrow so ordinary modules that merely
///   import HTTP support still compile through the selected target backend.
fn declares_router_function(syntax: &terlan_syntax::SyntaxModuleOutput) -> bool {
    declares_public_function(syntax, "router")
}

/// Checks whether a module declares a named zero-argument public function.
///
/// Inputs:
/// - `syntax`: parsed syntax-output module.
/// - `target`: function name to find.
///
/// Output:
/// - `true` when a matching `pub target()` declaration exists.
///
/// Transformation:
/// - Scans syntax declarations without invoking typechecking.
fn declares_zero_arg_public_function(
    syntax: &terlan_syntax::SyntaxModuleOutput,
    target: &str,
) -> bool {
    syntax.declarations.iter().any(|declaration| {
        matches!(
            &declaration.payload,
            SyntaxDeclarationPayload::Function {
                name,
                is_public: true,
                params,
                ..
            } if name == target && params.is_empty()
        )
    })
}

/// Checks whether a module declares a named public function.
///
/// Inputs:
/// - `syntax`: parsed syntax-output module.
/// - `target`: function name to find.
///
/// Output:
/// - `true` when a matching public function declaration exists.
///
/// Transformation:
/// - Performs a declaration-level scan for route-source classification.
fn declares_public_function(syntax: &terlan_syntax::SyntaxModuleOutput, target: &str) -> bool {
    syntax.declarations.iter().any(|declaration| {
        matches!(
            &declaration.payload,
            SyntaxDeclarationPayload::Function {
                name,
                is_public: true,
                ..
            } if name == target
        )
    })
}
