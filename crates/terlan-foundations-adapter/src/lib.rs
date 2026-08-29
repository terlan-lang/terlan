//! Optional native-host adapter from the Terlan service ABI to Foundations.
//!
//! This crate intentionally owns no listener, runtime, signal handling,
//! application configuration, or shutdown lifecycle. The embedding server
//! initializes Foundations and drives/flushes this sink.

use foundations::telemetry::log;
use foundations::telemetry::metrics::{metrics, Counter};
use terlan_service_foundation::{InMemorySink, ServiceEvent, ServiceSink, SinkError, SinkSnapshot};

#[metrics(unprefixed)]
mod adapter_metrics {
    /// Counts accepted Terlan service events by their portable event kind.
    pub fn terlan_service_events(kind: &'static str) -> Counter;
}

/// Exact upstream version validated by this adapter.
pub const FOUNDATIONS_VERSION: &str = "5.9.2";
/// Safe default features selected with `default-features = false`.
pub const FOUNDATIONS_FEATURES: &[&str] = &["logging", "metrics", "testing"];
/// Explicitly rejected feature bundles and platform facilities.
pub const EXCLUDED_FEATURES: &[&str] = &[
    "default",
    "platform-common-default",
    "telemetry",
    "cli",
    "settings",
    "security",
    "sentry",
    "jemalloc",
    "memory-profiling",
    "tracing",
    "telemetry-otlp-grpc",
    "telemetry-server",
];

/// Reference adapter with a bounded semantic mirror for diagnostics and tests.
pub struct FoundationsSink {
    mirror: InMemorySink,
}

impl FoundationsSink {
    /// Creates an adapter with a bounded in-memory diagnostic mirror.
    pub fn new(capacity: usize) -> Result<Self, SinkError> {
        Ok(Self {
            mirror: InMemorySink::new(capacity)?,
        })
    }

    /// Returns a point-in-time copy of the bounded diagnostic mirror.
    pub fn snapshot(&self) -> SinkSnapshot {
        self.mirror.snapshot()
    }

    /// Describes the embedded Terlan service to Foundations.
    pub fn service_info() -> foundations::ServiceInfo {
        foundations::ServiceInfo {
            name: "terlan-service",
            name_in_metrics: "terlan_service".to_owned(),
            version: env!("CARGO_PKG_VERSION"),
            author: env!("CARGO_PKG_AUTHORS"),
            description: "Terlan native service adapter",
        }
    }
}

impl ServiceSink for FoundationsSink {
    fn emit(&self, event: ServiceEvent) -> Result<(), SinkError> {
        // Preserve the portable corpus first. Optional upstream telemetry may
        // lose data, but it cannot alter or fail an admitted customer request.
        self.mirror.emit(event.clone())?;
        let payload = serde_json::to_string(&event).map_err(|error| SinkError {
            operation: "foundations.serialize",
            message: error.to_string(),
        })?;
        let kind = event_kind(&event);
        adapter_metrics::terlan_service_events(kind).inc();
        match &event {
            ServiceEvent::Log { level, message, .. } => match level {
                terlan_service_foundation::LogLevel::Debug => {
                    log::debug!("{}", message; "terlan_event" => payload)
                }
                terlan_service_foundation::LogLevel::Info => {
                    log::info!("{}", message; "terlan_event" => payload)
                }
                terlan_service_foundation::LogLevel::Warn => {
                    log::warn!("{}", message; "terlan_event" => payload)
                }
                terlan_service_foundation::LogLevel::Error => {
                    log::error!("{}", message; "terlan_event" => payload)
                }
            },
            ServiceEvent::Span { .. } => {
                log::debug!("Terlan span"; "terlan_event" => payload);
            }
            _ => log::debug!("Terlan service event"; "terlan_event" => payload),
        }
        Ok(())
    }

    fn flush(&self, timeout_millis: u64) -> Result<(), SinkError> {
        // Foundations is driven by the embedding server. The mirror is
        // synchronous; this method must never create a competing runtime.
        self.mirror.flush(timeout_millis)
    }
}

fn event_kind(event: &ServiceEvent) -> &'static str {
    match event {
        ServiceEvent::Log { .. } => "log",
        ServiceEvent::Metric { .. } => "metric",
        ServiceEvent::Span { .. } => "span",
        ServiceEvent::Health { .. } => "health",
        ServiceEvent::Drain { .. } => "drain",
        ServiceEvent::ConfigResolved { .. } => "config",
        ServiceEvent::SinkFailure { .. } => "sink_failure",
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
