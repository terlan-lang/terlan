use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::support::validate_required_terms;
use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-live-template-stream-report.json";

const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_SSE_ANCHORS: &[&str] = &[
    "VmSseEvent",
    "VmSseEndpointPlan",
    "VmSseStream",
    "enqueue",
    "flush_next",
    "close",
    "inspect",
    "keep_alive_frame",
    "BackpressureExceeded",
];

const REQUIRED_WEBSOCKET_ANCHORS: &[&str] = &[
    "VmWebSocketEndpointPlan",
    "VmWebSocketInboundQueue",
    "VmWebSocketRuntime",
    "accept_upgrade",
    "send_frame_to_all_open_sessions",
    "receive_frame_with_auto_pong",
    "inspect_sessions",
    "remove_closed_sessions",
    "terminate_session_and_stream",
];

const REQUIRED_SOURCE_ANCHORS: &[&str] = &[
    "module std.http.LiveChannelTest",
    "counter_events_response",
    "Router.sse",
    "Router.websocket",
    "Router.group",
    "Sse.data",
    "with_id",
    "with_name",
    "with_retry_ms",
    "WebSocket.endpoint",
];

const REQUIRED_SESSION_CLEANUP_ANCHORS: &[&str] = &[
    "http_session_live_template_subscribers_are_cleaned_after_actor_exit",
    "subscribe_live_template",
    "live_template_subscribers",
];

const REQUIRED_COMMAND_IDEMPOTENCY_ANCHORS: &[&str] = &[
    "http_session_idempotent_command_replays_duplicate_result_without_rerun",
    "apply_idempotent_command",
    "command_results",
];

const REQUIRED_COMMAND_PAYLOAD_REJECTION_ANCHORS: &[&str] = &[
    "http_session_rejects_malformed_live_template_command_payload_before_dispatch",
    "apply_live_template_command",
    "VmHttpSessionCommandPayload",
];

const REQUIRED_COMMAND_POSTBACK_RUNTIME_ANCHORS: &[&str] = &[
    "dispatch_live_template_command_to_actor_mailbox",
    "live_template_command_actor_message",
    "enqueue_actor_message",
];

const REQUIRED_COMMAND_POSTBACK_TEST_ANCHORS: &[&str] = &[
    "http_session_live_template_command_dispatches_actor_mailbox_postback_once",
    "live_template_command_dispatched",
    "live_template_command",
];

const REQUIRED_CROSS_TEMPLATE_FANOUT_RUNTIME_ANCHORS: &[&str] = &[
    "fanout_live_template_state_update",
    "VmHttpSessionLiveTemplateStateFanout",
    "live_template_state_patch_event_id",
];

const REQUIRED_CROSS_TEMPLATE_FANOUT_TEST_ANCHORS: &[&str] = &[
    "http_session_live_template_state_update_fans_out_to_all_subscribers",
    "live_template_state_update",
    "HTTP live-template patch event cannot be empty",
];

const REQUIRED_ACTOR_RETURN_TYPE_REJECTION_RUNTIME_ANCHORS: &[&str] = &[
    "VmHttpSessionLiveTemplateSourceSpan",
    "validate_live_template_patch_payload",
    "invalid_template_actor_return_type",
];

const REQUIRED_ACTOR_RETURN_TYPE_REJECTION_TEST_ANCHORS: &[&str] = &[
    "http_session_rejects_unsupported_actor_patch_return_before_state_update",
    "invalid_template_actor_return_type",
    "assert!(!update_ran)",
];

const REQUIRED_CAPABILITY_SUBSCRIPTION_RUNTIME_ANCHORS: &[&str] = &[
    "subscribe_live_template_with_capability",
    "VmHttpSessionLiveTemplateSubscriptionAuthorization",
    "live_template_subscriber_capability_diagnostic",
];

