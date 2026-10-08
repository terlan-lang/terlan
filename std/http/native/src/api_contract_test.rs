use super::*;

fn route(method: &str, path: &str, handler: &str) -> ApiRoute {
    ApiRoute {
        method: method.into(),
        path: path.into(),
        handler: handler.into(),
    }
}

#[test]
fn empty_contract_preserves_identity_and_minimal_openapi() {
    let contract = ApiContract::empty("Example", "0.0.1");
    assert_eq!(
        serde_json::to_value(&contract).unwrap(),
        serde_json::json!({
            "schema": "terlan-api-contract-v1",
            "service": {"name": "Example", "version": "0.0.1"},
            "routes": []
        })
    );
    assert_eq!(
        serde_json::to_value(contract.to_openapi()).unwrap(),
        serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Example", "version": "0.0.1"},
            "paths": {}
        })
    );
}

#[test]
fn discovery_order_does_not_change_contract_or_projection() {
    let expected = vec![
        route("GET", "/a", "a"),
        route("GET", "/a", "b"),
        route("POST", "/a", "a"),
        route("DELETE", "/z", "z"),
    ];
    let contract = ApiContract::from_routes("Service", "1", expected.clone());
    assert_eq!(contract.routes, expected);
    for offset in 0..expected.len() {
        let mut routes = expected.clone();
        routes.rotate_left(offset);
        routes.reverse();
        let reordered = ApiContract::from_routes("Service", "1", routes);
        assert_eq!(contract, reordered);
        assert_eq!(
            serde_json::to_string(&contract.to_openapi()).unwrap(),
            serde_json::to_string(&reordered.to_openapi()).unwrap()
        );
    }
    // Preserve the existing projection rule for duplicate method/path rows.
    assert_eq!(
        contract.to_openapi().paths["/a"]["get"].operation_id,
        "get_b"
    );
}

#[test]
fn openapi_projection_preserves_paths_methods_and_qualified_handlers() {
    let methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
    let routes = methods
        .iter()
        .map(|method| route(method, "/users/:id/orders/:order", "Users.show"))
        .chain([
            route("GET", "*", "fallback"),
            route("GET", "/", "home"),
            route("POST", "/literal:colon/", "create"),
        ])
        .collect();
    let contract = ApiContract::from_routes("Service", "1", routes);
    assert_eq!(contract.routes.len(), 10);
    let openapi = contract.to_openapi();
    assert_eq!(openapi.paths.len(), 4);
    for method in methods {
        let operation = &openapi.paths["/users/{id}/orders/{order}"][&method.to_lowercase()];
        assert_eq!(
            operation.operation_id,
            format!("{}_Users_show", method.to_lowercase())
        );
        assert_eq!(operation.responses.len(), 1);
        assert_eq!(
            operation.responses["200"].description,
            "Successful response"
        );
    }
    assert!(openapi.paths.contains_key("/*"));
    assert!(openapi.paths.contains_key("/"));
    assert!(openapi.paths.contains_key("/literal:colon/"));
    assert!(contract.routes.iter().any(|route| route.path == "*"));
}

#[test]
fn serialization_roundtrips_unicode_and_escaped_metadata() {
    let contract = ApiContract::from_routes(
        "Service \"quoted\"\n\u{03bb}",
        "1\\2",
        vec![route("GET", "/\u{7528}\u{6237}/:id", "Users.show")],
    );
    let json = serde_json::to_string(&contract).unwrap();
    assert_eq!(
        serde_json::from_str::<ApiContract>(&json).unwrap(),
        contract
    );
    let openapi = contract.to_openapi();
    let json = serde_json::to_string(&openapi).unwrap();
    assert_eq!(
        serde_json::from_str::<OpenApiDocument>(&json).unwrap(),
        openapi
    );
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let operation = &value["paths"]["/\u{7528}\u{6237}/{id}"]["get"];
    assert_eq!(operation["operationId"], "get_Users_show");
    assert!(operation.get("operation_id").is_none());
    assert!(operation.get("requestBody").is_none());
}
