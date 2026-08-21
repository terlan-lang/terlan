use crate::{
    Field, FieldSet, HealthState, LogLevel, Scalar, ServiceEvent, ServiceSink, SinkError,
    SpanStatus,
};

/// One deterministic event corpus shared by every adapter conformance test.
pub fn semantic_corpus() -> Vec<ServiceEvent> {
    let identity = FieldSet::try_new([
        Field {
            name: "route_id".into(),
            value: Scalar::String("registry.package".into()),
        },
        Field {
            name: "status_class".into(),
            value: Scalar::String("2xx".into()),
        },
    ])
    .expect("static semantic corpus fields are valid");
    vec![
        ServiceEvent::Log {
            sequence: 1,
            level: LogLevel::Info,
            message: "registry ready".into(),
            fields: identity.clone(),
            context: None,
        },
        ServiceEvent::Metric {
            sequence: 2,
            name: "http_requests".into(),
            value: 1.0,
            fields: identity.clone(),
            context: None,
        },
        ServiceEvent::Span {
            sequence: 3,
            name: "registry.publish".into(),
            status: SpanStatus::Ok,
            fields: identity,
            context: None,
        },
        ServiceEvent::Health {
            sequence: 4,
            state: HealthState {
                live: true,
                ready: true,
                draining: false,
            },
        },
        ServiceEvent::Drain {
            sequence: 5,
            stage: "admission_stopped".into(),
        },
        ServiceEvent::ConfigResolved {
            sequence: 6,
            name: "registry.signing_key".into(),
            secret: true,
        },
    ]
}

/// Emits the deterministic adapter-conformance corpus and flushes the sink.
pub fn emit_semantic_corpus(sink: &dyn ServiceSink) -> Result<(), SinkError> {
    for event in semantic_corpus() {
        sink.emit(event)?;
    }
    sink.flush(100)
}

#[cfg(test)]
#[path = "corpus_test.rs"]
mod tests;
