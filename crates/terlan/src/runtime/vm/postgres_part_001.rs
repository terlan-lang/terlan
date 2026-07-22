use std::collections::{BTreeMap, VecDeque};
pub(crate) use inspection::*;
pub(crate) use libpq_worker::*;
use state::*;
pub(crate) use types::*;

use super::{
    native_boundary::deadline::{
        VmNativeBoundaryDeadlineCompletion, VmNativeBoundaryDeadlineQueue,
    },
    process::{VmProcessId, VmProcessTable},
    scheduler::VmScheduler,
    timer::{VmTimerEvent, VmTimerTable},
};
use crate::terlan_native::json;
use crate::terlan_native_boundary::{request::RequestId, term::NativeBoundaryReplyTerm};

const EVENT_LIMIT: usize = 2_048;
const DISPATCH_REDUCTIONS: u64 = 2;
const ROW_DECODE_REDUCTIONS: u64 = 1;

/// VM-owned Postgres scheduling, typed-resource, and terminal-state registry.
#[derive(Debug)]
pub(crate) struct VmPostgresRuntime {
    deadlines: VmNativeBoundaryDeadlineQueue,
    next_request_id: u64,
    next_resource_id: u64,
    pending: BTreeMap<u64, PendingRequest>,
    dispatches: VecDeque<u64>,
    completion_controls: VecDeque<VmPostgresDriverControl>,
    replies: BTreeMap<u64, VmPostgresReply>,
    pools: BTreeMap<VmPostgresPool, PoolState>,
    connections: BTreeMap<VmPostgresConnection, ConnectionState>,
    transactions: BTreeMap<VmPostgresTransaction, TransactionState>,
    prepared: BTreeMap<VmPostgresPreparedStatement, PreparedStatementState>,
    result_sets: BTreeMap<VmPostgresResultSet, VmProcessId>,
    rows: BTreeMap<VmPostgresRow, RowState>,
    events: VecDeque<VmPostgresRuntimeEvent>,
    cleanup: CleanupMetrics,
}
