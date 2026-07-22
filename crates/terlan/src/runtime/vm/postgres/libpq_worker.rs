use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    terlan_native::{json, postgres},
    terlan_native_boundary::request::RequestId,
};

use super::{
    VmPostgresDecodeType, VmPostgresDecodedValue, VmPostgresDriverCompletion,
    VmPostgresDriverConnection, VmPostgresDriverControl, VmPostgresDriverOperation,
    VmPostgresDriverPool, VmPostgresDriverPreparedStatement, VmPostgresDriverQueryTarget,
    VmPostgresDriverRequest, VmPostgresDriverRow, VmPostgresDriverTransaction, VmPostgresFailure,
};
use crate::terlan_native::postgres::libpq::{ConnectPoll, DriverConnection, DriverResult};

const COMMAND_OK: i64 = 1;
const TUPLES_OK: i64 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmPostgresIoInterest {
    Drive,
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmPostgresDriverWait {
    pub(crate) request_id: RequestId,
    pub(crate) socket: i64,
    pub(crate) interest: VmPostgresIoInterest,
}

#[derive(Debug)]
struct PoolEntry {
    config: postgres::Config,
    idle: Vec<DriverConnection>,
}

#[derive(Debug)]
enum ReturnConnection {
    Acquire {
        pool: VmPostgresDriverPool,
    },
    Pool {
        pool: VmPostgresDriverPool,
    },
    Connection(VmPostgresDriverConnection),
    Transaction {
        transaction: VmPostgresDriverTransaction,
        connection: VmPostgresDriverConnection,
        finish_on_success: bool,
    },
}

#[derive(Debug)]
enum PendingTask {
    Acquire,
    Query { one: bool },
    Execute,
    BatchExecute,
    Begin,
    FinishTransaction,
    Prepare { sql: String, parameter_count: usize },
}

#[derive(Debug)]
enum PendingPhase {
    Connecting {
        queued: Option<(String, Vec<json::Json>)>,
    },
    Sending {
        sql: String,
        parameters: Vec<json::Json>,
    },
    Reading,
    Draining(VmPostgresDriverCompletion),
}

#[derive(Debug)]
struct PendingIo {
    request_id: RequestId,
    connection: Option<DriverConnection>,
    return_connection: ReturnConnection,
    task: PendingTask,
    phase: PendingPhase,
}

impl PendingIo {
    fn conflicts_with(&self, operation: &VmPostgresDriverOperation) -> bool {
        match operation {
            VmPostgresDriverOperation::Begin {
                driver_connection, ..
            }
            | VmPostgresDriverOperation::Prepare {
                driver_connection, ..
            } => self.owns_connection(*driver_connection),
            VmPostgresDriverOperation::Query {
                driver_target: VmPostgresDriverQueryTarget::Transaction(transaction),
                ..
            }
            | VmPostgresDriverOperation::Execute {
                driver_target: VmPostgresDriverQueryTarget::Transaction(transaction),
                ..
            }
            | VmPostgresDriverOperation::BatchExecute {
                driver_target: VmPostgresDriverQueryTarget::Transaction(transaction),
                ..
            }
            | VmPostgresDriverOperation::Commit {
                driver_transaction: transaction,
                ..
            }
            | VmPostgresDriverOperation::Rollback {
                driver_transaction: transaction,
                ..
            } => self.owns_transaction(*transaction),
            VmPostgresDriverOperation::Connect(_)
            | VmPostgresDriverOperation::Acquire { .. }
            | VmPostgresDriverOperation::Query {
                driver_target: VmPostgresDriverQueryTarget::Pool(_),
                ..
            }
            | VmPostgresDriverOperation::Execute {
                driver_target: VmPostgresDriverQueryTarget::Pool(_),
                ..
            }
            | VmPostgresDriverOperation::BatchExecute {
                driver_target: VmPostgresDriverQueryTarget::Pool(_),
                ..
            }
            | VmPostgresDriverOperation::Decode { .. } => false,
        }
    }

    fn owns_connection(&self, connection: VmPostgresDriverConnection) -> bool {
        match self.return_connection {
            ReturnConnection::Connection(active) => active == connection,
            ReturnConnection::Transaction {
                connection: active, ..
            } => active == connection,
            ReturnConnection::Acquire { .. } | ReturnConnection::Pool { .. } => false,
        }
    }

    fn owns_transaction(&self, transaction: VmPostgresDriverTransaction) -> bool {
        matches!(
            self.return_connection,
            ReturnConnection::Transaction {
                transaction: active,
                ..
            } if active == transaction
        )
    }
}

#[derive(Default)]
pub(crate) struct VmPostgresLibpqWorker {
    next_resource_id: u64,
    pools: BTreeMap<VmPostgresDriverPool, PoolEntry>,
    connections: BTreeMap<VmPostgresDriverConnection, DriverConnection>,
    connection_pools: BTreeMap<VmPostgresDriverConnection, VmPostgresDriverPool>,
    transactions: BTreeMap<VmPostgresDriverTransaction, VmPostgresDriverConnection>,
    prepared: BTreeMap<VmPostgresDriverPreparedStatement, String>,
    rows: BTreeMap<VmPostgresDriverRow, postgres::Row>,
    queued: VecDeque<VmPostgresDriverRequest>,
    active: BTreeMap<u64, PendingIo>,
    completions: VecDeque<(RequestId, VmPostgresDriverCompletion)>,
    cancelled: BTreeSet<u64>,
    waits: BTreeMap<u64, VmPostgresDriverWait>,
}

impl std::fmt::Debug for VmPostgresLibpqWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VmPostgresLibpqWorker")
            .field("pools", &self.pools.len())
            .field("connections", &self.connections.len())
            .field("transactions", &self.transactions.len())
            .field("prepared", &self.prepared.len())
            .field("rows", &self.rows.len())
            .field("queued", &self.queued.len())
            .field("active", &self.active.len())
            .field("completions", &self.completions.len())
            .field("cancelled", &self.cancelled.len())
            .finish()
    }
}

