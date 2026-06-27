use super::*;

/// Verifies empty API contracts have stable compiler-owned identity.
///
/// Inputs:
/// - A command-surface service name and version.
///
/// Output:
/// - Test passes when the generated contract preserves schema, service, and
///   empty-route state.
///
/// Transformation:
/// - Exercises the initial API contract constructor without involving CLI
///   output formatting.
#[test]
fn empty_api_contract_has_stable_identity() {
    let contract = ApiContract::empty("Example", "0.0.1");

    assert_eq!(contract.schema, API_CONTRACT_SCHEMA);
    assert_eq!(contract.service.name, "Example");
    assert_eq!(contract.service.version, "0.0.1");
    assert!(contract.routes.is_empty());
}

/// Verifies OpenAPI projection preserves service identity.
///
/// Inputs:
/// - An empty compiler-owned API contract.
///
/// Output:
/// - Test passes when OpenAPI metadata is deterministic.
///
/// Transformation:
/// - Confirms the first API command slice can emit stable OpenAPI JSON/YAML
///   before route extraction is implemented.
#[test]
fn empty_api_contract_projects_to_minimal_openapi() {
    let contract = ApiContract::empty("Example", "0.0.1");
    let openapi = contract.to_openapi();

    assert_eq!(openapi.openapi, OPENAPI_VERSION);
    assert_eq!(openapi.info.title, "Example");
    assert_eq!(openapi.info.version, "0.0.1");
    assert!(openapi.paths.is_empty());
}

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
    let contract = ApiContract::from_router_source(router_source(), "Example", "0.0.1")
        .expect("extract API routes");

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
/// - A route-bearing compiler-owned API contract.
///
/// Output:
/// - Test passes when OpenAPI path syntax and operation ids are deterministic.
///
/// Transformation:
/// - Converts Terlan `:id` path parameters to OpenAPI `{id}` syntax while
///   preserving the compiler-owned route model.
#[test]
fn router_source_contract_projects_to_openapi_paths() {
    let contract = ApiContract::from_router_source(router_source(), "Example", "0.0.1")
        .expect("extract API routes");
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
    let contract = ApiContract::from_router_source(grouped_router_source(), "Example", "0.0.1")
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

/// Returns a route source fixture.
fn router_source() -> &'static str {
    "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    let router = Router.get(Router.new(), \"/\", home);\n    router.get(\"/users/:id\", show_user).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n\npub show_user(_request: Request): Response ->\n    Response.text(\"user\").\n"
}

/// Returns a grouped route source fixture.
fn grouped_router_source() -> &'static str {
    "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.new().group(\"/admin\", (router) -> router.post(\"/users\", create_user)).\n\npub create_user(_request: Request): Response ->\n    Response.text(\"created\").\n"
}
