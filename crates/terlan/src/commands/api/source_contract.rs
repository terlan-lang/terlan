//! Static API/deployment discovery adapter. This does not execute Router source
//! or resolve computed routes; serving uses the package's evaluated descriptor.

use terlan_http_native::api_contract::{ApiContract, ApiRoute};

use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxImportKind, SyntaxModuleOutput,
};

use super::router_syntax::router_receiver_method_name;

pub(crate) fn from_router_source(
    source: &str,
    service_name: impl Into<String>,
    version: impl Into<String>,
) -> Result<ApiContract, String> {
    let syntax = parse_module_as_syntax_output(source)
        .map_err(|err| format!("error[api_emit]: cannot parse API source: {err:?}"))?;
    if !imports_std_http_router(&syntax) {
        return Err(format!(
            "error[api_emit]: module `{}` must import std.http.Router for API route extraction",
            syntax.module_name
        ));
    }
    Ok(ApiContract::from_routes(
        service_name,
        version,
        routes_from_syntax_module(&syntax)?,
    ))
}

/// Extracts API routes from one parsed module.
///
/// Inputs:
/// - `syntax`: parsed Terlan syntax output.
///
/// Output:
/// - Route rows discovered from public `router` functions.
///
/// Transformation:
/// - Walks each router body and recognizes direct static and receiver-style
///   `std.http.Router` builder calls.
pub(crate) fn routes_from_syntax_module(
    syntax: &SyntaxModuleOutput,
) -> Result<Vec<ApiRoute>, String> {
    let mut routes = Vec::new();
    for declaration in &syntax.declarations {
        let SyntaxDeclarationPayload::Function {
            name,
            is_public,
            clauses,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if name != "router" || !is_public {
            continue;
        }
        for clause in clauses {
            collect_routes_from_expr(&clause.body, &mut routes)?;
        }
    }
    Ok(routes)
}

/// Recursively collects route-builder calls from an expression.
///
/// Inputs:
/// - `expr`: syntax expression candidate.
/// - `routes`: mutable route output buffer.
///
/// Output:
/// - `Ok(())` after all recognized calls are collected.
///
/// Transformation:
/// - Handles router groups by applying their prefix to nested route rows, then
///   walks children so chained calls and let bodies are covered.
fn collect_routes_from_expr(
    expr: &SyntaxExprOutput,
    routes: &mut Vec<ApiRoute>,
) -> Result<(), String> {
    if let Some((prefix, body)) = router_group_body_expr(expr) {
        let mut grouped = Vec::new();
        collect_routes_from_expr(body, &mut grouped)?;
        for route in &mut grouped {
            route.path = prefixed_router_route(&prefix, &route.path);
        }
        routes.append(&mut grouped);
        return Ok(());
    }
    if let Some(mut route) = route_from_expr(expr)? {
        routes.append(&mut route);
    }
    for child in &expr.children {
        collect_routes_from_expr(child, routes)?;
    }
    Ok(())
}

/// Converts one router builder call into route rows.
///
/// Inputs:
/// - `expr`: syntax expression candidate.
///
/// Output:
/// - Route rows for supported builder calls, or `None` for unrelated
///   expressions.
///
/// Transformation:
/// - Reads route literal and handler identifier from syntax output. Fallback
///   routes expand across common HTTP methods because OpenAPI has no wildcard
///   method operation.
fn route_from_expr(expr: &SyntaxExprOutput) -> Result<Option<Vec<ApiRoute>>, String> {
    if expr.kind != SyntaxExprKind::Call {
        return Ok(None);
    }
    let (method_name, route_index, handler_index) = if expr.remote.as_deref() == Some("Router") {
        (
            expr.children
                .first()
                .and_then(|child| child.text.as_deref()),
            2,
            3,
        )
    } else {
        let Some(callee) = expr.children.first() else {
            return Ok(None);
        };
        let method_name = router_receiver_method_name(callee);
        if !callee
            .children
            .first()
            .is_some_and(is_router_builder_receiver)
        {
            return Ok(None);
        }
        (method_name, 1, 2)
    };
    let Some(method_name) = method_name else {
        return Ok(None);
    };
    if method_name == "fallback" {
        let Some(handler) = expr
            .children
            .get(handler_index - 1)
            .and_then(router_handler_name)
        else {
            return Err(
                "error[api_emit]: Router.fallback requires a handler reference".to_string(),
            );
        };
        return Ok(Some(
            ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]
                .into_iter()
                .map(|method| ApiRoute {
                    method: method.to_string(),
                    path: "/*".to_string(),
                    handler: handler.to_string(),
                })
                .collect(),
        ));
    }

    let method = match method_name {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "patch" => "PATCH",
        "delete" => "DELETE",
        "head" => "HEAD",
        "options" => "OPTIONS",
        "new" | "use" | "map_response" | "error" | "overload" | "lifecycle" => {
            return Ok(None);
        }
        _ => return Ok(None),
    };
    let Some(route) = expr
        .children
        .get(route_index)
        .and_then(router_route_literal)
    else {
        return Err(format!(
            "error[api_emit]: Router.{method_name} requires a literal route path"
        ));
    };
    let Some(handler) = expr
        .children
        .get(handler_index)
        .and_then(router_handler_name)
    else {
        return Err(format!(
            "error[api_emit]: Router.{method_name} requires a handler reference"
        ));
    };
    Ok(Some(vec![ApiRoute {
        method: method.to_string(),
        path: route,
        handler: handler.to_string(),
    }]))
}