impl VmPostgresLibpqWorker {
    pub(crate) fn submit(&mut self, request: VmPostgresDriverRequest) {
        self.queued.push_back(request);
    }

    pub(crate) fn wait(&self) -> Option<VmPostgresDriverWait> {
        self.waits.values().next().copied()
    }

    pub(crate) fn waits(&self) -> Vec<VmPostgresDriverWait> {
        self.waits.values().copied().collect()
    }

    pub(crate) fn drive_once(&mut self) -> Option<(RequestId, VmPostgresDriverCompletion)> {
        self.drive_ready(None)
    }

    pub(crate) fn drive_socket_ready(
        &mut self,
        ready: &BTreeSet<u64>,
    ) -> Option<(RequestId, VmPostgresDriverCompletion)> {
        self.drive_ready(Some(ready))
    }

    fn drive_ready(
        &mut self,
        socket_ready: Option<&BTreeSet<u64>>,
    ) -> Option<(RequestId, VmPostgresDriverCompletion)> {
        let queued = self.queued.len();
        for _ in 0..queued {
            let request = self
                .queued
                .pop_front()
                .expect("queued request count is stable");
            if self.request_conflicts(&request) {
                self.queued.push_back(request);
                continue;
            }
            let request_id = request.request_id;
            if self.cancelled.remove(&request_id.value) {
                self.completions.push_back((request_id, cancelled()));
                continue;
            }
            match self.start(request) {
                Ok(Some(completion)) => self.completions.push_back((request_id, completion)),
                Ok(None) => {}
                Err(error) => self.completions.push_back((request_id, failure(error))),
            }
        }

        let active_ids = self
            .active
            .keys()
            .copied()
            .filter(|request_id| self.should_advance(*request_id, socket_ready))
            .collect::<Vec<_>>();
        for active_id in active_ids {
            let mut pending = self
                .active
                .remove(&active_id)
                .expect("active request id was collected from the worker");
            self.waits.remove(&active_id);
            let request_id = pending.request_id;
            match self.advance(&mut pending) {
                Ok(Some(completion)) => self.completions.push_back((request_id, completion)),
                Ok(None) => {
                    self.active.insert(active_id, pending);
                }
                Err(error) => {
                    self.restore_after_failure(pending);
                    self.completions.push_back((request_id, failure(error)));
                }
            }
        }

        self.completions.pop_front()
    }

