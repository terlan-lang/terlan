use std::collections::HashMap;

use crate::terlan_syntax::{SyntaxExprKind, SyntaxExprOutput};

use super::super::manifest::{
    WebFileResponseArtifact, WebHandlerArtifact, WebResponseHeaderArtifact,
    WebStaticResponseArtifact,
};
use super::helpers::router_route_literal;
use super::RouterHandlerSignature;

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
/// - Recognizes `Response.text`, `Response.html`, and `Response.redirect`
///   bodies without evaluating arbitrary code.
pub(super) fn static_response_from_handler(
    handler: &WebHandlerArtifact,
    signatures: &HashMap<String, RouterHandlerSignature>,
) -> Option<WebStaticResponseArtifact> {
    let body = signatures.get(&handler.function)?.body.as_ref()?;
    let response = constant_response_from_expr(body)?;
    Some(WebStaticResponseArtifact {
        method: handler.method.clone(),
        route: handler.route.clone(),
        status: response.status,
        content_type: response.content_type,
        headers: response.headers,
        body: response.body,
        source: handler.source.clone(),
    })
}

/// Converts a constant file response handler into a file manifest row.
///
/// Inputs:
/// - `handler`: route row referencing a local handler function.
/// - `signatures`: local function signatures and single-clause bodies.
///
/// Output:
/// - File response row for supported constant file response builder bodies.
/// - `None` for dynamic handlers or unsupported response builders.
///
/// Transformation:
/// - Recognizes `Response.file("path", status = 200, content_type = "...")`
///   without evaluating arbitrary code.
pub(super) fn file_response_from_handler(
    handler: &WebHandlerArtifact,
    signatures: &HashMap<String, RouterHandlerSignature>,
) -> Option<WebFileResponseArtifact> {
    let body = signatures.get(&handler.function)?.body.as_ref()?;
    let response = constant_file_response_from_expr(body)?;
    Some(WebFileResponseArtifact {
        method: handler.method.clone(),
        route: handler.route.clone(),
        path: response.path,
        status: response.status,
        content_type: response.content_type,
        source: handler.source.clone(),
    })
}

/// Compile-time constant HTTP response extracted from a handler body.
///
/// Inputs:
/// - Produced by `constant_response_from_expr`.
///
/// Output:
/// - Status, content type, and body bytes represented as UTF-8 text.
///
/// Transformation:
/// - Keeps the intermediate local to route discovery so manifest serialization
///   remains the only public boundary.
struct ConstantResponse {
    status: u16,
    content_type: String,
    headers: Vec<WebResponseHeaderArtifact>,
    body: String,
}

/// Compile-time constant HTTP file response extracted from a handler body.
///
/// Inputs:
/// - Produced by `constant_file_response_from_expr`.
///
/// Output:
/// - Package-relative file path, status, and optional content type.
///
/// Transformation:
/// - Keeps file-response extraction local to route discovery so manifest
///   serialization remains the only public boundary.
struct ConstantFileResponse {
    path: String,
    status: u16,
    content_type: Option<String>,
}

/// Extracts a constant `std.http.Response` builder call.
///
/// Inputs:
/// - `expr`: handler body expression.
///
/// Output:
/// - Static response data for supported response builders.
/// - `None` when the handler must stay dynamic.
///
/// Transformation:
/// - Requires a direct `Response.text`, `Response.html`, or
///   `Response.redirect` call with literal metadata and optional integer
///   `status` argument.
fn constant_response_from_expr(expr: &SyntaxExprOutput) -> Option<ConstantResponse> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.as_deref() != Some("Response") {
        return None;
    }
    let builder = expr.children.first()?.text.as_deref()?;
    let (content_type, default_status, body, headers) = match builder {
        "text" => (
            "text/plain; charset=utf-8",
            200,
            router_route_literal(expr.children.get(1)?)?,
            Vec::new(),
        ),
        "html" => (
            "text/html; charset=utf-8",
            200,
            router_route_literal(expr.children.get(1)?)?,
            Vec::new(),
        ),
        "redirect" => {
            let location = router_route_literal(expr.children.get(1)?)?;
            (
                "text/plain; charset=utf-8",
                302,
                String::new(),
                vec![WebResponseHeaderArtifact {
                    name: "Location".to_string(),
                    value: location,
                }],
            )
        }
        _ => return None,
    };
    let status = constant_response_status(expr, default_status)?;
    Some(ConstantResponse {
        status,
        content_type: content_type.to_string(),
        headers,
        body,
    })
}

