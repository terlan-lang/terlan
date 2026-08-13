//! Canonical source/module inventory for dynamic web handlers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::handler::{
    sse_router_handler, static_response_router_handler, websocket_router_handler,
};
use super::{manifest, server_lifecycle::source_path_from_manifest};

/// Resolves the unique source/module pairs owned by one generated web package.
pub(super) fn dynamic_handler_source_modules(
    web_root: &Path,
) -> super::ServeResult<Vec<(PathBuf, String)>> {
    let manifest = manifest::read_web_manifest(web_root).map_err(|message| {
        format!(
            "error[serve_package]: cannot read browser package manifest `{}`: {message}",
            web_root.join("manifest.json").display()
        )
    })?;
    let project_root = manifest::adjacent_project_root(web_root).ok_or_else(|| {
        "error[serve_runtime]: dynamic handlers require an adjacent project root".to_string()
    })?;
    let mut handlers = manifest.handlers.clone();
    handlers.extend(
        manifest
            .static_responses
            .iter()
            .filter_map(static_response_router_handler),
    );
    handlers.extend(
        manifest
            .websockets
            .iter()
            .filter_map(websocket_router_handler),
    );
    handlers.extend(manifest.sse.iter().map(sse_router_handler));
    let mut modules = BTreeMap::new();
    for handler in handlers {
        let source = handler.source.as_ref().ok_or_else(|| {
            format!(
                "error[serve_runtime]: dynamic handler `{}.{}/{}` is missing source metadata",
                handler.module, handler.function, handler.arity
            )
        })?;
        let path = source_path_from_manifest(&project_root, &source.path).ok_or_else(|| {
            format!(
                "error[serve.aot.source_path]: dynamic handler source path `{}` is unsafe",
                source.path
            )
        })?;
        if let Some(previous) = modules.insert(path.clone(), handler.module.clone()) {
            if previous != handler.module {
                return Err(format!(
                    "error[serve.aot.module]: `{}` maps to both `{previous}` and `{}`",
                    path.display(),
                    handler.module
                )
                .into());
            }
        }
    }
    Ok(modules.into_iter().collect())
}