    fn should_advance(&self, request_id: u64, socket_ready: Option<&BTreeSet<u64>>) -> bool {
        let Some(ready) = socket_ready else {
            return true;
        };
        match self.waits.get(&request_id) {
            None
            | Some(VmPostgresDriverWait {
                interest: VmPostgresIoInterest::Drive,
                ..
            }) => true,
            Some(_) => ready.contains(&request_id),
        }
    }

    pub(crate) fn apply_control(
        &mut self,
        control: VmPostgresDriverControl,
    ) -> Result<(), VmPostgresFailure> {
        match control {
            VmPostgresDriverControl::Cancel(request_id) => {
                if let Some(mut pending) = self.active.remove(&request_id.value) {
                    pending
                        .connection
                        .as_mut()
                        .expect("active request owns its connection")
                        .abort()
                        .map_err(adapter_failure)?;
                    self.waits.remove(&request_id.value);
                } else if let Some(index) = self
                    .queued
                    .iter()
                    .position(|request| request.request_id == request_id)
                {
                    self.queued.remove(index);
                } else {
                    self.cancelled.insert(request_id.value);
                }
                Ok(())
            }
            VmPostgresDriverControl::Rollback {
                driver_transaction, ..
            } => {
                let connection = self
                    .transactions
                    .remove(&driver_transaction)
                    .ok_or_else(|| stale_failure("transaction"))?;
                let mut driver = self
                    .connections
                    .remove(&connection)
                    .ok_or_else(|| stale_failure("connection"))?;
                self.connection_pools.remove(&connection);
                driver.abort().map_err(adapter_failure)
            }
            VmPostgresDriverControl::Release {
                driver_connection, ..
            } => self.release_connection(driver_connection),
            VmPostgresDriverControl::ClosePool { driver_pool, .. } => {
                if self
                    .connection_pools
                    .values()
                    .any(|pool| *pool == driver_pool)
                {
                    return Err(VmPostgresFailure::new(
                        "postgres.pool.active_connections",
                        "Postgres pool still owns active connections.",
                    ));
                }
                self.pools
                    .remove(&driver_pool)
                    .ok_or_else(|| stale_failure("pool"))?;
                Ok(())
            }
            VmPostgresDriverControl::DropPreparedStatement {
                driver_statement, ..
            } => {
                self.prepared.remove(&driver_statement);
                Ok(())
            }
            VmPostgresDriverControl::DropRow { driver_row, .. } => {
                self.rows.remove(&driver_row);
                Ok(())
            }
        }
    }

