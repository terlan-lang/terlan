use std::collections::BTreeSet;

use super::*;

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
pub(super) fn validate_router_handler_rows(
    module_name: &str,
    handlers: &[WebHandlerArtifact],
    signatures: &HashMap<String, RouterHandlerSignature>,
) -> Result<(), String> {
    for handler in handlers {
        validate_route_pattern(&handler.route)
            .map_err(|message| message.replacen("error[serve_package]", "error[web_router]", 1))?;
        let Some(signature) = signatures.get(&handler.function) else {
            return Err(format!(
                "error[web_router]: handler `{}` referenced by `{}` `{}` is not defined in module `{}`",
                handler.function, handler.method, handler.route, module_name
            ));
        };
        let route_params = route_param_types(&handler.route)
            .map_err(|message| message.replacen("error[serve_package]", "error[web_router]", 1))?;
        let route_param_count = route_params.len();
        let expected_with_params = 1 + route_param_count;
        if signature.arity != 1 && signature.arity != expected_with_params {
            return Err(format!(
                "error[web_router]: handler `{}` referenced by `{}` `{}` must accept Request or Request plus {} route parameter(s), got arity {}",
                handler.function, handler.method, handler.route, route_param_count, signature.arity
            ));
        }
        let Some(request_type) = signature.param_types.first() else {
            return Err(format!(
                "error[web_router]: handler `{}` referenced by `{}` `{}` must accept Request",
                handler.function, handler.method, handler.route
            ));
        };
        if !is_request_type(request_type) {
            return Err(format!(
                "error[web_router]: handler `{}` referenced by `{}` `{}` must accept Request as parameter 1, got `{request_type}`",
                handler.function, handler.method, handler.route
            ));
        }
        if signature.arity == expected_with_params {
            validate_route_handler_param_types(handler, signature, &route_params)?;
        }
        if !is_response_type(&signature.return_type) {
            return Err(format!(
                "error[web_router]: handler `{}` referenced by `{}` `{}` must return Response, got `{}`",
                handler.function, handler.method, handler.route, signature.return_type
            ));
        }
    }
    Ok(())
}

/// Validates direct route-param handler parameter types.
///
/// Inputs:
/// - `handler`: route manifest row being validated.
/// - `signature`: local handler function signature.
/// - `route_params`: ordered route capture names and types.
///
/// Output:
/// - `Ok(())` when each handler parameter after `Request` matches the route
///   capture name and type.
/// - Stable `error[web_router]` diagnostic otherwise.
///
/// Transformation:
/// - Keeps typed route params meaningful at build time by comparing the source
///   route contract against the handler's declared parameter names and
///   annotations.
fn validate_route_handler_param_types(
    handler: &WebHandlerArtifact,
    signature: &RouterHandlerSignature,
    route_params: &[(String, String)],
) -> Result<(), String> {
    for (index, ((name, route_type), (handler_name, handler_type))) in route_params
        .iter()
        .zip(
            signature
                .param_names
                .iter()
                .skip(1)
                .zip(signature.param_types.iter().skip(1)),
        )
        .enumerate()
    {
        if handler_name != name {
            return Err(format!(
                "error[web_router]: handler `{}` parameter {} for route `{}` capture `{}` must be named `{}`, got `{}`",
                handler.function,
                index + 2,
                handler.route,
                name,
                name,
                handler_name
            ));
        }
        if !is_route_param_type_compatible(handler_type, route_type) {
            return Err(format!(
                "error[web_router]: handler `{}` parameter {} for route `{}` capture `{}` must be `{}`, got `{}`",
                handler.function,
                index + 2,
                handler.route,
                name,
                route_type,
                handler_type
            ));
        }
    }
    Ok(())
}

/// Returns whether a handler parameter type accepts one route capture type.
///
/// Inputs:
/// - `handler_type`: source annotation text from the handler signature.
/// - `route_type`: type text declared or implied by the route pattern.
///
/// Output:
/// - `true` when the final type segment matches.
///
/// Transformation:
/// - Compares final path segments so local aliases like `Int` and fully
///   qualified summaries such as `std.core.Int` can share the same route
///   parameter contract.
fn is_route_param_type_compatible(handler_type: &str, route_type: &str) -> bool {
    final_type_segment(handler_type) == final_type_segment(route_type)
}

/// Returns the final dot-qualified type segment.
///
/// Inputs:
/// - `type_text`: source type annotation text.
///
/// Output:
/// - Final type name segment.
///
/// Transformation:
/// - Trims whitespace and splits on `.` for lightweight route handler
///   compatibility checks without invoking full type resolution.
fn final_type_segment(type_text: &str) -> &str {
    type_text
        .trim()
        .rsplit('.')
        .next()
        .unwrap_or(type_text.trim())
}

/// Copies validated local handler arities into generated route manifest rows.
///
/// Inputs:
/// - `handlers`: route rows produced by router-builder extraction.
/// - `signatures`: local function signatures collected from the source module.
///
/// Output:
/// - No return value; rows are updated in place.
///
/// Transformation:
/// - Replaces the parser-level placeholder arity with the real function arity
///   after validation has proved the handler exists and matches the supported
///   route-handler shape.
pub(super) fn apply_router_handler_arities(
    handlers: &mut [WebHandlerArtifact],
    signatures: &HashMap<String, RouterHandlerSignature>,
) {
    for handler in handlers {
        if let Some(signature) = signatures.get(&handler.function) {
            handler.arity = signature.arity;
        }
    }
}

