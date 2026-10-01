use super::*;
use terlan_runtime_abi::NativeValue as Value;

fn matched(router: &Router<Value>, method: RouteMethod, path: &str) -> RouteDispatch<Value> {
    let RouterOutcome::Matched(route) = router.dispatch(method, path).unwrap() else {
        panic!("no route for {path}")
    };
    *route
}

#[test]
fn dispatch_uses_shared_precedence_independent_of_registration_order() {
    let declarations = ["/api/*", "/api/:id", "/api/status", "/api/nested/*", "*"];
    for reverse in [false, true] {
        let mut paths = declarations.to_vec();
        if reverse {
            paths.reverse();
        }
        let mut router = Router::new().fallback("fallback".into());
        for path in paths {
            router = router.get(path, path.into()).unwrap();
        }
        for (path, pattern) in [
            ("/api/status", "/api/status"),
            ("/api/item", "/api/:id"),
            ("/api", "/api/*"),
            ("/api/deep/path", "/api/*"),
            ("/api/nested/path", "/api/nested/*"),
            ("/api2/path", "*"),
        ] {
            let route = matched(&router, RouteMethod::Get, path);
            assert_eq!(route.route_pattern, pattern);
            assert_eq!(route.target, RouteTarget::Handler(pattern.into()));
        }
        assert_eq!(
            matched(&router, RouteMethod::Post, "/api/status").target,
            RouteTarget::Handler("fallback".into())
        );
    }
}

#[test]
fn typed_captures_suffixes_and_wildcards_decode_once() {
    let router = Router::new()
        .get("/int/{id:Int}", "int".into())
        .unwrap()
        .get("/bool/{flag:Bool}", "bool".into())
        .unwrap()
        .get("/text/{value:String}", "text".into())
        .unwrap()
        .get("/file/:name.json", "file".into())
        .unwrap()
        .get("/rest/*", "rest".into())
        .unwrap();
    for (path, name, value) in [
        ("/int/%34%32", "id", "42"),
        ("/int/-9223372036854775808", "id", "-9223372036854775808"),
        ("/bool/true", "flag", "true"),
        ("/bool/%66alse", "flag", "false"),
        ("/text/caf%C3%A9", "value", "caf\u{e9}"),
        ("/file/a%252Fb+c.json", "name", "a%2Fb+c"),
        ("/rest/a%2Fb/c", "*", "a/b/c"),
        ("/rest", "*", ""),
    ] {
        assert_eq!(
            matched(&router, RouteMethod::Get, path).route_params,
            vec![(name.into(), value.into())],
            "{path}"
        );
    }
    for path in [
        "/int/NaN",
        "/int/9223372036854775808",
        "/bool/TRUE",
        "/bool/1",
        "/text/%FF",
        "/file/.json",
        "/file/a.xml",
        "/rest/%FF",
    ] {
        assert_eq!(
            router.dispatch(RouteMethod::Get, path).unwrap(),
            RouterOutcome::NotFound,
            "{path}"
        );
    }
}

#[test]
fn admission_rejects_ambiguous_and_malformed_routes_without_mutating_snapshots() {
    let router = Router::<Value>::new()
        .get("/users/:id", "first".into())
        .unwrap();
    for path in ["/users/:name", "/users/{name:String}", "/users/{id:Int}"] {
        assert!(router.clone().get(path, "second".into()).is_err());
    }
    for path in [
        "relative",
        "/users/{id}",
        "/users/{id:Float}",
        "/users/:Id",
        "/users/:id:x",
        "/users/*/tail",
        "/../private",
        "/users//tail",
    ] {
        assert!(
            Router::<Value>::new().get(path, "bad".into()).is_err(),
            "{path}"
        );
    }
    let extended = router
        .clone()
        .route(RouteMethod::Post, "/users/:id", "post".into())
        .unwrap();
    assert_eq!(
        router.dispatch(RouteMethod::Post, "/users/1").unwrap(),
        RouterOutcome::NotFound
    );
    assert_eq!(
        matched(&extended, RouteMethod::Post, "/users/1").target,
        RouteTarget::Handler("post".into())
    );
    for path in ["*", "relative", "/../secret", "/users//1"] {
        assert!(router.dispatch(RouteMethod::Get, path).is_err(), "{path}");
    }
    assert!(Router::<Value>::new()
        .get("*", "one".into())
        .unwrap()
        .get("/*", "two".into())
        .is_err());
}

