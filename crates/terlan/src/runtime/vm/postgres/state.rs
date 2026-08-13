use serde::Serialize;

use super::{
    VmPostgresConnection, VmPostgresDriverConnection, VmPostgresDriverOperation,
    VmPostgresDriverPool, VmPostgresDriverPreparedStatement, VmPostgresDriverRow,
    VmPostgresDriverTransaction, VmPostgresPool, VmPostgresTransaction, VmPostgresTransactionState,
};
use crate::runtime::vm::{
    native_boundary::deadline::VmScheduledNativeBoundaryRequest, process::VmProcessId,
};

#[derive(Clone, Debug)]
pub(super) struct PoolState {
    pub(super) owner: VmProcessId,
    pub(super) driver_pool: VmPostgresDriverPool,
    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    pub(super) max_connections: usize,
    pub(super) active_connections: usize,
    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    pub(super) reserved_connections: usize,
    pub(super) open: bool,
}

#[derive(Clone, Debug)]
pub(super) struct ConnectionState {
    pub(super) owner: VmProcessId,
    pub(super) pool: VmPostgresPool,
    pub(super) driver_connection: VmPostgresDriverConnection,
    pub(super) active_transaction: Option<VmPostgresTransaction>,
    pub(super) open: bool,
}

#[derive(Clone, Debug)]
pub(super) struct TransactionState {
    pub(super) owner: VmProcessId,
    pub(super) connection: VmPostgresConnection,
    pub(super) driver_transaction: VmPostgresDriverTransaction,
    pub(super) state: VmPostgresTransactionState,
}

#[derive(Clone, Debug)]
pub(super) struct PreparedStatementState {
    pub(super) owner: VmProcessId,
    pub(super) driver_statement: VmPostgresDriverPreparedStatement,
}

#[derive(Clone, Debug)]
pub(super) struct RowState {
    pub(super) owner: VmProcessId,
    pub(super) driver_row: VmPostgresDriverRow,
}

#[derive(Clone, Debug)]
pub(super) struct PendingRequest {
    pub(super) owner: VmProcessId,
    pub(super) scheduled: VmScheduledNativeBoundaryRequest,
    pub(super) operation: VmPostgresDriverOperation,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CleanupMetrics {
    pub(super) cancellations: u64,
    pub(super) rollbacks: u64,
    pub(super) releases: u64,
    pub(super) closed_pools: u64,
    pub(super) dropped_prepared_statements: u64,
    pub(super) dropped_result_sets: u64,
    pub(super) dropped_rows: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VmPostgresRuntimeEvent {
    pub(super) request_id: u64,
    pub(super) owner_process_id: u64,
    pub(super) operation: String,
    pub(super) outcome: String,
    pub(super) sql_fingerprint: Option<String>,
    pub(super) error_code: Option<String>,
}