/// Extracts a constant `Response.file` builder call.
///
/// Inputs:
/// - `expr`: handler body expression.
///
/// Output:
/// - File response data for supported file response builders.
/// - `None` when the handler must stay dynamic.
///
/// Transformation:
/// - Requires a direct `Response.file` call with a string literal path,
///   optional integer `status`, and optional string `content_type`.
fn constant_file_response_from_expr(expr: &SyntaxExprOutput) -> Option<ConstantFileResponse> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.as_deref() != Some("Response") {
        return None;
    }
    let builder = expr.children.first()?.text.as_deref()?;
    if builder != "file" {
        return None;
    }
    let path = router_route_literal(expr.children.get(1)?)?;
    let (status, content_type) = constant_file_response_args(expr)?;
    Some(ConstantFileResponse {
        path,
        status,
        content_type,
    })
}

/// Extracts constant file response optional arguments.
///
/// Inputs:
/// - `expr`: `Response.file` call expression.
///
/// Output:
/// - HTTP status plus optional content type override.
/// - `None` for unsupported argument shapes.
///
/// Transformation:
/// - Accepts positional `path, status, content_type` and named `status = ...`
///   / `content_type = ...` forms without evaluating arbitrary expressions.
fn constant_file_response_args(expr: &SyntaxExprOutput) -> Option<(u16, Option<String>)> {
    let mut status = None;
    let mut content_type = None;
    for index in 2..expr.children.len() {
        let name = call_arg_name(expr, index);
        match (name, index) {
            (Some("status"), _) => {
                if status
                    .replace(constant_response_status_literal(&expr.children[index])?)
                    .is_some()
                {
                    return None;
                }
            }
            (Some("content_type"), _) => {
                if content_type
                    .replace(router_route_literal(&expr.children[index])?)
                    .is_some()
                {
                    return None;
                }
            }
            (Some(_), _) => return None,
            (None, 2) => {
                if status
                    .replace(constant_response_status_literal(&expr.children[index])?)
                    .is_some()
                {
                    return None;
                }
            }
            (None, 3) => {
                if content_type
                    .replace(router_route_literal(&expr.children[index])?)
                    .is_some()
                {
                    return None;
                }
            }
            (None, _) => return None,
        }
    }
    Some((status.unwrap_or(200), content_type))
}

/// Extracts an optional constant response status argument.
///
/// Inputs:
/// - `expr`: constant response builder call expression.
/// - `default_status`: builder-specific default status code.
///
/// Output:
/// - Explicit status value or the provided default status.
/// - `None` when the status argument is not a supported literal integer.
///
/// Transformation:
/// - Accepts positional second argument and named `status = ...`, rejecting
///   other named arguments by declining static-response classification.
fn constant_response_status(expr: &SyntaxExprOutput, default_status: u16) -> Option<u16> {
    if expr.children.len() <= 2 {
        return Some(default_status);
    }
    let mut status = None;
    for index in 2..expr.children.len() {
        let name = call_arg_name(expr, index);
        if let Some(name) = name {
            if name != "status" {
                return None;
            }
        }
        if status
            .replace(constant_response_status_literal(&expr.children[index])?)
            .is_some()
        {
            return None;
        }
    }
    status.or(Some(default_status))
}

/// Returns the source argument name for one call child index.
///
/// Inputs:
/// - `expr`: call expression syntax output.
/// - `child_index`: index into `expr.children`, where child `0` is the callee
///   expression and child `1` is the first source argument.
///
/// Output:
/// - Named argument label when present.
/// - `None` for positional arguments, missing metadata, and the callee child.
///
/// Transformation:
/// - Translates from syntax-output child indexes into source argument indexes
///   so route-manifest extraction handles named arguments consistently.
fn call_arg_name(expr: &SyntaxExprOutput, child_index: usize) -> Option<&str> {
    child_index
        .checked_sub(1)
        .and_then(|arg_index| expr.arg_names.get(arg_index))
        .and_then(|name| name.as_deref())
}

/// Converts one integer literal into an HTTP status code.
///
/// Inputs:
/// - `expr`: candidate status expression.
///
/// Output:
/// - `Some(status)` when the expression is an HTTP-range integer literal.
/// - `None` otherwise.
///
/// Transformation:
/// - Parses syntax-output integer text without accepting arbitrary expressions.
fn constant_response_status_literal(expr: &SyntaxExprOutput) -> Option<u16> {
    if expr.kind != SyntaxExprKind::Int {
        return None;
    }
    let status = expr.text.as_deref()?.parse::<u16>().ok()?;
    (100..=599).contains(&status).then_some(status)
}
