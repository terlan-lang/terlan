use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxImportKind, SyntaxModuleOutput,
};

/// Compiler-owned API contract schema emitted before OpenAPI conversion.
pub const API_CONTRACT_SCHEMA: &str = "terlan-api-contract-v1";

/// OpenAPI version emitted by the initial API command surface.
pub const OPENAPI_VERSION: &str = "3.1.0";

/// Compiler-owned API service contract.
///
/// Inputs:
/// - Typed route, handler, request, response, auth, and capability metadata.
///
/// Output:
/// - Stable service-level contract independent from OpenAPI's wire schema.
///
/// Transformation:
/// - Represents Terlan-owned API shape in compiler terms so OpenAPI can remain
///   one import/export format rather than the source of truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ApiContract {
    pub(crate) schema: String,
    pub(crate) service: ApiService,
    pub(crate) routes: Vec<ApiRoute>,
}

/// Service identity for an API contract.
///
/// Inputs:
/// - Project or command-local service metadata.
///
/// Output:
/// - Stable API title and version.
///
/// Transformation:
/// - Keeps human-facing API identity separate from route facts so route
///   generation can evolve without changing service metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ApiService {
    pub(crate) name: String,
    pub(crate) version: String,
}

/// One typed API route in the compiler-owned contract.
///
/// Inputs:
/// - Route method/path and typed handler metadata.
///
/// Output:
/// - Stable route contract consumed by OpenAPI emission and future cloud
///   capability checks.
///
/// Transformation:
/// - Stores only compiler-owned route identity in this first slice; request and
///   response schemas will be added when route extraction is implemented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ApiRoute {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) handler: String,
}

/// Minimal OpenAPI document emitted from a compiler API contract.
///
/// Inputs:
/// - `ApiContract` from compiler-owned route metadata.
///
/// Output:
/// - OpenAPI 3.1 document shape suitable for deterministic JSON/YAML output.
///
/// Transformation:
/// - Converts only the stable service identity and currently known paths.
///   Empty route sets are valid in this first command-surface slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OpenApiDocument {
    pub(crate) openapi: String,
    pub(crate) info: OpenApiInfo,
    pub(crate) paths: BTreeMap<String, BTreeMap<String, OpenApiOperation>>,
}

/// OpenAPI `info` object.
///
/// Inputs:
/// - Service identity from `ApiContract`.
///
/// Output:
/// - Stable OpenAPI title and version fields.
///
/// Transformation:
/// - Performs the narrow OpenAPI projection without leaking OpenAPI naming into
///   the compiler-owned service model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OpenApiInfo {
    pub(crate) title: String,
    pub(crate) version: String,
}

/// One OpenAPI operation emitted from a Terlan API route.
///
/// Inputs:
/// - Compiler-owned route identity.
///
/// Output:
/// - Serializable OpenAPI operation with stable operation id and default
///   response metadata.
///
/// Transformation:
/// - Keeps the initial OpenAPI projection intentionally conservative while
///   preserving method/path/handler identity for downstream tooling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OpenApiOperation {
    #[serde(rename = "operationId")]
    pub(crate) operation_id: String,
    pub(crate) responses: BTreeMap<String, OpenApiResponse>,
}

/// Minimal OpenAPI response emitted for discovered routes.
///
/// Inputs:
/// - Route projection currently lacks typed response schema data.
///
/// Output:
/// - Stable response description accepted by OpenAPI validators.
///
/// Transformation:
/// - Reserves the response object shape until handler return schemas are
///   extracted from Terlan types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OpenApiResponse {
    pub(crate) description: String,
}