/// Returns whether a source module imports `std.http.Router`.
pub(crate) fn imports_std_http_router(syntax: &SyntaxModuleOutput) -> bool {
    syntax.declarations.iter().any(|declaration| {
        matches!(
            &declaration.payload,
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                items,
                is_selected,
                ..
            } if module_name == "std.http.Router"
                || (module_name == "std.http"
                    && *is_selected
                    && items.iter().any(|item| item.name == "Router"))
        )
    })
}

/// Extracts a router group call and its lambda body.
fn router_group_body_expr(expr: &SyntaxExprOutput) -> Option<(String, &SyntaxExprOutput)> {
    if expr.kind != SyntaxExprKind::Call {
        return None;
    }
    let (method_name, prefix_index, configure_index) = if expr.remote.as_deref() == Some("Router") {
        (expr.children.first()?.text.as_deref()?, 2, 3)
    } else {
        let callee = expr.children.first()?;
        let method_name = router_receiver_method_name(callee)?;
        if !callee
            .children
            .first()
            .is_some_and(is_router_builder_receiver)
        {
            return None;
        }
        (method_name, 1, 2)
    };
    if method_name != "group" {
        return None;
    }
    let prefix = router_route_literal(expr.children.get(prefix_index)?)?;
    let configure = expr.children.get(configure_index)?;
    if configure.kind != SyntaxExprKind::Fun || configure.clauses.len() != 1 {
        return None;
    }
    Some((prefix, configure.clauses[0].body.as_ref()))
}

/// Returns whether a receiver expression is router-builder shaped.
fn is_router_builder_receiver(receiver: &SyntaxExprOutput) -> bool {
    if receiver.kind == SyntaxExprKind::Var && receiver.text.as_deref() == Some("router") {
        return true;
    }
    if receiver.kind != SyntaxExprKind::Call {
        return false;
    }
    if receiver.remote.as_deref() == Some("Router") {
        return receiver
            .children
            .first()
            .and_then(|child| child.text.as_deref())
            .is_some_and(|name| {
                matches!(
                    name,
                    "new"
                        | "get"
                        | "post"
                        | "put"
                        | "patch"
                        | "delete"
                        | "head"
                        | "options"
                        | "fallback"
                        | "group"
                )
            });
    }
    receiver.children.first().is_some_and(|callee| {
        router_receiver_method_name(callee).is_some()
            && callee
                .children
                .first()
                .is_some_and(is_router_builder_receiver)
    })
}

/// Extracts a route pattern from a syntax string literal.
fn router_route_literal(expr: &SyntaxExprOutput) -> Option<String> {
    if expr.kind != SyntaxExprKind::Binary {
        return None;
    }
    serde_json::from_str(expr.text.as_deref()?).ok()
}

/// Extracts a direct local handler function name.
fn router_handler_name(expr: &SyntaxExprOutput) -> Option<&str> {
    if expr.kind != SyntaxExprKind::Var {
        return None;
    }
    expr.text.as_deref()
}

/// Combines a group prefix with a nested route pattern.
fn prefixed_router_route(prefix: &str, route: &str) -> String {
    let normalized_prefix = if prefix == "/" {
        "/"
    } else {
        prefix.trim_end_matches('/')
    };
    if route == "*" || route == "/*" {
        return if normalized_prefix == "/" {
            "/*".to_string()
        } else {
            format!("{normalized_prefix}/*")
        };
    }
    if route == "/" {
        return normalized_prefix.to_string();
    }
    if normalized_prefix == "/" {
        return route.to_string();
    }
    if route.starts_with('/') {
        format!("{normalized_prefix}{route}")
    } else {
        format!("{normalized_prefix}/{route}")
    }
}

#[cfg(test)]
#[path = "source_contract_test.rs"]
mod tests;
