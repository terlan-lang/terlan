//! NativeBoundary export and worker manifest contracts.
//!
//! The compiler and VM use this schema to validate native operations before
//! scheduling workers or exposing resource handles to Terlan code.

use std::collections::BTreeSet;

/// Runtime engine responsible for native worker scheduling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBoundaryRuntime {
    /// The Terlan VM owns scheduling, parking, wakeups, and cancellation.
    VmWorker,
}

/// VM-owned transport used to dispatch native requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBoundaryTransport {
    /// Requests and completions cross a typed VM mailbox boundary.
    VmMailbox,
}

/// Explicit reason an operation must cross into a capability-worker process.
///
/// Ordinary local actor execution has no representation in this enum and must
/// remain inside its owning execution shard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBoundaryExecutionProfile {
    /// Calls an adapter implemented outside the VM execution shard.
    ExternalAdapter,
    /// Isolates an admitted adapter whose failure must not terminate the shard.
    CrashIsolated,
    /// Crosses a machine, process, or separately administered trust boundary.
    CrossBoundary,
}

impl NativeBoundaryExecutionProfile {
    vm_capability_component! {
        /// Returns the stable capability-worker startup name.
        pub(crate) const fn protocol_name(self) -> &'static str {
            match self {
                Self::ExternalAdapter => "external-adapter",
                Self::CrashIsolated => "crash-isolated",
                Self::CrossBoundary => "cross-boundary",
            }
        }
    }
}

/// Scheduler class selected for one native export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBoundaryWorkerClass {
    /// Guaranteed nonblocking work that may complete in the current VM slice.
    Fast,
    /// Potentially blocking work dispatched away from VM scheduler threads.
    Blocking,
    /// Long-running work that must observe cooperative cancellation.
    LongRunningCancellable,
    /// Work isolated by a sandbox runtime.
    Sandboxed,
    /// Work that creates or owns typed VM resource handles.
    ResourceOwning,
}

/// Cancellation behavior promised by one native export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBoundaryCancellationPolicy {
    /// The operation completes synchronously and cannot be cancelled mid-call.
    NotCancellable,
    /// The worker observes cancellation and suppresses stale completion values.
    Cooperative,
}

/// Resource authority required or produced by one export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBoundaryResourcePermission {
    /// Reads an existing typed resource without changing ownership.
    Read(&'static str),
    /// Creates a new VM-owned typed resource.
    Create(&'static str),
}

impl NativeBoundaryResourcePermission {
    fn resource_type(self) -> &'static str {
        match self {
            Self::Read(resource_type) | Self::Create(resource_type) => resource_type,
        }
    }
}

/// Ownership of arguments or results crossing the native boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBoundaryMemoryOwnership {
    /// Arguments remain borrowed VM values for the duration of dispatch.
    BorrowedArguments,
    /// The result is copied into a VM-owned value.
    VmOwnedResult,
    /// The result contains a VM-owned typed resource handle.
    ResourceOwnedResult(&'static str),
}

/// Complete manifest row for one source-visible native export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeBoundaryExportManifest {
    /// Source module declaring the native function.
    pub module: &'static str,
    /// Source function name.
    pub function: &'static str,
    /// Source function arity.
    pub arity: usize,
    /// Compiler-native operation identifier.
    pub operation: &'static str,
    /// Capability required from the calling VM process.
    pub required_capability: &'static str,
    /// Argument types in source order.
    pub argument_types: &'static [&'static str],
    /// Source-visible return type.
    pub return_type: &'static str,
    /// Worker scheduling class.
    pub worker_class: NativeBoundaryWorkerClass,
    /// Cancellation policy.
    pub cancellation: NativeBoundaryCancellationPolicy,
    /// Resource authorities required or produced by the call.
    pub resource_permissions: &'static [NativeBoundaryResourcePermission],
    /// Argument memory ownership.
    pub argument_memory: NativeBoundaryMemoryOwnership,
    /// Result memory ownership.
    pub result_memory: NativeBoundaryMemoryOwnership,
    /// Source-visible typed failure contract.
    pub failure_type: &'static str,
}

