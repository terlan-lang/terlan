use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-live-template-client-protocol-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_SSE_ANCHORS: &[&str] = &[
    "VmSseEvent",
    "VmSseStream",
    "VmSseHeartbeatState",
    "VmSseReconnectTokenState",
    "VmSseProtocolAssetHashState",
    "VmSseDomPatchBackpressure",
    "keep_alive_frame",
    "BackpressureExceeded",
    "HeartbeatTimedOut",
    "StaleReconnectToken",
    "StaleProtocolAssetHash",
    "DomPatchBackpressureExceeded",
];

const REQUIRED_SSE_TEST_ANCHORS: &[&str] = &[
    "vm_sse_heartbeat_timeout_tracks_stale_browser_streams",
    "vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens",
    "vm_sse_protocol_asset_hash_rejects_stale_browser_assets",
    "vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue",
];

const REQUIRED_WEBSOCKET_ANCHORS: &[&str] = &[
    "VmWebSocketFrame",
    "VmWebSocketRuntime",
    "receive_frame_with_auto_pong",
    "send_frame_to_all_open_sessions",
];

const REQUIRED_LIVE_CHANNEL_ANCHORS: &[&str] = &[
    "module std.http.LiveChannelTest",
    "Router.sse",
    "Router.websocket",
    "Sse.endpoint_with_keep_alive",
    "WebSocket.endpoint",
];

const REQUIRED_ANGULAR_CHECK_ANCHORS: &[&str] = &[
    "validate_external_sse_runtime_contract",
    "validate_external_sse_declaration_contract",
    "RealtimeProtocolEventDetail",
    "RealtimeProtocolMessage",
    "angular-ts namespace generation boundary passed",
];

const REQUIRED_PROTOCOL_MANIFEST_ANCHORS: &[&str] = &[
    "VmLiveTemplateProtocolManifest",
    "VmLiveTemplateProtocolEventKind",
    "VmLiveTemplateAngularTsRuntimeModule",
    "VmLiveTemplateJsProtocolBindingValidation",
    "VmLiveTemplateWasmProtocolBindingModule",
    "VmLiveTemplateWasmProtocolBindingValidation",
    "VmLiveTemplateRollingDeployCompatibilityPlan",
    "VmLiveTemplateRollingDeployCompatibilityValidation",
    "generate_vm_live_template_protocol_manifest",
    "generate_vm_live_template_angular_ts_runtime_module",
    "generate_vm_live_template_wasm_protocol_binding_module",
    "validate_vm_live_template_js_protocol_binding",
    "validate_vm_live_template_wasm_protocol_binding",
    "validate_vm_live_template_rolling_deploy_compatibility",
    "validate_vm_live_template_protocol_manifest",
    "terlanLiveTemplateProtocolManifest",
    "validateTerlanLiveTemplateEvent",
    "connectTerlanLiveTemplateSse",
    "terlan-vm-live-template-protocol-v1",
    "angular-wave/angular.ts",
];

const REQUIRED_PROTOCOL_MANIFEST_TEST_ANCHORS: &[&str] = &[
    "vm_live_template_protocol_manifest_lists_required_events_and_schema_hash",
    "vm_live_template_protocol_manifest_rejects_duplicate_event_names",
    "vm_live_template_protocol_generates_angular_ts_browser_runtime_module",
    "vm_live_template_protocol_rejects_angular_ts_runtime_generation_for_stale_manifest",
    "vm_live_template_protocol_validates_generated_js_protocol_binding",
    "vm_live_template_protocol_rejects_generated_js_binding_missing_event_field",
    "vm_live_template_protocol_generates_wasm_protocol_binding_manifest",
    "vm_live_template_protocol_validates_generated_wasm_protocol_binding",
    "vm_live_template_protocol_rejects_generated_wasm_binding_missing_export",
    "vm_live_template_protocol_accepts_mixed_version_rolling_deploy_compatibility",
    "vm_live_template_protocol_rejects_mixed_version_rolling_deploy_schema_drift",
    "vm_live_template_protocol_rejects_mixed_version_rolling_deploy_stale_assets",
    "vm_live_template_protocol_rejects_mixed_version_rolling_deploy_version_window_gap",
];

const REQUIRED_COMMAND_REPLAY_ANCHORS: &[&str] = &[
    "http_session_idempotent_command_replays_duplicate_result_without_rerun",
    "apply_idempotent_command",
    "command_results",
];

const REQUIRED_DOM_PATCH_REPLAY_ANCHORS: &[&str] = &[
    "VmDomPatchTemplateBinding",
    "VmDomPatchOperation",
    "replay_dom_patches_for_template_bindings",
];

