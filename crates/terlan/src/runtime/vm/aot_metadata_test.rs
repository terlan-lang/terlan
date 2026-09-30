use super::*;
use serde_json::json;

#[test]
fn persisted_router_metadata_validates_channel_policy_before_materialization() {
    let plan = AotRouterPlan {
        module: "app".into(),
        routes: vec![AotRouterRoute {
            method: "GET".into(),
            path: "/events".into(),
            target: AotRouterRouteTarget::Sse(SseEndpointPlan::new(3, 128).unwrap()),
            middleware: vec![],
            response_middleware: vec![],
        }],
        ..AotRouterPlan::default()
    };
    let encoded = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        serde_json::from_value::<AotRouterPlan>(encoded.clone()).unwrap(),
        plan
    );
    for key in ["max_pending_events", "max_event_bytes", "keep_alive_ms"] {
        let mut invalid = encoded.clone();
        invalid["routes"][0]["target"]["Sse"][key] = json!(0);
        assert!(
            serde_json::from_value::<AotRouterPlan>(invalid).is_err(),
            "{key}"
        );
    }
    let mut plan = plan;
    plan.routes[0].target =
        AotRouterRouteTarget::WebSocket(WebSocketEndpointPlan::new(3, 128).unwrap());
    let encoded = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        serde_json::from_value::<AotRouterPlan>(encoded.clone()).unwrap(),
        plan
    );
    for key in ["max_pending_frames", "max_frame_bytes"] {
        let mut invalid = encoded.clone();
        invalid["routes"][0]["target"]["WebSocket"][key] = json!(0);
        assert!(
            serde_json::from_value::<AotRouterPlan>(invalid).is_err(),
            "{key}"
        );
    }
    let callback = json!({"module":"app", "function":"callback", "arity":1});
    let mut invalid = encoded;
    let target = &mut invalid["routes"][0]["target"]["WebSocket"];
    target["callbacks"] = json!({"open":callback, "inbound":callback, "writable":callback,
        "close":callback, "cancellation":callback});
    target["pairing"] = json!({"waiting":"wait", "first_matched":"one", "second_matched":"two",
        "peer_left":"left", "inbound":callback, "cancellation":callback});
    assert!(serde_json::from_value::<AotRouterPlan>(invalid).is_err());
}
