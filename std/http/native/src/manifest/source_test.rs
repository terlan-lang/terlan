use super::*;
use crate::channel_plan::{SseEndpointPlan, WebSocketEndpointPlan};
use crate::routing::Fallback;
use crate::source_descriptor::Route;
use terlan_runtime_abi::NativeValue;

#[test]
fn entrypoint_discovery_uses_exports_not_handler_type_names() {
    for module in [
        "app.Http",
        "app.WebSocket",
        "app.RoomProtocol",
        "app.MyWebSocket",
        "WebSocket",
    ] {
        for (exports, expected) in [
            (vec![], false),
            (vec![("home", 1)], false),
            (vec![("router", 1)], false),
            (vec![("Router", 0)], false),
            (vec![("router", 0)], true),
            (vec![("ignored", 0), ("router", 0)], true),
            (vec![("route", 0)], false),
            (vec![("protocol", 0)], false),
            (vec![("route", 1), ("protocol", 0)], false),
            (vec![("route", 0), ("protocol", 1)], false),
            (
                vec![("protocol", 0), ("route", 0)],
                matches!(module, "app.WebSocket" | "app.RoomProtocol"),
            ),
        ] {
            assert_eq!(
                has_route_entrypoints(module, exports.clone()),
                expected,
                "{module}: {exports:?}"
            );
        }
    }
}

fn span() -> SourceSpan {
    SourceSpan {
        path: "src/app/Http.terl".into(),
        line: 1,
        column: 1,
    }
}
fn route(path: &str, target: RouteTarget<NativeValue>) -> Route<NativeValue> {
    Route {
        method: "GET".into(),
        path: path.into(),
        target,
        middleware: vec![],
        response_middleware: vec![],
    }
}
fn label(value: &NativeValue) -> Result<String, String> {
    match value {
        NativeValue::String(value) => Ok(value.clone()),
        _ => Err("bad label".into()),
    }
}
fn handler() -> RouteTarget<NativeValue> {
    RouteTarget::Handler(NativeValue::String("home".into()))
}

#[test]
fn source_websocket_exports_validate_values_and_metadata() {
    let values = [
        NativeValue::String("/ws".into()),
        NativeValue::String("chat.v1".into()),
    ];
    let row = WebSocketRoute::from_source_exports("app.WebSocket", span(), &values).unwrap();
    assert_eq!(row.route, "/ws");
    assert_eq!(row.protocol, "chat.v1");
    assert_eq!(row.module, "app.WebSocket");
    assert_eq!(row.source, Some(span()));
    for values in [
        vec![],
        vec![NativeValue::Int(1)],
        vec![NativeValue::Int(1); 3],
        vec![NativeValue::Int(1), NativeValue::String("chat".into())],
        vec![NativeValue::String("/ws".into()), NativeValue::Int(1)],
        vec![
            NativeValue::String("/ws".into()),
            NativeValue::String(String::new()),
        ],
        vec![
            NativeValue::String("/ws".into()),
            NativeValue::String("chat room".into()),
        ],
        vec![
            NativeValue::String("relative".into()),
            NativeValue::String("chat".into()),
        ],
    ] {
        assert!(WebSocketRoute::from_source_exports("app.WebSocket", span(), &values).is_err());
    }
    assert!(WebSocketRoute::from_source_exports("../Wrong", span(), &values).is_err());
    let mut invalid_span = span();
    invalid_span.path = "../private.terl".into();
    assert!(WebSocketRoute::from_source_exports("app.WebSocket", invalid_span, &values).is_err());
}

#[test]
fn source_routes_preserve_owners_channels_and_fallback_precedence() {
    assert_eq!(
        Routes::from_source_router("app.Http", span(), Router::default(), label).unwrap(),
        Routes::default()
    );
    let router = Router {
        routes: vec![
            route("/*", handler()),
            route(
                "/events",
                RouteTarget::Sse(SseEndpointPlan::new(1, 64).unwrap()),
            ),
            route(
                "/socket",
                RouteTarget::WebSocket(WebSocketEndpointPlan::new(1, 64).unwrap()),
            ),
        ],
        fallback: Some(Fallback {
            handler: NativeValue::String("fallback".into()),
            middleware: vec![],
            response_middleware: vec![],
        }),
        ..Router::default()
    };
    let rows = Routes::from_source_router("app.Http", span(), router, label).unwrap();
    assert_eq!(rows.handlers.len(), 7);
    for row in rows.handlers {
        assert_eq!(row.module, "app.Http");
        assert_eq!(
            row.function,
            if row.method == "GET" {
                "home"
            } else {
                "fallback"
            }
        );
        assert_eq!(row.source, Some(span()));
    }
    assert_eq!(
        rows.sse,
        vec![SseRoute {
            module: "app.Http".into(),
            route: "/events".into(),
            source: span()
        }]
    );
    assert_eq!(
        rows.websockets,
        vec![WebSocketRoute {
            module: "app.Http".into(),
            route: "/socket".into(),
            protocol: String::new(),
            source: Some(span())
        }]
    );
    assert!(rows.static_responses.is_empty() && rows.file_responses.is_empty());
}

