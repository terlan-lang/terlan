use std::path::{Component, Path};

use crate::terlan_syntax::{SyntaxExprKind, SyntaxExprOutput};

use super::super::manifest::WebSourceSpanArtifact;
use super::super::WebRouteSourceArtifact;
use super::WebRouteManifestRows;

/// Source metadata used while extracting route manifests.
///
/// Inputs:
/// - Produced from one route-source artifact and its source text.
///
/// Output:
/// - Package-safe source path and borrowed source text for span conversion.
///
/// Transformation:
/// - Separates absolute build paths from runtime-visible source metadata so
///   generated manifests can point diagnostics at source files without leaking
///   local machine paths.
pub(super) struct WebRouteSourceContext<'a> {
    pub(super) manifest_path: String,
    pub(super) source_text: &'a str,
}

/// Builds a route source context for manifest diagnostics.
///
/// Inputs:
/// - `source`: route source artifact produced by build discovery.
/// - `source_text`: source file contents used for line/column mapping.
///
/// Output:
/// - `WebRouteSourceContext` with package-safe manifest path and borrowed text.
///
/// Transformation:
/// - Converts the absolute source path into a stable manifest path while
///   preserving source text for span conversion.
pub(super) fn route_source_context<'a>(
    source: &WebRouteSourceArtifact,
    source_text: &'a str,
) -> WebRouteSourceContext<'a> {
    WebRouteSourceContext {
        manifest_path: route_source_manifest_path(&source.source_path),
        source_text,
    }
}

/// Converts a router expression span into manifest source metadata.
///
/// Inputs:
/// - `source`: source context for the file being inspected.
/// - `expr`: syntax-output expression whose span anchors the route call.
///
/// Output:
/// - Serializable source span with package-safe path and one-based position.
///
/// Transformation:
/// - Converts byte offsets into line/column pairs and clones only the safe
///   manifest-visible source path.
pub(super) fn source_span_for_expr(
    source: &WebRouteSourceContext<'_>,
    expr: &SyntaxExprOutput,
) -> WebSourceSpanArtifact {
    let (line, column) = line_column_for_offset(source.source_text, expr.span.start);
    WebSourceSpanArtifact {
        path: source.manifest_path.clone(),
        line,
        column,
    }
}