/// Validates one router middleware callback reference.
///
/// Inputs:
/// - `module_name`: Terlan module that owns the middleware function.
/// - `middleware`: callback name extracted from `Router.use`.
/// - `signatures`: local function signature map.
///
/// Output:
/// - `Ok(())` when the middleware has `Request -> Response` shape.
/// - Stable `error[web_router]` diagnostic otherwise.
///
/// Transformation:
/// - Checks only local function declarations until imported middleware
///   validation is routed through the full typechecker.
pub(super) fn validate_router_middleware(
    module_name: &str,
    middleware: &str,
    signatures: &HashMap<String, RouterHandlerSignature>,
) -> Result<(), String> {
    let Some(signature) = signatures.get(middleware) else {
        return Err(format!(
            "error[web_router]: middleware `{middleware}` is not defined in module `{module_name}`"
        ));
    };
    if signature.arity != 1 {
        return Err(format!(
            "error[web_router]: middleware `{middleware}` must accept one Request, got arity {}",
            signature.arity
        ));
    }
    let Some(request_type) = signature.param_types.first() else {
        return Err(format!(
            "error[web_router]: middleware `{middleware}` must accept Request"
        ));
    };
    if !is_request_type(request_type) {
        return Err(format!(
            "error[web_router]: middleware `{middleware}` must accept Request, got `{request_type}`"
        ));
    }
    if !is_response_type(&signature.return_type) {
        return Err(format!(
            "error[web_router]: middleware `{middleware}` must return Response, got `{}`",
            signature.return_type
        ));
    }
    Ok(())
}

/// Validates a router-level error handler before manifest serialization.
///
/// Inputs:
/// - `module_name`: Terlan module that owns the error handler function.
/// - `handler`: extracted error handler manifest row.
/// - `signatures`: local function signature map.
///
/// Output:
/// - `Ok(())` when the handler has `HttpError -> Response` shape.
/// - Stable `error[web_router]` diagnostic otherwise.
///
/// Transformation:
/// - Checks only local function declarations in the source module until richer
///   imported handler validation is routed through the full typechecker.
pub(super) fn validate_router_error_handler(
    module_name: &str,
    handler: &WebErrorHandlerArtifact,
    signatures: &HashMap<String, RouterHandlerSignature>,
) -> Result<(), String> {
    let Some(signature) = signatures.get(&handler.function) else {
        return Err(format!(
            "error[web_router]: error handler `{}` is not defined in module `{}`",
            handler.function, module_name
        ));
    };
    if signature.arity != 1 {
        return Err(format!(
            "error[web_router]: error handler `{}` must accept one HttpError, got arity {}",
            handler.function, signature.arity
        ));
    }
    let Some(param_type) = signature.param_types.first() else {
        return Err(format!(
            "error[web_router]: error handler `{}` must accept HttpError",
            handler.function
        ));
    };
    if !is_http_error_type(param_type) {
        return Err(format!(
            "error[web_router]: error handler `{}` must accept HttpError, got `{param_type}`",
            handler.function
        ));
    }
    if !is_response_type(&signature.return_type) {
        return Err(format!(
            "error[web_router]: error handler `{}` must return Response, got `{}`",
            handler.function, signature.return_type
        ));
    }
    Ok(())
}

/// Validates the complete discovered route set.
///
/// Inputs:
/// - `handlers`: all handler rows collected from source router builders.
///
/// Output:
/// - `Ok(())` when no method/route pair is duplicated or ambiguous.
/// - Stable `error[web_router]` diagnostic otherwise.
///
/// Transformation:
/// - Reuses the serve-side ambiguity model so build artifacts cannot encode
///   same-shape routes such as `/users/:id` and `/users/:name` for one method.
pub(super) fn validate_discovered_web_routes(rows: &WebRouteManifestRows) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for (method, route, kind) in rows
        .handlers
        .iter()
        .map(|handler| {
            (
                handler.method.as_str(),
                handler.route.as_str(),
                "handler route",
            )
        })
        .chain(
            rows.websockets
                .iter()
                .map(|websocket| ("GET", websocket.route.as_str(), "websocket route")),
        )
        .chain(rows.static_responses.iter().map(|response| {
            (
                response.method.as_str(),
                response.route.as_str(),
                "static response route",
            )
        }))
        .chain(rows.file_responses.iter().map(|response| {
            (
                response.method.as_str(),
                response.route.as_str(),
                "file response route",
            )
        }))
    {
        let key = (
            method,
            route_ambiguity_key(route).map_err(|message| {
                message.replacen("error[serve_package]", "error[web_router]", 1)
            })?,
        );
        if !seen.insert(key) {
            return Err(format!(
                "error[web_router]: duplicate or ambiguous {kind} `{method}` `{route}`"
            ));
        }
    }
    Ok(())
}

/// Validates dynamic handler routes for compatibility with earlier tests.
///
/// Inputs:
/// - `handlers`: dynamic handler rows collected from source router builders.
///
/// Output:
/// - `Ok(())` when no method/route pair is duplicated or ambiguous.
/// - Stable `error[web_router]` diagnostic otherwise.
///
/// Transformation:
/// - Wraps the dynamic rows in the newer route-manifest row container.
#[allow(dead_code)]
pub(super) fn validate_discovered_web_handler_routes(
    handlers: &[WebHandlerArtifact],
) -> Result<(), String> {
    validate_discovered_web_routes(&WebRouteManifestRows {
        handlers: handlers.to_vec(),
        websockets: Vec::new(),
        static_responses: Vec::new(),
        file_responses: Vec::new(),
    })
}
