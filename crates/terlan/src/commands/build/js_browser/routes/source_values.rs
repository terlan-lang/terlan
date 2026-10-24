//! Legacy WebSocket metadata executes ordinary source instead of a string interpreter.

use super::*;
use crate::commands::build::vm_artifact::source_exports::with_source_exports;
use std::path::Path;

/// Evaluates legacy metadata entrypoints and delegates value admission to HTTP.
pub(super) fn websocket_from_source(
    artifact: &WebRouteSourceArtifact,
    syntax: &crate::terlan_syntax::SyntaxModuleOutput,
    source: &WebRouteSourceContext<'_>,
    workspace: &Path,
) -> Result<Option<WebSocketArtifact>, String> {
    if !artifact.module.ends_with(".WebSocket") {
        return Ok(None);
    }
    let body = syntax
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, clauses, .. }
                if matches!(name.as_str(), "route" | "protocol") =>
            {
                clauses.first().map(|clause| &clause.body)
            }
            _ => None,
        });
    let Some(body) = body else {
        return Ok(None);
    };
    with_source_exports(
        Path::new(&artifact.source_path),
        workspace,
        &["route", "protocol"],
        |module, values, _exports| {
            if module != artifact.module {
                return Err(format!(
                    "error[web_router]: expected module `{}`, found `{module}`",
                    artifact.module
                ));
            }
            WebSocketArtifact::from_source_exports(
                module,
                source_span_for_expr(source, body),
                values,
            )
            .map(Some)
            .map_err(String::from)
        },
    )
}