/// Normalizes a source path for route manifest output.
///
/// Inputs:
/// - `source_path`: absolute or relative source file path.
///
/// Output:
/// - Relative, slash-separated path safe for runtime manifests.
///
/// Transformation:
/// - Prefers the path from the first `src` component onward. When a temporary
///   test path or unknown absolute path has no `src`, falls back to the file
///   name so manifests never embed host-specific absolute directories.
fn route_source_manifest_path(source_path: &str) -> String {
    let path = Path::new(source_path);
    let components = path.components().collect::<Vec<_>>();
    if let Some(index) = components
        .iter()
        .position(|component| {
            matches!(component, Component::Normal(value) if value.to_string_lossy() == "src")
        })
    {
        return components_to_manifest_path(&components[index..]);
    }
    if path.is_relative() {
        let relative = components_to_manifest_path(&components);
        if !relative.is_empty() {
            return relative;
        }
    }
    path.file_name()
        .map(|name| name.to_string_lossy().replace('\\', "/"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "unknown.terl".to_string())
}

/// Converts filesystem path components into manifest path text.
///
/// Inputs:
/// - `components`: path components selected for manifest output.
///
/// Output:
/// - Slash-separated relative path with unsafe prefix components removed.
///
/// Transformation:
/// - Keeps only normal components and drops roots, drive prefixes, current
///   directories, and parent markers.
fn components_to_manifest_path(components: &[Component<'_>]) -> String {
    components
        .iter()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Converts a byte offset into a one-based source position.
///
/// Inputs:
/// - `source`: UTF-8 source text.
/// - `offset`: byte offset from syntax-output span metadata.
///
/// Output:
/// - `(line, column)` pair, both one-based.
///
/// Transformation:
/// - Walks character boundaries up to the requested byte offset, incrementing
///   line on newline and column otherwise. Offsets beyond the source clamp to
///   the end of the source.
fn line_column_for_offset(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

/// Extracts a middleware function reference from a `Router.use` builder call.
///
/// Inputs:
/// - `expr`: syntax expression candidate.
///
/// Output:
/// - Middleware function name when `expr` is a supported `use` builder call.
/// - `None` for unrelated expressions or unsupported argument shapes.
///
/// Transformation:
/// - Reads direct local function references from static or receiver-style
///   middleware registration calls without evaluating router values.
pub(super) fn router_middleware_from_expr(expr: &SyntaxExprOutput) -> Option<&str> {
    if expr.kind != SyntaxExprKind::Call {
        return None;
    }
    let (method_name, middleware_index) = if expr.remote.as_deref() == Some("Router") {
        (expr.children.first()?.text.as_deref()?, 2)
    } else {
        let callee = expr.children.first()?;
        let method_name = router_receiver_method_name(callee)?;
        if !is_router_builder_receiver(callee.children.first()?) {
            return None;
        }
        (method_name, 1)
    };
    if method_name != "use" {
        return None;
    }
    router_handler_name(expr.children.get(middleware_index)?)
}

/// Extracts a router receiver-method name from a call callee.
///
/// Inputs:
/// - `callee`: first child of a syntax call expression.
///
/// Output:
/// - Router builder method name when the callee is a field access.
///
/// Transformation:
/// - Reads `router.get(...)` and `Router.new().get(...)` as source-level
///   receiver calls without requiring the full typechecker.
pub(super) fn router_receiver_method_name(callee: &SyntaxExprOutput) -> Option<&str> {
    if callee.kind != SyntaxExprKind::FieldAccess {
        return None;
    }
    let method = callee.text.as_deref()?;
    matches!(
        method,
        "get"
            | "post"
            | "put"
            | "patch"
            | "delete"
            | "head"
            | "options"
            | "use"
            | "fallback"
            | "error"
            | "group"
    )
    .then_some(method)
}

/// Returns whether a receiver expression is router-builder shaped.
///
/// Inputs:
/// - `receiver`: expression before a router receiver method.
///
/// Output:
/// - `true` for the conservative router-builder shapes accepted by manifest
///   extraction.
///
/// Transformation:
/// - Accepts `Router.new()`, chained router builder calls, and the conventional
///   local `router` binding used by scaffolded projects.
pub(super) fn is_router_builder_receiver(receiver: &SyntaxExprOutput) -> bool {
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
                        | "use"
                        | "fallback"
                        | "error"
                        | "group"
                )
            });
    }
    let Some(callee) = receiver.children.first() else {
        return false;
    };
    router_receiver_method_name(callee).is_some_and(|_| {
        callee
            .children
            .first()
            .is_some_and(is_router_builder_receiver)
    })
}

