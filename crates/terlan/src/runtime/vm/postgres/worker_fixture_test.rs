use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::terlan_native::{json, postgres};

use super::{
    VmPostgresDecodeType, VmPostgresDecodedValue, VmPostgresDriverCompletion,
    VmPostgresDriverConnection, VmPostgresDriverControl, VmPostgresDriverOperation,
    VmPostgresDriverPool, VmPostgresDriverPreparedStatement, VmPostgresDriverQueryTarget,
    VmPostgresDriverRequest, VmPostgresDriverRow, VmPostgresDriverTransaction, VmPostgresFailure,
};

pub(crate) trait VmPostgresDriverBackend {
    type Pool;
    type Connection;
    type PreparedStatement;
    type Row;

    fn connect(&self, config: &postgres::Config) -> Result<Self::Pool, postgres::PostgresError>;
    fn acquire(&self, pool: &Self::Pool) -> Result<Self::Connection, postgres::PostgresError>;
    fn query_pool(
        &self,
        pool: &Self::Pool,
        sql: &str,
        parameters: &[json::Json],
        one: bool,
    ) -> Result<Vec<Self::Row>, postgres::PostgresError>;
    fn query_connection(
        &self,
        connection: &Self::Connection,
        sql: &str,
        parameters: &[json::Json],
        one: bool,
    ) -> Result<Vec<Self::Row>, postgres::PostgresError>;
    fn execute_pool(
        &self,
        pool: &Self::Pool,
        sql: &str,
        parameters: &[json::Json],
    ) -> Result<i64, postgres::PostgresError>;
    fn execute_connection(
        &self,
        connection: &Self::Connection,
        sql: &str,
        parameters: &[json::Json],
    ) -> Result<i64, postgres::PostgresError>;
    fn begin(&self, connection: &mut Self::Connection) -> Result<(), postgres::PostgresError>;
    fn commit(&self, connection: &mut Self::Connection) -> Result<(), postgres::PostgresError>;
    fn rollback(&self, connection: &mut Self::Connection) -> Result<(), postgres::PostgresError>;
    fn prepare(
        &self,
        connection: &Self::Connection,
        sql: &str,
    ) -> Result<Self::PreparedStatement, postgres::PostgresError>;
    fn decode(
        &self,
        row: &Self::Row,
        column: &str,
        expected: VmPostgresDecodeType,
    ) -> Result<VmPostgresDecodedValue, postgres::PostgresError>;
}

pub(crate) struct VmPostgresDriverWorker<B>
where
    B: VmPostgresDriverBackend,
{
    backend: B,
    next_resource_id: u64,
    pools: BTreeMap<VmPostgresDriverPool, B::Pool>,
    connections: BTreeMap<VmPostgresDriverConnection, B::Connection>,
    connection_pools: BTreeMap<VmPostgresDriverConnection, VmPostgresDriverPool>,
    transactions: BTreeMap<VmPostgresDriverTransaction, VmPostgresDriverConnection>,
    prepared: BTreeMap<VmPostgresDriverPreparedStatement, B::PreparedStatement>,
    rows: BTreeMap<VmPostgresDriverRow, B::Row>,
    cancelled: BTreeSet<u64>,
}

impl<B> fmt::Debug for VmPostgresDriverWorker<B>
where
    B: VmPostgresDriverBackend,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VmPostgresDriverWorker")
            .field("pools", &self.pools.len())
            .field("connections", &self.connections.len())
            .field("transactions", &self.transactions.len())
            .field("prepared", &self.prepared.len())
            .field("rows", &self.rows.len())
            .field("cancelled", &self.cancelled.len())
            .finish()
    }
}

