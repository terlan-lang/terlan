use std::collections::HashMap;
use std::fs;

use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
};

use crate::commands::web_route::{route_ambiguity_key, route_param_types, validate_route_pattern};

use super::super::js::JsModuleArtifact;
use super::manifest::{
    WebErrorHandlerArtifact, WebFileResponseArtifact, WebHandlerArtifact, WebSocketArtifact,
    WebStaticResponseArtifact,
};
use super::WebRouteSourceArtifact;

mod helpers;
mod responses;
mod validation;

use validation::{
    apply_router_handler_arities, validate_discovered_web_handler_routes,
    validate_discovered_web_routes, validate_router_error_handler, validate_router_handler_rows,
    validate_router_middleware,
};

use helpers::{
    is_http_error_type, is_request_type, is_response_type, is_router_builder_receiver,
    prefix_web_route_manifest_rows, prefixed_router_route, route_source_context,
    router_group_body_expr, router_handler_name, router_middleware_from_expr,
    router_receiver_method_name, router_route_literal, source_span_for_expr, WebRouteSourceContext,
};
use responses::{file_response_from_handler, static_response_from_handler};

/// Discovered route-manifest rows for one browser package.
///
/// Inputs:
/// - Produced by route-manifest source extraction.
///
/// Output:
/// - Dynamic handler rows plus cacheable static and file response rows.
///
/// Transformation:
/// - Keeps compile-time constant responses out of the BEAM handler path while
///   preserving normal dynamic handlers for non-constant response functions.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct WebRouteManifestRows {
    pub(super) handlers: Vec<WebHandlerArtifact>,
    pub(super) websockets: Vec<WebSocketArtifact>,
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
/// - Classifies handler functions whose body is a constant `Response.text` or
///   `Response.html` builder as static responses and keeps all other supported
///   routes as dynamic BEAM-backed handlers.
pub(super) fn discover_web_route_manifest_from_sources(
    sources: &[WebRouteSourceArtifact],
) -> Result<WebRouteManifestRows, String> {
    let mut rows = WebRouteManifestRows::default();
    let string_constants = source_string_constants(sources)?;
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
        let signatures = router_handler_signatures(&syntax);
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
#[allow(dead_code)]
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
#[allow(dead_code)]
fn discover_web_handlers_from_sources(
    sources: &[WebRouteSourceArtifact],
) -> Result<Vec<WebHandlerArtifact>, String> {
    let mut handlers = Vec::new();
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
        let signatures = router_handler_signatures(&syntax);
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
                    &mut handlers,
                )?;
            }
        }
    }
    validate_discovered_web_handler_routes(&handlers)?;
    Ok(handlers)
}

/// Discovers an optional router-level error handler from route-source modules.
///
/// Inputs:
/// - `sources`: Terlan source modules known to contain HTTP router metadata.
///
/// Output:
/// - `Ok(Some(handler))` when a supported router error handler is found.
/// - `Ok(None)` when no error handler is found.
/// - Stable diagnostics for invalid handler declarations.
///
/// Transformation:
/// - Converts JS module artifacts to route-source artifacts before using the
///   route-source error-handler discovery path.
#[allow(dead_code)]
pub(super) fn discover_web_error_handler_from_modules(
    modules: &[JsModuleArtifact],
) -> Result<Option<WebErrorHandlerArtifact>, String> {
    let sources = modules
        .iter()
        .map(WebRouteSourceArtifact::from_js_module)
        .collect::<Vec<_>>();
    discover_web_error_handler_from_sources(&sources)
}