/// Stable VM worker manifest containing validated native exports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeBoundaryWorkerManifest {
    /// Source-visible adapter module.
    pub adapter: &'static str,
    /// Runtime engine selected for this worker.
    pub runtime: NativeBoundaryRuntime,
    /// Typed VM transport selected for this worker.
    pub transport: NativeBoundaryTransport,
    /// Default maximum in-flight requests per worker.
    pub default_credit_limit: u64,
    /// Source-visible resource types owned by the worker.
    pub resource_types: &'static [&'static str],
    /// Export contracts dispatched by the worker.
    pub exports: &'static [NativeBoundaryExportManifest],
}

impl NativeBoundaryWorkerManifest {
    /// Finds an export by compiler-native operation identifier.
    pub fn export_for_operation(&self, operation: &str) -> Option<&NativeBoundaryExportManifest> {
        self.exports
            .iter()
            .find(|export| export.operation == operation)
    }

    /// Finds an export by source module, function, and arity.
    pub fn export(
        &self,
        module: &str,
        function: &str,
        arity: usize,
    ) -> Option<&NativeBoundaryExportManifest> {
        self.exports.iter().find(|export| {
            export.module == module && export.function == function && export.arity == arity
        })
    }

    /// Returns whether this worker owns one source-visible resource type.
    pub fn owns_resource_type(&self, resource_type: &str) -> bool {
        self.resource_types.contains(&resource_type)
    }

    /// Validates schema completeness and deterministic uniqueness constraints.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut diagnostics = Vec::new();
        if self.adapter.trim().is_empty() {
            diagnostics.push("NativeBoundary worker adapter must not be empty".to_string());
        }
        if self.default_credit_limit == 0 {
            diagnostics.push("NativeBoundary worker credit limit must be positive".to_string());
        }
        if self.exports.is_empty() {
            diagnostics.push("NativeBoundary worker must declare at least one export".to_string());
        }
        let mut operations = BTreeSet::new();
        let mut source_exports = BTreeSet::new();
        for export in self.exports {
            validate_export(self, export, &mut diagnostics);
            if !operations.insert(export.operation) {
                diagnostics.push(format!(
                    "duplicate NativeBoundary operation `{}`",
                    export.operation
                ));
            }
            if !source_exports.insert((export.module, export.function, export.arity)) {
                diagnostics.push(format!(
                    "duplicate NativeBoundary export `{}.{}/{}`",
                    export.module, export.function, export.arity
                ));
            }
        }
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(diagnostics)
        }
    }
}

fn validate_export(
    worker: &NativeBoundaryWorkerManifest,
    export: &NativeBoundaryExportManifest,
    diagnostics: &mut Vec<String>,
) {
    let identity = format!("{}.{}/{}", export.module, export.function, export.arity);
    for (field, value) in [
        ("module", export.module),
        ("function", export.function),
        ("operation", export.operation),
        ("required capability", export.required_capability),
        ("return type", export.return_type),
        ("failure type", export.failure_type),
    ] {
        if value.trim().is_empty() {
            diagnostics.push(format!(
                "NativeBoundary export `{identity}` has empty {field}"
            ));
        }
    }
    if export.arity != export.argument_types.len() {
        diagnostics.push(format!(
            "NativeBoundary export `{identity}` arity {} does not match {} argument types",
            export.arity,
            export.argument_types.len()
        ));
    }
    for argument in export.argument_types {
        if argument.trim().is_empty() {
            diagnostics.push(format!(
                "NativeBoundary export `{identity}` has an empty argument type"
            ));
        }
    }
    for permission in export.resource_permissions {
        validate_owned_resource(worker, &identity, permission.resource_type(), diagnostics);
    }
    if let NativeBoundaryMemoryOwnership::ResourceOwnedResult(resource_type) = export.result_memory
    {
        validate_owned_resource(worker, &identity, resource_type, diagnostics);
    }
}

fn validate_owned_resource(
    worker: &NativeBoundaryWorkerManifest,
    identity: &str,
    resource_type: &str,
    diagnostics: &mut Vec<String>,
) {
    if !worker.owns_resource_type(resource_type) {
        diagnostics.push(format!(
            "NativeBoundary export `{identity}` references unowned resource `{resource_type}`"
        ));
    }
}