impl<B> VmPostgresDriverWorker<B>
where
    B: VmPostgresDriverBackend,
{
    pub(crate) fn new(backend: B) -> Self {
        Self {
            backend,
            next_resource_id: 1,
            pools: BTreeMap::new(),
            connections: BTreeMap::new(),
            connection_pools: BTreeMap::new(),
            transactions: BTreeMap::new(),
            prepared: BTreeMap::new(),
            rows: BTreeMap::new(),
            cancelled: BTreeSet::new(),
        }
    }

    pub(crate) fn execute(
        &mut self,
        request: VmPostgresDriverRequest,
    ) -> VmPostgresDriverCompletion {
        if self.cancelled.remove(&request.request_id.value) {
            return failure("postgres.cancelled", "Postgres request was cancelled.");
        }
        self.execute_operation(request.operation)
            .unwrap_or_else(|error| failure(error.code(), error.message()))
    }

    pub(crate) fn apply_control(
        &mut self,
        control: VmPostgresDriverControl,
    ) -> Result<(), VmPostgresFailure> {
        match control {
            VmPostgresDriverControl::Cancel(request_id) => {
                self.cancelled.insert(request_id.value);
                Ok(())
            }
            VmPostgresDriverControl::Rollback {
                driver_transaction, ..
            } => self.cleanup_transaction(driver_transaction),
            VmPostgresDriverControl::Release {
                driver_connection, ..
            } => {
                if self
                    .transactions
                    .values()
                    .any(|connection| *connection == driver_connection)
                {
                    return Err(VmPostgresFailure::new(
                        "postgres.transaction.active",
                        "Postgres driver connection still owns an active transaction.",
                    ));
                }
                self.connections.remove(&driver_connection).ok_or_else(|| {
                    VmPostgresFailure::new(
                        "postgres.connection.stale",
                        "Postgres driver connection is not live.",
                    )
                })?;
                self.connection_pools.remove(&driver_connection);
                Ok(())
            }
            VmPostgresDriverControl::ClosePool { driver_pool, .. } => {
                if self
                    .connection_pools
                    .values()
                    .any(|pool| *pool == driver_pool)
                {
                    return Err(VmPostgresFailure::new(
                        "postgres.pool.active_connections",
                        "Postgres driver pool still owns active connections.",
                    ));
                }
                self.pools.remove(&driver_pool).ok_or_else(|| {
                    VmPostgresFailure::new(
                        "postgres.pool.stale",
                        "Postgres driver pool is not live.",
                    )
                })?;
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

    fn execute_operation(
        &mut self,
        operation: VmPostgresDriverOperation,
    ) -> Result<VmPostgresDriverCompletion, postgres::PostgresError> {
        match operation {
            VmPostgresDriverOperation::Connect(config) => {
                let pool = self.backend.connect(config.driver_config())?;
                let handle = VmPostgresDriverPool(self.allocate_resource_id()?);
                self.pools.insert(handle, pool);
                Ok(VmPostgresDriverCompletion::Connected(handle))
            }
            VmPostgresDriverOperation::Acquire { driver_pool, .. } => {
                let pool = self.pool(driver_pool)?;
                let connection = self.backend.acquire(pool)?;
                let handle = VmPostgresDriverConnection(self.allocate_resource_id()?);
                self.connections.insert(handle, connection);
                self.connection_pools.insert(handle, driver_pool);
                Ok(VmPostgresDriverCompletion::Acquired(handle))
            }
            VmPostgresDriverOperation::Query {
                driver_target,
                sql,
                parameters,
                one,
                ..
            } => {
                let rows = match driver_target {
                    VmPostgresDriverQueryTarget::Pool(pool) => {
                        self.backend
                            .query_pool(self.pool(pool)?, &sql, &parameters, one)?
                    }
                    VmPostgresDriverQueryTarget::Transaction(transaction) => {
                        let connection = self.transaction_connection(transaction)?;
                        self.backend
                            .query_connection(connection, &sql, &parameters, one)?
                    }
                };
                self.store_rows(rows)
            }
            VmPostgresDriverOperation::Execute {
                driver_target,
                sql,
                parameters,
                ..
            } => {
                let affected = match driver_target {
                    VmPostgresDriverQueryTarget::Pool(pool) => {
                        self.backend
                            .execute_pool(self.pool(pool)?, &sql, &parameters)?
                    }
                    VmPostgresDriverQueryTarget::Transaction(transaction) => {
                        let connection = self.transaction_connection(transaction)?;
                        self.backend
                            .execute_connection(connection, &sql, &parameters)?
                    }
                };
                Ok(VmPostgresDriverCompletion::AffectedRows(affected))
            }
            VmPostgresDriverOperation::BatchExecute {
                driver_target, sql, ..
            } => {
                match driver_target {
                    VmPostgresDriverQueryTarget::Pool(pool) => {
                        self.backend.execute_pool(self.pool(pool)?, &sql, &[])?;
                    }
                    VmPostgresDriverQueryTarget::Transaction(transaction) => {
                        let connection = self.transaction_connection(transaction)?;
                        self.backend.execute_connection(connection, &sql, &[])?;
                    }
                }
                Ok(VmPostgresDriverCompletion::Unit)
            }
            VmPostgresDriverOperation::Begin {
                driver_connection, ..
            } => {
                let backend = &self.backend;
                let connection = self
                    .connections
                    .get_mut(&driver_connection)
                    .ok_or_else(|| stale("connection"))?;
                backend.begin(connection)?;
                let transaction = VmPostgresDriverTransaction(self.allocate_resource_id()?);
                self.transactions.insert(transaction, driver_connection);
                Ok(VmPostgresDriverCompletion::TransactionStarted(transaction))
            }
            VmPostgresDriverOperation::Commit {
                driver_transaction, ..
            } => {
                self.finish_transaction(driver_transaction, true)
                    .map_err(failure_to_adapter)?;
                Ok(VmPostgresDriverCompletion::Unit)
            }
            VmPostgresDriverOperation::Rollback {
                driver_transaction, ..
            } => {
                self.finish_transaction(driver_transaction, false)
                    .map_err(failure_to_adapter)?;
                Ok(VmPostgresDriverCompletion::Unit)
            }
            VmPostgresDriverOperation::Prepare {
                driver_connection,
                sql,
                parameter_count,
                ..
            } => {
                reject_unbound_parameters(parameter_count)?;
                let statement = self
                    .backend
                    .prepare(self.connection(driver_connection)?, &sql)?;
                let handle = VmPostgresDriverPreparedStatement(self.allocate_resource_id()?);
                self.prepared.insert(handle, statement);
                Ok(VmPostgresDriverCompletion::Prepared(handle))
            }
            VmPostgresDriverOperation::Decode {
                driver_row,
                column,
                expected,
                ..
            } => self
                .backend
                .decode(self.row(driver_row)?, &column, expected)
                .map(VmPostgresDriverCompletion::Decoded),
        }
    }

    fn store_rows(
        &mut self,
        rows: Vec<B::Row>,
    ) -> Result<VmPostgresDriverCompletion, postgres::PostgresError> {
        let mut handles = Vec::with_capacity(rows.len());
        for row in rows {
            let handle = VmPostgresDriverRow(self.allocate_resource_id()?);
            self.rows.insert(handle, row);
            handles.push(handle);
        }
        Ok(VmPostgresDriverCompletion::Rows { rows: handles })
    }

    fn finish_transaction(
        &mut self,
        transaction: VmPostgresDriverTransaction,
        commit: bool,
    ) -> Result<(), VmPostgresFailure> {
        let connection = *self.transactions.get(&transaction).ok_or_else(|| {
            VmPostgresFailure::new(
                "postgres.transaction.stale",
                "Postgres driver transaction is not live.",
            )
        })?;
        let session = self.connections.get_mut(&connection).ok_or_else(|| {
            VmPostgresFailure::new(
                "postgres.connection.stale",
                "Postgres driver connection is not live.",
            )
        })?;
        let result = if commit {
            self.backend.commit(session)
        } else {
            self.backend.rollback(session)
        };
        result.map_err(adapter_failure)?;
        self.transactions.remove(&transaction);
        Ok(())
    }

    fn cleanup_transaction(
        &mut self,
        transaction: VmPostgresDriverTransaction,
    ) -> Result<(), VmPostgresFailure> {
        let connection = self.transactions.remove(&transaction).ok_or_else(|| {
            VmPostgresFailure::new(
                "postgres.transaction.stale",
                "Postgres driver transaction is not live.",
            )
        })?;
        let mut session = self.connections.remove(&connection).ok_or_else(|| {
            VmPostgresFailure::new(
                "postgres.connection.stale",
                "Postgres driver connection is not live.",
            )
        })?;
        self.connection_pools.remove(&connection);
        self.backend.rollback(&mut session).map_err(adapter_failure)
    }

    fn allocate_resource_id(&mut self) -> Result<u64, postgres::PostgresError> {
        let id = self.next_resource_id;
        self.next_resource_id = id.checked_add(1).ok_or_else(|| {
            postgres::PostgresError::new(
                "postgres.driver.resource_overflow",
                "Postgres driver resource id overflow.",
            )
        })?;
        Ok(id)
    }

    fn pool(&self, handle: VmPostgresDriverPool) -> Result<&B::Pool, postgres::PostgresError> {
        self.pools.get(&handle).ok_or_else(|| stale("pool"))
    }

    fn connection(
        &self,
        handle: VmPostgresDriverConnection,
    ) -> Result<&B::Connection, postgres::PostgresError> {
        self.connections
            .get(&handle)
            .ok_or_else(|| stale("connection"))
    }

    fn transaction_connection(
        &self,
        handle: VmPostgresDriverTransaction,
    ) -> Result<&B::Connection, postgres::PostgresError> {
        let connection = self
            .transactions
            .get(&handle)
            .ok_or_else(|| stale("transaction"))?;
        self.connection(*connection)
    }

    fn row(&self, handle: VmPostgresDriverRow) -> Result<&B::Row, postgres::PostgresError> {
        self.rows.get(&handle).ok_or_else(|| stale("row"))
    }
}

fn reject_unbound_parameters(parameter_count: usize) -> Result<(), postgres::PostgresError> {
    if parameter_count == 0 {
        Ok(())
    } else {
        Err(postgres::PostgresError::new(
            "postgres.parameters.unbound",
            "Postgres parameter values were not attached to the driver request.",
        ))
    }
}

fn stale(kind: &str) -> postgres::PostgresError {
    postgres::PostgresError::new(
        "postgres.driver.stale_resource",
        format!("Postgres driver {kind} resource is not live."),
    )
}

fn adapter_failure(error: postgres::PostgresError) -> VmPostgresFailure {
    VmPostgresFailure::new(error.code(), error.message())
}

fn failure(code: impl Into<String>, message: impl Into<String>) -> VmPostgresDriverCompletion {
    VmPostgresDriverCompletion::Failed(VmPostgresFailure::new(code, message))
}

fn failure_to_adapter(error: VmPostgresFailure) -> postgres::PostgresError {
    postgres::PostgresError::new(
        "postgres.driver.transaction",
        format!("{}: {}", error.code, error.message),
    )
}

#[cfg(test)]
#[path = "worker_test.rs"]
mod worker_test;
