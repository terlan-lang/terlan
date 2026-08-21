//! Portable service semantics shared by Terlan hosts.
//!
//! This crate deliberately contains no network listener, async runtime, signal
//! handler, collector client, or third-party telemetry representation. Hosts
//! implement [`ServiceSink`]; Terlan programs observe only these bounded types.

mod config;
mod context;
mod corpus;
mod lifecycle;
mod metric;
mod sink;
mod value;

pub use config::{ConfigRef, SecretRef};
pub use context::{
    ContextDisposition, RequestContext, SourceIdentity, TraceContext, TraceparentError, WorkOutcome,
};
pub use corpus::{emit_semantic_corpus, semantic_corpus};
pub use lifecycle::{
    DrainBounds, DrainProgress, HealthState, Lifecycle, LifecycleError, LifecyclePhase,
};
pub use metric::{InstrumentKind, MetricDeclaration, MetricError, MetricRegistry};
pub use sink::{
    emit_best_effort, DisabledSink, InMemorySink, LocalFormat, LocalSink, ServiceEvent,
    ServiceSink, SinkError, SinkOutcome, SinkSnapshot, SpanStatus,
};
pub use value::{Field, FieldError, FieldSet, Scalar};

/// Version of the portable source-visible contract.
pub const PUBLIC_API_VERSION: &str = "terlan-service-foundation-v1";
/// Version of the lifecycle-neutral host sink ABI.
pub const HOST_ABI_VERSION: &str = "terlan-service-host-abi-v1";
/// Maximum number of structured fields on one event.
pub const MAX_FIELDS: usize = 32;
/// Maximum UTF-8 bytes in a field value.
pub const MAX_FIELD_VALUE_BYTES: usize = 1_024;
/// Maximum UTF-8 bytes in an event, instrument, or label name.
pub const MAX_NAME_BYTES: usize = 64;

/// Stable log severity independent of a host logging implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}