impl ApiContract {
    /// Builds an empty contract for command-surface validation.
    ///
    /// Inputs:
    /// - `service_name`: service title to place in the contract.
    /// - `version`: service version to place in the contract.
    ///
    /// Output:
    /// - `ApiContract` with no routes.
    ///
    /// Transformation:
    /// - Establishes deterministic output for `terlc api emit` before typed
    ///   route extraction is wired in.
    pub(crate) fn empty(service_name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            schema: API_CONTRACT_SCHEMA.to_string(),
            service: ApiService {
                name: service_name.into(),
                version: version.into(),
            },
            routes: Vec::new(),
        }
    }

    /// Builds a contract from one Terlan HTTP router source file.
    ///
    /// Inputs:
    /// - `source`: Terlan source text containing a `pub router(): Router`
    ///   function that uses `std.http.Router`.
    /// - `service_name`: service title to place in the contract.
    /// - `version`: service version to place in the contract.
    ///
    /// Output:
    /// - `ApiContract` populated with route rows discovered from router
    ///   builder calls.
    ///
    /// Transformation:
    /// - Parses syntax output, finds supported `Router.get/post/...` calls, and
    ///   records method/path/handler identity without evaluating user code.
    pub(crate) fn from_router_source(
        source: &str,
        service_name: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self, String> {
        let syntax = parse_module_as_syntax_output(source)
            .map_err(|err| format!("error[api_emit]: cannot parse API source: {err:?}"))?;
        if !imports_std_http_router(&syntax) {
            return Err(format!(
                "error[api_emit]: module `{}` must import std.http.Router for API route extraction",
                syntax.module_name
            ));
        }
        let routes = routes_from_syntax_module(&syntax)?;
        Ok(Self {
            schema: API_CONTRACT_SCHEMA.to_string(),
            service: ApiService {
                name: service_name.into(),
                version: version.into(),
            },
            routes,
        })
    }

    /// Projects a compiler-owned contract into OpenAPI.
    ///
    /// Inputs:
    /// - `self`: compiler-owned API contract.
    ///
    /// Output:
    /// - Minimal OpenAPI document.
    ///
    /// Transformation:
    /// - Copies service identity and expands route paths into an OpenAPI path
    ///   map. Route operation schema details are intentionally deferred.
    pub(crate) fn to_openapi(&self) -> OpenApiDocument {
        let mut paths = BTreeMap::<String, BTreeMap<String, OpenApiOperation>>::new();
        for route in &self.routes {
            let operation = OpenApiOperation {
                operation_id: openapi_operation_id(route),
                responses: BTreeMap::from([(
                    "200".to_string(),
                    OpenApiResponse {
                        description: "Successful response".to_string(),
                    },
                )]),
            };
            paths
                .entry(openapi_path(&route.path))
                .or_default()
                .insert(route.method.to_lowercase(), operation);
        }
        OpenApiDocument {
            openapi: OPENAPI_VERSION.to_string(),
            info: OpenApiInfo {
                title: self.service.name.clone(),
                version: self.service.version.clone(),
            },
            paths,
        }
    }
}

/// Extracts API routes from one parsed module.
///
/// Inputs:
/// - `syntax`: parsed Terlan syntax output.
///
/// Output:
/// - Sorted route rows discovered from public `router` functions.
///
/// Transformation:
/// - Walks each router body and recognizes direct static and receiver-style
///   `std.http.Router` builder calls.
fn routes_from_syntax_module(syntax: &SyntaxModuleOutput) -> Result<Vec<ApiRoute>, String> {
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
    routes.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.method.cmp(&right.method))
            .then(left.handler.cmp(&right.handler))
    });
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
        "new" | "use" | "error" => return Ok(None),
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
fn imports_std_http_router(syntax: &SyntaxModuleOutput) -> bool {
    syntax.declarations.iter().any(|declaration| {
        matches!(
            &declaration.payload,
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                ..
            } if module_name == "std.http.Router"
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

/// Extracts a router receiver-method name from a call callee.
fn router_receiver_method_name(callee: &SyntaxExprOutput) -> Option<&str> {
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

/// Converts a Terlan router path into OpenAPI path syntax.
fn openapi_path(path: &str) -> String {
    let normalized = if path == "*" { "/*" } else { path };
    normalized
        .split('/')
        .map(|segment| {
            if let Some(param) = segment.strip_prefix(':') {
                format!("{{{param}}}")
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Builds a deterministic OpenAPI operation id.
fn openapi_operation_id(route: &ApiRoute) -> String {
    let handler = route.handler.replace('.', "_");
    format!("{}_{}", route.method.to_lowercase(), handler)
}

#[cfg(test)]
#[path = "api_contract_test.rs"]
mod api_contract_test;
