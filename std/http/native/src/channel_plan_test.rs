use super::*;
use serde_json::{json, Value};

// No Default implementation: callback identities are opaque to the package.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Callback(String);

fn callback() -> Callback {
    Callback("application.handler".into())
}

fn sse_callbacks() -> SseCallbacks<Callback> {
    SseCallbacks {
        open: callback(),
        event_ready: callback(),
        keep_alive: callback(),
        drain: callback(),
        cancellation: callback(),
    }
}

fn ws_callbacks() -> WebSocketCallbacks<Callback> {
    WebSocketCallbacks {
        open: callback(),
        inbound: callback(),
        writable: callback(),
        close: callback(),
        cancellation: callback(),
    }
}

fn pairing() -> WebSocketPairing<Callback> {
    WebSocketPairing {
        waiting: "waiting".into(),
        first_matched: "first".into(),
        second_matched: "second".into(),
        peer_left: "left".into(),
        restoration: None,
        inbound: callback(),
        cancellation: callback(),
    }
}

fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
) {
    let encoded = serde_json::to_string(value).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&encoded).unwrap(), value);
}

fn reject<T: serde::de::DeserializeOwned>(value: Value) {
    assert!(serde_json::from_str::<T>(&value.to_string()).is_err());
    assert!(serde_json::from_value::<T>(value).is_err());
}

#[test]
fn sse_policy_validates_limits_and_callbacks() {
    for limits in [(0, 1), (1, 0), (0, 0)] {
        assert_eq!(
            SseEndpointPlan::<Callback>::new(limits.0, limits.1),
            Err(SseError::BackpressureExceeded)
        );
    }
    let plan = SseEndpointPlan::new(3, 4096).unwrap();
    assert_eq!(plan.max_pending_events(), 3);
    assert_eq!(plan.max_event_bytes(), 4096);
    assert_eq!(plan.keep_alive_ms(), None);
    assert!(plan.callbacks().is_none());
    roundtrip(&plan);
    assert_eq!(
        plan.clone().with_keep_alive_ms(0),
        Err(SseError::InvalidKeepAlive)
    );
    let plan = plan
        .with_keep_alive_ms(1000)
        .unwrap()
        .with_callbacks(sse_callbacks())
        .unwrap();
    assert_eq!(plan.keep_alive_ms(), Some(1000));
    assert_eq!(plan.callbacks(), Some(&sse_callbacks()));
    roundtrip(&plan);
    assert_eq!(
        plan.with_callbacks(sse_callbacks()),
        Err(SseError::CallbacksAlreadyConfigured)
    );
}

#[test]
fn websocket_policy_validates_limits_and_exclusive_callback_modes() {
    assert_eq!(
        WebSocketEndpointPlan::<Callback>::new(0, 1),
        Err(WebSocketPlanError::EmptyQueue)
    );
    assert_eq!(
        WebSocketEndpointPlan::<Callback>::new(1, 0),
        Err(WebSocketPlanError::EmptyFrame)
    );
    let plan = WebSocketEndpointPlan::new(3, 4096).unwrap();
    assert_eq!(plan.max_pending_frames(), 3);
    assert_eq!(plan.max_frame_bytes(), 4096);
    assert_eq!(plan.binary_payload_policy(), BinaryPayloadPolicy::Reject);
    assert!(plan.callbacks().is_none());
    assert!(plan.pairing().is_none());
    roundtrip(&plan);
    let callbacks = plan.clone().with_callbacks(ws_callbacks()).unwrap();
    assert_eq!(callbacks.callbacks(), Some(&ws_callbacks()));
    roundtrip(&callbacks);
    assert_eq!(
        callbacks.clone().with_callbacks(ws_callbacks()),
        Err(WebSocketPlanError::DuplicateCallbacks)
    );
    assert_eq!(
        callbacks.with_pairing(pairing()),
        Err(WebSocketPlanError::PairingConflict)
    );
    let paired = plan.with_pairing(pairing()).unwrap();
    assert_eq!(paired.pairing(), Some(&pairing()));
    roundtrip(&paired);
    assert_eq!(
        paired.clone().with_callbacks(ws_callbacks()),
        Err(WebSocketPlanError::CallbacksConflict)
    );
    assert_eq!(
        paired.with_pairing(pairing()),
        Err(WebSocketPlanError::DuplicatePairing)
    );
}

#[test]
fn paired_restoration_roundtrips_without_a_vm_callback_type() {
    let mut pair = pairing();
    pair.restoration = Some(WebSocketRestoration {
        waiting: callback(),
        peer_left: callback(),
        identity: callback(),
        room_identity: callback(),
        retention_ms: 5000,
        retained_room_capacity: 16,
        matched: callback(),
        restored: callback(),
    });
    let plan = WebSocketEndpointPlan::new(1, 1024)
        .unwrap()
        .with_pairing(pair)
        .unwrap();
    roundtrip(&plan);
    let mut legacy_naming = serde_json::to_value(&plan).unwrap();
    let restoration = legacy_naming["pairing"]["restoration"]
        .as_object_mut()
        .unwrap();
    restoration.remove("room_identity");
    restoration.insert("room_prefix".into(), json!("room-"));
    reject::<WebSocketEndpointPlan<Callback>>(legacy_naming);
    let mut legacy = serde_json::to_value(plan).unwrap();
    let restoration = legacy["pairing"]["restoration"].as_object_mut().unwrap();
    restoration.remove("identity");
    restoration.insert("room_query".into(), json!("room"));
    restoration.insert("player_query".into(), json!("player"));
    reject::<WebSocketEndpointPlan<Callback>>(legacy);
}