/// Discovers an optional router-level error handler from emitted source modules.
///
/// Inputs:
/// - `modules`: emitted JS module artifacts containing original source paths.
///
/// Output:
/// - `Ok(Some(handler))` when a supported `Router.error(handler)` call is
///   found.
/// - `Ok(None)` when no router-level error handler is declared.
/// - Stable `error[web_router]` diagnostics for duplicate or invalid handlers.
///
/// Transformation:
/// - Reparses source modules, walks `router` function bodies, extracts
///   `Router.error` calls, and validates the referenced local function has the
///   current `HttpError -> Response` shape.
pub(super) fn discover_web_error_handler_from_sources(
    sources: &[WebRouteSourceArtifact],
) -> Result<Option<WebErrorHandlerArtifact>, String> {
    let mut discovered: Option<WebErrorHandlerArtifact> = None;
    for source_artifact in sources {
        let source = fs::read_to_string(&source_artifact.source_path).map_err(|err| {
            format!(
                "cannot read source {} for web error handler discovery: {err}",
                source_artifact.source_path
            )
        })?;
        let syntax = parse_module_as_syntax_output(&source).map_err(|err| {
            format!(
                "cannot parse source {} for web error handler discovery: {err:?}",
                source_artifact.source_path
            )
        })?;
        let signatures = router_handler_signatures(&syntax);
        for declaration in &syntax.declarations {
            let SyntaxDeclarationPayload::Function { name, clauses, .. } = &declaration.payload
            else {
                continue;
            };
            if name != "router" {
                continue;
            }
            for clause in clauses {
                collect_router_error_handlers_from_expr(
                    &source_artifact.module,
                    &clause.body,
                    &signatures,
                    &mut discovered,
                )?;
            }
        }
    }
    Ok(discovered)
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
struct RouterHandlerSignature {
    arity: usize,
    param_names: Vec<String>,
    param_types: Vec<String>,
    return_type: String,
    body: Option<SyntaxExprOutput>,
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
                    body: clauses
                        .first()
                        .and_then(|clause| (clauses.len() == 1).then(|| clause.body.clone())),
                },
            ))
        })
        .collect()
}

fn source_string_constants(
    sources: &[WebRouteSourceArtifact],
) -> Result<HashMap<(String, String), String>, String> {
    let mut constants = HashMap::new();
    for source_artifact in sources {
        let source = fs::read_to_string(&source_artifact.source_path).map_err(|err| {
            format!(
                "cannot read source {} for web constant discovery: {err}",
                source_artifact.source_path
            )
        })?;
        let syntax = parse_module_as_syntax_output(&source).map_err(|err| {
            format!(
                "cannot parse source {} for web constant discovery: {err:?}",
                source_artifact.source_path
            )
        })?;
        for declaration in &syntax.declarations {
            let SyntaxDeclarationPayload::Function {
                name,
                params,
                clauses,
                ..
            } = &declaration.payload
            else {
                continue;
            };
            if !params.is_empty() || clauses.len() != 1 {
                continue;
            }
            if let Some(value) = string_literal_from_expr(&clauses[0].body) {
                constants.insert((source_artifact.module.clone(), name.clone()), value);
            }
        }
    }
    Ok(constants)
}

/// Builds a WebSocket artifact from a declarative source module.
///
/// Inputs:
/// - `source_artifact`: route-source module reference.
/// - `syntax`: parsed module syntax.
/// - `source`: source context used for spans.
/// - `constants`: known constant string functions from route-source modules.
///
/// Output:
/// - A WebSocket artifact, no artifact for non-WebSocket modules, or a stable
///   validation error.
///
/// Transformation:
/// - Reads constant `route()` and `protocol()` functions and converts them into
///   package metadata consumed by the runtime server.
fn websocket_from_source(
    source_artifact: &WebRouteSourceArtifact,
    syntax: &crate::terlan_syntax::SyntaxModuleOutput,
    source: &WebRouteSourceContext<'_>,
    constants: &HashMap<(String, String), String>,
) -> Result<Option<WebSocketArtifact>, String> {
    if !source_artifact.module.ends_with(".WebSocket") {
        return Ok(None);
    }
    let route_body = zero_arg_function_body(syntax, "route");
    let protocol_body = zero_arg_function_body(syntax, "protocol");
    if route_body.is_none() && protocol_body.is_none() {
        return Ok(None);
    }
    let route_body = route_body.ok_or_else(|| {
        format!(
            "error[web_router]: websocket module `{}` must define route(): String",
            source_artifact.module
        )
    })?;
    let protocol_body = protocol_body.ok_or_else(|| {
        format!(
            "error[web_router]: websocket module `{}` must define protocol(): String",
            source_artifact.module
        )
    })?;
    let route = constant_string_from_expr(route_body, constants).ok_or_else(|| {
        format!(
            "error[web_router]: websocket `{}` route() must return a constant string",
            source_artifact.module
        )
    })?;
    let protocol = constant_string_from_expr(protocol_body, constants).ok_or_else(|| {
        format!(
            "error[web_router]: websocket `{}` protocol() must return a constant string",
            source_artifact.module
        )
    })?;
    validate_route_pattern(&route)
        .map_err(|message| message.replacen("error[serve_package]", "error[web_router]", 1))?;
    if protocol.trim().is_empty() {
        return Err(format!(
            "error[web_router]: websocket `{}` protocol() cannot be empty",
            source_artifact.module
        ));
    }
    Ok(Some(WebSocketArtifact {
        route,
        protocol,
        source: Some(source_span_for_expr(source, route_body)),
    }))
}