#[test]
fn middleware_short_circuit_retains_scope_and_decoded_params() {
    let router = Router::<Value>::new()
        .scoped_target(
            RouteMethod::Get,
            "/items/{id:Int}",
            RouteTarget::Handler("handler".into()),
            vec!["outer".into(), "inner".into(), "unreachable".into()],
            vec!["response".into(), "inner-response".into()],
        )
        .unwrap();
    let response = Value::Record {
        name: "Response".into(),
        fields: vec![],
    };
    let mut invoked = Vec::new();
    let result = router
        .dispatch_with_typed_middleware(RouteMethod::Get, "/items/%34%32", |callback, _| {
            invoked.push(callback.clone());
            Ok(if callback == &Value::from("outer") {
                Value::Atom("continue".into())
            } else {
                Value::Record {
                    name: "Respond".into(),
                    fields: vec![("response".into(), response.clone())],
                }
            })
        })
        .unwrap();
    assert_eq!(invoked, vec![Value::from("outer"), Value::from("inner")]);
    let RouterOutcome::ShortCircuited(result) = result else {
        panic!("short circuit")
    };
    assert_eq!(result.response, response);
    assert_eq!(result.route_params, vec![("id".into(), "42".into())]);
    assert_eq!(
        result.response_middleware,
        vec![Value::from("response"), Value::from("inner-response")]
    );
}

#[test]
fn middleware_uses_closed_source_contract_not_legacy_tuples() {
    let response = Value::Record {
        name: "Response".into(),
        fields: vec![],
    };
    for value in [
        Value::Unit,
        Value::Tuple(vec![Value::Atom("continue".into())]),
        Value::Record {
            name: "Continue".into(),
            fields: vec![("extra".into(), Value::Unit)],
        },
        Value::Record {
            name: "Respond".into(),
            fields: vec![],
        },
        Value::Record {
            name: "Respond".into(),
            fields: vec![("response".into(), Value::Unit)],
        },
        Value::Record {
            name: "Respond".into(),
            fields: vec![
                ("response".into(), response.clone()),
                ("response".into(), response),
            ],
        },
    ] {
        assert!(MiddlewareResult::from_value(value).is_err());
    }
}

#[test]
fn source_table_admission_preserves_root_policy_and_rejects_unsupported_policy() {
    use crate::source_descriptor::{
        Route as SourceRoute, RouteTarget as SourceTarget, Router as SourceRouter,
    };
    let plan = SourceRouter {
        routes: vec![SourceRoute {
            method: "GET".into(),
            path: "/api/*".into(),
            target: SourceTarget::Handler(Value::from("handler")),
            middleware: vec!["outer".into(), "inner".into()],
            response_middleware: vec!["outer-response".into(), "inner-response".into()],
        }],
        fallback: Some(Fallback {
            handler: "fallback".into(),
            middleware: vec!["fallback-request".into()],
            response_middleware: vec!["fallback-response".into()],
        }),
        error: Some("recover".into()),
        ..SourceRouter::default()
    };
    let router = plan.into_routing_table().unwrap();
    let route = matched(&router, RouteMethod::Get, "/api/missing");
    assert_eq!(
        route.middleware,
        vec![Value::from("outer"), Value::from("inner")]
    );
    assert_eq!(
        route.response_middleware,
        vec![Value::from("outer-response"), Value::from("inner-response")]
    );
    assert_eq!(router.error_handler(), Some(&Value::from("recover")));
    let fallback = matched(&router, RouteMethod::Get, "/elsewhere");
    assert_eq!(fallback.target, RouteTarget::Handler("fallback".into()));
    assert_eq!(fallback.middleware, vec![Value::from("fallback-request")]);
    assert_eq!(
        fallback.response_middleware,
        vec![Value::from("fallback-response")]
    );
    for plan in [
        SourceRouter::<Value> {
            lifecycle: Some(Value::Unit),
            ..SourceRouter::default()
        },
        SourceRouter {
            overload: Some(("reject".into(), 1)),
            ..SourceRouter::default()
        },
    ] {
        assert!(plan.into_routing_table().is_err());
    }
}
