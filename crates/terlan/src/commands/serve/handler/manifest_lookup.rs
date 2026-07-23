use super::route::select_handler_for_request_ref;
use super::*;

pub(crate) fn manifest_handler_for_request(
    web_root: &Path,
    method: &str,
    request_path: &str,
) -> Option<MatchedWebPackageHandler> {
    let manifest = read_web_manifest(web_root).ok()?;
    select_handler_for_request_ref(&manifest.handlers, method, request_path)
}

pub(crate) fn manifest_static_response_for_request(
    web_root: &Path,
    method: &str,
    request_path: &str,
) -> Option<WebPackageStaticResponse> {
    let manifest = read_web_manifest(web_root).ok()?;
    let candidates = manifest
        .static_responses
        .iter()
        .map(|response| WebPackageHandler {
            method: response.method.clone(),
            route: response.route.clone(),
            module: response.module.clone(),
            function: response.function.clone(),
            arity: response.arity,
            source: response.source.clone(),
        })
        .collect();
    let matched = select_handler_for_request(candidates, method, request_path)?;
    manifest
        .static_responses
        .iter()
        .find(|response| {
            response.method == matched.handler.method && response.route == matched.handler.route
        })
        .cloned()
}

pub(crate) fn manifest_file_response_for_request(
    web_root: &Path,
    method: &str,
    request_path: &str,
) -> Option<(WebPackageFileResponse, PathBuf)> {
    let manifest = read_web_manifest(web_root).ok()?;
    let candidates = manifest
        .file_responses
        .iter()
        .map(|response| WebPackageHandler {
            method: response.method.clone(),
            route: response.route.clone(),
            module: "static".to_string(),
            function: "file".to_string(),
            arity: 1,
            source: response.source.clone(),
        })
        .collect();
    let matched = select_handler_for_request(candidates, method, request_path)?;
    let response = manifest
        .file_responses
        .iter()
        .find(|response| {
            response.method == matched.handler.method && response.route == matched.handler.route
        })?
        .clone();
    let path = package_relative_path(web_root, &response.path)?;
    path.is_file().then_some((response, path))
}
