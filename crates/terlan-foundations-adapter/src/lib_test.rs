use foundations::telemetry::{TelemetryContext, TestTelemetryContext};
use terlan_service_foundation::{
    emit_semantic_corpus, semantic_corpus, FieldSet, LogLevel, ServiceEvent, ServiceSink,
};

use super::*;

#[test]
fn maps_portable_event_without_owning_lifecycle() {
    let context: TestTelemetryContext = TelemetryContext::test();
    let _scope = context.scope();
    let adapter = FoundationsSink::new(8).unwrap();
    let event = ServiceEvent::Log {
        sequence: 1,
        level: LogLevel::Info,
        message: "registry ready".into(),
        fields: FieldSet::default(),
        context: None,
    };
    adapter.emit(event.clone()).unwrap();
    adapter.flush(10).unwrap();
    assert_eq!(adapter.snapshot().events, vec![event]);
    assert_eq!(context.log_records().len(), 1);
    assert_eq!(context.log_records()[0].message, "registry ready");
}

#[test]
fn upstream_selection_is_narrow_and_default_free() {
    assert_eq!(FOUNDATIONS_VERSION, "5.9.0");
    assert!(FOUNDATIONS_FEATURES.contains(&"telemetry-otlp-grpc"));
    assert!(EXCLUDED_FEATURES.contains(&"platform-common-default"));
    let info = FoundationsSink::service_info();
    assert_eq!(info.name_in_metrics, "terlan_service");
}

#[test]
fn foundations_adapter_preserves_shared_semantic_corpus() {
    let context: TestTelemetryContext = TelemetryContext::test();
    let _scope = context.scope();
    let adapter = FoundationsSink::new(16).unwrap();
    emit_semantic_corpus(&adapter).unwrap();
    assert_eq!(adapter.snapshot().events, semantic_corpus());
    assert!(!context.log_records().is_empty());
}