#[test]
fn deserialization_cannot_bypass_positive_limits_or_keep_alive() {
    let sse = json!({"max_pending_events": 1, "max_event_bytes": 1024});
    let ws = json!({"max_pending_frames": 1, "max_frame_bytes": 1024, "binary_payload_policy": "Reject"});
    for bad in [json!(0), json!(-1), json!("1"), Value::Null, json!(1.5)] {
        for key in ["max_pending_events", "max_event_bytes"] {
            let mut input = sse.clone();
            input[key] = bad.clone();
            reject::<SseEndpointPlan<Callback>>(input);
        }
        for key in ["max_pending_frames", "max_frame_bytes"] {
            let mut input = ws.clone();
            input[key] = bad.clone();
            reject::<WebSocketEndpointPlan<Callback>>(input);
        }
    }
    let mut input = sse;
    input["keep_alive_ms"] = json!(0);
    reject::<SseEndpointPlan<Callback>>(input);
    for input in [
        r#"{"max_pending_events":18446744073709551616,"max_event_bytes":1}"#,
        r#"{"max_pending_events":1,"max_event_bytes":1,"max_event_bytes":2}"#,
        r#"{"max_pending_events":1}"#,
    ] {
        assert!(serde_json::from_str::<SseEndpointPlan<Callback>>(input).is_err());
    }
    for input in [
        r#"{"max_pending_frames":18446744073709551616,"max_frame_bytes":1,"binary_payload_policy":"Reject"}"#,
        r#"{"max_pending_frames":1,"max_frame_bytes":1,"max_frame_bytes":2,"binary_payload_policy":"Reject"}"#,
    ] {
        assert!(serde_json::from_str::<WebSocketEndpointPlan<Callback>>(input).is_err());
    }
}

#[test]
fn deserialization_rejects_conflicting_or_incomplete_callbacks_and_unknown_policy() {
    let plan = WebSocketEndpointPlan::new(1, 1024)
        .unwrap()
        .with_callbacks(ws_callbacks())
        .unwrap();
    let mut input = serde_json::to_value(&plan).unwrap();
    input["pairing"] = serde_json::to_value(pairing()).unwrap();
    reject::<WebSocketEndpointPlan<Callback>>(input);
    let mut input = serde_json::to_value(&plan).unwrap();
    input["callbacks"]
        .as_object_mut()
        .unwrap()
        .remove("inbound");
    reject::<WebSocketEndpointPlan<Callback>>(input);
    let mut input = serde_json::to_value(&plan).unwrap();
    input["binary_payload_policy"] = json!("Allow");
    reject::<WebSocketEndpointPlan<Callback>>(input);
    let plan = SseEndpointPlan::new(1, 1024)
        .unwrap()
        .with_callbacks(sse_callbacks())
        .unwrap();
    let mut input = serde_json::to_value(plan).unwrap();
    input["callbacks"].as_object_mut().unwrap().remove("drain");
    reject::<SseEndpointPlan<Callback>>(input);
}

#[test]
fn optional_fields_preserve_legacy_wire_defaults() {
    let plan: SseEndpointPlan<Callback> = serde_json::from_value(json!({
        "max_pending_events": 1, "max_event_bytes": 1024,
    }))
    .unwrap();
    assert_eq!(plan, SseEndpointPlan::new(1, 1024).unwrap());
    let plan: WebSocketEndpointPlan<Callback> = serde_json::from_value(json!({
        "max_pending_frames": 1, "max_frame_bytes": 1024, "binary_payload_policy": "Reject",
    }))
    .unwrap();
    assert_eq!(plan, WebSocketEndpointPlan::new(1, 1024).unwrap());
    let mut pair = serde_json::to_value(pairing()).unwrap();
    pair.as_object_mut().unwrap().remove("restoration");
    assert_eq!(
        serde_json::from_value::<WebSocketPairing<Callback>>(pair).unwrap(),
        pairing()
    );
}

#[test]
fn pairing_rejects_obsolete_native_broadcast_switch() {
    for stateful in [false, true] {
        let mut pair = serde_json::to_value(pairing()).unwrap();
        pair["stateful"] = json!(stateful);
        reject::<WebSocketPairing<Callback>>(pair);
    }
}

#[test]
fn endpoint_errors_preserve_diagnostics() {
    for (error, reason) in [
        (
            WebSocketPlanError::EmptyQueue,
            "max_pending_frames must be greater than 0",
        ),
        (
            WebSocketPlanError::EmptyFrame,
            "max_frame_bytes must be greater than 0",
        ),
        (
            WebSocketPlanError::CallbacksConflict,
            "callbacks conflict with pairing",
        ),
        (
            WebSocketPlanError::DuplicateCallbacks,
            "callbacks already configured",
        ),
        (
            WebSocketPlanError::PairingConflict,
            "pairing conflicts with callbacks",
        ),
        (
            WebSocketPlanError::DuplicatePairing,
            "pairing already configured",
        ),
    ] {
        assert_eq!(
            error.to_string(),
            format!("error[vm_websocket_endpoint]: {reason}")
        );
    }
    assert_eq!(SseError::InvalidKeepAlive.to_string(), "InvalidKeepAlive");
}