/// Finds the body of a single-clause zero-argument function.
///
/// Inputs:
/// - `syntax`: parsed module syntax.
/// - `target`: function name to locate.
///
/// Output:
/// - Body expression when a matching simple function exists.
///
/// Transformation:
/// - Filters declarations to the route-metadata function shape.
fn zero_arg_function_body<'a>(
    syntax: &'a crate::terlan_syntax::SyntaxModuleOutput,
    target: &str,
) -> Option<&'a SyntaxExprOutput> {
    syntax.declarations.iter().find_map(|declaration| {
        let SyntaxDeclarationPayload::Function {
            name,
            params,
            clauses,
            ..
        } = &declaration.payload
        else {
            return None;
        };
        if name == target && params.is_empty() && clauses.len() == 1 {
            Some(&clauses[0].body)
        } else {
            None
        }
    })
}

/// Resolves a constant string expression used by route metadata.
///
/// Inputs:
/// - `expr`: expression that should evaluate to a route/protocol string.
/// - `constants`: cross-module constant string function map.
///
/// Output:
/// - Constant string value when it can be determined statically.
///
/// Transformation:
/// - Accepts direct string literals and remote calls to previously discovered
///   zero-argument string constants.
fn constant_string_from_expr(
    expr: &SyntaxExprOutput,
    constants: &HashMap<(String, String), String>,
) -> Option<String> {
    if let Some(value) = string_literal_from_expr(expr) {
        return Some(value);
    }
    if expr.kind != SyntaxExprKind::Call || expr.children.len() != 1 {
        return None;
    }
    let function = expr.children.first()?.text.as_deref()?;
    let remote = expr.remote.as_deref()?;
    remote_constant_string(remote, function, constants)
}

/// Extracts a string literal from a route metadata expression.
///
/// Inputs:
/// - `expr`: syntax expression to inspect.
///
/// Output:
/// - Literal string value when the expression is a supported string literal.
///
/// Transformation:
/// - Delegates to the shared router route-literal parser.
fn string_literal_from_expr(expr: &SyntaxExprOutput) -> Option<String> {
    router_route_literal(expr)
}

