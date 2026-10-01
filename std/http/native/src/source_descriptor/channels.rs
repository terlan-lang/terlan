use super::*;
use crate::channel_plan::{
    SseCallbacks, SseEndpointPlan, WebSocketCallbacks, WebSocketEndpointPlan, WebSocketPairing,
    WebSocketRestoration,
};

/// Admits an executed SSE endpoint; callback authority remains with the host.
pub fn sse_endpoint<V: DescriptorValue, C>(
    value: &V,
    mut callback: impl FnMut(&V, usize) -> Result<C>,
) -> Result<SseEndpointPlan<C>> {
    let [pending, bytes, keep_alive, callbacks] = record(
        value,
        "Endpoint",
        [
            "max_pending_events",
            "max_event_bytes",
            "keep_alive_ms",
            "callbacks",
        ],
    )?;
    let mut plan = SseEndpointPlan::new(positive(pending)?, positive(bytes)?).map_err(error)?;
    match keep_alive.descriptor_view() {
        DescriptorView::Atom("none") => {}
        DescriptorView::Record("None", []) => {}
        _ => {
            let [interval] = record(keep_alive, "Some", ["value"])?;
            plan = plan
                .with_keep_alive_ms(positive_u64(interval)?)
                .map_err(error)?;
        }
    }
    let callbacks = list(callbacks)?;
    if callbacks.len() > 1 {
        return Err(error("SSE callbacks already configured"));
    }
    if let Some(value) = callbacks.first() {
        let [open, event_ready, keep_alive, drain, cancellation] = record(
            value,
            "Callbacks",
            ["open", "event_ready", "keep_alive", "drain", "cancellation"],
        )?;
        plan = plan
            .with_callbacks(SseCallbacks {
                open: callback(open, 0)?,
                event_ready: callback(event_ready, 1)?,
                keep_alive: callback(keep_alive, 0)?,
                drain: callback(drain, 0)?,
                cancellation: callback(cancellation, 1)?,
            })
            .map_err(error)?;
    }
    Ok(plan)
}

/// Admits an executed WebSocket endpoint, including stateful recovery policy.
pub fn websocket_endpoint<V: DescriptorValue, C>(
    value: &V,
    mut callback: impl FnMut(&V, usize) -> Result<C>,
) -> Result<WebSocketEndpointPlan<C>> {
    let [pending, bytes, policies] = record(
        value,
        "Endpoint",
        ["max_pending_frames", "max_frame_bytes", "policies"],
    )?;
    let plan = WebSocketEndpointPlan::new(positive(pending)?, positive(bytes)?)
        .map_err(|cause| error(cause.to_string()))?;
    let policies = list(policies)?;
    if policies.len() > 1 {
        return Err(error("WebSocket policies conflict or are duplicated"));
    }
    let Some(policy) = policies.first() else {
        return Ok(plan);
    };
    let (tag, fields) = variant(
        policy,
        &[
            (
                "Callbacks",
                &["open", "inbound", "writable", "close", "cancellation"],
            ),
            (
                "Pairing",
                &[
                    "waiting",
                    "first_matched",
                    "second_matched",
                    "peer_left",
                    "inbound",
                    "cancellation",
                ],
            ),
            (
                "Stateful_pairing",
                &[
                    "waiting",
                    "first_matched",
                    "second_matched",
                    "peer_left",
                    "inbound",
                    "cancellation",
                ],
            ),
            (
                "Restorable_pairing",
                &[
                    "waiting",
                    "peer_left",
                    "identity",
                    "room_prefix",
                    "retention_ms",
                    "retained_room_capacity",
                    "matched",
                    "restored",
                    "inbound",
                    "cancellation",
                ],
            ),
        ],
    )?;
    match (tag, fields.as_slice()) {
        ("Callbacks", [open, inbound, writable, close, cancellation]) => plan
            .with_callbacks(WebSocketCallbacks {
                open: callback(open, 0)?,
                inbound: callback(inbound, 1)?,
                writable: callback(writable, 0)?,
                close: callback(close, 0)?,
                cancellation: callback(cancellation, 1)?,
            })
            .map_err(|cause| error(cause.to_string())),
        (
            "Pairing" | "Stateful_pairing",
            [waiting, first, second, peer_left, inbound, cancellation],
        ) => plan
            .with_pairing(WebSocketPairing {
                waiting: text(*waiting)?,
                first_matched: text(*first)?,
                second_matched: text(*second)?,
                peer_left: text(*peer_left)?,
                stateful: tag == "Stateful_pairing",
                restoration: None,
                inbound: callback(inbound, if tag == "Stateful_pairing" { 5 } else { 1 })?,
                cancellation: callback(cancellation, 1)?,
            })
            .map_err(|cause| error(cause.to_string())),
        (
            "Restorable_pairing",
            [waiting, peer_left, identity, room_prefix, retention, capacity, matched, restored, inbound, cancellation],
        ) => plan
            .with_pairing(WebSocketPairing {
                waiting: String::new(),
                first_matched: String::new(),
                second_matched: String::new(),
                peer_left: String::new(),
                stateful: true,
                restoration: Some(WebSocketRestoration {
                    waiting: callback(waiting, 0)?,
                    peer_left: callback(peer_left, 0)?,
                    identity: callback(identity, 1)?,
                    room_prefix: text(*room_prefix)?,
                    retention_ms: positive_u64(*retention)?,
                    retained_room_capacity: positive(*capacity)?,
                    matched: callback(matched, 4)?,
                    restored: callback(restored, 5)?,
                }),
                inbound: callback(inbound, 5)?,
                cancellation: callback(cancellation, 1)?,
            })
            .map_err(|cause| error(cause.to_string())),
        _ => Err(error("unknown or malformed WebSocket policy")),
    }
}