/// Extracts a simple router group call and its lambda body.
///
/// Inputs:
/// - `expr`: syntax expression candidate from a router function body.
///
/// Output:
/// - Route prefix and lambda body for supported `Router.group(...)` or
///   `router.group(...)` calls.
/// - `None` for unrelated expressions or unsupported group argument shapes.
///
/// Transformation:
/// - Recognizes ordinary router-builder calls without evaluating the router
///   value, then unwraps the single-clause lambda used to configure the scoped
///   router.
pub(super) fn router_group_body_expr(
    expr: &SyntaxExprOutput,
) -> Option<(String, &SyntaxExprOutput)> {
    if expr.kind != SyntaxExprKind::Call {
        return None;
    }
    let (method_name, prefix_index, configure_index) = if expr.remote.as_deref() == Some("Router") {
        (expr.children.first()?.text.as_deref()?, 2, 3)
    } else {
        let callee = expr.children.first()?;
        let method_name = router_receiver_method_name(callee)?;
        if !is_router_builder_receiver(callee.children.first()?) {
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

/// Applies a route group prefix to every discovered manifest row.
///
/// Inputs:
/// - `prefix`: literal group prefix from `Router.group`.
/// - `rows`: grouped route rows collected from the configure lambda.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Mutates every route in place so nested routes enter normal manifest
///   validation with their fully qualified paths.
pub(super) fn prefix_web_route_manifest_rows(prefix: &str, rows: &mut WebRouteManifestRows) {
    for handler in &mut rows.handlers {
        handler.route = prefixed_router_route(prefix, &handler.route);
    }
    for websocket in &mut rows.websockets {
        websocket.route = prefixed_router_route(prefix, &websocket.route);
    }
    for response in &mut rows.static_responses {
        response.route = prefixed_router_route(prefix, &response.route);
    }
    for response in &mut rows.file_responses {
        response.route = prefixed_router_route(prefix, &response.route);
    }
}

/// Combines a group prefix with a nested route pattern.
///
/// Inputs:
/// - `prefix`: route prefix passed to `Router.group`.
/// - `route`: nested route pattern emitted by a route builder.
///
/// Output:
/// - Fully qualified route pattern.
///
/// Transformation:
/// - Keeps root routes stable, maps grouped fallback `*` to `prefix/*`, and
///   joins ordinary slash-prefixed routes without double slashes.
pub(super) fn prefixed_router_route(prefix: &str, route: &str) -> String {
    let normalized_prefix = if prefix == "/" {
        "/"
    } else {
        prefix.trim_end_matches('/')
    };
    if route == "*" || route == "/*" {
        return if normalized_prefix == "/" {
            "*".to_string()
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

/// Extracts a route pattern from a syntax string literal.
///
/// Inputs:
/// - `expr`: route argument candidate.
///
/// Output:
/// - Decoded route string for literal arguments.
///
/// Transformation:
/// - Uses JSON string decoding because syntax-output string text preserves
///   quoted literal spelling.
pub(super) fn router_route_literal(expr: &SyntaxExprOutput) -> Option<String> {
    if expr.kind != SyntaxExprKind::Binary {
        return None;
    }
    serde_json::from_str(expr.text.as_deref()?).ok()
}

/// Extracts a handler function name from a syntax variable reference.
///
/// Inputs:
/// - `expr`: handler argument candidate.
///
/// Output:
/// - Function name referenced by the router builder call.
///
/// Transformation:
/// - Accepts only direct local function references so manifest generation
///   stays deterministic until richer route lowering is implemented.
pub(super) fn router_handler_name(expr: &SyntaxExprOutput) -> Option<&str> {
    if expr.kind != SyntaxExprKind::Var {
        return None;
    }
    expr.text.as_deref()
}

/// Returns whether a parameter type denotes `std.http.Request.Request`.
///
/// Inputs:
/// - `type_text`: source-like type annotation text.
///
/// Output:
/// - `true` for simple or qualified request aliases accepted by 0.0.5 router
///   extraction.
///
/// Transformation:
/// - Performs conservative textual recognition until route extraction is wired
///   through the full resolved typechecker.
pub(super) fn is_request_type(type_text: &str) -> bool {
    matches!(
        type_text,
        "Request" | "std.http.Request.Request" | "Request.Request"
    )
}

/// Returns whether a return type denotes `std.http.Response.Response`.
///
/// Inputs:
/// - `type_text`: source-like type annotation text.
///
/// Output:
/// - `true` for simple or qualified response aliases accepted by 0.0.5 router
///   extraction.
///
/// Transformation:
/// - Performs conservative textual recognition until route extraction is wired
///   through the full resolved typechecker.
pub(super) fn is_response_type(type_text: &str) -> bool {
    matches!(
        type_text,
        "Response" | "std.http.Response.Response" | "Response.Response"
    )
}

/// Returns whether a type annotation denotes `std.http.Error.HttpError`.
///
/// Inputs:
/// - `type_text`: source-like type annotation text.
///
/// Output:
/// - `true` for simple or qualified HTTP error aliases accepted by 0.0.5
///   router extraction.
///
/// Transformation:
/// - Performs conservative textual recognition until router error extraction
///   is wired through the full resolved typechecker.
pub(super) fn is_http_error_type(type_text: &str) -> bool {
    matches!(
        type_text,
        "HttpError" | "std.http.Error.HttpError" | "Error.HttpError"
    )
}