const MODULE: &str = "std.db.Postgres";
const POSTGRES_CAPABILITY: &str = "postgres";
const ERROR: &str = "std.core.Error";
const POOL: &str = "std.db.Postgres.Pool";
const CONNECTION: &str = "std.db.Postgres.Connection";
const ROW: &str = "std.db.Postgres.Row";

const CONNECT_ARGS: &[&str] = &["std.db.Postgres.Config"];
const QUERY_ARGS: &[&str] = &["Pool | Connection", "String", "List[std.data.Json]"];
const TRANSACTION_ARGS: &[&str] = &[POOL, "(Connection) -> Result[T, Error]"];
const ROW_ACCESS_ARGS: &[&str] = &[ROW, "String"];
const CREATE_POOL: &[NativeBoundaryResourcePermission] =
    &[NativeBoundaryResourcePermission::Create(POOL)];
const READ_QUERY_TARGET_CREATE_ROW: &[NativeBoundaryResourcePermission] = &[
    NativeBoundaryResourcePermission::Read(POOL),
    NativeBoundaryResourcePermission::Read(CONNECTION),
    NativeBoundaryResourcePermission::Create(ROW),
];
const READ_QUERY_TARGET: &[NativeBoundaryResourcePermission] = &[
    NativeBoundaryResourcePermission::Read(POOL),
    NativeBoundaryResourcePermission::Read(CONNECTION),
];
const READ_POOL_CREATE_CONNECTION: &[NativeBoundaryResourcePermission] = &[
    NativeBoundaryResourcePermission::Read(POOL),
    NativeBoundaryResourcePermission::Create(CONNECTION),
];
const READ_ROW: &[NativeBoundaryResourcePermission] =
    &[NativeBoundaryResourcePermission::Read(ROW)];

struct NativeBoundaryExportRuntimePolicy {
    worker_class: NativeBoundaryWorkerClass,
    cancellation: NativeBoundaryCancellationPolicy,
    result_memory: NativeBoundaryMemoryOwnership,
}

const fn export(
    function: &'static str,
    arity: usize,
    operation: &'static str,
    argument_types: &'static [&'static str],
    return_type: &'static str,
    resource_permissions: &'static [NativeBoundaryResourcePermission],
    policy: NativeBoundaryExportRuntimePolicy,
) -> NativeBoundaryExportManifest {
    let NativeBoundaryExportRuntimePolicy {
        worker_class,
        cancellation,
        result_memory,
    } = policy;
    NativeBoundaryExportManifest {
        module: MODULE,
        function,
        arity,
        operation,
        required_capability: POSTGRES_CAPABILITY,
        argument_types,
        return_type,
        worker_class,
        cancellation,
        resource_permissions,
        argument_memory: NativeBoundaryMemoryOwnership::BorrowedArguments,
        result_memory,
        failure_type: ERROR,
    }
}

