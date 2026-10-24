//! Source-executed route discovery shared by VM services and browser packages.

use super::*;
use crate::commands::api::evaluated_contract::{callable_label, with_router_file};
use std::path::Path;

pub(in crate::commands::build::js_browser) fn discover_web_route_manifest_from_sources(
    sources: &[WebRouteSourceArtifact],
) -> Result<WebRouteManifestRows, String> {
    let workspace = tempfile::tempdir().map_err(|error| error.to_string())?;
    let mut rows = WebRouteManifestRows::default();
    for artifact in sources {
        let text = fs::read_to_string(&artifact.source_path).map_err(|error| error.to_string())?;
        let syntax = parse_module_as_syntax_output(&text)
            .map_err(|error| format!("error[web_router]: {error:?}"))?;
        let context = route_source_context(artifact, &text);
        if let Some(websocket) =
            websocket_from_source(artifact, &syntax, &context, workspace.path())?
        {
            rows.websockets.push(websocket);
        }
        let Some(body) =
            syntax
                .declarations
                .iter()
                .find_map(|declaration| match &declaration.payload {
                    SyntaxDeclarationPayload::Function {
                        name,
                        is_public: true,
                        clauses,
                        ..
                    } if name == "router" => clauses
                        .iter()
                        .find(|clause| clause.patterns.is_empty())
                        .map(|clause| &clause.body),
                    _ => None,
                })
        else {
            continue;
        };
        let source = source_span_for_expr(&context, body);
        let mut evaluated = with_router_file(
            Path::new(&artifact.source_path),
            workspace.path(),
            |module, router, exports| {
                if module != artifact.module {
                    return Err(format!(
                        "error[web_router]: expected module `{}`, found `{module}`",
                        artifact.module
                    ));
                }
                WebRouteManifestRows::from_source_router(module, source, router, |value| {
                    let label = callable_label(value, module, exports)?;
                    // The owning graph dispatches the actual callback. The manifest's
                    // function field is a local diagnostic label, not an import rewrite.
                    Ok(label.rsplit('.').next().unwrap_or(&label).to_string())
                })
                .map_err(String::from)
            },
        )?;
        rows.handlers.append(&mut evaluated.handlers);
        rows.websockets.append(&mut evaluated.websockets);
        rows.sse.append(&mut evaluated.sse);
    }
    validate_discovered_web_routes(&rows)?;
    Ok(rows)
}