/// Resolves a remote zero-argument constant string function.
///
/// Inputs:
/// - `remote`: remote module segment from the call.
/// - `function`: called function name.
/// - `constants`: discovered module/function constant string table.
///
/// Output:
/// - Constant string when exactly one matching remote function exists.
///
/// Transformation:
/// - Supports exact module matches and unambiguous suffix matches for local
///   route-source modules.
fn remote_constant_string(
    remote: &str,
    function: &str,
    constants: &HashMap<(String, String), String>,
) -> Option<String> {
    if let Some(value) = constants.get(&(remote.to_string(), function.to_string())) {
        return Some(value.clone());
    }
    let suffix = format!(".{remote}");
    let mut matches = constants
        .iter()
        .filter(|((module, name), _)| name == function && module.ends_with(&suffix))
        .map(|(_, value)| value.clone());
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// Recursively collects route-builder calls and classifies static responses.
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
/// - Reuses route-builder extraction, then moves handlers with constant
///   response bodies into `static_responses`.
fn collect_router_routes_from_expr(
    module_name: &str,
    expr: &SyntaxExprOutput,
    source: &WebRouteSourceContext<'_>,
    signatures: &HashMap<String, RouterHandlerSignature>,
    rows: &mut WebRouteManifestRows,
) -> Result<(), String> {
    if let Some(middleware) = router_middleware_from_expr(expr) {
        validate_router_middleware(module_name, middleware, signatures)?;
    }
    if let Some((prefix, body)) = router_group_body_expr(expr) {
        let mut grouped_rows = WebRouteManifestRows::default();
        collect_router_routes_from_expr(module_name, body, source, signatures, &mut grouped_rows)?;
        prefix_web_route_manifest_rows(&prefix, &mut grouped_rows);
        rows.handlers.append(&mut grouped_rows.handlers);
        rows.websockets.append(&mut grouped_rows.websockets);
        rows.static_responses
            .append(&mut grouped_rows.static_responses);
        rows.file_responses.append(&mut grouped_rows.file_responses);
        return Ok(());
    }
    if let Some(handlers) = router_handler_from_expr(module_name, expr, source) {
        validate_router_handler_rows(module_name, &handlers, signatures)?;
        let mut handlers = handlers;
        apply_router_handler_arities(&mut handlers, signatures);
        for handler in handlers {
            if let Some(response) = static_response_from_handler(&handler, signatures) {
                rows.static_responses.push(response);
            } else if let Some(response) = file_response_from_handler(&handler, signatures) {
                rows.file_responses.push(response);
            } else {
                rows.handlers.push(handler);
            }
        }
    }
    for child in &expr.children {
        collect_router_routes_from_expr(module_name, child, source, signatures, rows)?;
    }
    Ok(())
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
#[allow(dead_code)]
fn collect_router_handlers_from_expr(
    module_name: &str,
    expr: &SyntaxExprOutput,
    source: &WebRouteSourceContext<'_>,
    signatures: &HashMap<String, RouterHandlerSignature>,
    handlers: &mut Vec<WebHandlerArtifact>,
) -> Result<(), String> {
    if let Some(middleware) = router_middleware_from_expr(expr) {
        validate_router_middleware(module_name, middleware, signatures)?;
    }
    if let Some((prefix, body)) = router_group_body_expr(expr) {
        let mut grouped_handlers = Vec::new();
        collect_router_handlers_from_expr(
            module_name,
            body,
            source,
            signatures,
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
        handlers.extend(handler);
    }
    for child in &expr.children {
        collect_router_handlers_from_expr(module_name, child, source, signatures, handlers)?;
    }
    Ok(())
}

/// Recursively collects router-level error-handler calls from a syntax tree.
///
/// Inputs:
/// - `module_name`: Terlan module that owns the error handler function.
/// - `expr`: syntax expression to inspect.
/// - `signatures`: local function signature map.
/// - `discovered`: mutable slot for the single supported router error handler.
///
/// Output:
/// - `Ok(())` when zero or one valid error handler is found.
/// - Stable diagnostic for duplicate or invalid handler shape.
///
/// Transformation:
/// - Walks expression children and records recognized `Router.error(...)`
///   calls without evaluating the router builder value.
fn collect_router_error_handlers_from_expr(
    module_name: &str,
    expr: &SyntaxExprOutput,
    signatures: &HashMap<String, RouterHandlerSignature>,
    discovered: &mut Option<WebErrorHandlerArtifact>,
) -> Result<(), String> {
    if let Some(handler) = router_error_handler_from_expr(module_name, expr) {
        validate_router_error_handler(module_name, &handler, signatures)?;
        if discovered.replace(handler).is_some() {
            return Err(
                "error[web_router]: duplicate router-level error handler declaration".to_string(),
            );
        }
    }
    for child in &expr.children {
        collect_router_error_handlers_from_expr(module_name, child, signatures, discovered)?;
    }
    Ok(())
}

/// Validates handler rows before they enter the web manifest.
///
/// Inputs:
/// - `module_name`: Terlan module that owns the handler functions.
/// - `handlers`: one or more rows produced from a router builder call.
/// - `signatures`: local function signature map.
///
/// Output:
/// - `Ok(())` when every handler target is a local `Request -> Response`
///   function.
/// - Stable error for missing, wrong-arity, wrong-request, or wrong-return
///   handlers.
///
/// Transformation:
/// - Checks only local function declarations; richer imported handler and
///   higher-order handler validation remains future compiler work.

/// Converts a constant response handler into a static manifest row.
///
/// Inputs:
/// - `handler`: route row referencing a local handler function.
/// - `signatures`: local function signatures and single-clause bodies.
///
/// Output:
/// - Static response row for supported constant response builder bodies.
/// - `None` for dynamic handlers or unsupported response builders.
///
/// Transformation:
/// - Recognizes `Response.text("body", status = 200)` and
///   `Response.html("body", status = 200)` without evaluating arbitrary code.
/// Transformation:
/// - Performs conservative textual recognition until route extraction is wired
///   through the full resolved typechecker.
///   generation remains deterministic before higher-order router values land.
fn router_error_handler_from_expr(
    module_name: &str,
    expr: &SyntaxExprOutput,
) -> Option<WebErrorHandlerArtifact> {
    if expr.kind != SyntaxExprKind::Call {
        return None;
    }
    let (method_name, handler_index) = if expr.remote.as_deref() == Some("Router") {
        (expr.children.first()?.text.as_deref()?, 2)
    } else {
        let callee = expr.children.first()?;
        let method_name = router_receiver_method_name(callee)?;
        if !is_router_builder_receiver(callee.children.first()?) {
            return None;
        }
        (method_name, 1)
    };
    if method_name != "error" {
        return None;
    }
    let handler = router_handler_name(expr.children.get(handler_index)?)?;
    Some(WebErrorHandlerArtifact {
        module: module_name.to_string(),
        function: handler.to_string(),
        arity: 1,
    })
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
