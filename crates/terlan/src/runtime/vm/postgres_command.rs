//! Synchronous command facade over the VM-owned Postgres actor runtime.
//!
//! CLI commands are synchronous at their outer boundary. This facade pumps the
//! same nonblocking libpq worker used by VM actors, preserving typed resource
//! ownership, cancellation, redaction, and cleanup without creating a second
//! database runtime.

use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use super::{
    actor::VmActorRuntime,
    postgres::{
        VmPostgresConnectConfig, VmPostgresDecodeType, VmPostgresDecodedValue, VmPostgresPool,
        VmPostgresQueryTarget, VmPostgresReply, VmPostgresRow, VmPostgresTransaction,
    },
    process::{VmExitReason, VmProcessId, VmProcessSource},
};
use crate::{
    terlan_native::{json, postgres, postgres::libpq::DriverReadinessPoller},
    terlan_native_boundary::request::RequestId,
};

/// Blocking command adapter backed by the VM actor and Postgres worker.
#[derive(Debug)]
pub(crate) struct VmPostgresCommandClient {
    runtime: VmActorRuntime,
    owner: VmProcessId,
    pool: Option<VmPostgresPool>,
    operation_timeout: Duration,
    readiness: DriverReadinessPoller,
}

impl VmPostgresCommandClient {
    /// Creates a VM-owned pool for one synchronous command invocation.
    pub(crate) fn connect(config: &postgres::Config) -> Result<Self, String> {
        let timeout_ms = config
            .wait_timeout_ms()
            .max(config.connect_timeout_ms())
            .max(1);
        let operation_timeout = Duration::from_millis(timeout_ms);
        let timeout_ticks = timeout_ms;
        let mut runtime = VmActorRuntime::default();
        let owner = runtime.spawn_root(VmProcessSource::new("terlc.db", "command", 0));
        let vm_config = VmPostgresConnectConfig::new(config.clone())
            .map_err(|error| format_postgres_error(error.code(), error.message()))?;
        let request = runtime.postgres_connect(owner, vm_config, 0, timeout_ticks)?;
        let mut client = Self {
            runtime,
            owner,
            pool: None,
            operation_timeout,
            readiness: DriverReadinessPoller::new()
                .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))?,
        };
        client.pool = Some(match client.await_reply(request)? {
            VmPostgresReply::Pool(pool) => pool,
            reply => return Err(unexpected_reply("connect", &reply)),
        });
        Ok(client)
    }

    /// Executes trusted multi-statement SQL through the VM-owned pool.
    pub(crate) fn batch_execute(&mut self, sql: &str) -> Result<(), String> {
        self.batch_execute_target(VmPostgresQueryTarget::Pool(self.pool()), sql)
    }

    /// Executes one parameterized statement through the VM-owned pool.
    pub(crate) fn execute(
        &mut self,
        sql: &str,
        parameters: Vec<json::Json>,
    ) -> Result<i64, String> {
        self.execute_target(VmPostgresQueryTarget::Pool(self.pool()), sql, parameters)
    }

    /// Queries rows through the VM-owned pool.
    pub(crate) fn query(
        &mut self,
        sql: &str,
        parameters: Vec<json::Json>,
    ) -> Result<Vec<VmPostgresRow>, String> {
        self.query_target(
            VmPostgresQueryTarget::Pool(self.pool()),
            sql,
            parameters,
            false,
        )
    }

    /// Queries at most one row through the VM-owned pool.
    pub(crate) fn query_one(
        &mut self,
        sql: &str,
        parameters: Vec<json::Json>,
    ) -> Result<Option<VmPostgresRow>, String> {
        let mut rows = self.query_target(
            VmPostgresQueryTarget::Pool(self.pool()),
            sql,
            parameters,
            true,
        )?;
        Ok(rows.pop())
    }

    /// Queries at most one row through one typed VM transaction.
    pub(crate) fn query_one_transaction(
        &mut self,
        transaction: VmPostgresTransaction,
        sql: &str,
        parameters: Vec<json::Json>,
    ) -> Result<Option<VmPostgresRow>, String> {
        let mut rows = self.query_target(
            VmPostgresQueryTarget::Transaction(transaction),
            sql,
            parameters,
            true,
        )?;
        Ok(rows.pop())
    }

    /// Queries rows through one typed VM transaction.
    pub(crate) fn query_transaction(
        &mut self,
        transaction: VmPostgresTransaction,
        sql: &str,
        parameters: Vec<json::Json>,
    ) -> Result<Vec<VmPostgresRow>, String> {
        self.query_target(
            VmPostgresQueryTarget::Transaction(transaction),
            sql,
            parameters,
            false,
        )
    }

    fn query_target(
        &mut self,
        target: VmPostgresQueryTarget,
        sql: &str,
        parameters: Vec<json::Json>,
        one: bool,
    ) -> Result<Vec<VmPostgresRow>, String> {
        let request = self.runtime.postgres_query(
            self.owner,
            target,
            sql,
            parameters,
            one,
            0,
            self.timeout_ticks(),
        )?;
        match self.await_reply(request)? {
            VmPostgresReply::Rows { rows, .. } => Ok(rows),
            reply => Err(unexpected_reply("query", &reply)),
        }
    }

    /// Decodes one column using the concrete libpq type retained by the worker.
    pub(crate) fn decode_dynamic(
        &mut self,
        row: VmPostgresRow,
        column: &str,
    ) -> Result<VmPostgresDecodedValue, String> {
        let request = self.runtime.postgres_decode(
            self.owner,
            row,
            column,
            VmPostgresDecodeType::Dynamic,
            0,
            self.timeout_ticks(),
        )?;
        match self.await_reply(request)? {
            VmPostgresReply::Decoded(value) => Ok(value),
            reply => Err(unexpected_reply("decode", &reply)),
        }
    }

    /// Decodes one text column through the maintained driver worker.
    pub(crate) fn decode_string(
        &mut self,
        row: VmPostgresRow,
        column: &str,
    ) -> Result<String, String> {
        let request = self.runtime.postgres_decode(
            self.owner,
            row,
            column,
            VmPostgresDecodeType::String,
            0,
            self.timeout_ticks(),
        )?;
        match self.await_reply(request)? {
            VmPostgresReply::Decoded(VmPostgresDecodedValue::String(value)) => Ok(value),
            reply => Err(unexpected_reply("decode", &reply)),
        }
    }

    /// Acquires one connection and starts a typed VM transaction.
    pub(crate) fn begin(&mut self) -> Result<VmPostgresTransaction, String> {
        let acquire =
            self.runtime
                .postgres_acquire(self.owner, self.pool(), 0, self.timeout_ticks())?;
        let connection = match self.await_reply(acquire)? {
            VmPostgresReply::Connection(connection) => connection,
            reply => return Err(unexpected_reply("acquire", &reply)),
        };
        let begin = self
            .runtime
            .postgres_begin(self.owner, connection, 0, self.timeout_ticks())?;
        match self.await_reply(begin)? {
            VmPostgresReply::Transaction(transaction) => Ok(transaction),
            reply => Err(unexpected_reply("begin", &reply)),
        }
    }

    /// Executes SQL through one typed VM transaction.
    pub(crate) fn execute_transaction(
        &mut self,
        transaction: VmPostgresTransaction,
        sql: &str,
        parameters: Vec<json::Json>,
    ) -> Result<i64, String> {
        self.execute_target(
            VmPostgresQueryTarget::Transaction(transaction),
            sql,
            parameters,
        )
    }

    /// Executes a trusted multi-statement SQL batch in one typed transaction.
    pub(crate) fn batch_execute_transaction(
        &mut self,
        transaction: VmPostgresTransaction,
        sql: &str,
    ) -> Result<(), String> {
        self.batch_execute_target(VmPostgresQueryTarget::Transaction(transaction), sql)
    }

    /// Commits or rolls back one typed VM transaction.
    pub(crate) fn finish_transaction(
        &mut self,
        transaction: VmPostgresTransaction,
        commit: bool,
    ) -> Result<(), String> {
        let request = self.runtime.postgres_finish_transaction(
            self.owner,
            transaction,
            commit,
            0,
            self.timeout_ticks(),
        )?;
        match self.await_reply(request)? {
            VmPostgresReply::Unit => Ok(()),
            reply => Err(unexpected_reply(
                if commit { "commit" } else { "rollback" },
                &reply,
            )),
        }
    }

    fn execute_target(
        &mut self,
        target: VmPostgresQueryTarget,
        sql: &str,
        parameters: Vec<json::Json>,
    ) -> Result<i64, String> {
        let request = self.runtime.postgres_execute(
            self.owner,
            target,
            sql,
            parameters,
            0,
            self.timeout_ticks(),
        )?;
        match self.await_reply(request)? {
            VmPostgresReply::AffectedRows(count) => Ok(count),
            reply => Err(unexpected_reply("execute", &reply)),
        }
    }

    fn batch_execute_target(
        &mut self,
        target: VmPostgresQueryTarget,
        sql: &str,
    ) -> Result<(), String> {
        let request = self.runtime.postgres_batch_execute(
            self.owner,
            target,
            sql,
            0,
            self.timeout_ticks(),
        )?;
        match self.await_reply(request)? {
            VmPostgresReply::Unit => Ok(()),
            reply => Err(unexpected_reply("batch_execute", &reply)),
        }
    }

    fn await_reply(&mut self, request: RequestId) -> Result<VmPostgresReply, String> {
        let deadline = Instant::now() + self.operation_timeout;
        let mut ready = BTreeSet::new();
        loop {
            self.runtime.drive_postgres_controls()?;
            if self.runtime.drive_postgres_socket_ready(&ready)? == Some(request) {
                return reply_result(self.runtime.take_postgres_reply(self.owner, request)?);
            }
            ready.clear();
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.runtime.cancel_postgres(request)?;
                self.runtime.drive_postgres_controls()?;
                return Err(
                    "error[postgres.timeout]: Postgres command operation timed out.".to_string(),
                );
            }
            ready = self
                .runtime
                .wait_postgres_ready(&self.readiness, Some(remaining))?;
        }
    }

    fn timeout_ticks(&self) -> u64 {
        u64::try_from(self.operation_timeout.as_millis()).unwrap_or(u64::MAX)
    }

    fn pool(&self) -> VmPostgresPool {
        self.pool
            .expect("connected VM Postgres command client owns a pool")
    }
}

