#[path = "postgres/batch.rs"]
mod batch;
#[path = "postgres/inspection.rs"]
#[cfg(test)]
mod inspection;
#[cfg(all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq"))]
#[path = "postgres/libpq_worker.rs"]
pub(crate) mod libpq_worker;
#[cfg(any(feature = "serve-runtime-bin", not(feature = "postgres-libpq")))]
#[path = "postgres/capability_worker_required.rs"]
pub(crate) mod libpq_worker;
#[path = "postgres/report.rs"]
mod report;
#[path = "postgres/state.rs"]
mod state;
#[path = "postgres/types.rs"]
mod types;
#[cfg(test)]
#[path = "postgres/worker_fixture_test.rs"]
mod worker;

#[cfg(test)]
#[path = "postgres_test.rs"]
#[cfg(test)]
mod postgres_test;

#[cfg(test)]
#[path = "postgres_report_test.rs"]
#[cfg(test)]
mod postgres_report_test;

#[cfg(test)]
#[path = "postgres_inspection_test.rs"]
#[cfg(test)]
mod postgres_inspection_test;

#[cfg(test)]
#[path = "actor_postgres_test.rs"]
#[cfg(test)]
mod actor_postgres_test;

#[path = "postgres/completion.rs"]
mod completion;
#[path = "postgres/runtime.rs"]
mod runtime;

#[cfg(test)]
use crate::runtime::vm::{
    process::{VmProcessId, VmProcessTable},
    scheduler::VmScheduler,
    timer::VmTimerTable,
};
#[cfg(test)]
use crate::terlan_native_boundary::request::RequestId;
#[cfg(test)]
pub(crate) use inspection::*;
pub(crate) use libpq_worker::*;
pub(crate) use runtime::{VmPostgresRequestContext, VmPostgresRuntime};
#[cfg(test)]
use state::{PreparedStatementState, RowState};
pub(crate) use types::*;

/// Readiness interest exposed by the selected Postgres worker implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(
    test,
    all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq")
))]
pub(crate) enum VmPostgresIoInterest {
    #[cfg(all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq"))]
    Drive,
    Read,
    #[cfg(all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq"))]
    Write,
}

/// One Postgres driver socket wait owned by the VM reactor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(
    test,
    all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq")
))]
pub(crate) struct VmPostgresDriverWait {
    pub(crate) request_id: crate::terlan_native_boundary::request::RequestId,
    pub(crate) socket: i64,
    pub(crate) interest: VmPostgresIoInterest,
}
