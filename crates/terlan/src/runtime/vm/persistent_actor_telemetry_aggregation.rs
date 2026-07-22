#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use super::persistent_actor_telemetry::{
    validate_persistent_actor_telemetry_trace, VmPersistentActorTelemetryError,
    VmPersistentActorTelemetryKind, VmPersistentActorTelemetrySpan,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorMetricLimits {
    pub(crate) actor_families: usize,
    pub(crate) schema_ids: usize,
    pub(crate) adapter_ids: usize,
    pub(crate) series: usize,
}

impl Default for VmPersistentActorMetricLimits {
    fn default() -> Self {
        Self {
            actor_families: 64,
            schema_ids: 128,
            adapter_ids: 16,
            series: 1024,
        }
    }
}

#[derive(Clone, Debug, Ord, PartialEq, PartialOrd, Eq)]
pub(crate) struct VmPersistentActorMetricKey {
    pub(crate) actor_family: String,
    pub(crate) schema_id: String,
    pub(crate) adapter_id: String,
    pub(crate) operation: VmPersistentActorTelemetryKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorMetricSeries {
    pub(crate) key: VmPersistentActorMetricKey,
    pub(crate) span_count: u64,
    pub(crate) scheduler_ticks: u64,
    pub(crate) durable_bytes: u64,
    pub(crate) retry_count: u64,
    pub(crate) failure_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VmPersistentActorMetricError {
    InvalidLimit {
        dimension: &'static str,
    },
    CardinalityLimitExceeded {
        dimension: &'static str,
        limit: usize,
    },
    CounterOverflow {
        field: &'static str,
    },
    InvalidTrace(VmPersistentActorTelemetryError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorMetricAggregator {
    limits: VmPersistentActorMetricLimits,
    actor_families: BTreeSet<String>,
    schema_ids: BTreeSet<String>,
    adapter_ids: BTreeSet<String>,
    trace_count: u64,
    series: BTreeMap<VmPersistentActorMetricKey, VmPersistentActorMetricSeries>,
}

impl VmPersistentActorMetricAggregator {
    pub(crate) fn new(
        limits: VmPersistentActorMetricLimits,
    ) -> Result<Self, VmPersistentActorMetricError> {
        for (dimension, limit) in [
            ("actor_family", limits.actor_families),
            ("schema_id", limits.schema_ids),
            ("adapter_id", limits.adapter_ids),
            ("series", limits.series),
        ] {
            if limit == 0 {
                return Err(VmPersistentActorMetricError::InvalidLimit { dimension });
            }
        }
        Ok(Self {
            limits,
            actor_families: BTreeSet::new(),
            schema_ids: BTreeSet::new(),
            adapter_ids: BTreeSet::new(),
            trace_count: 0,
            series: BTreeMap::new(),
        })
    }

    pub(crate) fn ingest_trace(
        &mut self,
        spans: &[VmPersistentActorTelemetrySpan],
    ) -> Result<(), VmPersistentActorMetricError> {
        validate_persistent_actor_telemetry_trace(spans)
            .map_err(VmPersistentActorMetricError::InvalidTrace)?;
        let mut candidate = self.clone();
        candidate.ingest_validated(spans)?;
        *self = candidate;
        Ok(())
    }

    pub(crate) fn trace_count(&self) -> u64 {
        self.trace_count
    }

    pub(crate) fn series(&self) -> Vec<VmPersistentActorMetricSeries> {
        self.series.values().cloned().collect()
    }

    fn ingest_validated(
        &mut self,
        spans: &[VmPersistentActorTelemetrySpan],
    ) -> Result<(), VmPersistentActorMetricError> {
        self.trace_count = checked_add(self.trace_count, 1, "trace_count")?;
        for span in spans {
            insert_dimension(
                &mut self.actor_families,
                &span.actor_family,
                self.limits.actor_families,
                "actor_family",
            )?;
            insert_dimension(
                &mut self.schema_ids,
                &span.schema_id,
                self.limits.schema_ids,
                "schema_id",
            )?;
            insert_dimension(
                &mut self.adapter_ids,
                &span.adapter_id,
                self.limits.adapter_ids,
                "adapter_id",
            )?;
            let key = VmPersistentActorMetricKey {
                actor_family: span.actor_family.clone(),
                schema_id: span.schema_id.clone(),
                adapter_id: span.adapter_id.clone(),
                operation: span.kind.clone(),
            };
            if !self.series.contains_key(&key) && self.series.len() >= self.limits.series {
                return Err(VmPersistentActorMetricError::CardinalityLimitExceeded {
                    dimension: "series",
                    limit: self.limits.series,
                });
            }
            let series =
                self.series
                    .entry(key.clone())
                    .or_insert_with(|| VmPersistentActorMetricSeries {
                        key,
                        span_count: 0,
                        scheduler_ticks: 0,
                        durable_bytes: 0,
                        retry_count: 0,
                        failure_count: 0,
                    });
            series.span_count = checked_add(series.span_count, 1, "span_count")?;
            series.scheduler_ticks = checked_add(
                series.scheduler_ticks,
                span.scheduler_ticks,
                "scheduler_ticks",
            )?;
            series.durable_bytes =
                checked_add(series.durable_bytes, span.durable_bytes, "durable_bytes")?;
            series.retry_count = checked_add(series.retry_count, span.retry_count, "retry_count")?;
            if span.typed_failure_reason.is_some() {
                series.failure_count = checked_add(series.failure_count, 1, "failure_count")?;
            }
        }
        Ok(())
    }
}

fn insert_dimension(
    values: &mut BTreeSet<String>,
    value: &str,
    limit: usize,
    dimension: &'static str,
) -> Result<(), VmPersistentActorMetricError> {
    if !values.contains(value) && values.len() >= limit {
        return Err(VmPersistentActorMetricError::CardinalityLimitExceeded { dimension, limit });
    }
    values.insert(value.to_string());
    Ok(())
}

fn checked_add(
    current: u64,
    increment: u64,
    field: &'static str,
) -> Result<u64, VmPersistentActorMetricError> {
    current
        .checked_add(increment)
        .ok_or(VmPersistentActorMetricError::CounterOverflow { field })
}

#[cfg(test)]
#[path = "persistent_actor_telemetry_aggregation_test.rs"]
mod persistent_actor_telemetry_aggregation_test;