    fn start(
        &mut self,
        request: VmPostgresDriverRequest,
    ) -> Result<Option<VmPostgresDriverCompletion>, postgres::PostgresError> {
        let request_id = request.request_id;
        match request.operation {
            VmPostgresDriverOperation::Connect(config) => {
                let handle = VmPostgresDriverPool(self.allocate_resource_id()?);
                self.pools.insert(
                    handle,
                    PoolEntry {
                        config: config.driver_config().clone(),
                        idle: Vec::new(),
                    },
                );
                Ok(Some(VmPostgresDriverCompletion::Connected(handle)))
            }
            VmPostgresDriverOperation::Acquire { driver_pool, .. } => {
                if let Some(connection) = self.take_idle(driver_pool)? {
                    let handle = VmPostgresDriverConnection(self.allocate_resource_id()?);
                    self.connections.insert(handle, connection);
                    self.connection_pools.insert(handle, driver_pool);
                    return Ok(Some(VmPostgresDriverCompletion::Acquired(handle)));
                }
                let connection = self.start_pool_connection(driver_pool)?;
                self.activate(PendingIo {
                    request_id,
                    connection: Some(connection),
                    return_connection: ReturnConnection::Acquire { pool: driver_pool },
                    task: PendingTask::Acquire,
                    phase: PendingPhase::Connecting { queued: None },
                })?;
                Ok(None)
            }
            VmPostgresDriverOperation::Query {
                driver_target,
                sql,
                parameters,
                one,
                ..
            } => {
                let (connection, return_connection, connecting) =
                    self.target_connection(driver_target)?;
                self.activate(PendingIo {
                    request_id,
                    connection: Some(connection),
                    return_connection,
                    task: PendingTask::Query { one },
                    phase: if connecting {
                        PendingPhase::Connecting {
                            queued: Some((sql, parameters)),
                        }
                    } else {
                        PendingPhase::Sending { sql, parameters }
                    },
                })?;
                Ok(None)
            }
            VmPostgresDriverOperation::Execute {
                driver_target,
                sql,
                parameters,
                ..
            } => {
                let (connection, return_connection, connecting) =
                    self.target_connection(driver_target)?;
                self.activate(PendingIo {
                    request_id,
                    connection: Some(connection),
                    return_connection,
                    task: PendingTask::Execute,
                    phase: if connecting {
                        PendingPhase::Connecting {
                            queued: Some((sql, parameters)),
                        }
                    } else {
                        PendingPhase::Sending { sql, parameters }
                    },
                })?;
                Ok(None)
            }
            VmPostgresDriverOperation::BatchExecute {
                driver_target, sql, ..
            } => {
                let (connection, return_connection, connecting) =
                    self.target_connection(driver_target)?;
                self.activate(PendingIo {
                    request_id,
                    connection: Some(connection),
                    return_connection,
                    task: PendingTask::BatchExecute,
                    phase: if connecting {
                        PendingPhase::Connecting {
                            queued: Some((sql, Vec::new())),
                        }
                    } else {
                        PendingPhase::Sending {
                            sql,
                            parameters: Vec::new(),
                        }
                    },
                })?;
                Ok(None)
            }
            VmPostgresDriverOperation::Begin {
                driver_connection, ..
            } => {
                let connection = self.take_connection(driver_connection)?;
                self.activate(sql_pending(
                    request_id,
                    connection,
                    ReturnConnection::Connection(driver_connection),
                    PendingTask::Begin,
                    "BEGIN".to_string(),
                ))?;
                Ok(None)
            }
            VmPostgresDriverOperation::Commit {
                driver_transaction, ..
            } => self.start_finish(request_id, driver_transaction, true),
            VmPostgresDriverOperation::Rollback {
                driver_transaction, ..
            } => self.start_finish(request_id, driver_transaction, false),
            VmPostgresDriverOperation::Prepare {
                driver_connection,
                sql,
                parameter_count,
                ..
            } => {
                let connection = self.take_connection(driver_connection)?;
                let name = format!("terlan_{}", self.next_resource_id);
                self.activate(sql_pending(
                    request_id,
                    connection,
                    ReturnConnection::Connection(driver_connection),
                    PendingTask::Prepare {
                        sql: sql.clone(),
                        parameter_count,
                    },
                    format!("PREPARE {name} AS {sql}"),
                ))?;
                Ok(None)
            }
            VmPostgresDriverOperation::Decode {
                driver_row,
                column,
                expected,
                ..
            } => self
                .decode(driver_row, &column, expected)
                .map(VmPostgresDriverCompletion::Decoded)
                .map(Some),
        }
    }