impl Drop for VmPostgresCommandClient {
    fn drop(&mut self) {
        let _ = self.runtime.exit_actor(self.owner, VmExitReason::Normal);
        let _ = self.runtime.drive_postgres_controls();
    }
}

fn reply_result(reply: VmPostgresReply) -> Result<VmPostgresReply, String> {
    match reply {
        VmPostgresReply::Error(error) => Err(format_postgres_error(&error.code, &error.message)),
        reply => Ok(reply),
    }
}

fn format_postgres_error(code: &str, message: &str) -> String {
    format!("error[{code}]: {message}")
}

fn unexpected_reply(operation: &str, reply: &VmPostgresReply) -> String {
    format!(
        "error[postgres.driver.protocol]: unexpected VM Postgres reply for {operation}: {}",
        reply_name(reply)
    )
}

fn reply_name(reply: &VmPostgresReply) -> &'static str {
    match reply {
        VmPostgresReply::Pool(_) => "pool",
        VmPostgresReply::Connection(_) => "connection",
        VmPostgresReply::Transaction(_) => "transaction",
        VmPostgresReply::PreparedStatement(_) => "prepared_statement",
        VmPostgresReply::Rows { .. } => "rows",
        VmPostgresReply::AffectedRows(_) => "affected_rows",
        VmPostgresReply::Decoded(_) => "decoded",
        VmPostgresReply::Unit => "unit",
        VmPostgresReply::Error(_) => "error",
    }
}

#[cfg(test)]
#[path = "postgres_command_test.rs"]
mod postgres_command_test;
