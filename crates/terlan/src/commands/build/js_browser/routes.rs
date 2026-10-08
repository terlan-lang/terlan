use std::collections::HashMap;
use std::fs;

use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
};

#[cfg(test)]
use crate::commands::build::js::JsModuleArtifact;
use crate::web_route::{route_param_types, validate_route_pattern};

use super::manifest::{
    WebFileResponseArtifact, WebHandlerArtifact, WebSocketArtifact, WebSseArtifact,
    WebStaticResponseArtifact,
};
use super::WebRouteSourceArtifact;

mod helpers;
mod validation;

#[cfg(test)]
use validation::validate_discovered_web_handler_routes;
use validation::{
    apply_router_handler_arities, validate_discovered_web_routes, validate_router_handler_rows,
};

#[cfg(test)]
use helpers::prefixed_router_route;
use helpers::{
    is_request_type, is_response_type, is_router_builder_receiver, prefix_web_route_manifest_rows,
    route_source_context, router_group_body_expr, router_handler_name, router_receiver_method_name,
    router_route_literal, source_span_for_expr, WebRouteSourceContext,
};

/// Discovered route-manifest rows for one browser package.
///
/// Inputs:
/// - Produced by route-manifest source extraction.
///
/// Output:
/// - Dynamic handler rows plus cacheable static and file response rows.
///
/// Transformation:
/// - Keeps source handlers executable; explicit static/file rows remain part
///   of the manifest format but are not inferred from handler bodies.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct WebRouteManifestRows {
    pub(super) handlers: Vec<WebHandlerArtifact>,
    pub(super) websockets: Vec<WebSocketArtifact>,
    pub(super) sse: Vec<WebSseArtifact>,
    pub(super) static_responses: Vec<WebStaticResponseArtifact>,
    pub(super) file_responses: Vec<WebFileResponseArtifact>,
}

/// Discovers dynamic and static web route manifest rows from emitted modules.
///
/// Inputs:
/// - `modules`: emitted JS module artifacts containing original source paths.
///
/// Output:
/// - Route manifest rows for simple `std.http.Router` builder calls.
/// - Stable error if a source file cannot be read, reparsed, or validated.
///
/// Transformation:
/// - Preserves handler calls without interpreting response-provider semantics.
pub(super) fn discover_web_route_manifest_from_sources(
    sources: &[WebRouteSourceArtifact],
) -> Result<WebRouteManifestRows, String> {
    let mut rows = WebRouteManifestRows::default();
    let string_constants = source_string_constants(sources)?;
    let signature_catalog = router_handler_signature_catalog(sources)?;
    for source_artifact in sources {
        let source = fs::read_to_string(&source_artifact.source_path).map_err(|err| {
            format!(
                "cannot read source {} for web route discovery: {err}",
                source_artifact.source_path
            )
        })?;
        let syntax = parse_module_as_syntax_output(&source).map_err(|err| {
            format!(
                "cannot parse source {} for web route discovery: {err:?}",
                source_artifact.source_path
            )
        })?;
        let (signatures, handler_targets) =
            router_handler_scope(&syntax, &source_artifact.module, &signature_catalog);
        let source_context = route_source_context(source_artifact, &source);
        for declaration in &syntax.declarations {
            let SyntaxDeclarationPayload::Function { name, clauses, .. } = &declaration.payload
            else {
                continue;
            };
            if name != "router" {
                continue;
            }
            for clause in clauses {
                collect_router_routes_from_expr(
                    &source_artifact.module,
                    &clause.body,
                    &source_context,
                    &signatures,
                    &handler_targets,
                    &mut rows,
                )?;
            }
        }
        if let Some(websocket) =
            websocket_from_source(source_artifact, &syntax, &source_context, &string_constants)?
        {
            rows.websockets.push(websocket);
        }
    }
    validate_discovered_web_routes(&rows)?;
    Ok(rows)
}

