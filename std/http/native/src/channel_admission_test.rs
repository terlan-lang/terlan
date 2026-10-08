use super::*;
use crate::channel_plan::{SseCallbacks, WebSocketCallbacks};
use crate::routing::MiddlewareStep;
use terlan_runtime_abi::NativeValue as Value;

type Invoke<'a> = dyn FnMut(&Value, &MiddlewareContinuation<Value>) -> Result<Value, String> + 'a;

#[derive(Clone, Copy, Debug)]
enum Channel {
    WebSocket,
    Sse,
}

impl Channel {
    fn label(self) -> &'static str {
        match self {
            Self::WebSocket => "websocket",
            Self::Sse => "SSE",
        }
    }

    fn endpoint_description(self) -> &'static str {
        match self {
            Self::WebSocket => "a WebSocket",
            Self::Sse => "an SSE",
        }
    }

    fn target(self) -> RouteTarget<Value> {
        match self {
            Self::WebSocket => RouteTarget::WebSocketEndpoint(
                WebSocketEndpointPlan::new(8, 2048)
                    .unwrap()
                    .with_callbacks(WebSocketCallbacks {
                        open: "open".into(),
                        inbound: "inbound".into(),
                        writable: "writable".into(),
                        close: "close".into(),
                        cancellation: "cancellation".into(),
                    })
                    .unwrap(),
            ),
            Self::Sse => RouteTarget::SseEndpoint(
                SseEndpointPlan::new(4, 1024)
                    .unwrap()
                    .with_keep_alive_ms(250)
                    .unwrap()
                    .with_callbacks(SseCallbacks {
                        open: "open".into(),
                        event_ready: "event_ready".into(),
                        keep_alive: "keep_alive".into(),
                        drain: "drain".into(),
                        cancellation: "cancellation".into(),
                    })
                    .unwrap(),
            ),
        }
    }

    fn admit(
        self,
        router: &Router<Value>,
        declared: &str,
        path: &str,
        invoke: &mut Invoke<'_>,
    ) -> Result<Admission<Value, RouteTarget<Value>>, WebRouteError> {
        match self {
            Self::WebSocket => {
                websocket(router, declared, path, invoke).map(|result| match result {
                    Admission::Open(plan) => Admission::Open(RouteTarget::WebSocketEndpoint(plan)),
                    Admission::Respond(response) => Admission::Respond(response),
                })
            }
            Self::Sse => sse(router, declared, path, invoke).map(|result| match result {
                Admission::Open(plan) => Admission::Open(RouteTarget::SseEndpoint(plan)),
                Admission::Respond(response) => Admission::Respond(response),
            }),
        }
    }
}

const CHANNELS: [Channel; 2] = [Channel::WebSocket, Channel::Sse];

fn router(target: RouteTarget<Value>) -> Router<Value> {
    Router::new()
        .scoped_target(
            RouteMethod::Get,
            "/items/{id:Int}",
            target,
            vec!["outer".into(), "inner".into()],
            vec!["response-outer".into(), "response-inner".into()],
        )
        .unwrap()
}

fn continue_request(_: &Value, _: &MiddlewareContinuation<Value>) -> Result<Value, String> {
    Ok(Value::Atom("continue".into()))
}

#[test]
fn admitted_plans_preserve_callbacks_and_policy_after_ordered_middleware() {
    for channel in CHANNELS {
        let target = channel.target();
        let router = router(target.clone());
        let mut invoked = Vec::new();
        let outcome = channel
            .admit(
                &router,
                "/items/{id:Int}",
                "/items/%34%32",
                &mut |callback, next| {
                    invoked.push(callback.clone());
                    match next.step() {
                        MiddlewareStep::Middleware { middleware, .. } => {
                            assert_eq!(callback, &Value::from("outer"));
                            assert_eq!(middleware, Value::from("inner"));
                        }
                        MiddlewareStep::Handler(dispatch) => {
                            assert_eq!(callback, &Value::from("inner"));
                            assert_eq!(dispatch.route_params, [("id".into(), "42".into())]);
                        }
                    }
                    continue_request(callback, next)
                },
            )
            .unwrap();
        assert_eq!(invoked, [Value::from("outer"), Value::from("inner")]);
        assert_eq!(outcome, Admission::Open(target));
    }
}