    fn start_finish(
        &mut self,
        request_id: RequestId,
        transaction: VmPostgresDriverTransaction,
        commit: bool,
    ) -> Result<Option<VmPostgresDriverCompletion>, postgres::PostgresError> {
        let connection_handle = *self
            .transactions
            .get(&transaction)
            .ok_or_else(|| stale("transaction"))?;
        let connection = self.take_connection(connection_handle)?;
        self.activate(sql_pending(
            request_id,
            connection,
            ReturnConnection::Transaction {
                transaction,
                connection: connection_handle,
                finish_on_success: true,
            },
            PendingTask::FinishTransaction,
            if commit { "COMMIT" } else { "ROLLBACK" }.to_string(),
        ))?;
        Ok(None)
    }

    fn advance(
        &mut self,
        pending: &mut PendingIo,
    ) -> Result<Option<VmPostgresDriverCompletion>, postgres::PostgresError> {
        match &mut pending.phase {
            PendingPhase::Connecting { queued } => match pending
                .connection
                .as_mut()
                .expect("pending request owns its connection")
                .poll_connect()?
            {
                ConnectPoll::Ready => {
                    if let Some((sql, parameters)) = queued.take() {
                        pending.phase = PendingPhase::Sending { sql, parameters };
                        self.set_wait(pending, VmPostgresIoInterest::Drive)?;
                    } else if matches!(pending.task, PendingTask::Acquire) {
                        return self.finish(pending, VmPostgresDriverCompletion::Unit);
                    } else {
                        return Err(postgres::PostgresError::new(
                            "postgres.driver.state",
                            "Postgres connection became ready without queued work.",
                        ));
                    }
                }
                ConnectPoll::Read => self.set_wait(pending, VmPostgresIoInterest::Read)?,
                ConnectPoll::Write => self.set_wait(pending, VmPostgresIoInterest::Write)?,
                ConnectPoll::Active => self.set_wait(pending, VmPostgresIoInterest::Drive)?,
            },
            PendingPhase::Sending { sql, parameters } => {
                let connection = pending
                    .connection
                    .as_mut()
                    .expect("pending request owns its connection");
                if matches!(pending.task, PendingTask::BatchExecute) {
                    connection.send_batch(sql)?;
                } else {
                    connection.send_query(sql, parameters)?;
                }
                pending.phase = PendingPhase::Reading;
                self.set_wait(pending, VmPostgresIoInterest::Read)?;
            }
            PendingPhase::Reading => {
                let connection = pending
                    .connection
                    .as_mut()
                    .expect("pending request owns its connection");
                connection.consume_input()?;
                if connection.is_busy()? {
                    self.set_wait(pending, VmPostgresIoInterest::Read)?;
                } else {
                    let result = connection.next_result()?.ok_or_else(|| {
                        postgres::PostgresError::new(
                            "postgres.query.empty_result",
                            "Postgres completed without a result.",
                        )
                    })?;
                    let completion = self.decode_result(connection, &pending.task, result)?;
                    pending.phase = PendingPhase::Draining(completion);
                    self.set_wait(pending, VmPostgresIoInterest::Drive)?;
                }
            }
            PendingPhase::Draining(_) => {
                if let Some(result) = pending
                    .connection
                    .as_mut()
                    .expect("pending request owns its connection")
                    .next_result()?
                {
                    if matches!(pending.task, PendingTask::BatchExecute) {
                        let status = result.status()?;
                        if status == COMMAND_OK {
                            return Ok(None);
                        }
                        return Err(pending
                            .connection
                            .as_mut()
                            .expect("pending request owns its connection")
                            .result_error());
                    }
                    return Err(postgres::PostgresError::new(
                        "postgres.query.multiple_results",
                        "Postgres requests must contain exactly one statement.",
                    ));
                }
                let PendingPhase::Draining(completion) =
                    std::mem::replace(&mut pending.phase, PendingPhase::Reading)
                else {
                    unreachable!("matched draining phase")
                };
                return self.finish(pending, completion);
            }
        }
        Ok(None)
    }