/// Discovers web handler manifest rows from emitted source modules.
///
/// Inputs:
/// - `modules`: emitted JS module artifacts containing original source paths.
///
/// Output:
/// - Handler rows for simple `std.http.Router` builder calls.
/// - Stable error if a previously compiled source file cannot be read or
///   reparsed.
///
/// Transformation:
/// - Reparses source modules, finds `router` functions, and extracts direct
///   `Router.get/post/put/patch/delete/head/options/fallback` calls from their
///   body.
#[cfg(test)]
pub(super) fn discover_web_handlers_from_modules(
    modules: &[JsModuleArtifact],
) -> Result<Vec<WebHandlerArtifact>, String> {
    let sources = modules
        .iter()
        .map(WebRouteSourceArtifact::from_js_module)
        .collect::<Vec<_>>();
    discover_web_handlers_from_sources(&sources)
}

/// Discovers web handler manifest rows from route-source modules.
///
/// Inputs:
/// - `sources`: Terlan source modules known to contain HTTP router metadata.
///
/// Output:
/// - Handler rows for simple `std.http.Router` builder calls.
/// - Stable error if a source file cannot be read or parsed.
///
/// Transformation:
/// - Reparses route sources and extracts dynamic route rows without depending
///   on browser JavaScript artifacts.
#[cfg(test)]
fn discover_web_handlers_from_sources(
    sources: &[WebRouteSourceArtifact],
) -> Result<Vec<WebHandlerArtifact>, String> {
    let mut handlers = Vec::new();
    let signature_catalog = router_handler_signature_catalog(sources)?;
    for source_artifact in sources {
        let source = fs::read_to_string(&source_artifact.source_path).map_err(|err| {
            format!(
                "cannot read source {} for web handler discovery: {err}",
                source_artifact.source_path
            )
        })?;
        let syntax = parse_module_as_syntax_output(&source).map_err(|err| {
            format!(
                "cannot parse source {} for web handler discovery: {err:?}",
                source_artifact.source_path
            )
        })?;
        let (signatures, handler_targets) =
            router_handler_scope(&syntax, &source_artifact.module, &signature_catalog);
        let source_context = route_source_context(source_artifact, &source);
        for declaration in &syntax.declarations {
            let SyntaxDeclarationPayload::Function { name, clauses, .. } = &declaration.payload
            else {
                continue;
            };
            if name != "router" {
                continue;
            }
            for clause in clauses {
                collect_router_handlers_from_expr(
                    &source_artifact.module,
                    &clause.body,
                    &source_context,
                    &signatures,
                    &handler_targets,
                    &mut handlers,
                )?;
            }
        }
    }
    validate_discovered_web_handler_routes(&handlers)?;
    Ok(handlers)
}

/// Local function signature data used by router-manifest extraction.
///
/// Inputs:
/// - Produced from syntax-output function declarations.
///
/// Output:
/// - Arity, parameter names, parameter types, and return type text for one
///   local function.
///
/// Transformation:
/// - Keeps route extraction independent of the full typechecker while still
///   validating the handler surface needed by the serve manifest.
#[derive(Clone)]
struct RouterHandlerSignature {
    arity: usize,
    param_names: Vec<String>,
    param_types: Vec<String>,
    return_type: String,
    source: Option<super::manifest::WebSourceSpanArtifact>,
}

#[derive(Clone)]
struct RouterHandlerTarget {
    module: String,
    function: String,
    source: Option<super::manifest::WebSourceSpanArtifact>,
}

type RouterHandlerSignatureCatalog = HashMap<String, HashMap<String, RouterHandlerSignature>>;

