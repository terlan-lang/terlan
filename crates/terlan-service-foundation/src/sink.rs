use std::fmt;
use std::io::Write;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::{FieldSet, HealthState, LogLevel, RequestContext};

/// Portable completion status for a service span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Error,
    Cancelled,
    TimedOut,
}

/// Bounded, host-independent event accepted by a service sink.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServiceEvent {
    Log {
        sequence: u64,
        level: LogLevel,
        message: String,
        fields: FieldSet,
        context: Option<RequestContext>,
    },
    Metric {
        sequence: u64,
        name: String,
        value: f64,
        fields: FieldSet,
        context: Option<RequestContext>,
    },
    Span {
        sequence: u64,
        name: String,
        status: SpanStatus,
        fields: FieldSet,
        context: Option<RequestContext>,
    },
    Health {
        sequence: u64,
        state: HealthState,
    },
    Drain {
        sequence: u64,
        stage: String,
    },
    ConfigResolved {
        sequence: u64,
        name: String,
        secret: bool,
    },
    SinkFailure {
        sequence: u64,
        operation: String,
    },
}

impl ServiceEvent {
    /// Returns the event's monotonic producer sequence number.
    pub fn sequence(&self) -> u64 {
        match self {
            Self::Log { sequence, .. }
            | Self::Metric { sequence, .. }
            | Self::Span { sequence, .. }
            | Self::Health { sequence, .. }
            | Self::Drain { sequence, .. }
            | Self::ConfigResolved { sequence, .. }
            | Self::SinkFailure { sequence, .. } => *sequence,
        }
    }
}

/// Lifecycle-neutral host ABI. All calls are synchronous, bounded acceptance
/// operations; a host may queue work but cannot fail the customer request.
pub trait ServiceSink: Send + Sync {
    fn emit(&self, event: ServiceEvent) -> Result<(), SinkError>;
    fn flush(&self, timeout_millis: u64) -> Result<(), SinkError>;
}

/// Result of best-effort event delivery to a host sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkOutcome {
    Accepted,
    Dropped { operation: &'static str },
}

/// Contains adapter failure and reports it to a bounded diagnostic sink.
/// Neither failure path is returned to application request execution.
pub fn emit_best_effort(
    primary: &dyn ServiceSink,
    diagnostic: &dyn ServiceSink,
    event: ServiceEvent,
) -> SinkOutcome {
    match primary.emit(event) {
        Ok(()) => SinkOutcome::Accepted,
        Err(error) => {
            let sequence = match error.operation {
                "emit" => 0,
                _ => u64::MAX,
            };
            let _ = diagnostic.emit(ServiceEvent::SinkFailure {
                sequence,
                operation: error.operation.to_owned(),
            });
            SinkOutcome::Dropped {
                operation: error.operation,
            }
        }
    }
}

/// Stable error returned by a host sink operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkError {
    pub operation: &'static str,
    pub message: String,
}

impl fmt::Display for SinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "service sink {} failed: {}",
            self.operation, self.message
        )
    }
}

impl std::error::Error for SinkError {}

/// No-op sink used when host telemetry is disabled.
#[derive(Debug, Default)]
pub struct DisabledSink;

impl ServiceSink for DisabledSink {
    fn emit(&self, _event: ServiceEvent) -> Result<(), SinkError> {
        Ok(())
    }
    fn flush(&self, _timeout_millis: u64) -> Result<(), SinkError> {
        Ok(())
    }
}

/// Bounded snapshot captured by an in-memory sink.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SinkSnapshot {
    pub capacity: usize,
    pub dropped: u64,
    pub events: Vec<ServiceEvent>,
}

/// Thread-safe bounded sink for conformance tests and diagnostics.
#[derive(Debug)]
pub struct InMemorySink {
    capacity: usize,
    state: Mutex<SinkSnapshot>,
}

impl InMemorySink {
    /// Creates an in-memory sink with a positive event capacity.
    pub fn new(capacity: usize) -> Result<Self, SinkError> {
        if capacity == 0 {
            return Err(SinkError {
                operation: "create",
                message: "capacity must be positive".into(),
            });
        }
        Ok(Self {
            capacity,
            state: Mutex::new(SinkSnapshot {
                capacity,
                dropped: 0,
                events: Vec::new(),
            }),
        })
    }

    /// Returns a point-in-time copy of accepted and dropped event counts.
    pub fn snapshot(&self) -> SinkSnapshot {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl ServiceSink for InMemorySink {
    fn emit(&self, event: ServiceEvent) -> Result<(), SinkError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.events.len() == self.capacity {
            state.dropped = state.dropped.saturating_add(1);
        } else {
            state.events.push(event);
        }
        Ok(())
    }
    fn flush(&self, _timeout_millis: u64) -> Result<(), SinkError> {
        Ok(())
    }
}

/// Output representation used by the local writer sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFormat {
    Human,
    Json,
}

/// Synchronous service sink that writes human or JSON lines locally.
#[derive(Debug)]
pub struct LocalSink<W: Write + Send> {
    format: LocalFormat,
    writer: Mutex<W>,
}

impl<W: Write + Send> LocalSink<W> {
    /// Creates a local sink with the selected output representation.
    pub fn new(format: LocalFormat, writer: W) -> Self {
        Self {
            format,
            writer: Mutex::new(writer),
        }
    }
}

impl<W: Write + Send> ServiceSink for LocalSink<W> {
    fn emit(&self, event: ServiceEvent) -> Result<(), SinkError> {
        let line = match self.format {
            LocalFormat::Json => serde_json::to_string(&event).map_err(|error| SinkError {
                operation: "serialize",
                message: error.to_string(),
            })?,
            LocalFormat::Human => format!("terlan-service seq={} {event:?}", event.sequence()),
        };
        writeln!(
            self.writer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            "{line}"
        )
        .map_err(|error| SinkError {
            operation: "write",
            message: error.to_string(),
        })
    }
    fn flush(&self, _timeout_millis: u64) -> Result<(), SinkError> {
        self.writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .flush()
            .map_err(|error| SinkError {
                operation: "flush",
                message: error.to_string(),
            })
    }
}

#[cfg(test)]
#[path = "sink_test.rs"]
mod tests;