    fn finish(
        &mut self,
        pending: &mut PendingIo,
        mut completion: VmPostgresDriverCompletion,
    ) -> Result<Option<VmPostgresDriverCompletion>, postgres::PostgresError> {
        let connection = pending
            .connection
            .take()
            .expect("completed request owns its connection");
        match pending.return_connection {
            ReturnConnection::Acquire { pool } => {
                let handle = VmPostgresDriverConnection(self.allocate_resource_id()?);
                self.connections.insert(handle, connection);
                self.connection_pools.insert(handle, pool);
                completion = VmPostgresDriverCompletion::Acquired(handle);
            }
            ReturnConnection::Pool { pool } => self.pool_mut(pool)?.idle.push(connection),
            ReturnConnection::Connection(handle) => {
                self.connections.insert(handle, connection);
                if matches!(pending.task, PendingTask::Begin) {
                    let transaction = VmPostgresDriverTransaction(self.allocate_resource_id()?);
                    self.transactions.insert(transaction, handle);
                    completion = VmPostgresDriverCompletion::TransactionStarted(transaction);
                } else if let PendingTask::Prepare {
                    ref sql,
                    parameter_count,
                } = pending.task
                {
                    let statement = VmPostgresDriverPreparedStatement(self.allocate_resource_id()?);
                    self.prepared
                        .insert(statement, format!("{parameter_count}:{sql}"));
                    completion = VmPostgresDriverCompletion::Prepared(statement);
                }
            }
            ReturnConnection::Transaction {
                transaction,
                connection: handle,
                finish_on_success,
            } => {
                self.connections.insert(handle, connection);
                if finish_on_success {
                    self.transactions.remove(&transaction);
                }
            }
        }
        self.waits.remove(&pending.request_id.value);
        Ok(Some(completion))
    }

    fn decode_result(
        &mut self,
        connection: &mut DriverConnection,
        task: &PendingTask,
        result: DriverResult,
    ) -> Result<VmPostgresDriverCompletion, postgres::PostgresError> {
        let status = result.status()?;
        match task {
            PendingTask::Query { one } if status == TUPLES_OK => {
                let rows = result.rows()?;
                if *one && rows.len() > 1 {
                    return Err(postgres::PostgresError::new(
                        "postgres.query_one.cardinality",
                        "Postgres query_one returned more than one row.",
                    ));
                }
                self.store_rows(rows)
            }
            PendingTask::Execute if status == COMMAND_OK => result
                .affected_rows()
                .map(VmPostgresDriverCompletion::AffectedRows),
            PendingTask::BatchExecute if status == COMMAND_OK => {
                Ok(VmPostgresDriverCompletion::Unit)
            }
            PendingTask::Begin | PendingTask::FinishTransaction | PendingTask::Prepare { .. }
                if status == COMMAND_OK =>
            {
                Ok(VmPostgresDriverCompletion::Unit)
            }
            _ => Err(connection.result_error()),
        }
    }

    fn store_rows(
        &mut self,
        rows: Vec<postgres::Row>,
    ) -> Result<VmPostgresDriverCompletion, postgres::PostgresError> {
        let mut handles = Vec::with_capacity(rows.len());
        for row in rows {
            let handle = VmPostgresDriverRow(self.allocate_resource_id()?);
            self.rows.insert(handle, row);
            handles.push(handle);
        }
        Ok(VmPostgresDriverCompletion::Rows { rows: handles })
    }

