use super::super::persistent_actor_telemetry::deterministic_restore_trace;
use super::{
    VmPersistentActorMetricAggregator, VmPersistentActorMetricError, VmPersistentActorMetricLimits,
};

#[test]
fn vm_persistent_actor_metrics_aggregate_cross_actor_without_actor_id_labels() {
    let mut first = deterministic_restore_trace();
    let mut second = deterministic_restore_trace();
    for span in &mut first {
        span.actor_id = "actor-private-1".to_string();
    }
    for span in &mut second {
        span.actor_id = "actor-private-2".to_string();
    }

    let mut aggregator =
        VmPersistentActorMetricAggregator::new(VmPersistentActorMetricLimits::default())
            .expect("aggregator");
    aggregator.ingest_trace(&first).expect("first trace");
    aggregator.ingest_trace(&second).expect("second trace");

    assert_eq!(aggregator.trace_count(), 2);
    assert_eq!(aggregator.series().len(), first.len());
    assert!(aggregator
        .series()
        .iter()
        .all(|series| series.span_count == 2));
    let rendered = format!("{aggregator:?}");
    assert!(!rendered.contains("actor-private-1"));
    assert!(!rendered.contains("actor-private-2"));
}

#[test]
fn vm_persistent_actor_metrics_reject_limits_and_overflow_atomically() {
    let limits = VmPersistentActorMetricLimits {
        actor_families: 1,
        schema_ids: 1,
        adapter_ids: 1,
        series: 16,
    };
    let mut aggregator = VmPersistentActorMetricAggregator::new(limits).expect("aggregator");
    let first = deterministic_restore_trace();
    aggregator.ingest_trace(&first).expect("first trace");
    let baseline = aggregator.clone();

    let mut another_family = deterministic_restore_trace();
    for span in &mut another_family {
        span.actor_id = "actor-2".to_string();
        span.actor_family = "payments".to_string();
    }
    assert_eq!(
        aggregator.ingest_trace(&another_family),
        Err(VmPersistentActorMetricError::CardinalityLimitExceeded {
            dimension: "actor_family",
            limit: 1,
        })
    );
    assert_eq!(aggregator, baseline);

    let mut overflow = deterministic_restore_trace();
    overflow[0].scheduler_ticks = u64::MAX;
    for span in overflow.iter_mut().skip(1) {
        span.scheduler_ticks = 0;
    }
    assert_eq!(
        aggregator.ingest_trace(&overflow),
        Err(VmPersistentActorMetricError::CounterOverflow {
            field: "scheduler_ticks",
        })
    );
    assert_eq!(aggregator, baseline);
}
