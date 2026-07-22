use super::{
    VmPersistentActorTelemetryError, VmPersistentActorTelemetryLifecycle,
    VmPersistentActorTelemetryTrace,
};
use crate::runtime::vm::persistent_actor_store::VmPersistentActorStoreAdapter;
use crate::runtime::vm::persistent_actor_telemetry_aggregation::{
    VmPersistentActorMetricAggregator, VmPersistentActorMetricError, VmPersistentActorMetricLimits,
    VmPersistentActorMetricSeries,
};

/// Completed lifecycle trace together with bounded cross-actor metric series.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorTelemetryReport {
    /// Validated trace for the completed actor lifecycle.
    pub(crate) trace: VmPersistentActorTelemetryTrace,
    /// Bounded metric series aggregated from the trace.
    pub(crate) metrics: Vec<VmPersistentActorMetricSeries>,
}

impl<A: VmPersistentActorStoreAdapter> VmPersistentActorTelemetryLifecycle<A> {
    /// Finishes lifecycle telemetry while preserving the trace-only API.
    pub(crate) fn finish(
        self,
    ) -> Result<VmPersistentActorTelemetryTrace, VmPersistentActorTelemetryError> {
        self.finish_with_metrics(VmPersistentActorMetricLimits::default())
            .map(|report| report.trace)
    }

    /// Finishes lifecycle telemetry and aggregates its bounded metric series.
    pub(crate) fn finish_with_metrics(
        self,
        limits: VmPersistentActorMetricLimits,
    ) -> Result<VmPersistentActorTelemetryReport, VmPersistentActorTelemetryError> {
        let spans = self.collector.spans().to_vec();
        let trace = self.collector.finish()?;
        let mut aggregator =
            VmPersistentActorMetricAggregator::new(limits).map_err(metric_aggregation_error)?;
        aggregator
            .ingest_trace(&spans)
            .map_err(metric_aggregation_error)?;
        Ok(VmPersistentActorTelemetryReport {
            trace,
            metrics: aggregator.series(),
        })
    }
}

/// Converts aggregate failures into the existing telemetry error vocabulary.
fn metric_aggregation_error(
    error: VmPersistentActorMetricError,
) -> VmPersistentActorTelemetryError {
    match error {
        VmPersistentActorMetricError::InvalidLimit { dimension } => {
            VmPersistentActorTelemetryError::CardinalityLimitExceeded {
                dimension,
                limit: 0,
            }
        }
        VmPersistentActorMetricError::CardinalityLimitExceeded { dimension, limit } => {
            VmPersistentActorTelemetryError::CardinalityLimitExceeded { dimension, limit }
        }
        VmPersistentActorMetricError::CounterOverflow { field } => {
            VmPersistentActorTelemetryError::CounterOverflow { sequence: 0, field }
        }
        VmPersistentActorMetricError::InvalidTrace(error) => error,
    }
}