    fn decode(
        &self,
        row: VmPostgresDriverRow,
        column: &str,
        expected: VmPostgresDecodeType,
    ) -> Result<VmPostgresDecodedValue, postgres::PostgresError> {
        let row = self.rows.get(&row).ok_or_else(|| stale("row"))?;
        match expected {
            VmPostgresDecodeType::Dynamic => {
                postgres::value(row, column).and_then(|value| match value {
                    postgres::DecodedValue::Null => Ok(VmPostgresDecodedValue::Null),
                    postgres::DecodedValue::String(value) => {
                        Ok(VmPostgresDecodedValue::String(value))
                    }
                    postgres::DecodedValue::Int(value) => Ok(VmPostgresDecodedValue::Int(value)),
                    postgres::DecodedValue::Bool(value) => Ok(VmPostgresDecodedValue::Bool(value)),
                    postgres::DecodedValue::Json(value) => serde_json::to_string(value.as_serde())
                        .map(VmPostgresDecodedValue::Json)
                        .map_err(|error| {
                            postgres::PostgresError::new(
                                "postgres.decode.json",
                                format!("Could not serialize decoded Postgres JSON: {error}."),
                            )
                        }),
                })
            }
            VmPostgresDecodeType::String => {
                postgres::string(row, column).map(VmPostgresDecodedValue::String)
            }
            VmPostgresDecodeType::Int => {
                postgres::int(row, column).map(VmPostgresDecodedValue::Int)
            }
            VmPostgresDecodeType::Bool => {
                postgres::r#bool(row, column).map(VmPostgresDecodedValue::Bool)
            }
            VmPostgresDecodeType::Json => postgres::json(row, column).and_then(|value| {
                serde_json::to_string(value.as_serde())
                    .map(VmPostgresDecodedValue::Json)
                    .map_err(|error| {
                        postgres::PostgresError::new(
                            "postgres.decode.json",
                            format!("Could not serialize decoded Postgres JSON: {error}."),
                        )
                    })
            }),
        }
    }

    fn target_connection(
        &mut self,
        target: VmPostgresDriverQueryTarget,
    ) -> Result<(DriverConnection, ReturnConnection, bool), postgres::PostgresError> {
        match target {
            VmPostgresDriverQueryTarget::Pool(pool) => {
                if let Some(connection) = self.take_idle(pool)? {
                    Ok((connection, ReturnConnection::Pool { pool }, false))
                } else {
                    Ok((
                        self.start_pool_connection(pool)?,
                        ReturnConnection::Pool { pool },
                        true,
                    ))
                }
            }
            VmPostgresDriverQueryTarget::Transaction(transaction) => {
                let handle = *self
                    .transactions
                    .get(&transaction)
                    .ok_or_else(|| stale("transaction"))?;
                let connection = self.take_connection(handle)?;
                Ok((
                    connection,
                    ReturnConnection::Transaction {
                        transaction,
                        connection: handle,
                        finish_on_success: false,
                    },
                    false,
                ))
            }
        }
    }

    fn take_idle(
        &mut self,
        pool: VmPostgresDriverPool,
    ) -> Result<Option<DriverConnection>, postgres::PostgresError> {
        Ok(self.pool_mut(pool)?.idle.pop())
    }

    fn start_pool_connection(
        &mut self,
        pool: VmPostgresDriverPool,
    ) -> Result<DriverConnection, postgres::PostgresError> {
        let url = self.pool_mut(pool)?.config.url().to_string();
        DriverConnection::start(&url)
    }

    fn take_connection(
        &mut self,
        handle: VmPostgresDriverConnection,
    ) -> Result<DriverConnection, postgres::PostgresError> {
        self.connections
            .remove(&handle)
            .ok_or_else(|| stale("connection"))
    }

    fn release_connection(
        &mut self,
        handle: VmPostgresDriverConnection,
    ) -> Result<(), VmPostgresFailure> {
        if self
            .transactions
            .values()
            .any(|connection| *connection == handle)
        {
            return Err(VmPostgresFailure::new(
                "postgres.transaction.active",
                "Postgres connection still owns an active transaction.",
            ));
        }
        let connection = self
            .connections
            .remove(&handle)
            .ok_or_else(|| stale_failure("connection"))?;
        let pool = self
            .connection_pools
            .remove(&handle)
            .ok_or_else(|| stale_failure("pool"))?;
        self.pools
            .get_mut(&pool)
            .ok_or_else(|| stale_failure("pool"))?
            .idle
            .push(connection);
        Ok(())
    }