const REQUIRED_DOM_PATCH_REPLAY_TEST_ANCHORS: &[&str] = &[
    "vm_model_sync_replays_dom_patches_against_typed_template_bindings",
    "vm_model_sync_rejects_dom_patch_replay_for_missing_template_binding_field",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-live-template-client-protocol-check: vm-live-template-stream-check",
    "$(MAKE) angular-ts-terlan-integration-check",
    "$(MAKE) angular-ts-namespace-generation-check",
    "vm_live_template_protocol_manifest_lists_required_events_and_schema_hash",
    "vm_live_template_protocol_generates_angular_ts_browser_runtime_module",
    "vm_live_template_protocol_validates_generated_js_protocol_binding",
    "vm_live_template_protocol_validates_generated_wasm_protocol_binding",
    "vm_live_template_protocol_accepts_mixed_version_rolling_deploy_compatibility",
    "vm_model_sync_replays_dom_patches_against_typed_template_bindings",
    "vm_live_template_client_protocol_test",
    "vm-live-template-client-protocol",
];

const PROTOCOL_EVENTS: &[&str] = &[
    "initialRender",
    "incrementalPatch",
    "commandPostback",
    "errorPatch",
    "redirectPatch",
    "reconnectToken",
    "heartbeat",
    "clientCancellation",
    "backpressureSignal",
    "versionNegotiation",
];

const PAYLOAD_VALIDATION_CASES: &[&str] = &[
    "malformed patch payload",
    "unknown patch event kind",
    "missing command id",
    "duplicate command id",
    "replayed command id",
    "missing reconnect token",
    "wrong protocol version",
    "stale compiler asset hash",
];

const COMPATIBILITY_CASES: &[&str] = &[
    "VM hot reload with unchanged protocol hash",
    "VM hot reload with incompatible protocol hash",
    "generated JS binding refresh",
    "generated Wasm binding refresh",
    "browser reconnect after server restart",
    "mixed old and new client assets",
    "package update changes template binding schema",
];

