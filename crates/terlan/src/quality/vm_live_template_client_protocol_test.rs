use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_live_template_client_protocol, validate_no_placeholder_report_entries};

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-vm-live-template-client-protocol-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    fn write_complete_fixture(&self) -> io::Result<()> {
        self.write(
            "crates/terlan/src/runtime/vm/sse.rs",
            r#"
VmSseEvent VmSseStream VmSseHeartbeatState keep_alive_frame
BackpressureExceeded HeartbeatTimedOut
VmSseReconnectTokenState StaleReconnectToken
VmSseProtocolAssetHashState StaleProtocolAssetHash
VmSseDomPatchBackpressure DomPatchBackpressureExceeded
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/sse_test.rs",
            r#"
vm_sse_heartbeat_timeout_tracks_stale_browser_streams
vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens
vm_sse_protocol_asset_hash_rejects_stale_browser_assets
vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/websocket.rs",
            r#"
VmWebSocketFrame VmWebSocketRuntime receive_frame_with_auto_pong
send_frame_to_all_open_sessions
"#,
        )?;
        self.write(
            "std/http/LiveChannelTest.terl",
            r#"
module std.http.LiveChannelTest.
Router.sse Router.websocket Sse.endpoint_with_keep_alive WebSocket.endpoint
"#,
        )?;
        self.write(
            "scripts/self_validation/AngularTsIntegrationTest.terl",
            r#"
external_sse_runtime_holds external_declarations_holds
RealtimeProtocolEventDetail RealtimeProtocolMessage
selected_angular_ts_integration_holds
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/live_template_protocol.rs",
            r#"
VmLiveTemplateProtocolManifest VmLiveTemplateProtocolEventKind
VmLiveTemplateAngularTsRuntimeModule
VmLiveTemplateJsProtocolBindingValidation
VmLiveTemplateWasmProtocolBindingModule VmLiveTemplateWasmProtocolBindingValidation
VmLiveTemplateRollingDeployCompatibilityPlan
VmLiveTemplateRollingDeployCompatibilityValidation
generate_vm_live_template_protocol_manifest generate_vm_live_template_angular_ts_runtime_module
generate_vm_live_template_wasm_protocol_binding_module
validate_vm_live_template_js_protocol_binding validate_vm_live_template_wasm_protocol_binding
validate_vm_live_template_rolling_deploy_compatibility
validate_vm_live_template_protocol_manifest
terlanLiveTemplateProtocolManifest validateTerlanLiveTemplateEvent connectTerlanLiveTemplateSse
terlan-vm-live-template-protocol-v1 angular-wave/angular.ts
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/live_template_protocol_test.rs",
            r#"
vm_live_template_protocol_manifest_lists_required_events_and_schema_hash
vm_live_template_protocol_manifest_rejects_duplicate_event_names
vm_live_template_protocol_generates_angular_ts_browser_runtime_module
vm_live_template_protocol_rejects_angular_ts_runtime_generation_for_stale_manifest
vm_live_template_protocol_validates_generated_js_protocol_binding
vm_live_template_protocol_rejects_generated_js_binding_missing_event_field
vm_live_template_protocol_generates_wasm_protocol_binding_manifest
vm_live_template_protocol_validates_generated_wasm_protocol_binding
vm_live_template_protocol_rejects_generated_wasm_binding_missing_export
vm_live_template_protocol_accepts_mixed_version_rolling_deploy_compatibility
vm_live_template_protocol_rejects_mixed_version_rolling_deploy_schema_drift
vm_live_template_protocol_rejects_mixed_version_rolling_deploy_stale_assets
vm_live_template_protocol_rejects_mixed_version_rolling_deploy_version_window_gap
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_session_test.rs",
            r#"
http_session_idempotent_command_replays_duplicate_result_without_rerun
apply_idempotent_command command_results
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/model_sync.rs",
            r#"
VmDomPatchTemplateBinding VmDomPatchOperation
replay_dom_patches_for_template_bindings
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/model_sync_test.rs",
            r#"
vm_model_sync_replays_dom_patches_against_typed_template_bindings
vm_model_sync_rejects_dom_patch_replay_for_missing_template_binding_field
"#,
        )?;
        self.write("Makefile", COMPLETE_MAKEFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
vm-live-template-client-protocol-check: \
	vm-live-template-stream-check \
	angular-ts-terlan-integration-check \
	angular-ts-namespace-generation-check
	$(CARGO) run -p terlan --bin terlan-quality --features quality-tools --quiet -- vm-live-template-client-protocol
"#;

#[test]
fn vm_live_template_client_protocol_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_live_template_client_protocol(repo.root()).expect("quality check");

    assert_eq!(summary.protocol_event_count, 10);
    assert_eq!(summary.payload_validation_case_count, 8);
    assert_eq!(summary.compatibility_case_count, 7);
    assert_eq!(summary.rejected_protocol_path_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-live-template-client-protocol-report-v1"));
    assert!(report.contains("protocolManifest"));
    assert!(report.contains("terlan-vm-live-template-protocol-v1"));
    assert!(report.contains("generate_vm_live_template_protocol_manifest"));
    assert!(report.contains("angularTsBrowserRuntimeModule"));
    assert!(report.contains("generated/terlan/live-template-protocol.mjs"));
    assert!(report.contains("generate_vm_live_template_angular_ts_runtime_module"));
    assert!(report.contains("jsProtocolBindingValidation"));
    assert!(report.contains("validate_vm_live_template_js_protocol_binding"));
    assert!(report.contains("wasmProtocolBindingValidation"));
    assert!(report.contains("validate_vm_live_template_wasm_protocol_binding"));
    assert!(report.contains("rollingDeployCompatibility"));
    assert!(report.contains("validate_vm_live_template_rolling_deploy_compatibility"));
    assert!(report.contains("commandReplayProtection"));
    assert!(
        report.contains("http_session_idempotent_command_replays_duplicate_result_without_rerun")
    );
    assert!(report.contains("heartbeatTimeoutHandling"));
    assert!(report.contains("vm_sse_heartbeat_timeout_tracks_stale_browser_streams"));
    assert!(report.contains("reconnectTokenRotation"));
    assert!(report.contains("vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens"));
    assert!(report.contains("staleAssetRejections"));
    assert!(report.contains("vm_sse_protocol_asset_hash_rejects_stale_browser_assets"));
    assert!(report.contains("domPatchReplayResults"));
    assert!(report.contains("vm_model_sync_replays_dom_patches_against_typed_template_bindings"));
    assert!(!report.contains("DOM patch replay against typed template bindings"));
    assert!(report.contains("domPatchBackpressure"));
    assert!(report.contains("vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue"));
    assert!(report.contains("angular-wave/angular.ts"));
    assert!(!report.contains("slow DOM patch application backpressure"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_angular_anchor() {
    let repo = TestRepo::new("missing-angular-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("scripts/self_validation/AngularTsIntegrationTest.terl");
    let source = fs::read_to_string(&path).expect("angular checker");
    repo.write(
        "scripts/self_validation/AngularTsIntegrationTest.terl",
        &source.replace("RealtimeProtocolMessage", ""),
    )
    .expect("rewrite angular checker");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("RealtimeProtocolMessage"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_protocol_manifest_anchor() {
    let repo = TestRepo::new("missing-protocol-manifest-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/live_template_protocol.rs");
    let source = fs::read_to_string(&path).expect("manifest source");
    repo.write(
        "crates/terlan/src/runtime/vm/live_template_protocol.rs",
        &source.replace("VmLiveTemplateProtocolManifest", ""),
    )
    .expect("rewrite manifest");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmLiveTemplateProtocolManifest"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_transport_anchor() {
    let repo = TestRepo::new("missing-transport-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/websocket.rs");
    let source = fs::read_to_string(&path).expect("websocket source");
    repo.write(
        "crates/terlan/src/runtime/vm/websocket.rs",
        &source.replace("receive_frame_with_auto_pong", ""),
    )
    .expect("rewrite websocket");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("receive_frame_with_auto_pong"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_heartbeat_anchor() {
    let repo = TestRepo::new("missing-heartbeat-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/sse.rs");
    let source = fs::read_to_string(&path).expect("sse source");
    repo.write(
        "crates/terlan/src/runtime/vm/sse.rs",
        &source.replace("VmSseHeartbeatState", ""),
    )
    .expect("rewrite sse source");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmSseHeartbeatState"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_reconnect_anchor() {
    let repo = TestRepo::new("missing-reconnect-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/sse.rs");
    let source = fs::read_to_string(&path).expect("sse source");
    repo.write(
        "crates/terlan/src/runtime/vm/sse.rs",
        &source.replace("VmSseReconnectTokenState", ""),
    )
    .expect("rewrite sse source");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmSseReconnectTokenState"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_asset_hash_anchor() {
    let repo = TestRepo::new("missing-asset-hash-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/sse.rs");
    let source = fs::read_to_string(&path).expect("sse source");
    repo.write(
        "crates/terlan/src/runtime/vm/sse.rs",
        &source.replace("VmSseProtocolAssetHashState", ""),
    )
    .expect("rewrite sse source");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmSseProtocolAssetHashState"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_dom_patch_backpressure_anchor() {
    let repo = TestRepo::new("missing-dom-patch-backpressure-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/sse.rs");
    let source = fs::read_to_string(&path).expect("sse source");
    repo.write(
        "crates/terlan/src/runtime/vm/sse.rs",
        &source.replace("VmSseDomPatchBackpressure", ""),
    )
    .expect("rewrite sse source");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmSseDomPatchBackpressure"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_command_replay_anchor() {
    let repo = TestRepo::new("missing-command-replay-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/http_session_test.rs");
    let source = fs::read_to_string(&path).expect("http session source");
    repo.write(
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        &source.replace("apply_idempotent_command", ""),
    )
    .expect("rewrite http session");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("apply_idempotent_command"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_dom_patch_replay_anchor() {
    let repo = TestRepo::new("missing-dom-patch-replay-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/model_sync.rs");
    let source = fs::read_to_string(&path).expect("model sync source");
    repo.write(
        "crates/terlan/src/runtime/vm/model_sync.rs",
        &source.replace("replay_dom_patches_for_template_bindings", ""),
    )
    .expect("rewrite model sync");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("replay_dom_patches_for_template_bindings"));
}

#[test]
fn vm_live_template_client_protocol_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate-term").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("angular-ts-namespace-generation-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_live_template_client_protocol(repo.root()).expect_err("gate should fail");

    assert!(error.contains("angular-ts-namespace-generation-check"));
}

#[test]
fn vm_live_template_client_protocol_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries(
        "payload validation cases",
        &["payload validation placeholder case"],
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected placeholder report entry diagnostic: {diagnostics:?}"
    );
}