#[test]
fn middleware_response_precedes_route_and_endpoint_validation() {
    for channel in CHANNELS {
        // Neither this target nor the declared route is suitable for a channel.
        let router = router(RouteTarget::Handler("must-not-run".into()));
        let response = Value::Record {
            name: "Response".into(),
            fields: vec![("status".into(), Value::Int(403))],
        };
        let mut invoked = Vec::new();
        let outcome = channel
            .admit(
                &router,
                "/stale-manifest",
                "/items/42",
                &mut |callback, _| {
                    invoked.push(callback.clone());
                    Ok(Value::Record {
                        name: "Respond".into(),
                        fields: vec![("response".into(), response.clone())],
                    })
                },
            )
            .unwrap();
        assert_eq!(invoked, [Value::from("outer")]);
        assert_eq!(
            outcome,
            Admission::Respond(RouteShortCircuit {
                middleware: "outer".into(),
                response,
                route_params: vec![("id".into(), "42".into())],
                response_middleware: vec!["response-outer".into(), "response-inner".into()],
            })
        );
    }
}

#[test]
fn wrong_target_rejected_after_middleware_without_opening_callbacks() {
    for channel in CHANNELS {
        let other = match channel {
            Channel::WebSocket => Channel::Sse,
            Channel::Sse => Channel::WebSocket,
        };
        for target in [RouteTarget::Handler("ordinary".into()), other.target()] {
            let error = channel
                .admit(
                    &router(target),
                    "/items/{id:Int}",
                    "/items/42",
                    &mut continue_request,
                )
                .unwrap_err();
            assert_eq!(error.to_string(), format!(
                "error[serve_router]: {} route `GET` `/items/{{id:Int}}` did not resolve to {} endpoint",
                channel.label(), channel.endpoint_description(),
            ));
        }
    }
}

#[test]
fn declared_route_must_match_materialized_pattern_not_concrete_path() {
    for channel in CHANNELS {
        for (router, pattern) in [
            (router(channel.target()), "/items/{id:Int}"),
            (Router::new().fallback("fallback".into()), "*"),
        ] {
            let error = channel
                .admit(&router, "/items/42", "/items/42", &mut continue_request)
                .unwrap_err();
            assert_eq!(error.to_string(), format!(
                "error[serve_router]: {} route `GET` `/items/42` does not match materialized route `GET` `{pattern}`",
                channel.label(),
            ));
        }
    }
}

#[test]
fn missing_wrong_method_and_invalid_typed_capture_never_invoke_middleware() {
    for channel in CHANNELS {
        let post = Router::new()
            .scoped_target(
                RouteMethod::Post,
                "/items/{id:Int}",
                channel.target(),
                vec!["must-not-run".into()],
                vec![],
            )
            .unwrap();
        for (router, path) in [
            (Router::new(), "/missing"),
            (post, "/items/42"),
            (router(channel.target()), "/items/not-an-int"),
            (router(channel.target()), "/items/9223372036854775808"),
        ] {
            let error = channel
                .admit(&router, "/items/{id:Int}", path, &mut |_, _| {
                    panic!("unmatched route")
                })
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!(
                    "error[serve_router]: materialized router did not match {} GET {path}",
                    channel.label(),
                )
            );
        }
    }
}

#[test]
fn invalid_paths_and_middleware_errors_fail_closed() {
    for channel in CHANNELS {
        let router = router(channel.target());
        for path in ["relative", "/../secret", "/items//42"] {
            let expected = router.dispatch(RouteMethod::Get, path).unwrap_err();
            assert_eq!(
                channel
                    .admit(&router, "/items/{id:Int}", path, &mut |_, _| panic!(
                        "invalid path"
                    ),)
                    .unwrap_err(),
                expected
            );
        }
        let mut calls = 0;
        let error = channel
            .admit(&router, "/items/{id:Int}", "/items/42", &mut |_, _| {
                calls += 1;
                Err("source authorization failed".into())
            })
            .unwrap_err();
        assert_eq!(calls, 1);
        assert_eq!(error.to_string(), "source authorization failed");
        let error = channel
            .admit(&router, "/items/{id:Int}", "/items/42", &mut |_, _| {
                Ok(Value::Unit)
            })
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "error[vm_http_router_middleware]: expected Continue or Respond(Response)"
        );
    }
}
