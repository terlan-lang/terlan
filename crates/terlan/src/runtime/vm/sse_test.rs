use super::{
    data, endpoint, endpoint_with_keep_alive, keep_alive_frame, response, with_id, with_name,
    with_retry_ms, VmAccountedSseError, VmAccountedSseStream, VmSseDomPatchBackpressure,
    VmSseEndpointPlan, VmSseError, VmSseEvent, VmSseHeartbeatState, VmSseProtocolAssetHashState,
    VmSseReconnectTokenState, VmSseStream,
};
use crate::runtime::vm::{
    memory::{VmMemoryAccountant, VmMemoryLimits},
    process::{VmProcessSource, VmProcessTable},
    scheduler::VmScheduler,
};

#[test]
fn vm_sse_event_encodes_typed_metadata_and_multiline_data() {
    let event = VmSseEvent::data("one\ntwo")
        .with_id("42")
        .with_event("counter")
        .with_retry_ms(1500);

    assert_eq!(
        String::from_utf8(event.encode().expect("encode")).expect("utf8"),
        "id: 42\nevent: counter\nretry: 1500\ndata: one\ndata: two\n\n"
    );
}

#[test]
fn vm_sse_event_rejects_invalid_metadata_and_retry() {
    assert_eq!(
        VmSseEvent::data("ok")
            .with_event("bad\nevent")
            .encode()
            .expect_err("newline event should fail"),
        VmSseError::InvalidEventName
    );
    assert_eq!(
        VmSseEvent::data("ok")
            .with_id("bad\0id")
            .encode()
            .expect_err("nul id should fail"),
        VmSseError::InvalidEventName
    );
    assert_eq!(
        VmSseEvent::data("ok")
            .with_retry_ms(0)
            .encode()
            .expect_err("zero retry should fail"),
        VmSseError::InvalidRetry
    );
}

#[test]
fn vm_sse_stream_preserves_order_and_reports_inspection_state() {
    let mut stream = VmSseStream::new(4, 128).expect("stream");

    stream.enqueue(VmSseEvent::data("one")).expect("one");
    stream.enqueue(VmSseEvent::data("two")).expect("two");

    assert_eq!(stream.inspect().pending_events, 2);
    assert_eq!(
        stream.flush_next().expect("flush one"),
        Some(b"data: one\n\n".to_vec())
    );
    assert_eq!(
        stream.flush_next().expect("flush two"),
        Some(b"data: two\n\n".to_vec())
    );
    assert_eq!(stream.flush_next().expect("empty"), None);
    assert_eq!(stream.inspect().emitted_events, 2);
}

#[test]
fn vm_sse_stream_enforces_backpressure_and_event_size() {
    let mut stream = VmSseStream::new(1, 16).expect("stream");

    stream.enqueue(VmSseEvent::data("ok")).expect("first");
    assert_eq!(
        stream
            .enqueue(VmSseEvent::data("second"))
            .expect_err("queue full"),
        VmSseError::BackpressureExceeded
    );

    let mut small = VmSseStream::new(2, 8).expect("small stream");
    assert_eq!(
        small
            .enqueue(VmSseEvent::data("too-large"))
            .expect_err("too large"),
        VmSseError::EventTooLarge
    );
}

#[test]
fn vm_sse_stream_close_rejects_new_events_but_flushes_pending() {
    let mut stream = VmSseStream::new(2, 128).expect("stream");

    stream.enqueue(VmSseEvent::data("queued")).expect("queued");
    stream.close();

    assert!(stream.inspect().closed);
    assert_eq!(
        stream
            .enqueue(VmSseEvent::data("late"))
            .expect_err("closed"),
        VmSseError::Closed
    );
    assert_eq!(
        stream.flush_next().expect("flush queued"),
        Some(b"data: queued\n\n".to_vec())
    );
}

#[test]
fn vm_accounted_sse_stream_rejects_pressure_before_queue_mutation() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Http", "sse", 0));
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(12, 20).expect("limits"));
    let mut scheduler = VmScheduler::default();
    let mut stream = VmAccountedSseStream::new(owner, 4, 128).expect("stream");

    stream
        .enqueue(
            &mut memory,
            &mut scheduler,
            &mut processes,
            VmSseEvent::data("one"),
        )
        .expect("first event");
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 11);
    assert_eq!(
        stream
            .enqueue(
                &mut memory,
                &mut scheduler,
                &mut processes,
                VmSseEvent::data("two"),
            )
            .expect_err("second event exceeds hard limit"),
        VmAccountedSseError::MemoryPressureRejected
    );
    assert_eq!(stream.inspect().pending_events, 1);
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 11);
    assert_eq!(
        stream
            .flush_next(&mut memory, &mut scheduler, &mut processes)
            .expect("flush"),
        Some(b"data: one\n\n".to_vec())
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(scheduler.memory_reductions(owner), 6);
    assert_eq!(scheduler.total_memory_reductions(), 6);
}