    fn restore_after_failure(&mut self, pending: PendingIo) {
        match pending.return_connection {
            ReturnConnection::Connection(handle)
            | ReturnConnection::Transaction {
                connection: handle, ..
            } => {
                if let Some(connection) = pending.connection {
                    self.connections.insert(handle, connection);
                }
            }
            ReturnConnection::Acquire { .. } | ReturnConnection::Pool { .. } => {}
        }
        self.waits.remove(&pending.request_id.value);
    }

    fn set_wait(
        &mut self,
        pending: &PendingIo,
        interest: VmPostgresIoInterest,
    ) -> Result<(), postgres::PostgresError> {
        self.waits.insert(
            pending.request_id.value,
            VmPostgresDriverWait {
                request_id: pending.request_id,
                socket: pending
                    .connection
                    .as_ref()
                    .expect("pending request owns its connection")
                    .socket()?,
                interest,
            },
        );
        Ok(())
    }

    fn activate(&mut self, pending: PendingIo) -> Result<(), postgres::PostgresError> {
        let request_id = pending.request_id.value;
        if self.active.contains_key(&request_id) {
            return Err(postgres::PostgresError::new(
                "postgres.driver.duplicate_request",
                format!("Postgres request id {request_id} is already active."),
            ));
        }
        self.active.insert(request_id, pending);
        Ok(())
    }

    fn request_conflicts(&self, request: &VmPostgresDriverRequest) -> bool {
        self.active
            .values()
            .any(|pending| pending.conflicts_with(&request.operation))
    }

    fn pool_mut(
        &mut self,
        handle: VmPostgresDriverPool,
    ) -> Result<&mut PoolEntry, postgres::PostgresError> {
        self.pools.get_mut(&handle).ok_or_else(|| stale("pool"))
    }

    fn allocate_resource_id(&mut self) -> Result<u64, postgres::PostgresError> {
        let id = self.next_resource_id.max(1);
        self.next_resource_id = id.checked_add(1).ok_or_else(|| {
            postgres::PostgresError::new(
                "postgres.driver.resource_overflow",
                "Postgres driver resource id overflow.",
            )
        })?;
        Ok(id)
    }
}

fn sql_pending(
    request_id: RequestId,
    connection: DriverConnection,
    return_connection: ReturnConnection,
    task: PendingTask,
    sql: String,
) -> PendingIo {
    PendingIo {
        request_id,
        connection: Some(connection),
        return_connection,
        task,
        phase: PendingPhase::Sending {
            sql,
            parameters: Vec::new(),
        },
    }
}

fn stale(kind: &str) -> postgres::PostgresError {
    postgres::PostgresError::new(
        "postgres.driver.stale_resource",
        format!("Postgres driver {kind} resource is not live."),
    )
}

fn stale_failure(kind: &str) -> VmPostgresFailure {
    VmPostgresFailure::new(
        "postgres.driver.stale_resource",
        format!("Postgres driver {kind} resource is not live."),
    )
}

fn cancelled() -> VmPostgresDriverCompletion {
    VmPostgresDriverCompletion::Failed(VmPostgresFailure::new(
        "postgres.cancelled",
        "Postgres request was cancelled.",
    ))
}

fn failure(error: postgres::PostgresError) -> VmPostgresDriverCompletion {
    VmPostgresDriverCompletion::Failed(adapter_failure(error))
}

fn adapter_failure(error: postgres::PostgresError) -> VmPostgresFailure {
    VmPostgresFailure::new(error.code(), error.message())
}

#[path = "libpq_worker/readiness.rs"]
mod readiness;

#[cfg(test)]
#[path = "libpq_worker_test_support_test.rs"]
mod test_support;

#[cfg(test)]
#[path = "libpq_worker_test.rs"]
mod libpq_worker_test;

#[cfg(test)]
#[path = "libpq_docker_gate_test.rs"]
pub(crate) mod libpq_docker_gate_test;