const REQUIRED_CAPABILITY_SUBSCRIPTION_TEST_ANCHORS: &[&str] = &[
    "http_session_live_template_subscription_requires_capability_before_registering",
    "template:admin",
    "HTTP live-template granted capability cannot be empty",
];

const REQUIRED_ACTOR_BINDING_RUNTIME_ANCHORS: &[&str] = &[
    "bind_live_template_to_actor_state",
    "VmHttpSessionLiveTemplateActorBinding",
    "live_template_actor_binding_diagnostic",
];

const REQUIRED_ACTOR_BINDING_TEST_ANCHORS: &[&str] = &[
    "http_session_binds_typed_live_template_to_actor_state",
    "dashboard.counter",
    "HTTP live-template state key cannot be empty",
];

const REQUIRED_SOURCE_MAP_TRACE_RUNTIME_ANCHORS: &[&str] = &[
    "trace_live_template_subscription_with_source_map",
    "VmHttpSessionLiveTemplateSubscriptionTrace",
    "live_template_source_map_trace_diagnostic",
];

const REQUIRED_SOURCE_MAP_TRACE_TEST_ANCHORS: &[&str] = &[
    "http_session_traces_live_template_subscription_source_map",
    "app.Dashboard:12:5",
    "HTTP live-template source line must be greater than 0",
];

const REQUIRED_DOM_PATCH_BACKPRESSURE_ANCHORS: &[&str] = &[
    "vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue",
    "VmSseDomPatchBackpressure",
    "DomPatchBackpressureExceeded",
];

const REQUIRED_RECONNECT_TOKEN_ANCHORS: &[&str] = &[
    "vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens",
    "vm_sse_reconnect_token_rejects_empty_and_control_tokens",
    "VmSseReconnectTokenState",
    "StaleReconnectToken",
    "InvalidReconnectToken",
];

const REQUIRED_HOT_RELOAD_RUNTIME_ANCHORS: &[&str] = &[
    "VmHttpSessionHotReloadMigrationReport",
    "hot_reload_migration_compatibility_report",
    "hot_reload_migration_compatibility_diagnostic",
    "transient_subscribers",
];

const REQUIRED_HOT_RELOAD_TEST_ANCHORS: &[&str] = &[
    "http_session_reports_hot_reload_migration_compatibility",
    "live-template subscribers remain transient",
    "stale HTTP session",
];

const REQUIRED_ACTOR_RESTART_ANCHORS: &[&str] = &[
    "http_session_actor_crash_during_request_cleans_state_and_replaces_cookie",
    "http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state",
    "exit_actor",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-live-template-stream-check: \\",
    "vm-http-stateful-actor-session-check \\",
    "vm-http-sse-check \\",
    "vm-http-websocket-source-check \\",
    "vm-http-websocket-queue-check \\",
    "vm-http-websocket-termination-check \\",
    "vm-http-live-channel-source-check",
    "vm-live-template-stream",
];

const TEMPLATE_FIXTURES: &[&str] = &[
    "typed SSE live response descriptor",
    "grouped live channel routes",
    "bounded SSE endpoint with keep-alive",
    "bounded WebSocket endpoint",
    "VM SSE stream queue",
    "VM WebSocket inbound queue",
    "VM WebSocket session lifecycle",
    "VM WebSocket termination lifecycle",
];

const PATCH_EVENTS: &[&str] = &[
    "initial render event",
    "incremental DOM patch event",
    "typed error patch event",
    "redirect patch event",
    "heartbeat event",
];

const REJECTED_STREAM_PATHS: &[&str] =
    &["stateful VM values are rejected before actor-bound template update and fanout"];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm live template stream summary.
