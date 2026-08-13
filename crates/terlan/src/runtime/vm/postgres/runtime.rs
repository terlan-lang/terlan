use std::collections::{BTreeMap, VecDeque};

use super::completion::{
    deadline_reply, validate_driver_completion, validate_sql, worker_reply_to_postgres,
};
use super::state::*;
use super::types::*;
use crate::runtime::vm::{
    native_boundary::deadline::{VmNativeBoundaryDeadlineQueue, VmNativeBoundaryDeadlineStart},
    process::{VmProcessId, VmProcessTable},
    scheduler::VmScheduler,
    timer::{VmTimerEvent, VmTimerTable},
};
use crate::terlan_native::json;
use crate::terlan_native_boundary::request::RequestId;

const EVENT_LIMIT: usize = 2_048;
const DISPATCH_REDUCTIONS: u64 = 2;
#[cfg(any(
    test,
    all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
))]
const ROW_DECODE_REDUCTIONS: u64 = 1;

/// VM-owned Postgres scheduling, typed-resource, and terminal-state registry.
#[derive(Debug)]
pub(crate) struct VmPostgresRuntime {
    pub(super) deadlines: VmNativeBoundaryDeadlineQueue,
    pub(super) next_request_id: u64,
    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    pub(super) next_resource_id: u64,
    pub(super) pending: BTreeMap<u64, PendingRequest>,
    pub(super) dispatches: VecDeque<u64>,
    pub(super) completion_controls: VecDeque<VmPostgresDriverControl>,
    pub(super) replies: BTreeMap<u64, VmPostgresReply>,
    pub(super) pools: BTreeMap<VmPostgresPool, PoolState>,
    pub(super) connections: BTreeMap<VmPostgresConnection, ConnectionState>,
    pub(super) transactions: BTreeMap<VmPostgresTransaction, TransactionState>,
    pub(super) prepared: BTreeMap<VmPostgresPreparedStatement, PreparedStatementState>,
    pub(super) result_sets: BTreeMap<VmPostgresResultSet, VmProcessId>,
    pub(super) rows: BTreeMap<VmPostgresRow, RowState>,
    pub(super) events: VecDeque<VmPostgresRuntimeEvent>,
    pub(super) cleanup: CleanupMetrics,
}

/// Scheduler-owned state and deadline policy shared by one Postgres request.
pub(crate) struct VmPostgresRequestContext<'a> {
    pub(super) timers: &'a mut VmTimerTable,
    pub(super) processes: &'a mut VmProcessTable,
    pub(super) scheduler: &'a mut VmScheduler,
    pub(super) owner: VmProcessId,
    pub(super) now_tick: u64,
    pub(super) timeout_ticks: u64,
}

impl<'a> VmPostgresRequestContext<'a> {
    pub(crate) fn new(
        timers: &'a mut VmTimerTable,
        processes: &'a mut VmProcessTable,
        scheduler: &'a mut VmScheduler,
        owner: VmProcessId,
        deadline: VmPostgresDeadline,
    ) -> Self {
        Self {
            timers,
            processes,
            scheduler,
            owner,
            now_tick: deadline.now_tick,
            timeout_ticks: deadline.timeout_ticks,
        }
    }
}

