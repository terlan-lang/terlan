use super::*;
use crate::commands::serve::args::{
    ServeArgs, ServeCliOverrides, ServeHandlerRuntime, DEFAULT_POLL_MS, DEFAULT_SERVE_HOST,
    DEFAULT_SERVE_PORT,
};
use crate::commands::serve::config::resolve_effective_serve_config_with_env;
use crate::support::test_fs;

fn fixture(name: &str) -> (PathBuf, EffectiveServeConfig) {
    let root = test_fs::temp_path("serve-observability", name);
    let web_root = root.join("_build/web");
    fs::create_dir_all(&web_root).expect("create web root");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"observability_test\"\nversion = \"0.0.1\"\n",
    )
    .expect("write manifest");
    let args = ServeArgs {
        web_root,
        host: DEFAULT_SERVE_HOST.to_string(),
        port: DEFAULT_SERVE_PORT,
        poll_ms: DEFAULT_POLL_MS,
        handler_runtime: ServeHandlerRuntime::Static,
        check_only: true,
        overrides: ServeCliOverrides::default(),
    };
    let config = resolve_effective_serve_config_with_env(&args, []).expect("effective config");
    (root, config)
}

#[test]
fn observability_schema_spans_every_vm_serve_domain() {
    let (_root, config) = fixture("domains");
    let mut telemetry = VmServeObservability::new(&config, 16).expect("telemetry");
    for (domain, name) in [
        (VmEventDomain::Process, "process.startup"),
        (VmEventDomain::Socket, "socket.readiness"),
        (VmEventDomain::Request, "request.completed"),
        (VmEventDomain::Scheduler, "scheduler.dispatch"),
        (VmEventDomain::Capability, "capability.rpc"),
        (VmEventDomain::Tls, "tls.handshake"),
        (VmEventDomain::Cleanup, "resources.cleanup"),
    ] {
        telemetry.record(domain, name, VmEventStatus::Completed);
    }
    assert_eq!(telemetry.metrics().recorded_events, 7);
    assert_eq!(telemetry.metrics().domain_events.len(), 7);
}

#[test]
fn observability_is_bounded_and_reports_overflow_and_partial_failure() {
    let (_root, config) = fixture("pressure");
    let mut telemetry = VmServeObservability::new(&config, 2).expect("telemetry");
    telemetry.record(
        VmEventDomain::Process,
        "process.startup",
        VmEventStatus::Started,
    );
    telemetry.record(VmEventDomain::Socket, "socket.ready", VmEventStatus::Ready);
    telemetry.record(
        VmEventDomain::Request,
        "request.extra",
        VmEventStatus::Completed,
    );
    telemetry.record(VmEventDomain::Capability, "", VmEventStatus::Failed);
    assert_eq!(telemetry.metrics().recorded_events, 2);
    assert_eq!(telemetry.metrics().dropped_events, 2);
}

#[test]
fn traceparent_validation_rejects_malformed_and_zero_identifiers() {
    for malformed in [
        "garbage",
        "00-00000000000000000000000000000000-0123456789abcdef-01",
        "00-0123456789abcdef0123456789abcdef-0000000000000000-01",
        "01-0123456789abcdef0123456789abcdef-0123456789abcdef-01",
    ] {
        assert!(parse_traceparent(malformed).is_err(), "{malformed}");
    }
    let trace = parse_traceparent("00-0123456789abcdef0123456789abcdef-0123456789abcdef-01")
        .expect("valid trace");
    assert!(trace.sampled);
}

#[test]
fn observability_correlates_request_without_recording_payloads() {
    let (_root, config) = fixture("correlation");
    let trace = parse_traceparent("00-0123456789abcdef0123456789abcdef-0123456789abcdef-00")
        .expect("valid trace");
    let mut telemetry = VmServeObservability::new(&config, 8).expect("telemetry");
    telemetry.record_correlated(
        VmEventDomain::Request,
        "request.completed",
        VmEventStatus::Completed,
        VmEventCorrelation {
            request_id: Some(7),
            connection_id: Some(3),
            actor_id: Some(11),
            route_id: Some("GET /users/:id"),
            trace: Some(&trace),
        },
    );
    assert_eq!(telemetry.traces.len(), 1);
    assert_eq!(
        telemetry.events[0].trace_id.as_deref(),
        Some(trace.trace_id.as_str())
    );
}

#[test]
fn shutdown_lifecycle_handles_repeated_signals_and_forced_deadline() {
    let (_root, config) = fixture("shutdown");
    let mut telemetry = VmServeObservability::new(&config, 16).expect("telemetry");
    telemetry.begin_shutdown("SIGTERM");
    telemetry.begin_shutdown("SIGINT");
    telemetry.finish_shutdown(true);
    assert_eq!(telemetry.metrics().repeated_signals, 1);
    assert_eq!(telemetry.metrics().forced_shutdowns, 1);
    assert!(telemetry
        .events
        .iter()
        .any(|event| event.name == "shutdown.deadline"));
    assert_eq!(
        telemetry.events.last().map(|event| event.name.as_str()),
        Some("process.exit")
    );
}

#[test]
fn observability_flushes_replayable_events_metrics_and_traces() {
    let (root, config) = fixture("flush");
    let mut telemetry = VmServeObservability::new(&config, 8).expect("telemetry");
    telemetry.record(
        VmEventDomain::Process,
        "process.startup",
        VmEventStatus::Started,
    );
    telemetry.record(
        VmEventDomain::Socket,
        "socket.readiness",
        VmEventStatus::Ready,
    );
    telemetry.finish_shutdown(false);
    let artifacts = telemetry
        .flush(&root.join("_build/web"))
        .expect("flush telemetry");
    for path in [&artifacts.events, &artifacts.metrics, &artifacts.traces] {
        assert!(path.is_file(), "missing {}", path.display());
    }
    let events = fs::read_to_string(artifacts.events).expect("events");
    assert!(events
        .lines()
        .all(|line| serde_json::from_str::<serde_json::Value>(line).is_ok()));
    assert!(events.contains(&config.fingerprint));
    let metrics: serde_json::Value =
        serde_json::from_slice(&fs::read(artifacts.metrics).expect("metrics"))
            .expect("parse metrics");
    assert_eq!(metrics["schema"], OBSERVABILITY_SCHEMA);
    assert_eq!(metrics["configFingerprint"], config.fingerprint);
}

#[test]
fn observability_rejects_zero_capacity() {
    let (_root, config) = fixture("capacity");
    let error = VmServeObservability::new(&config, 0).expect_err("zero capacity");
    assert!(error.contains("capacity must be greater than zero"));
}