#[test]
fn vm_accounted_sse_stream_cancellation_releases_all_pending_buffers() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Http", "sse", 0));
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(64, 128).expect("limits"));
    let mut scheduler = VmScheduler::default();
    let mut stream = VmAccountedSseStream::new(owner, 4, 128).expect("stream");

    stream
        .enqueue(
            &mut memory,
            &mut scheduler,
            &mut processes,
            VmSseEvent::data("one"),
        )
        .expect("first event");
    stream
        .enqueue(
            &mut memory,
            &mut scheduler,
            &mut processes,
            VmSseEvent::data("two"),
        )
        .expect("second event");
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 22);
    assert_eq!(
        stream
            .cancel(&mut memory, &mut scheduler, &mut processes)
            .expect("cancel stream"),
        2
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(stream.inspect().pending_events, 0);
    assert!(stream.inspect().closed);
    assert_eq!(scheduler.memory_reductions(owner), 6);
    assert_eq!(
        stream
            .enqueue(
                &mut memory,
                &mut scheduler,
                &mut processes,
                VmSseEvent::data("late"),
            )
            .expect_err("cancelled stream rejects new event"),
        VmAccountedSseError::Stream(VmSseError::Closed)
    );
}

#[test]
fn vm_sse_keep_alive_frame_is_stable() {
    assert_eq!(keep_alive_frame(), b": keep-alive\n\n".to_vec());
}

#[test]
fn vm_sse_stream_rejects_zero_limits() {
    assert_eq!(
        VmSseStream::new(0, 1).expect_err("zero pending"),
        VmSseError::BackpressureExceeded
    );
    assert_eq!(
        VmSseStream::new(1, 0).expect_err("zero bytes"),
        VmSseError::BackpressureExceeded
    );
}

#[test]
fn vm_sse_endpoint_plan_opens_bounded_stream_with_keep_alive_policy() {
    let plan = VmSseEndpointPlan::new(3, 256)
        .expect("plan")
        .with_keep_alive_ms(10_000)
        .expect("keep alive");
    let stream = plan.open_stream().expect("stream");
    let info = stream.inspect();

    assert_eq!(plan.max_pending_events(), 3);
    assert_eq!(plan.max_event_bytes(), 256);
    assert_eq!(plan.keep_alive_ms(), Some(10_000));
    assert_eq!(info.max_pending_events, 3);
    assert_eq!(info.max_event_bytes, 256);
    assert_eq!(info.pending_events, 0);
}

#[test]
fn vm_sse_endpoint_plan_rejects_invalid_limits_and_keep_alive() {
    assert_eq!(
        VmSseEndpointPlan::new(0, 256).expect_err("zero pending"),
        VmSseError::BackpressureExceeded
    );
    assert_eq!(
        VmSseEndpointPlan::new(3, 0).expect_err("zero bytes"),
        VmSseError::BackpressureExceeded
    );
    assert_eq!(
        VmSseEndpointPlan::new(3, 256)
            .expect("plan")
            .with_keep_alive_ms(0)
            .expect_err("zero keep alive"),
        VmSseError::InvalidKeepAlive
    );
}

#[test]
fn vm_sse_heartbeat_timeout_tracks_stale_browser_streams() {
    let mut heartbeat = VmSseHeartbeatState::new(1_000, 10).expect("heartbeat");

    heartbeat.evaluate_timeout(1_010).expect("on boundary");
    assert!(!heartbeat.inspect().timed_out);

    assert_eq!(
        heartbeat
            .evaluate_timeout(1_011)
            .expect_err("stale heartbeat"),
        VmSseError::HeartbeatTimedOut
    );
    assert!(heartbeat.inspect().timed_out);

    heartbeat.record_heartbeat(1_500);
    assert_eq!(heartbeat.inspect().last_seen_ms, 1_500);
    assert!(!heartbeat.inspect().timed_out);
    heartbeat.evaluate_timeout(2_500).expect("recovered");
}

#[test]
fn vm_sse_heartbeat_timeout_rejects_zero_policy() {
    assert_eq!(
        VmSseHeartbeatState::new(0, 10).expect_err("zero timeout"),
        VmSseError::InvalidKeepAlive
    );
}

#[test]
fn vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens() {
    let mut tokens = VmSseReconnectTokenState::new("token-1", 10).expect("tokens");

    assert_eq!(tokens.current_token(), "token-1");
    let rotated = tokens
        .rotate("token-1", "token-2", 20)
        .expect("rotate token");

    assert_eq!(rotated.token, "token-2");
    assert_eq!(rotated.generation, 1);
    assert_eq!(rotated.rotated_at_ms, 20);
    assert_eq!(tokens.current_token(), "token-2");

    assert_eq!(
        tokens
            .rotate("token-1", "token-3", 30)
            .expect_err("stale token"),
        VmSseError::StaleReconnectToken
    );
    assert_eq!(
        tokens
            .rotate("token-2", "token-2", 40)
            .expect_err("unchanged token"),
        VmSseError::InvalidReconnectToken
    );

    let second = tokens
        .rotate("token-2", "token-3", 50)
        .expect("second rotate");
    assert_eq!(second.generation, 2);
    assert_eq!(tokens.inspect().token, "token-3");
}