impl VmPostgresRuntime {
    pub(crate) fn new(credit_limit: u64) -> Self {
        Self {
            deadlines: VmNativeBoundaryDeadlineQueue::new(credit_limit),
            next_request_id: 1,
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            next_resource_id: 1,
            pending: BTreeMap::new(),
            dispatches: VecDeque::new(),
            completion_controls: VecDeque::new(),
            replies: BTreeMap::new(),
            pools: BTreeMap::new(),
            connections: BTreeMap::new(),
            transactions: BTreeMap::new(),
            prepared: BTreeMap::new(),
            result_sets: BTreeMap::new(),
            rows: BTreeMap::new(),
            events: VecDeque::new(),
            cleanup: CleanupMetrics::default(),
        }
    }

    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    pub(crate) fn connect(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        config: VmPostgresConnectConfig,
    ) -> Result<RequestId, String> {
        self.submit(context, VmPostgresDriverOperation::Connect(config))
    }

    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    pub(crate) fn acquire(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        pool: VmPostgresPool,
    ) -> Result<RequestId, String> {
        let owner = context.owner;
        let pool_state = self.owned_pool_mut(pool, owner)?;
        if pool_state.active_connections + pool_state.reserved_connections
            >= pool_state.max_connections
        {
            return Err(
                "error[postgres.pool.exhausted]: Postgres pool has no available connection slots"
                    .to_string(),
            );
        }
        pool_state.reserved_connections += 1;
        let driver_pool = pool_state.driver_pool;
        let result = self.submit(
            context,
            VmPostgresDriverOperation::Acquire { pool, driver_pool },
        );
        if result.is_err() {
            self.pools
                .get_mut(&pool)
                .expect("validated pool remains live")
                .reserved_connections -= 1;
        }
        result
    }
    pub(crate) fn query(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        target: VmPostgresQueryTarget,
        sql: impl Into<String>,
        parameters: Vec<json::Json>,
        one: bool,
    ) -> Result<RequestId, String> {
        let owner = context.owner;
        let driver_target = self.driver_target(target, owner)?;
        let sql = validate_sql(sql.into())?;
        self.submit(
            context,
            VmPostgresDriverOperation::Query {
                target,
                driver_target,
                sql,
                parameters,
                one,
            },
        )
    }

    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    pub(crate) fn begin(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        connection: VmPostgresConnection,
    ) -> Result<RequestId, String> {
        let owner = context.owner;
        let state = self.owned_connection(connection, owner)?;
        if state.active_transaction.is_some() {
            return Err(
                "error[postgres.transaction.active]: connection already owns a transaction"
                    .to_string(),
            );
        }
        let driver_connection = state.driver_connection;
        self.submit(
            context,
            VmPostgresDriverOperation::Begin {
                connection,
                driver_connection,
            },
        )
    }
    pub(crate) fn execute(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        target: VmPostgresQueryTarget,
        sql: impl Into<String>,
        parameters: Vec<json::Json>,
    ) -> Result<RequestId, String> {
        let owner = context.owner;
        let driver_target = self.driver_target(target, owner)?;
        let sql = validate_sql(sql.into())?;
        self.submit(
            context,
            VmPostgresDriverOperation::Execute {
                target,
                driver_target,
                sql,
                parameters,
            },
        )
    }
    #[cfg(test)]
    pub(crate) fn prepare(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        connection: VmPostgresConnection,
        sql: impl Into<String>,
        parameter_count: usize,
    ) -> Result<RequestId, String> {
        let owner = context.owner;
        let driver_connection = self.owned_connection(connection, owner)?.driver_connection;
        let sql = validate_sql(sql.into())?;
        self.submit(
            context,
            VmPostgresDriverOperation::Prepare {
                connection,
                driver_connection,
                sql,
                parameter_count,
            },
        )
    }
    pub(crate) fn decode(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        row: VmPostgresRow,
        column: impl Into<String>,
        expected: VmPostgresDecodeType,
    ) -> Result<RequestId, String> {
        let owner = context.owner;
        let driver_row = match self.rows.get(&row) {
            Some(state) if state.owner == owner => state.driver_row,
            Some(_) => {
                return Err(
                    "error[postgres.resource.owner]: row belongs to another process".to_string(),
                );
            }
            None => return Err("error[postgres.row.stale]: Postgres row is not live".to_string()),
        };
        let column = column.into();
        if column.trim().is_empty() {
            return Err("error[postgres.decode.column]: column name must not be empty".to_string());
        }
        self.submit(
            context,
            VmPostgresDriverOperation::Decode {
                row,
                driver_row,
                column,
                expected,
            },
        )
    }

