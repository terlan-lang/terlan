//! SafeNative worker metadata contracts.
//!
//! This module records runtime-facing metadata that the compiler, BEAM bridge,
//! and future release manifests can agree on before live adapters are wired.

/// Runtime engine required by a SafeNative worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeNativeWorkerRuntime {
    /// Rust/Tokio owns async socket I/O and adapter task scheduling.
    Tokio,
}

/// Transport boundary used to reach a SafeNative worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeNativeWorkerTransport {
    /// A supervised BEAM process owns the worker lifecycle and request mailbox.
    BeamProcess,
}

/// Native resource cleanup policy exposed by a worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeNativeResourcePolicy {
    /// Resources are owned by a supervised worker and disposed explicitly.
    SupervisedExplicitDispose,
}

/// Static metadata for one SafeNative worker adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafeNativeWorkerSpec {
    /// Source-visible adapter module that owns this worker.
    pub adapter: &'static str,
    /// Runtime engine selected for this worker.
    pub runtime: SafeNativeWorkerRuntime,
    /// Bridge transport selected for this worker.
    pub transport: SafeNativeWorkerTransport,
    /// Native resource ownership policy selected for this worker.
    pub resource_policy: SafeNativeResourcePolicy,
    /// Default maximum in-flight requests for one worker instance.
    pub default_credit_limit: u64,
    /// Compiler-native operation ids accepted by this worker.
    pub operations: &'static [&'static str],
    /// Opaque source-visible resource types owned by this worker.
    pub resource_types: &'static [&'static str],
}

impl SafeNativeWorkerSpec {
    /// Returns whether this worker accepts one operation id.
    ///
    /// Inputs:
    /// - `operation`: compiler-native operation id.
    ///
    /// Output:
    /// - `true` when the operation is listed in this worker spec.
    ///
    /// Transformation:
    /// - Performs exact string matching against the static operation list.
    pub fn accepts_operation(&self, operation: &str) -> bool {
        self.operations.contains(&operation)
    }

    /// Returns whether this worker owns one source-visible resource type.
    ///
    /// Inputs:
    /// - `resource_type`: fully qualified Terlan resource type name.
    ///
    /// Output:
    /// - `true` when the type is listed in this worker spec.
    ///
    /// Transformation:
    /// - Performs exact string matching against the static resource-type list.
    pub fn owns_resource_type(&self, resource_type: &str) -> bool {
        self.resource_types.contains(&resource_type)
    }
}

/// Compiler-native Postgres operation ids owned by the SafeNative worker.
pub const POSTGRES_WORKER_OPERATIONS: &[&str] = &[
    "std.db.postgres.connect",
    "std.db.postgres.query",
    "std.db.postgres.query_one",
    "std.db.postgres.execute",
    "std.db.postgres.transaction",
    "std.db.postgres.string",
    "std.db.postgres.int",
    "std.db.postgres.bool",
    "std.db.postgres.json",
];

/// Source-visible Postgres resources owned by the SafeNative worker.
pub const POSTGRES_WORKER_RESOURCE_TYPES: &[&str] = &[
    "std.db.Postgres.Pool",
    "std.db.Postgres.Connection",
    "std.db.Postgres.Row",
];

/// Static Postgres worker metadata for the BEAM/SafeNative bridge.
pub const POSTGRES_WORKER_SPEC: SafeNativeWorkerSpec = SafeNativeWorkerSpec {
    adapter: "std.db.Postgres",
    runtime: SafeNativeWorkerRuntime::Tokio,
    transport: SafeNativeWorkerTransport::BeamProcess,
    resource_policy: SafeNativeResourcePolicy::SupervisedExplicitDispose,
    default_credit_limit: 64,
    operations: POSTGRES_WORKER_OPERATIONS,
    resource_types: POSTGRES_WORKER_RESOURCE_TYPES,
};

/// Returns the static Postgres worker spec.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Shared immutable Postgres worker metadata.
///
/// Transformation:
/// - Exposes the static contract through a function so downstream code can
///   depend on a stable accessor rather than a literal constant name.
pub fn postgres_worker_spec() -> &'static SafeNativeWorkerSpec {
    &POSTGRES_WORKER_SPEC
}

#[cfg(test)]
#[path = "metadata_test.rs"]
mod metadata_test;