/// Postgres NativeBoundary export contracts aligned with `std.db.Postgres`.
pub const POSTGRES_EXPORTS: &[NativeBoundaryExportManifest] = &[
    export(
        "connect",
        1,
        "std.db.postgres.connect",
        CONNECT_ARGS,
        "Result[Pool, Error]",
        CREATE_POOL,
        NativeBoundaryExportRuntimePolicy {
            worker_class: NativeBoundaryWorkerClass::ResourceOwning,
            cancellation: NativeBoundaryCancellationPolicy::Cooperative,
            result_memory: NativeBoundaryMemoryOwnership::ResourceOwnedResult(POOL),
        },
    ),
    export(
        "query",
        3,
        "std.db.postgres.query",
        QUERY_ARGS,
        "Result[List[Row], Error]",
        READ_QUERY_TARGET_CREATE_ROW,
        NativeBoundaryExportRuntimePolicy {
            worker_class: NativeBoundaryWorkerClass::ResourceOwning,
            cancellation: NativeBoundaryCancellationPolicy::Cooperative,
            result_memory: NativeBoundaryMemoryOwnership::ResourceOwnedResult(ROW),
        },
    ),
    export(
        "query_one",
        3,
        "std.db.postgres.query_one",
        QUERY_ARGS,
        "Result[Option[Row], Error]",
        READ_QUERY_TARGET_CREATE_ROW,
        NativeBoundaryExportRuntimePolicy {
            worker_class: NativeBoundaryWorkerClass::ResourceOwning,
            cancellation: NativeBoundaryCancellationPolicy::Cooperative,
            result_memory: NativeBoundaryMemoryOwnership::ResourceOwnedResult(ROW),
        },
    ),
    export(
        "execute",
        3,
        "std.db.postgres.execute",
        QUERY_ARGS,
        "Result[Int, Error]",
        READ_QUERY_TARGET,
        NativeBoundaryExportRuntimePolicy {
            worker_class: NativeBoundaryWorkerClass::Blocking,
            cancellation: NativeBoundaryCancellationPolicy::Cooperative,
            result_memory: NativeBoundaryMemoryOwnership::VmOwnedResult,
        },
    ),
    export(
        "transaction",
        2,
        "std.db.postgres.transaction",
        TRANSACTION_ARGS,
        "Result[T, Error]",
        READ_POOL_CREATE_CONNECTION,
        NativeBoundaryExportRuntimePolicy {
            worker_class: NativeBoundaryWorkerClass::ResourceOwning,
            cancellation: NativeBoundaryCancellationPolicy::Cooperative,
            result_memory: NativeBoundaryMemoryOwnership::VmOwnedResult,
        },
    ),
    export(
        "string",
        2,
        "std.db.postgres.string",
        ROW_ACCESS_ARGS,
        "Result[String, Error]",
        READ_ROW,
        NativeBoundaryExportRuntimePolicy {
            worker_class: NativeBoundaryWorkerClass::Fast,
            cancellation: NativeBoundaryCancellationPolicy::NotCancellable,
            result_memory: NativeBoundaryMemoryOwnership::VmOwnedResult,
        },
    ),
    export(
        "int",
        2,
        "std.db.postgres.int",
        ROW_ACCESS_ARGS,
        "Result[Int, Error]",
        READ_ROW,
        NativeBoundaryExportRuntimePolicy {
            worker_class: NativeBoundaryWorkerClass::Fast,
            cancellation: NativeBoundaryCancellationPolicy::NotCancellable,
            result_memory: NativeBoundaryMemoryOwnership::VmOwnedResult,
        },
    ),
    export(
        "bool",
        2,
        "std.db.postgres.bool",
        ROW_ACCESS_ARGS,
        "Result[Bool, Error]",
        READ_ROW,
        NativeBoundaryExportRuntimePolicy {
            worker_class: NativeBoundaryWorkerClass::Fast,
            cancellation: NativeBoundaryCancellationPolicy::NotCancellable,
            result_memory: NativeBoundaryMemoryOwnership::VmOwnedResult,
        },
    ),
    export(
        "json",
        2,
        "std.db.postgres.json",
        ROW_ACCESS_ARGS,
        "Result[std.data.Json, Error]",
        READ_ROW,
        NativeBoundaryExportRuntimePolicy {
            worker_class: NativeBoundaryWorkerClass::Fast,
            cancellation: NativeBoundaryCancellationPolicy::NotCancellable,
            result_memory: NativeBoundaryMemoryOwnership::VmOwnedResult,
        },
    ),
];

/// Source-visible Postgres resources owned by the NativeBoundary worker.
pub const POSTGRES_RESOURCE_TYPES: &[&str] = &[POOL, CONNECTION, ROW];

/// Static Postgres NativeBoundary worker manifest.
pub const POSTGRES_WORKER_MANIFEST: NativeBoundaryWorkerManifest = NativeBoundaryWorkerManifest {
    adapter: MODULE,
    runtime: NativeBoundaryRuntime::VmWorker,
    transport: NativeBoundaryTransport::VmMailbox,
    default_credit_limit: 64,
    resource_types: POSTGRES_RESOURCE_TYPES,
    exports: POSTGRES_EXPORTS,
};

/// Returns the static Postgres NativeBoundary worker manifest.
pub fn postgres_worker_manifest() -> &'static NativeBoundaryWorkerManifest {
    &POSTGRES_WORKER_MANIFEST
}

#[cfg(test)]
#[path = "metadata_test.rs"]
#[cfg(test)]
mod metadata_test;
