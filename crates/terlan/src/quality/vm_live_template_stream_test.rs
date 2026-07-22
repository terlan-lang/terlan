use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_live_template_stream, validate_entries_for_placeholder_terms,
    validate_no_placeholder_report_entries,
};

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
            "terlan-vm-live-template-stream-{name}-{}-{unique}",
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
	VmSseEvent VmSseEndpointPlan VmSseStream enqueue flush_next close inspect
	keep_alive_frame BackpressureExceeded
	"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/sse_test.rs",
            r#"
	vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue
	VmSseDomPatchBackpressure DomPatchBackpressureExceeded
	vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens
	vm_sse_reconnect_token_rejects_empty_and_control_tokens
	VmSseReconnectTokenState StaleReconnectToken InvalidReconnectToken
	"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/websocket.rs",
            r#"
VmWebSocketEndpointPlan VmWebSocketInboundQueue VmWebSocketRuntime accept_upgrade
send_frame_to_all_open_sessions receive_frame_with_auto_pong inspect_sessions
remove_closed_sessions terminate_session_and_stream
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_session.rs",
            r#"
VmHttpSessionHotReloadMigrationReport hot_reload_migration_compatibility_report
hot_reload_migration_compatibility_diagnostic transient_subscribers
	fanout_live_template_state_update VmHttpSessionLiveTemplateStateFanout
	live_template_state_patch_event_id
	subscribe_live_template_with_capability
	VmHttpSessionLiveTemplateSubscriptionAuthorization
	live_template_subscriber_capability_diagnostic
	bind_live_template_to_actor_state
	VmHttpSessionLiveTemplateActorBinding
	live_template_actor_binding_diagnostic
	trace_live_template_subscription_with_source_map
	VmHttpSessionLiveTemplateSubscriptionTrace
	live_template_source_map_trace_diagnostic
	"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_session/live_template_command.rs",
            r#"
	dispatch_live_template_command_to_actor_mailbox live_template_command_actor_message
	enqueue_actor_message
	"#,
        )?;
        self.write(
            "std/http/LiveChannelTest.terl",
            r#"
module std.http.LiveChannelTest.
counter_events_response Router.sse Router.websocket Router.group Sse.data
with_id with_name with_retry_ms WebSocket.endpoint
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_session_test.rs",
            r#"
	http_session_live_template_subscribers_are_cleaned_after_actor_exit
	subscribe_live_template live_template_subscribers
	http_session_idempotent_command_replays_duplicate_result_without_rerun
	apply_idempotent_command command_results
	http_session_rejects_malformed_live_template_command_payload_before_dispatch
	apply_live_template_command VmHttpSessionCommandPayload
	http_session_live_template_command_dispatches_actor_mailbox_postback_once
	live_template_command_dispatched live_template_command
	http_session_live_template_state_update_fans_out_to_all_subscribers
	live_template_state_update HTTP live-template patch event cannot be empty
	http_session_live_template_subscription_requires_capability_before_registering
	template:admin HTTP live-template granted capability cannot be empty
	http_session_binds_typed_live_template_to_actor_state
	dashboard.counter HTTP live-template state key cannot be empty
	http_session_traces_live_template_subscription_source_map
	app.Dashboard:12:5 HTTP live-template source line must be greater than 0
	http_session_actor_crash_during_request_cleans_state_and_replaces_cookie
	http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state
	http_session_reports_hot_reload_migration_compatibility
	live-template subscribers remain transient
	stale HTTP session
	exit_actor
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_session/live_template_payload.rs",
            r#"
	VmHttpSessionLiveTemplateSourceSpan validate_live_template_patch_payload
	invalid_template_actor_return_type
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_session_live_template_payload_test.rs",
            r#"
	http_session_rejects_unsupported_actor_patch_return_before_state_update
	invalid_template_actor_return_type assert!(!update_ran)
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
vm-live-template-stream-check: \
	vm-http-stateful-actor-session-check \
	vm-http-sse-check \
	vm-http-websocket-source-check \
	vm-http-websocket-queue-check \
	vm-http-websocket-termination-check \
	vm-http-live-channel-source-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_session::http_session_live_template_payload_test::http_session_rejects_unsupported_actor_patch_return_before_state_update -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_live_template_stream_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-live-template-stream
"#;

#[test]
fn vm_live_template_stream_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_live_template_stream(repo.root()).expect("quality check");

    assert_eq!(summary.template_fixture_count, 8);
    assert_eq!(summary.patch_event_count, 5);
    assert_eq!(summary.rejected_stream_path_count, 1);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-live-template-stream-report-v1"));
    assert!(report.contains("templateFixtures"));
    assert!(report.contains("actorBindingTraces"));
    assert!(report.contains("http_session_binds_typed_live_template_to_actor_state"));
    assert!(!report.contains("typed template-to-actor binding"));
    assert!(report.contains("sourceMapSubscriptionTraces"));
    assert!(report.contains("http_session_traces_live_template_subscription_source_map"));
    assert!(!report.contains("source-map aware template subscription traces"));
    assert!(report.contains("commandPostbacks"));
    assert!(report
        .contains("http_session_live_template_command_dispatches_actor_mailbox_postback_once"));
    assert!(!report.contains("command postback dispatch into actor mailbox"));
    assert!(!report.contains("command postback dispatch remains rejected"));
    assert!(report.contains("crossTemplateStateUpdateFanout"));
    assert!(report.contains("http_session_live_template_state_update_fans_out_to_all_subscribers"));
    assert!(!report.contains("cross-template state update fanout"));
    assert!(report.contains("actorReturnTypeRejection"));
    assert!(report.contains("invalid_template_actor_return_type"));
    assert!(
        report.contains("http_session_rejects_unsupported_actor_patch_return_before_state_update")
    );
    assert!(report.contains("capabilityCheckedSubscriptions"));
    assert!(report.contains(
        "http_session_live_template_subscription_requires_capability_before_registering"
    ));
    assert!(!report.contains("capability-checked template subscriptions"));
    assert!(report.contains("actorRestartDuringStream"));
    assert!(
        report.contains("http_session_actor_crash_during_request_cleans_state_and_replaces_cookie")
    );
    assert!(report.contains(
        "http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state"
    ));
    assert!(report.contains("duplicateCommandIdempotency"));
    assert!(
        report.contains("http_session_idempotent_command_replays_duplicate_result_without_rerun")
    );
    assert!(report.contains("malformedCommandPayloadRejection"));
    assert!(report
        .contains("http_session_rejects_malformed_live_template_command_payload_before_dispatch"));
    assert!(!report.contains("malformed command payload rejection"));
    assert!(report.contains("domPatchBackpressure"));
    assert!(report.contains("vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue"));
    assert!(!report.contains("slow client DOM patch backpressure"));
    assert!(report.contains("reconnectCases"));
    assert!(report.contains("vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens"));
    assert!(report.contains("vm_sse_reconnect_token_rejects_empty_and_control_tokens"));
    assert!(!report.contains("dropped client reconnect token validation"));
    assert!(!report.contains("reconnect token validation remains rejected"));
    assert!(report.contains("hotReloadSubscriberMigration"));
    assert!(report.contains("http_session_reports_hot_reload_migration_compatibility"));
    assert!(!report.contains("incompatible hot reload subscriber migration"));
    assert!(report.contains("subscriberCleanupResults"));
    assert!(report.contains("http_session_live_template_subscribers_are_cleaned_after_actor_exit"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_live_template_stream_rejects_missing_sse_anchor() {
    let repo = TestRepo::new("missing-sse-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let sse =
        fs::read_to_string(repo.root().join("crates/terlan/src/runtime/vm/sse.rs")).expect("sse");
    repo.write(
        "crates/terlan/src/runtime/vm/sse.rs",
        &sse.replace("BackpressureExceeded", ""),
    )
    .expect("rewrite sse");

    let error = run_vm_live_template_stream(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("BackpressureExceeded"));
}

#[test]
fn vm_live_template_stream_rejects_missing_source_anchor() {
    let repo = TestRepo::new("missing-source-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let source =
        fs::read_to_string(repo.root().join("std/http/LiveChannelTest.terl")).expect("source");
    repo.write(
        "std/http/LiveChannelTest.terl",
        &source.replace("Router.websocket", ""),
    )
    .expect("rewrite source");

    let error = run_vm_live_template_stream(repo.root()).expect_err("source should fail");

    assert!(error.contains("Router.websocket"));
}

#[test]
fn vm_live_template_stream_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate-term").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("\tvm-http-websocket-queue-check \\\n", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_live_template_stream(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm-http-websocket-queue-check"));
}

#[test]
fn vm_live_template_stream_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM live template stream report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected =
        validate_entries_for_placeholder_terms("patch events", &["placeholder patch event"]);
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
