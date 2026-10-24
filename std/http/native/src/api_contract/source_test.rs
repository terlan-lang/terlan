use super::*;
use crate::channel_plan::{SseEndpointPlan, WebSocketEndpointPlan};
use crate::routing::Fallback;
use crate::source_descriptor::Route;
use terlan_runtime_abi::NativeValue;

fn route(method: &str, path: &str) -> Route<NativeValue> {
    Route {
        method: method.into(),
        path: path.into(),
        target: RouteTarget::Handler(NativeValue::String("handler".into())),
        middleware: vec![],
        response_middleware: vec![],
    }
}

fn label(value: &NativeValue) -> Result<String, String> {
    let NativeValue::String(value) = value else {
        return Err("invalid label".into());
    };
    Ok(value.clone())
}

#[test]
fn source_projection_handles_empty_routes_and_fallback_precedence() {
    let empty = ApiContract::from_source_router("Service", "1", Router::default(), label).unwrap();
    assert_eq!(empty, ApiContract::empty("Service", "1"));
    let router = Router {
        routes: vec![route("GET", "/*"), route("POST", "/nested/:id")],
        fallback: Some(Fallback {
            handler: NativeValue::String("fallback".into()),
            middleware: vec![],
            response_middleware: vec![],
        }),
        ..Router::default()
    };
    let contract = ApiContract::from_source_router("Service", "1", router, label).unwrap();
    assert_eq!(contract.routes.len(), 8);
    assert_eq!(
        contract
            .routes
            .iter()
            .filter(|route| route.path == "/*")
            .count(),
        7
    );
    for route in &contract.routes {
        assert_eq!(
            route.handler,
            if route.method == "GET" || route.path != "/*" {
                "handler"
            } else {
                "fallback"
            }
        );
    }
    assert!(contract.to_openapi().paths.contains_key("/nested/{id}"));
}

#[test]
fn source_projection_uses_serving_validation_before_labeling() {
    for routes in [
        vec![route("TRACE", "/")],
        vec![route("GET", "relative")],
        vec![route("GET", "/same"), route("GET", "/same")],
        vec![route("GET", "/:one"), route("GET", "/:two")],
    ] {
        assert!(ApiContract::from_source_router(
            "Service",
            "1",
            Router {
                routes,
                ..Router::default()
            },
            |_| panic!("invalid routes must not reach labeling")
        )
        .is_err());
    }
    for router in [
        Router {
            lifecycle: Some(NativeValue::Unit),
            ..Router::default()
        },
        Router {
            overload: Some(("queue".into(), 1)),
            ..Router::default()
        },
    ] {
        let error = ApiContract::from_source_router("Service", "1", router, label).unwrap_err();
        assert!(
            error.to_string().contains("lifecycle and overload"),
            "{error}"
        );
    }
}

#[test]
fn source_projection_rejects_channel_schemas_and_label_failures() {
    for target in [
        RouteTarget::Sse(SseEndpointPlan::new(1, 256).unwrap()),
        RouteTarget::WebSocket(WebSocketEndpointPlan::new(1, 256).unwrap()),
    ] {
        let mut channel = route("GET", "/stream");
        channel.target = target;
        let router = Router {
            routes: vec![channel],
            ..Router::default()
        };
        let error = ApiContract::from_source_router("Service", "1", router, label).unwrap_err();
        assert!(
            error.to_string().contains("error[api_emit.channel_schema]"),
            "{error}"
        );
    }
    let mut invalid_label = route("GET", "/");
    invalid_label.target = RouteTarget::Handler(NativeValue::Unit);
    for router in [
        Router {
            routes: vec![invalid_label],
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
    ] {
        assert_eq!(
            ApiContract::from_source_router("Service", "1", router, label)
                .unwrap_err()
                .to_string(),
            "invalid label"
        );
    }
}
