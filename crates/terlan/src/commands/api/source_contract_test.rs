use super::*;

/// Verifies router source extraction records typed API route identity.
///
/// Inputs:
/// - A Terlan module importing `std.http.Router` with static and receiver-style
///   route builder calls.
///
/// Output:
/// - Test passes when routes are discovered in deterministic order.
///
/// Transformation:
/// - Parses source text and extracts method/path/handler rows without invoking
///   backend emission.
#[test]
fn router_source_contract_extracts_routes() {
    let contract =
        from_router_source(router_source(), "Example", "0.0.1").expect("extract API routes");

    assert_eq!(
        contract.routes,
        vec![
            ApiRoute {
                method: "GET".to_string(),
                path: "/".to_string(),
                handler: "home".to_string(),
            },
            ApiRoute {
                method: "GET".to_string(),
                path: "/users/:id".to_string(),
                handler: "show_user".to_string(),
            },
        ]
    );
}

/// Verifies OpenAPI projection includes discovered route paths.
///
/// Inputs:
/// - A route-bearing package-owned API contract.
///
/// Output:
/// - Test passes when OpenAPI path syntax and operation ids are deterministic.
///
/// Transformation:
/// - Converts Terlan `:id` path parameters to OpenAPI `{id}` syntax while
///   preserving the package-owned route model.
#[test]
fn router_source_contract_projects_to_openapi_paths() {
    let contract =
        from_router_source(router_source(), "Example", "0.0.1").expect("extract API routes");
    let openapi = contract.to_openapi();
    let users = openapi
        .paths
        .get("/users/{id}")
        .and_then(|methods| methods.get("get"))
        .expect("users GET operation");

    assert_eq!(users.operation_id, "get_show_user");
    assert!(users.responses.contains_key("200"));
}

/// Verifies router groups are flattened into API route paths.
///
/// Inputs:
/// - A router module with a grouped `/admin` route.
///
/// Output:
/// - Test passes when the nested route path is prefixed.
///
/// Transformation:
/// - Confirms API schema extraction follows the same functional router shape
///   used by the web package manifest.
#[test]
fn router_source_contract_extracts_group_routes() {
    let contract = from_router_source(grouped_router_source(), "Example", "0.0.1")
        .expect("extract grouped API routes");

    assert_eq!(
        contract.routes,
        vec![ApiRoute {
            method: "POST".to_string(),
            path: "/admin/users".to_string(),
            handler: "create_user".to_string(),
        }]
    );
}

/// Verifies grouped HTTP imports retain the Router capability marker.
///
/// Cloud applications commonly import `Response` and `Router` together; API
/// and deploy-plan extraction must recognize the same grouped syntax as web
/// package route discovery.
#[test]
fn router_source_contract_accepts_grouped_router_import() {
    let source = router_source().replace(
        "import std.http.Router.\nimport std.http.Response.",
        "import std.http.{Response, Router}.",
    );

    let contract = from_router_source(&source, "Example", "0.0.1")
        .expect("extract routes from grouped Router import");

    assert_eq!(contract.routes.len(), 2);
}

#[test]
fn static_discovery_rejects_nonliteral_routes_and_anonymous_handlers() {
    for (source, diagnostic) in [
        (
            router_source().replace("\"/users/:id\"", "computed_path()"),
            "Router.get requires a literal route path",
        ),
        (
            router_source().replace(", show_user)", ", (request) -> show_user(request))"),
            "Router.get requires a handler reference",
        ),
    ] {
        let error = from_router_source(&source, "Example", "1").unwrap_err();
        assert!(error.contains(diagnostic), "{error}");
    }
}

#[test]
fn static_discovery_rejects_missing_router_import_and_malformed_source() {
    let source = router_source()
        .replace("import std.http.Router.", "")
        .replace("import type std.http.Router.Router.", "");
    let error = from_router_source(&source, "Example", "1").unwrap_err();
    assert!(error.contains("must import std.http.Router"), "{error}");
    let error = from_router_source("pub router(", "Example", "1").unwrap_err();
    assert!(error.contains("cannot parse API source"), "{error}");
}

#[test]
fn static_discovery_preserves_fallback_methods() {
    let source = grouped_router_source().replace(
        "router.post(\"/users\", create_user)",
        "router.fallback(create_user)",
    );
    let contract = from_router_source(&source, "Example", "1").unwrap();
    assert_eq!(
        contract
            .routes
            .iter()
            .map(|route| route.method.as_str())
            .collect::<Vec<_>>(),
        ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"],
    );
    for route in &contract.routes {
        assert_eq!(route.path, "/admin/*");
        assert_eq!(route.handler, "create_user");
    }
}

/// Returns a route source fixture.
fn router_source() -> &'static str {
    "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    let router = Router.get(Router.new(), \"/\", home);\n    router.get(\"/users/:id\", show_user).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n\npub show_user(_request: Request): Response ->\n    Response.text(\"user\").\n"
}

/// Returns a grouped route source fixture.
fn grouped_router_source() -> &'static str {
    "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.new().group(\"/admin\", (router) -> router.post(\"/users\", create_user)).\n\npub create_user(_request: Request): Response ->\n    Response.text(\"created\").\n"
}
