use super::*;

#[test]
fn rejects_dynamic_and_secret_dimensions() {
    let mut metrics = MetricRegistry::default();
    assert_eq!(
        metrics.declare(MetricDeclaration {
            name: "request_count".into(),
            kind: InstrumentKind::Counter,
            label_keys: vec!["raw_url".into()],
            cardinality_limit: 8,
        }),
        Err(MetricError::UnboundedIdentity)
    );
    metrics
        .declare(MetricDeclaration {
            name: "http_requests".into(),
            kind: InstrumentKind::Counter,
            label_keys: vec!["route_id".into(), "status_class".into()],
            cardinality_limit: 64,
        })
        .unwrap();
    assert_eq!(
        metrics.validate_sample("runtime_name", []),
        Err(MetricError::UndeclaredInstrument)
    );
}