pub struct VmLiveTemplateStreamSummary {
    pub template_fixture_count: usize,
    pub patch_event_count: usize,
    pub rejected_stream_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm live template stream.
pub fn run_vm_live_template_stream(root: &Path) -> QualityResult<VmLiveTemplateStreamSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/sse.rs",
        REQUIRED_SSE_ANCHORS,
        "VM SSE live-template transport",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/websocket.rs",
        REQUIRED_WEBSOCKET_ANCHORS,
        "VM WebSocket live-template transport",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/LiveChannelTest.terl",
        REQUIRED_SOURCE_ANCHORS,
        "typed live-channel source fixture",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_SESSION_CLEANUP_ANCHORS,
        "live-template subscriber cleanup evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_COMMAND_IDEMPOTENCY_ANCHORS,
        "live-template command idempotency evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_COMMAND_PAYLOAD_REJECTION_ANCHORS,
        "live-template malformed command payload evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session/live_template_command.rs",
        REQUIRED_COMMAND_POSTBACK_RUNTIME_ANCHORS,
        "live-template command postback runtime evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_COMMAND_POSTBACK_TEST_ANCHORS,
        "live-template command postback test evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session.rs",
        REQUIRED_CROSS_TEMPLATE_FANOUT_RUNTIME_ANCHORS,
        "live-template cross-template fanout runtime evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_CROSS_TEMPLATE_FANOUT_TEST_ANCHORS,
        "live-template cross-template fanout test evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session/live_template_payload.rs",
        REQUIRED_ACTOR_RETURN_TYPE_REJECTION_RUNTIME_ANCHORS,
        "live-template actor return-type rejection runtime evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_live_template_payload_test.rs",
        REQUIRED_ACTOR_RETURN_TYPE_REJECTION_TEST_ANCHORS,
        "live-template actor return-type rejection test evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session.rs",
        REQUIRED_CAPABILITY_SUBSCRIPTION_RUNTIME_ANCHORS,
        "live-template capability subscription runtime evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_CAPABILITY_SUBSCRIPTION_TEST_ANCHORS,
        "live-template capability subscription test evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session.rs",
        REQUIRED_ACTOR_BINDING_RUNTIME_ANCHORS,
        "live-template actor binding runtime evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_ACTOR_BINDING_TEST_ANCHORS,
        "live-template actor binding test evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session.rs",
        REQUIRED_SOURCE_MAP_TRACE_RUNTIME_ANCHORS,
        "live-template source-map trace runtime evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_SOURCE_MAP_TRACE_TEST_ANCHORS,
        "live-template source-map trace test evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/sse_test.rs",
        REQUIRED_DOM_PATCH_BACKPRESSURE_ANCHORS,
        "live-template DOM patch backpressure evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/sse_test.rs",
        REQUIRED_RECONNECT_TOKEN_ANCHORS,
        "live-template reconnect token evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session.rs",
        REQUIRED_HOT_RELOAD_RUNTIME_ANCHORS,
        "live-template hot reload migration runtime evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_HOT_RELOAD_TEST_ANCHORS,
        "live-template hot reload migration test evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_ACTOR_RESTART_ANCHORS,
        "live-template actor restart evidence",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-live-template-stream", &diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan-vm-live-template-stream-report-v1",
        "templateFixtures": TEMPLATE_FIXTURES,
        "actorBindingTraces": {
            "implemented": true,
            "boundary": "bind_live_template_to_actor_state resolves typed template ids to VM actor/table state without browser-side untyped glue",
            "evidence": "http_session_binds_typed_live_template_to_actor_state"
        },
        "sourceMapSubscriptionTraces": {
            "implemented": true,
            "boundary": "trace_live_template_subscription_with_source_map records subscriber, template, actor, state version, and source location for replayable support bundles",
            "evidence": "http_session_traces_live_template_subscription_source_map"
        },
        "patchEvents": PATCH_EVENTS,
        "commandPostbacks": {
            "implemented": true,
            "boundary": "dispatch_live_template_command_to_actor_mailbox validates browser command postbacks and enqueues typed actor mailbox messages exactly once",
            "evidence": "http_session_live_template_command_dispatches_actor_mailbox_postback_once"
        },
        "crossTemplateStateUpdateFanout": {
            "implemented": true,
            "boundary": "fanout_live_template_state_update applies optimistic actor state updates and emits typed patch payloads for every live subscriber",
            "evidence": "http_session_live_template_state_update_fans_out_to_all_subscribers"
        },
        "actorReturnTypeRejection": {
            "implemented": true,
            "diagnostic": "invalid_template_actor_return_type",
            "boundary": "validate_live_template_patch_payload rejects stateful VM values with an exact source span before actor state mutation or subscriber fanout",
            "evidence": "http_session_rejects_unsupported_actor_patch_return_before_state_update"
        },
        "capabilityCheckedSubscriptions": {
            "implemented": true,
            "boundary": "subscribe_live_template_with_capability validates required template capability before subscriber registration",
            "evidence": "http_session_live_template_subscription_requires_capability_before_registering"
        },
        "duplicateCommandIdempotency": {
            "implemented": true,
            "evidence": "http_session_idempotent_command_replays_duplicate_result_without_rerun"
        },
        "malformedCommandPayloadRejection": {
            "implemented": true,
            "boundary": "apply_live_template_command validates command id and command name before handler dispatch",
            "evidence": "http_session_rejects_malformed_live_template_command_payload_before_dispatch"
        },
        "domPatchBackpressure": {
            "implemented": true,
            "boundary": "VmSseDomPatchBackpressure rejects slow browser patch queues before unbounded lag accumulates",
            "evidence": "vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue"
        },
        "actorRestartDuringStream": {
            "implemented": true,
            "evidence": [
                "http_session_actor_crash_during_request_cleans_state_and_replaces_cookie",
                "http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state"
            ]
        },
        "reconnectCases": {
            "implemented": true,
            "boundary": "VmSseReconnectTokenState rotates accepted tokens and rejects stale or malformed browser tokens",
            "evidence": [
                "vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens",
                "vm_sse_reconnect_token_rejects_empty_and_control_tokens"
            ]
        },
        "hotReloadSubscriberMigration": {
            "implemented": true,
            "boundary": "hot_reload_migration_compatibility_report preserves durable session state and reports live-template subscribers as transient across generations",
            "evidence": "http_session_reports_hot_reload_migration_compatibility"
        },
        "cancellationCases": [
            "SSE stream close rejects new events but flushes pending events",
            "WebSocket cancelled termination cancels stream without close frame"
        ],
        "backpressureCases": [
            "SSE pending event bound",
            "SSE event byte bound",
            "SSE DOM patch pending queue bound",
            "WebSocket inbound frame bound",
            "WebSocket frame byte bound"
        ],
        "subscriberCleanupResults": {
            "implemented": true,
            "evidence": "http_session_live_template_subscribers_are_cleaned_after_actor_exit"
        },
        "rejectedStreamPaths": REJECTED_STREAM_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM live template stream report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmLiveTemplateStreamSummary {
        template_fixture_count: TEMPLATE_FIXTURES.len(),
        patch_event_count: PATCH_EVENTS.len(),
        rejected_stream_path_count: REJECTED_STREAM_PATHS.len(),
        report_path,
    })
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile"))
        .map_err(|err| format!("Makefile: failed to read VM live template stream gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing VM live template stream gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "template fixtures",
        TEMPLATE_FIXTURES,
    ));
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "patch events",
        PATCH_EVENTS,
    ));
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "rejected stream paths",
        REJECTED_STREAM_PATHS,
    ));
    diagnostics
}

pub(crate) fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| {
                    format!(
                        "VM live template stream {label} entry `{entry}` uses placeholder term `{term}`"
                    )
                })
        })
        .collect()
}

fn render_failure(label: &str, diagnostics: &[String]) -> String {
    let mut message = format!("[{label}] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_live_template_stream_test.rs"]
#[cfg(test)]
mod vm_live_template_stream_test;