    pub(crate) fn finish_transaction(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        transaction: VmPostgresTransaction,
        commit: bool,
    ) -> Result<RequestId, String> {
        let owner = context.owner;
        let driver_transaction = self
            .active_transaction(transaction, owner)?
            .driver_transaction;
        let operation = if commit {
            VmPostgresDriverOperation::Commit {
                transaction,
                driver_transaction,
            }
        } else {
            VmPostgresDriverOperation::Rollback {
                transaction,
                driver_transaction,
            }
        };
        self.submit(context, operation)
    }

    pub(crate) fn take_dispatch(&mut self) -> Option<VmPostgresDriverRequest> {
        while let Some(request_id) = self.dispatches.pop_front() {
            if let Some(pending) = self.pending.get(&request_id) {
                return Some(VmPostgresDriverRequest {
                    request_id: pending.scheduled.request_id,
                    owner: pending.owner,
                    operation: pending.operation.clone(),
                });
            }
        }
        None
    }

    pub(crate) fn take_completion_control(&mut self) -> Option<VmPostgresDriverControl> {
        self.completion_controls.pop_front()
    }

    pub(crate) fn complete(
        &mut self,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        request_id: RequestId,
        completion: VmPostgresDriverCompletion,
    ) -> Result<(), String> {
        let pending = self
            .pending
            .get(&request_id.value)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "error[postgres.request.stale]: request {} is not pending",
                    request_id.value
                )
            })?;
        validate_driver_completion(&pending.operation, &completion)?;
        self.deadlines
            .require_completable(timers, processes, pending.scheduled.timer_id)?;
        let (reply, control) = self.apply_completion(processes, scheduler, &pending, completion)?;
        self.deadlines
            .complete(timers, processes, scheduler, pending.scheduled.timer_id)?;
        self.pending.remove(&request_id.value);
        self.record_terminal(&pending, &reply, "completed");
        self.replies.insert(request_id.value, reply);
        self.completion_controls.extend(control);
        Ok(())
    }

    pub(crate) fn cancel(
        &mut self,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        request_id: RequestId,
    ) -> Result<VmPostgresDriverControl, String> {
        let pending = self
            .pending
            .get(&request_id.value)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "error[postgres.request.stale]: request {} is not pending",
                    request_id.value
                )
            })?;
        self.deadlines
            .cancel(timers, processes, scheduler, pending.scheduled.timer_id)?;
        self.release_reservation(&pending.operation);
        self.cleanup.cancellations += 1;
        self.pending.remove(&request_id.value);
        let reply = VmPostgresReply::Error(VmPostgresFailure::new(
            "postgres.cancelled",
            "Postgres operation was cancelled.",
        ));
        self.record_terminal(&pending, &reply, "cancelled");
        self.replies.insert(request_id.value, reply);
        Ok(VmPostgresDriverControl::Cancel(request_id))
    }

    pub(crate) fn handle_timer_event(
        &mut self,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        event: &VmTimerEvent,
    ) -> Result<Option<VmPostgresDriverControl>, String> {
        let completion = self
            .deadlines
            .handle_timer_event(processes, scheduler, event)?;
        let Some(completion) = completion else {
            return Ok(None);
        };
        let (request_id, outcome, reply) = deadline_reply(completion);
        let pending = self.pending.remove(&request_id.value).ok_or_else(|| {
            format!(
                "error[postgres.request.stale]: request {} is not pending",
                request_id.value
            )
        })?;
        self.release_reservation(&pending.operation);
        self.cleanup.cancellations += 1;
        let typed_reply = worker_reply_to_postgres(reply, outcome);
        self.record_terminal(&pending, &typed_reply, outcome);
        self.replies.insert(request_id.value, typed_reply);
        Ok(Some(VmPostgresDriverControl::Cancel(request_id)))
    }

    pub(crate) fn take_reply(
        &mut self,
        owner: VmProcessId,
        request_id: RequestId,
    ) -> Result<VmPostgresReply, String> {
        if self
            .events
            .iter()
            .find(|event| event.request_id == request_id.value)
            .is_some_and(|event| event.owner_process_id != owner.as_u64())
        {
            return Err(
                "error[postgres.resource.owner]: request belongs to another process".to_string(),
            );
        }
        self.replies.remove(&request_id.value).ok_or_else(|| {
            format!(
                "error[postgres.reply.unavailable]: request {} has no reply",
                request_id.value
            )
        })
    }

    pub(crate) fn cleanup_owner(&mut self, owner: VmProcessId) -> Vec<VmPostgresDriverControl> {
        let mut controls = Vec::new();
        let mut transaction_connections = Vec::new();
        for (&transaction, state) in &mut self.transactions {
            if state.owner == owner && state.state == VmPostgresTransactionState::Active {
                state.state = VmPostgresTransactionState::RolledBack;
                transaction_connections.push(state.connection);
                self.cleanup.rollbacks += 1;
                controls.push(VmPostgresDriverControl::Rollback {
                    transaction,
                    driver_transaction: state.driver_transaction,
                });
            }
        }
        for connection in transaction_connections {
            let state = self
                .connections
                .get_mut(&connection)
                .expect("active transaction connection remains registered");
            state.active_transaction = None;
            if state.open {
                state.open = false;
                if let Some(pool) = self.pools.get_mut(&state.pool) {
                    pool.active_connections = pool.active_connections.saturating_sub(1);
                }
                self.cleanup.releases += 1;
            }
        }
        let prepared = self
            .prepared
            .iter()
            .filter_map(|(&statement, state)| {
                (state.owner == owner).then_some((statement, state.driver_statement))
            })
            .collect::<Vec<_>>();
        for (statement, driver_statement) in prepared {
            self.prepared.remove(&statement);
            controls.push(VmPostgresDriverControl::DropPreparedStatement {
                statement,
                driver_statement,
            });
            self.cleanup.dropped_prepared_statements += 1;
        }
        let rows = self
            .rows
            .iter()
            .filter_map(|(&row, state)| (state.owner == owner).then_some((row, state.driver_row)))
            .collect::<Vec<_>>();
        for (row, driver_row) in rows {
            self.rows.remove(&row);
            controls.push(VmPostgresDriverControl::DropRow { row, driver_row });
            self.cleanup.dropped_rows += 1;
        }
        for (&connection, state) in &mut self.connections {
            if state.owner == owner && state.open {
                state.open = false;
                if let Some(pool) = self.pools.get_mut(&state.pool) {
                    pool.active_connections = pool.active_connections.saturating_sub(1);
                }
                self.cleanup.releases += 1;
                controls.push(VmPostgresDriverControl::Release {
                    connection,
                    driver_connection: state.driver_connection,
                });
            }
        }
        for (&pool, state) in &mut self.pools {
            if state.owner == owner && state.open {
                state.open = false;
                self.cleanup.closed_pools += 1;
                controls.push(VmPostgresDriverControl::ClosePool {
                    pool,
                    driver_pool: state.driver_pool,
                });
            }
        }
        let result_sets_before = self.result_sets.len();
        self.result_sets
            .retain(|_, resource_owner| *resource_owner != owner);
        self.cleanup.dropped_result_sets += (result_sets_before - self.result_sets.len()) as u64;
        controls
    }

    #[cfg(test)]
    pub(crate) fn transaction_state(
        &self,
        transaction: VmPostgresTransaction,
    ) -> Option<VmPostgresTransactionState> {
        self.transactions.get(&transaction).map(|state| state.state)
    }
    pub(super) fn submit(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        operation: VmPostgresDriverOperation,
    ) -> Result<RequestId, String> {
        let VmPostgresRequestContext {
            timers,
            processes,
            scheduler,
            owner,
            now_tick,
            timeout_ticks,
        } = context;
        let request_id = RequestId {
            value: self.next_request_id,
        };
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| "error[postgres.request.overflow]: request id overflow".to_string())?;
        let scheduled = self.deadlines.start(
            timers,
            processes,
            scheduler,
            VmNativeBoundaryDeadlineStart {
                owner,
                request_id,
                now_tick,
                timeout_ticks,
            },
        )?;
        scheduler.charge_runtime_reductions(processes, owner, DISPATCH_REDUCTIONS)?;
        let event = VmPostgresRuntimeEvent {
            request_id: request_id.value,
            owner_process_id: owner.as_u64(),
            operation: operation.name().to_string(),
            outcome: "dispatched".to_string(),
            sql_fingerprint: operation.sql_fingerprint(),
            error_code: None,
        };
        self.push_event(event);
        self.pending.insert(
            request_id.value,
            PendingRequest {
                owner,
                scheduled,
                operation,
            },
        );
        self.dispatches.push_back(request_id.value);
        Ok(request_id)
    }

    fn apply_completion(
        &mut self,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        pending: &PendingRequest,
        completion: VmPostgresDriverCompletion,
    ) -> Result<(VmPostgresReply, Option<VmPostgresDriverControl>), String> {
        #[cfg(not(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        )))]
        {
            let _ = (processes, scheduler);
            let VmPostgresDriverCompletion::Failed(error) = completion;
            self.release_reservation(&pending.operation);
            Ok((VmPostgresReply::Error(error), None))
        }
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        if let VmPostgresDriverCompletion::Failed(error) = completion {
            self.release_reservation(&pending.operation);
            return Ok((VmPostgresReply::Error(error), None));
        }
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        match (&pending.operation, completion) {
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            (
                VmPostgresDriverOperation::Connect(config),
                VmPostgresDriverCompletion::Connected(driver_pool),
            ) => {
                let pool = VmPostgresPool(self.allocate_resource_id()?);
                self.pools.insert(
                    pool,
                    PoolState {
                        owner: pending.owner,
                        driver_pool,
                        max_connections: config.max_connections(),
                        active_connections: 0,
                        reserved_connections: 0,
                        open: true,
                    },
                );
                Ok((VmPostgresReply::Pool(pool), None))
            }
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            (
                VmPostgresDriverOperation::Acquire { pool, .. },
                VmPostgresDriverCompletion::Acquired(driver_connection),
            ) => {
                let connection = VmPostgresConnection(self.allocate_resource_id()?);
                let state = self
                    .pools
                    .get_mut(pool)
                    .expect("pending acquire owns live pool");
                state.reserved_connections -= 1;
                state.active_connections += 1;
                self.connections.insert(
                    connection,
                    ConnectionState {
                        owner: pending.owner,
                        pool: *pool,
                        driver_connection,
                        active_transaction: None,
                        open: true,
                    },
                );
                Ok((VmPostgresReply::Connection(connection), None))
            }
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            (
                VmPostgresDriverOperation::Begin { connection, .. },
                VmPostgresDriverCompletion::TransactionStarted(driver_transaction),
            ) => {
                let transaction = VmPostgresTransaction(self.allocate_resource_id()?);
                self.connections
                    .get_mut(connection)
                    .expect("pending begin owns live connection")
                    .active_transaction = Some(transaction);
                self.transactions.insert(
                    transaction,
                    TransactionState {
                        owner: pending.owner,
                        connection: *connection,
                        driver_transaction,
                        state: VmPostgresTransactionState::Active,
                    },
                );
                Ok((VmPostgresReply::Transaction(transaction), None))
            }
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            (
                VmPostgresDriverOperation::Commit { transaction, .. },
                VmPostgresDriverCompletion::Unit,
            ) => {
                let control =
                    self.terminal_transaction(*transaction, VmPostgresTransactionState::Committed)?;
                Ok((VmPostgresReply::Unit, Some(control)))
            }
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            (
                VmPostgresDriverOperation::Rollback { transaction, .. },
                VmPostgresDriverCompletion::Unit,
            ) => {
                let control = self
                    .terminal_transaction(*transaction, VmPostgresTransactionState::RolledBack)?;
                Ok((VmPostgresReply::Unit, Some(control)))
            }
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            (
                VmPostgresDriverOperation::Query { .. },
                VmPostgresDriverCompletion::Rows { rows: driver_rows },
            ) => {
                let count = driver_rows.len();
                let result_set = VmPostgresResultSet(self.allocate_resource_id()?);
                self.result_sets.insert(result_set, pending.owner);
                let mut rows = Vec::with_capacity(count);
                for driver_row in driver_rows {
                    let row = VmPostgresRow(self.allocate_resource_id()?);
                    self.rows.insert(
                        row,
                        RowState {
                            owner: pending.owner,
                            driver_row,
                        },
                    );
                    rows.push(row);
                }
                scheduler.charge_runtime_reductions(
                    processes,
                    pending.owner,
                    ROW_DECODE_REDUCTIONS.saturating_mul(count as u64),
                )?;
                Ok((VmPostgresReply::Rows { result_set, rows }, None))
            }
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            (
                VmPostgresDriverOperation::Execute { .. },
                VmPostgresDriverCompletion::AffectedRows(count),
            ) => Ok((VmPostgresReply::AffectedRows(count), None)),
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            (VmPostgresDriverOperation::BatchExecute { .. }, VmPostgresDriverCompletion::Unit) => {
                Ok((VmPostgresReply::Unit, None))
            }
            #[cfg(test)]
            (
                VmPostgresDriverOperation::Prepare { .. },
                VmPostgresDriverCompletion::Prepared(driver_statement),
            ) => {
                let statement = VmPostgresPreparedStatement(self.allocate_resource_id()?);
                self.prepared.insert(
                    statement,
                    PreparedStatementState {
                        owner: pending.owner,
                        driver_statement,
                    },
                );
                Ok((VmPostgresReply::PreparedStatement(statement), None))
            }
            #[cfg(any(
                test,
                all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
            ))]
            (
                VmPostgresDriverOperation::Decode { .. },
                VmPostgresDriverCompletion::Decoded(value),
            ) => {
                scheduler.charge_runtime_reductions(
                    processes,
                    pending.owner,
                    ROW_DECODE_REDUCTIONS,
                )?;
                Ok((VmPostgresReply::Decoded(value), None))
            }
            _ => Err(format!(
                "error[postgres.driver.protocol]: invalid completion for {}",
                pending.operation.name()
            )),
        }
    }

    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    fn allocate_resource_id(&mut self) -> Result<u64, String> {
        let id = self.next_resource_id;
        self.next_resource_id = id
            .checked_add(1)
            .ok_or_else(|| "error[postgres.resource.overflow]: resource id overflow".to_string())?;
        Ok(id)
    }

    fn owned_pool_mut(
        &mut self,
        pool: VmPostgresPool,
        owner: VmProcessId,
    ) -> Result<&mut PoolState, String> {
        let state = self
            .pools
            .get_mut(&pool)
            .ok_or_else(|| "error[postgres.pool.stale]: Postgres pool is not live".to_string())?;
        if state.owner != owner {
            return Err(
                "error[postgres.resource.owner]: pool belongs to another process".to_string(),
            );
        }
        if !state.open {
            return Err("error[postgres.pool.closed]: Postgres pool is closed".to_string());
        }
        Ok(state)
    }

    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    fn owned_connection(
        &self,
        connection: VmPostgresConnection,
        owner: VmProcessId,
    ) -> Result<&ConnectionState, String> {
        let state = self.connections.get(&connection).ok_or_else(|| {
            "error[postgres.connection.stale]: Postgres connection is not live".to_string()
        })?;
        if state.owner != owner {
            return Err(
                "error[postgres.resource.owner]: connection belongs to another process".to_string(),
            );
        }
        if !state.open {
            return Err(
                "error[postgres.connection.closed]: Postgres connection is closed".to_string(),
            );
        }
        Ok(state)
    }

    fn active_transaction(
        &self,
        transaction: VmPostgresTransaction,
        owner: VmProcessId,
    ) -> Result<&TransactionState, String> {
        let state = self.transactions.get(&transaction).ok_or_else(|| {
            "error[postgres.transaction.stale]: Postgres transaction is not live".to_string()
        })?;
        if state.owner != owner {
            return Err(
                "error[postgres.resource.owner]: transaction belongs to another process"
                    .to_string(),
            );
        }
        if state.state != VmPostgresTransactionState::Active {
            return Err(
                "error[postgres.transaction.terminal]: Postgres transaction is terminal"
                    .to_string(),
            );
        }
        Ok(state)
    }

    pub(super) fn driver_target(
        &mut self,
        target: VmPostgresQueryTarget,
        owner: VmProcessId,
    ) -> Result<VmPostgresDriverQueryTarget, String> {
        match target {
            VmPostgresQueryTarget::Pool(pool) => self
                .owned_pool_mut(pool, owner)
                .map(|state| VmPostgresDriverQueryTarget::Pool(state.driver_pool)),
            VmPostgresQueryTarget::Transaction(transaction) => self
                .active_transaction(transaction, owner)
                .map(|state| VmPostgresDriverQueryTarget::Transaction(state.driver_transaction)),
        }
    }

    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    fn terminal_transaction(
        &mut self,
        transaction: VmPostgresTransaction,
        terminal: VmPostgresTransactionState,
    ) -> Result<VmPostgresDriverControl, String> {
        let state = self.transactions.get_mut(&transaction).ok_or_else(|| {
            "error[postgres.transaction.stale]: Postgres transaction is not live".to_string()
        })?;
        if state.state != VmPostgresTransactionState::Active {
            return Err(
                "error[postgres.transaction.terminal]: Postgres transaction is terminal"
                    .to_string(),
            );
        }
        state.state = terminal;
        let connection = self
            .connections
            .get_mut(&state.connection)
            .expect("transaction connection remains registered");
        connection.active_transaction = None;
        connection.open = false;
        self.pools
            .get_mut(&connection.pool)
            .expect("transaction pool remains registered")
            .active_connections -= 1;
        self.cleanup.releases += 1;
        Ok(VmPostgresDriverControl::Release {
            connection: state.connection,
            driver_connection: connection.driver_connection,
        })
    }

    fn release_reservation(&mut self, operation: &VmPostgresDriverOperation) {
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        if let VmPostgresDriverOperation::Acquire { pool, .. } = operation {
            if let Some(state) = self.pools.get_mut(pool) {
                state.reserved_connections = state.reserved_connections.saturating_sub(1);
            }
        }
        #[cfg(not(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        )))]
        let _ = operation;
    }

    fn record_terminal(
        &mut self,
        pending: &PendingRequest,
        reply: &VmPostgresReply,
        outcome: &str,
    ) {
        self.push_event(VmPostgresRuntimeEvent {
            request_id: pending.scheduled.request_id.value,
            owner_process_id: pending.owner.as_u64(),
            operation: pending.operation.name().to_string(),
            outcome: outcome.to_string(),
            sql_fingerprint: pending.operation.sql_fingerprint(),
            error_code: match reply {
                VmPostgresReply::Error(error) => Some(error.code.clone()),
                #[cfg(any(
                    test,
                    all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
                ))]
                _ => None,
            },
        });
    }

    fn push_event(&mut self, event: VmPostgresRuntimeEvent) {
        if self.events.len() == EVENT_LIMIT {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}
