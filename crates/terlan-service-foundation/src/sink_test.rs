use super::*;

#[test]
fn bounded_sink_preserves_order_and_counts_loss() {
    let sink = InMemorySink::new(1).unwrap();
    let event = ServiceEvent::Drain {
        sequence: 1,
        stage: "admission_stopped".into(),
    };
    sink.emit(event.clone()).unwrap();
    sink.emit(ServiceEvent::Drain {
        sequence: 2,
        stage: "flushed".into(),
    })
    .unwrap();
    let snapshot = sink.snapshot();
    assert_eq!(snapshot.events, vec![event]);
    assert_eq!(snapshot.dropped, 1);
}

struct FailingSink;

impl ServiceSink for FailingSink {
    fn emit(&self, _event: ServiceEvent) -> Result<(), SinkError> {
        Err(SinkError {
            operation: "emit",
            message: "collector unavailable".into(),
        })
    }
    fn flush(&self, _timeout_millis: u64) -> Result<(), SinkError> {
        Err(SinkError {
            operation: "flush",
            message: "collector unavailable".into(),
        })
    }
}

#[test]
fn sink_failure_is_observable_but_request_neutral() {
    let diagnostics = InMemorySink::new(2).unwrap();
    let outcome = emit_best_effort(
        &FailingSink,
        &diagnostics,
        ServiceEvent::Drain {
            sequence: 9,
            stage: "start".into(),
        },
    );
    assert_eq!(outcome, SinkOutcome::Dropped { operation: "emit" });
    assert!(matches!(
        diagnostics.snapshot().events.as_slice(),
        [ServiceEvent::SinkFailure { .. }]
    ));
}