/// Collects handler declarations from every route-source module.
///
/// Route assembly may reference selected imports whose declarations live in a
/// dedicated handler module. The catalog keeps those declarations available
/// without coupling source-level route extraction to backend artifacts.
fn router_handler_signature_catalog(
    sources: &[WebRouteSourceArtifact],
) -> Result<RouterHandlerSignatureCatalog, String> {
    let mut catalog = HashMap::new();
    for source_artifact in sources {
        let source = fs::read_to_string(&source_artifact.source_path).map_err(|err| {
            format!(
                "cannot read source {} for web handler discovery: {err}",
                source_artifact.source_path
            )
        })?;
        let syntax = parse_module_as_syntax_output(&source).map_err(|err| {
            format!(
                "cannot parse source {} for web handler discovery: {err:?}",
                source_artifact.source_path
            )
        })?;
        let source_context = route_source_context(source_artifact, &source);
        catalog.insert(
            source_artifact.module.clone(),
            router_handler_signatures(&syntax, Some(&source_context)),
        );
    }
    Ok(catalog)
}

/// Builds the callable scope visible to one router declaration.
///
/// Local declarations retain their source-module identity. Selected value
/// imports add their local alias while preserving the provider module and
/// exported function name used by runtime dispatch.
fn router_handler_scope(
    syntax: &crate::terlan_syntax::SyntaxModuleOutput,
    module_name: &str,
    catalog: &RouterHandlerSignatureCatalog,
) -> (
    HashMap<String, RouterHandlerSignature>,
    HashMap<String, RouterHandlerTarget>,
) {
    let mut signatures = catalog.get(module_name).cloned().unwrap_or_default();
    let mut targets = signatures
        .iter()
        .map(|(name, signature)| {
            (
                name.clone(),
                RouterHandlerTarget {
                    module: module_name.to_string(),
                    function: name.clone(),
                    source: signature.source.clone(),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    for declaration in &syntax.declarations {
        let SyntaxDeclarationPayload::Import {
            module_name: provider,
            items,
            is_type,
            is_selected,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if *is_type || !*is_selected {
            continue;
        }
        let Some(provider_signatures) = catalog.get(provider) else {
            continue;
        };
        for item in items {
            let Some(signature) = provider_signatures.get(&item.name) else {
                continue;
            };
            let local_name = item.as_alias.as_ref().unwrap_or(&item.name);
            signatures.insert(local_name.clone(), signature.clone());
            targets.insert(
                local_name.clone(),
                RouterHandlerTarget {
                    module: provider.clone(),
                    function: item.name.clone(),
                    source: signature.source.clone(),
                },
            );
        }
    }

    (signatures, targets)
}

/// Collects local function signatures visible to router builders.
///
/// Inputs:
/// - `syntax`: parsed syntax-output module.
///
/// Output:
/// - Function signatures keyed by function name.
///
/// Transformation:
/// - Scans function declarations and records the declared arity and return type
///   for simple local handler validation.
fn router_handler_signatures(
    syntax: &crate::terlan_syntax::SyntaxModuleOutput,
    source: Option<&WebRouteSourceContext<'_>>,
) -> HashMap<String, RouterHandlerSignature> {
    syntax
        .declarations
        .iter()
        .filter_map(|declaration| {
            let SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                clauses,
                ..
            } = &declaration.payload
            else {
                return None;
            };
            Some((
                name.clone(),
                RouterHandlerSignature {
                    arity: params.len(),
                    param_names: params.iter().map(|param| param.name.clone()).collect(),
                    param_types: params
                        .iter()
                        .map(|param| param.annotation.text.clone())
                        .collect(),
                    return_type: return_type.text.clone(),
                    source: clauses.first().and_then(|clause| {
                        source.map(|source| source_span_for_expr(source, &clause.body))
                    }),
                },
            ))
        })
        .collect()
}

/// Recursively collects route-builder calls and their source handler targets.
///
/// Inputs:
/// - `module_name`: Terlan module that owns discovered handlers.
/// - `expr`: syntax expression to inspect.
/// - `signatures`: local function signature map.
/// - `rows`: output buffers for dynamic and static route rows.
///
/// Output:
/// - `Ok(())` after recognized route builders have been added.
/// - Stable `error[web_router]` diagnostic for invalid handler references.
///
/// Transformation:
/// - Resolves handler targets without replacing source execution with payloads.
fn collect_router_routes_from_expr(
    module_name: &str,
    expr: &SyntaxExprOutput,
    source: &WebRouteSourceContext<'_>,
    signatures: &HashMap<String, RouterHandlerSignature>,
    handler_targets: &HashMap<String, RouterHandlerTarget>,
    rows: &mut WebRouteManifestRows,
) -> Result<(), String> {
    if let Some((prefix, body)) = router_group_body_expr(expr) {
        let mut grouped_rows = WebRouteManifestRows::default();
        collect_router_routes_from_expr(
            module_name,
            body,
            source,
            signatures,
            handler_targets,
            &mut grouped_rows,
        )?;
        prefix_web_route_manifest_rows(&prefix, &mut grouped_rows);
        rows.handlers.append(&mut grouped_rows.handlers);
        rows.websockets.append(&mut grouped_rows.websockets);
        rows.sse.append(&mut grouped_rows.sse);
        rows.static_responses
            .append(&mut grouped_rows.static_responses);
        rows.file_responses.append(&mut grouped_rows.file_responses);
        return Ok(());
    }
    if let Some(endpoint) = router_sse_from_expr(module_name, expr, source) {
        rows.sse.push(endpoint);
    }
    if let Some(handlers) = router_handler_from_expr(module_name, expr, source) {
        validate_router_handler_rows(module_name, &handlers, signatures)?;
        let mut handlers = handlers;
        apply_router_handler_arities(&mut handlers, signatures);
        for mut handler in handlers {
            let local_name = handler.function.clone();
            apply_router_target(
                &local_name,
                &mut handler.module,
                &mut handler.function,
                &mut handler.source,
                handler_targets,
            );
            rows.handlers.push(handler);
        }
    }
    for child in &expr.children {
        collect_router_routes_from_expr(
            module_name,
            child,
            source,
            signatures,
            handler_targets,
            rows,
        )?;
    }
    Ok(())
}

/// Converts one direct or receiver-style `Router.sse` call into manifest metadata.
fn router_sse_from_expr(
    module_name: &str,
    expr: &SyntaxExprOutput,
    source: &WebRouteSourceContext<'_>,
) -> Option<WebSseArtifact> {
    if expr.kind != SyntaxExprKind::Call {
        return None;
    }
    let (method_name, route_index) = if expr.remote.as_deref() == Some("Router") {
        (expr.children.first()?.text.as_deref()?, 2)
    } else {
        let callee = expr.children.first()?;
        let method_name = router_receiver_method_name(callee)?;
        if !is_router_builder_receiver(callee.children.first()?) {
            return None;
        }
        (method_name, 1)
    };
    if method_name != "sse" {
        return None;
    }
    Some(WebSseArtifact {
        module: module_name.to_string(),
        route: router_route_literal(expr.children.get(route_index)?)?,
        source: source_span_for_expr(source, expr),
    })
}

/// Recursively collects route-builder calls from a syntax expression.
///
/// Inputs:
/// - `module_name`: Terlan module that owns discovered handler functions.
/// - `expr`: syntax expression to inspect.
/// - `handlers`: output buffer for manifest handler rows.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Walks expression children and appends rows for recognized `Router.*`
///   calls, leaving unsupported route-builder forms for later diagnostics.
#[cfg(test)]
fn collect_router_handlers_from_expr(
    module_name: &str,
    expr: &SyntaxExprOutput,
    source: &WebRouteSourceContext<'_>,
    signatures: &HashMap<String, RouterHandlerSignature>,
    handler_targets: &HashMap<String, RouterHandlerTarget>,
    handlers: &mut Vec<WebHandlerArtifact>,
) -> Result<(), String> {
    if let Some((prefix, body)) = router_group_body_expr(expr) {
        let mut grouped_handlers = Vec::new();
        collect_router_handlers_from_expr(
            module_name,
            body,
            source,
            signatures,
            handler_targets,
            &mut grouped_handlers,
        )?;
        for handler in &mut grouped_handlers {
            handler.route = prefixed_router_route(&prefix, &handler.route);
        }
        handlers.append(&mut grouped_handlers);
        return Ok(());
    }
    if let Some(handler) = router_handler_from_expr(module_name, expr, source) {
        validate_router_handler_rows(module_name, &handler, signatures)?;
        let mut handler = handler;
        apply_router_handler_arities(&mut handler, signatures);
        for row in &mut handler {
            let local_name = row.function.clone();
            apply_router_target(
                &local_name,
                &mut row.module,
                &mut row.function,
                &mut row.source,
                handler_targets,
            );
        }
        handlers.extend(handler);
    }
    for child in &expr.children {
        collect_router_handlers_from_expr(
            module_name,
            child,
            source,
            signatures,
            handler_targets,
            handlers,
        )?;
    }
    Ok(())
}

/// Rewrites a router-local callback reference to its runtime provider.
fn apply_router_target(
    local_name: &str,
    module: &mut String,
    function: &mut String,
    source: &mut Option<super::manifest::WebSourceSpanArtifact>,
    handler_targets: &HashMap<String, RouterHandlerTarget>,
) {
    let Some(target) = handler_targets.get(local_name) else {
        return;
    };
    module.clone_from(&target.module);
    function.clone_from(&target.function);
    source.clone_from(&target.source);
}

/// Converts one direct `Router.*` call into manifest handler rows.
///
/// Inputs:
/// - `module_name`: Terlan module that owns the referenced handler function.
/// - `expr`: syntax expression candidate.
///
/// Output:
/// - Handler rows when `expr` is a supported route-builder call.
/// - `None` for unrelated expressions or unsupported argument shapes.
///
/// Transformation:
/// - Reads route pattern strings and handler variable names from syntax output
///   without evaluating the router value.
fn router_handler_from_expr(
    module_name: &str,
    expr: &SyntaxExprOutput,
    source: &WebRouteSourceContext<'_>,
) -> Option<Vec<WebHandlerArtifact>> {
    if expr.kind != SyntaxExprKind::Call {
        return None;
    }
    let (method_name, route_index, handler_index) = if expr.remote.as_deref() == Some("Router") {
        (expr.children.first()?.text.as_deref()?, 2, 3)
    } else {
        let callee = expr.children.first()?;
        let method_name = router_receiver_method_name(callee)?;
        if !is_router_builder_receiver(callee.children.first()?) {
            return None;
        }
        (method_name, 1, 2)
    };
    if method_name == "fallback" {
        let handler = router_handler_name(expr.children.get(handler_index - 1)?)?;
        let source = Some(source_span_for_expr(source, expr));
        return Some(
            ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]
                .into_iter()
                .map(|method| WebHandlerArtifact {
                    method: method.to_string(),
                    route: "*".to_string(),
                    module: module_name.to_string(),
                    function: handler.to_string(),
                    arity: 1,
                    source: source.clone(),
                })
                .collect(),
        );
    }

    let method = match method_name {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "patch" => "PATCH",
        "delete" => "DELETE",
        "head" => "HEAD",
        "options" => "OPTIONS",
        _ => return None,
    };
    let route = router_route_literal(expr.children.get(route_index)?)?;
    let handler = router_handler_name(expr.children.get(handler_index)?)?;
    Some(vec![WebHandlerArtifact {
        method: method.to_string(),
        route,
        module: module_name.to_string(),
        function: handler.to_string(),
        arity: 1,
        source: Some(source_span_for_expr(source, expr)),
    }])
}

#[path = "routes/source_values.rs"]
mod source_values;
use source_values::{source_string_constants, websocket_from_source};