#[test]
fn vm_sse_reconnect_token_rejects_empty_and_control_tokens() {
    assert_eq!(
        VmSseReconnectTokenState::new("", 10).expect_err("empty token"),
        VmSseError::InvalidReconnectToken
    );
    assert_eq!(
        VmSseReconnectTokenState::new("bad token", 10).expect_err("space token"),
        VmSseError::InvalidReconnectToken
    );
    assert_eq!(
        VmSseReconnectTokenState::new("token-1", 10)
            .expect("tokens")
            .rotate("token-1", "bad\nnext", 20)
            .expect_err("control token"),
        VmSseError::InvalidReconnectToken
    );
}

#[test]
fn vm_sse_protocol_asset_hash_rejects_stale_browser_assets() {
    let mut asset_hash =
        VmSseProtocolAssetHashState::new("asset-hash-a").expect("asset hash state");

    asset_hash
        .validate_presented_hash("asset-hash-a")
        .expect("current hash");
    assert_eq!(
        asset_hash
            .validate_presented_hash("asset-hash-old")
            .expect_err("stale hash"),
        VmSseError::StaleProtocolAssetHash
    );

    asset_hash
        .replace_hash("asset-hash-b")
        .expect("replace hash");
    assert_eq!(asset_hash.inspect().asset_hash, "asset-hash-b");
    assert_eq!(asset_hash.inspect().generation, 1);
    assert_eq!(
        asset_hash
            .validate_presented_hash("asset-hash-a")
            .expect_err("old hash after replace"),
        VmSseError::StaleProtocolAssetHash
    );
    asset_hash
        .validate_presented_hash("asset-hash-b")
        .expect("new hash");
}

#[test]
fn vm_sse_protocol_asset_hash_rejects_empty_and_control_hashes() {
    assert_eq!(
        VmSseProtocolAssetHashState::new("").expect_err("empty hash"),
        VmSseError::InvalidProtocolAssetHash
    );
    assert_eq!(
        VmSseProtocolAssetHashState::new("bad hash").expect_err("space hash"),
        VmSseError::InvalidProtocolAssetHash
    );
    assert_eq!(
        VmSseProtocolAssetHashState::new("asset-hash-a")
            .expect("asset hash")
            .validate_presented_hash("bad\nhash")
            .expect_err("control hash"),
        VmSseError::InvalidProtocolAssetHash
    );
}

#[test]
fn vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue() {
    let mut backpressure = VmSseDomPatchBackpressure::new(2).expect("backpressure");

    backpressure.queue_patch("patch-1").expect("patch 1");
    backpressure.queue_patch("patch-2").expect("patch 2");
    assert_eq!(backpressure.inspect().pending_patches, 2);
    assert_eq!(
        backpressure
            .queue_patch("patch-3")
            .expect_err("slow browser queue should be rejected"),
        VmSseError::DomPatchBackpressureExceeded
    );
    assert_eq!(backpressure.inspect().rejected_patches, 1);

    assert_eq!(
        backpressure.acknowledge_applied_patch().as_deref(),
        Some("patch-1")
    );
    assert_eq!(backpressure.inspect().applied_patches, 1);
    backpressure
        .queue_patch("patch-3")
        .expect("recovered queue");
    assert_eq!(backpressure.inspect().pending_patches, 2);
}

#[test]
fn vm_sse_dom_patch_backpressure_rejects_zero_limit() {
    assert_eq!(
        VmSseDomPatchBackpressure::new(0).expect_err("zero limit"),
        VmSseError::DomPatchBackpressureExceeded
    );
}

#[test]
fn vm_sse_adapter_functions_cover_manifest_surface() {
    let event = with_retry_ms(
        with_name(
            with_id(data("tick".to_string()), "42".to_string()),
            "counter".to_string(),
        ),
        1500,
    );

    assert_eq!(
        String::from_utf8(event.encode().expect("encode")).expect("utf8"),
        "id: 42\nevent: counter\nretry: 1500\ndata: tick\n\n"
    );
    assert_eq!(response(vec![event], 200).expect("response").1, 200);
    assert_eq!(endpoint(4, 128).expect("endpoint").max_pending_events(), 4);
    assert_eq!(
        endpoint_with_keep_alive(4, 128, 30_000)
            .expect("endpoint")
            .keep_alive_ms(),
        Some(30_000)
    );
}