const REJECTED_PROTOCOL_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmLiveTemplateClientProtocolSummary {
    pub protocol_event_count: usize,
    pub payload_validation_case_count: usize,
    pub compatibility_case_count: usize,
    pub rejected_protocol_path_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_live_template_client_protocol(
    root: &Path,
) -> QualityResult<VmLiveTemplateClientProtocolSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/sse.rs",
        REQUIRED_SSE_ANCHORS,
        "VM SSE protocol transport",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/sse_test.rs",
        REQUIRED_SSE_TEST_ANCHORS,
        "VM SSE protocol transport tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/websocket.rs",
        REQUIRED_WEBSOCKET_ANCHORS,
        "VM WebSocket protocol transport",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/LiveChannelTest.terl",
        REQUIRED_LIVE_CHANNEL_ANCHORS,
        "typed live-channel protocol fixture",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "tools/check_angular_ts_terlan_integration.py",
        REQUIRED_ANGULAR_CHECK_ANCHORS,
        "Angular.ts browser protocol boundary",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/live_template_protocol.rs",
        REQUIRED_PROTOCOL_MANIFEST_ANCHORS,
        "typed protocol manifest generation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/live_template_protocol_test.rs",
        REQUIRED_PROTOCOL_MANIFEST_TEST_ANCHORS,
        "typed protocol manifest tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_COMMAND_REPLAY_ANCHORS,
        "command replay protection evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/model_sync.rs",
        REQUIRED_DOM_PATCH_REPLAY_ANCHORS,
        "typed DOM patch replay evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/model_sync_test.rs",
        REQUIRED_DOM_PATCH_REPLAY_TEST_ANCHORS,
        "typed DOM patch replay tests",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries(
        "protocol events",
        PROTOCOL_EVENTS,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "payload validation cases",
        PAYLOAD_VALIDATION_CASES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "compatibility cases",
        COMPATIBILITY_CASES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "rejected protocol paths",
        REJECTED_PROTOCOL_PATHS,
    ));
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "vm-live-template-client-protocol",
            &diagnostics,
        ));
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
        "schema": "terlan-vm-live-template-client-protocol-report-v1",
        "protocolManifest": {
            "implemented": true,
            "schema": "terlan-vm-live-template-protocol-v1",
            "version": 1,
            "requiredEvents": PROTOCOL_EVENTS,
            "evidence": [
                "VmLiveTemplateProtocolManifest",
                "VmLiveTemplateProtocolEventKind",
                "generate_vm_live_template_protocol_manifest",
                "validate_vm_live_template_protocol_manifest",
                "vm_live_template_protocol_manifest_lists_required_events_and_schema_hash"
            ]
        },
        "generatedTargets": ["js.shared", "js.browser", "wasm"],
        "payloadValidationCases": PAYLOAD_VALIDATION_CASES,
        "compatibilityMatrix": COMPATIBILITY_CASES,
        "reconnectCases": [
            "browser reconnect after server restart",
            "dropped reconnect token",
            "stale reconnect token after hot reload"
        ],
        "staleAssetRejections": {
            "implemented": true,
            "evidence": [
                "VmSseProtocolAssetHashState",
                "StaleProtocolAssetHash",
                "vm_sse_protocol_asset_hash_rejects_stale_browser_assets"
            ]
        },
        "domPatchReplayResults": {
            "implemented": true,
            "evidence": [
                "VmDomPatchTemplateBinding",
                "VmDomPatchOperation",
                "replay_dom_patches_for_template_bindings",
                "vm_model_sync_replays_dom_patches_against_typed_template_bindings"
            ]
        },
        "domPatchBackpressure": {
            "implemented": true,
            "evidence": [
                "VmSseDomPatchBackpressure",
                "DomPatchBackpressureExceeded",
                "vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue"
            ]
        },
        "commandReplayProtection": {
            "implemented": true,
            "evidence": [
                "http_session_idempotent_command_replays_duplicate_result_without_rerun",
                "apply_idempotent_command",
                "command_results"
            ]
        },
        "heartbeatTimeoutHandling": {
            "implemented": true,
            "evidence": [
                "VmSseHeartbeatState",
                "HeartbeatTimedOut",
                "vm_sse_heartbeat_timeout_tracks_stale_browser_streams"
            ]
        },
        "reconnectTokenRotation": {
            "implemented": true,
            "evidence": [
                "VmSseReconnectTokenState",
                "StaleReconnectToken",
                "vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens"
            ]
        },
        "angularTsBoundary": {
            "required": true,
            "source": "angular-wave/angular.ts",
            "validatedBy": [
                "angular-ts-terlan-integration-check",
                "angular-ts-namespace-generation-check"
            ]
        },
        "angularTsBrowserRuntimeModule": {
            "implemented": true,
            "path": "generated/terlan/live-template-protocol.mjs",
            "evidence": [
                "VmLiveTemplateAngularTsRuntimeModule",
                "generate_vm_live_template_angular_ts_runtime_module",
                "terlanLiveTemplateProtocolManifest",
                "validateTerlanLiveTemplateEvent",
                "connectTerlanLiveTemplateSse",
                "vm_live_template_protocol_generates_angular_ts_browser_runtime_module"
            ]
        },
        "jsProtocolBindingValidation": {
            "implemented": true,
            "target": "js.browser",
            "evidence": [
                "VmLiveTemplateJsProtocolBindingValidation",
                "validate_vm_live_template_js_protocol_binding",
                "vm_live_template_protocol_validates_generated_js_protocol_binding",
                "vm_live_template_protocol_rejects_generated_js_binding_missing_event_field"
            ]
        },
        "wasmProtocolBindingValidation": {
            "implemented": true,
            "target": "wasm.core",
            "evidence": [
                "VmLiveTemplateWasmProtocolBindingModule",
                "VmLiveTemplateWasmProtocolBindingValidation",
                "generate_vm_live_template_wasm_protocol_binding_module",
                "validate_vm_live_template_wasm_protocol_binding",
                "vm_live_template_protocol_generates_wasm_protocol_binding_manifest",
                "vm_live_template_protocol_validates_generated_wasm_protocol_binding",
                "vm_live_template_protocol_rejects_generated_wasm_binding_missing_export"
            ]
        },
        "rollingDeployCompatibility": {
            "implemented": true,
            "evidence": [
                "VmLiveTemplateRollingDeployCompatibilityPlan",
                "VmLiveTemplateRollingDeployCompatibilityValidation",
                "validate_vm_live_template_rolling_deploy_compatibility",
                "vm_live_template_protocol_accepts_mixed_version_rolling_deploy_compatibility",
                "vm_live_template_protocol_rejects_mixed_version_rolling_deploy_schema_drift",
                "vm_live_template_protocol_rejects_mixed_version_rolling_deploy_stale_assets",
                "vm_live_template_protocol_rejects_mixed_version_rolling_deploy_version_window_gap"
            ],
            "policy": [
                "protocol windows must overlap",
                "schema hashes must match",
                "protocol version changes must rotate browser asset hashes"
            ]
        },
        "rejectedProtocolPaths": REJECTED_PROTOCOL_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report).map_err(|err| {
        format!("failed to serialize VM live template client protocol report: {err}")
    })?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmLiveTemplateClientProtocolSummary {
        protocol_event_count: PROTOCOL_EVENTS.len(),
        payload_validation_case_count: PAYLOAD_VALIDATION_CASES.len(),
        compatibility_case_count: COMPATIBILITY_CASES.len(),
        rejected_protocol_path_count: REJECTED_PROTOCOL_PATHS.len(),
        report_path,
    })
}

fn validate_required_terms(
    root: &Path,
    relative: &str,
    terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read {label}: {err}"))?;
    Ok(terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing {label} anchor `{term}`"))
        .collect())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile")).map_err(|err| {
        format!("Makefile: failed to read VM live template client protocol gate: {err}")
    })?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| {
            format!("Makefile: missing VM live template client protocol gate term `{term}`")
        })
        .collect())
}

pub fn validate_no_placeholder_report_entries(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| {
                    format!(
                        "VM live template client protocol {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_live_template_client_protocol_test.rs"]
mod vm_live_template_client_protocol_test;