#[test]
fn source_routes_fail_closed_for_invalid_owners_labels_and_declarations() {
    for routes in [
        vec![route("relative", handler())],
        vec![route("/", handler()), route("/", handler())],
    ] {
        assert!(Routes::from_source_router(
            "app.Http",
            span(),
            Router {
                routes,
                ..Router::default()
            },
            label
        )
        .is_err());
    }
    for target in [
        handler(),
        RouteTarget::Sse(SseEndpointPlan::new(1, 64).unwrap()),
        RouteTarget::WebSocket(WebSocketEndpointPlan::new(1, 64).unwrap()),
    ] {
        let router = Router {
            routes: vec![route("/", target)],
            ..Router::default()
        };
        for (owner, source) in [
            ("bad/owner", span()),
            ("app.Http", SourceSpan { line: 0, ..span() }),
        ] {
            assert!(Routes::from_source_router(owner, source, router.clone(), label).is_err());
        }
    }
    for router in [
        Router {
            routes: vec![route("/", RouteTarget::Handler(NativeValue::Unit))],
            ..Router::default()
        },
        Router {
            fallback: Some(Fallback {
                handler: NativeValue::Unit,
                middleware: vec![],
                response_middleware: vec![],
            }),
            ..Router::default()
        },
        Router {
            lifecycle: Some(NativeValue::Unit),
            ..Router::default()
        },
    ] {
        assert!(Routes::from_source_router("app.Http", span(), router, label).is_err());
    }
}

#[test]
fn unspecified_websocket_protocol_requires_source_ownership() {
    let mut row = WebSocketRoute {
        module: "app.Http".into(),
        route: "/socket".into(),
        protocol: String::new(),
        source: Some(span()),
    };
    validate_websocket(&row).unwrap();
    row.module.clear();
    assert!(validate_websocket(&row).is_err());
    row.module = "app.Http".into();
    row.source = None;
    assert!(validate_websocket(&row).is_err());
    row.source = Some(span());
    for protocol in [" ", "app v1", "app\nv1"] {
        row.protocol = protocol.into();
        assert!(validate_websocket(&row).is_err());
    }
}

#[test]
fn source_channel_wildcards_override_get_fallback_without_losing_other_methods() {
    for target in [
        RouteTarget::Sse(SseEndpointPlan::new(1, 64).unwrap()),
        RouteTarget::WebSocket(WebSocketEndpointPlan::new(1, 64).unwrap()),
    ] {
        let router = Router {
            routes: vec![route("/*", target.clone())],
            fallback: Some(Fallback {
                handler: NativeValue::String("fallback".into()),
                middleware: vec![],
                response_middleware: vec![],
            }),
            ..Router::default()
        };
        let rows = Routes::from_source_router("app.Http", span(), router, label).unwrap();
        assert_eq!(rows.handlers.len(), 6);
        assert!(rows.handlers.iter().all(|row| row.method != "GET"));
        let mut channel = route("/", target);
        channel.method = "POST".into();
        let error = Routes::from_source_router(
            "app.Http",
            span(),
            Router {
                routes: vec![channel],
                ..Router::default()
            },
            label,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("channel declarations require GET"),
            "{error}"
        );
    }
}

#[test]
fn fallback_manifest_routes_agree_with_source_dispatch_for_every_method() {
    use crate::routing::{RouteMethod, RouterOutcome};

    let router = Router {
        fallback: Some(Fallback {
            handler: NativeValue::String("fallback".into()),
            middleware: vec![],
            response_middleware: vec![],
        }),
        ..Router::default()
    };
    let runtime = router.clone().into_routing_table().unwrap();
    let rows = Routes::from_source_router("app.Http", span(), router, label).unwrap();
    assert_eq!(rows.handlers.len(), 7);
    for row in rows.handlers {
        for path in ["/", "/favicon.ico", "/nested/missing"] {
            let method = RouteMethod::from_name(&row.method).unwrap();
            let RouterOutcome::Matched(dispatch) = runtime.dispatch(method, path).unwrap() else {
                panic!("fallback must match {path}");
            };
            assert_eq!(row.route, dispatch.route_pattern);
            assert!(dispatch.route_params.is_empty());
        }
    }
}
