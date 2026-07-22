use std::fmt;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    runtime::vm::process::VmProcessId,
    terlan_native::{json, postgres},
    terlan_native_boundary::request::RequestId,
};

macro_rules! database_handle {
    ($name:ident) => {
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub(crate) struct $name(pub(super) u64);

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "(<opaque>)"))
            }
        }
    };
}

database_handle!(VmPostgresPool);
database_handle!(VmPostgresConnection);
database_handle!(VmPostgresTransaction);
database_handle!(VmPostgresPreparedStatement);
database_handle!(VmPostgresResultSet);
database_handle!(VmPostgresRow);

database_handle!(VmPostgresDriverPool);
database_handle!(VmPostgresDriverConnection);
database_handle!(VmPostgresDriverTransaction);
database_handle!(VmPostgresDriverPreparedStatement);
database_handle!(VmPostgresDriverRow);

#[cfg(test)]
impl VmPostgresTransaction {
    #[allow(dead_code)]
    pub(crate) const fn fixture(id: u64) -> Self {
        Self(id)
    }
}

/// Redacted connection configuration carried only to the maintained driver worker.
#[derive(Clone)]
pub(crate) struct VmPostgresConnectConfig {
    config: postgres::Config,
}

impl VmPostgresConnectConfig {
    #[allow(dead_code)]
    pub(crate) fn new(config: postgres::Config) -> Result<Self, postgres::PostgresError> {
        postgres::validate_config(&config)?;
        Ok(Self { config })
    }

    pub(super) fn max_connections(&self) -> usize {
        self.config.max_connections()
    }

    pub(super) fn driver_config(&self) -> &postgres::Config {
        &self.config
    }
}

impl fmt::Debug for VmPostgresConnectConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VmPostgresConnectConfig")
            .field("url", &"<redacted>")
            .field("min_connections", &self.config.min_connections())
            .field("max_connections", &self.config.max_connections())
            .field("wait_timeout_ms", &self.config.wait_timeout_ms())
            .field("connect_timeout_ms", &self.config.connect_timeout_ms())
            .finish()
    }
}

/// Resource accepted by a query or execute request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum VmPostgresQueryTarget {
    Pool(VmPostgresPool),
    Transaction(VmPostgresTransaction),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmPostgresDriverQueryTarget {
    Pool(VmPostgresDriverPool),
    Transaction(VmPostgresDriverTransaction),
}

/// Expected row-decoding type. The maintained driver owns concrete conversion.
#[allow(
    dead_code,
    reason = "the benchmark binary shares protocol types without source Postgres dispatch"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VmPostgresDecodeType {
    Dynamic,
    String,
    Int,
    Bool,
    Json,
}

/// VM-to-driver work. Debug output intentionally excludes credentials, SQL, and values.
#[derive(Clone)]
pub(crate) enum VmPostgresDriverOperation {
    Connect(VmPostgresConnectConfig),
    Acquire {
        pool: VmPostgresPool,
        driver_pool: VmPostgresDriverPool,
    },
    Query {
        target: VmPostgresQueryTarget,
        driver_target: VmPostgresDriverQueryTarget,
        sql: String,
        parameters: Vec<json::Json>,
        one: bool,
    },
    Execute {
        target: VmPostgresQueryTarget,
        driver_target: VmPostgresDriverQueryTarget,
        sql: String,
        parameters: Vec<json::Json>,
    },
    BatchExecute {
        target: VmPostgresQueryTarget,
        driver_target: VmPostgresDriverQueryTarget,
        sql: String,
    },
    Begin {
        connection: VmPostgresConnection,
        driver_connection: VmPostgresDriverConnection,
    },
    Commit {
        transaction: VmPostgresTransaction,
        driver_transaction: VmPostgresDriverTransaction,
    },
    Rollback {
        transaction: VmPostgresTransaction,
        driver_transaction: VmPostgresDriverTransaction,
    },
    Prepare {
        connection: VmPostgresConnection,
        driver_connection: VmPostgresDriverConnection,
        sql: String,
        parameter_count: usize,
    },
    Decode {
        row: VmPostgresRow,
        driver_row: VmPostgresDriverRow,
        column: String,
        expected: VmPostgresDecodeType,
    },
}

impl VmPostgresDriverOperation {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Connect(_) => "connect",
            Self::Acquire { .. } => "acquire",
            Self::Query { one: true, .. } => "query_one",
            Self::Query { one: false, .. } => "query",
            Self::Execute { .. } => "execute",
            Self::BatchExecute { .. } => "batch_execute",
            Self::Begin { .. } => "begin",
            Self::Commit { .. } => "commit",
            Self::Rollback { .. } => "rollback",
            Self::Prepare { .. } => "prepare",
            Self::Decode { .. } => "decode",
        }
    }

    pub(super) fn sql_fingerprint(&self) -> Option<String> {
        let sql = match self {
            Self::Query { sql, .. }
            | Self::Execute { sql, .. }
            | Self::BatchExecute { sql, .. }
            | Self::Prepare { sql, .. } => sql,
            _ => return None,
        };
        let hexadecimal = Sha256::digest(sql.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Some(format!("sha256:{hexadecimal}"))
    }
}

impl fmt::Debug for VmPostgresDriverOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("VmPostgresDriverOperation");
        debug.field("name", &self.name());
        debug.field("sql_fingerprint", &self.sql_fingerprint());
        match self {
            Self::Query {
                target,
                parameters,
                one,
                ..
            } => {
                debug.field("target", target);
                debug.field("parameter_count", &parameters.len());
                debug.field("one", one);
            }
            Self::Execute {
                target, parameters, ..
            } => {
                debug.field("target", target);
                debug.field("parameter_count", &parameters.len());
            }
            Self::BatchExecute { target, .. } => {
                debug.field("target", target);
            }
            Self::Prepare {
                connection,
                parameter_count,
                ..
            } => {
                debug.field("connection", connection);
                debug.field("parameter_count", parameter_count);
            }
            Self::Decode {
                row,
                column,
                expected,
                ..
            } => {
                debug.field("row", row);
                debug.field("column", column);
                debug.field("expected", expected);
            }
            Self::Acquire { pool, driver_pool } => {
                debug.field("pool", pool);
                debug.field("driver_pool", driver_pool);
            }
            Self::Begin {
                connection,
                driver_connection,
            } => {
                debug.field("connection", connection);
                debug.field("driver_connection", driver_connection);
            }
            Self::Commit {
                transaction,
                driver_transaction,
            }
            | Self::Rollback {
                transaction,
                driver_transaction,
            } => {
                debug.field("transaction", transaction);
                debug.field("driver_transaction", driver_transaction);
            }
            Self::Connect(_) => {}
        }
        debug.finish_non_exhaustive()
    }
}

/// One request ready for a maintained Postgres driver worker.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the maintained Postgres driver worker has not consumed VM requests yet"
    )
)]
#[derive(Clone, Debug)]
pub(crate) struct VmPostgresDriverRequest {
    pub(crate) request_id: RequestId,
    pub(crate) owner: VmProcessId,
    pub(crate) operation: VmPostgresDriverOperation,
}

/// Stable driver failure. Adapter-specific values must be mapped before completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPostgresFailure {
    pub(crate) code: String,
    pub(crate) message: String,
}

impl VmPostgresFailure {
    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: redact_database_message(&message.into()),
        }
    }
}

/// Maintained-driver completion payload consumed by the VM.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmPostgresDriverCompletion {
    Connected(VmPostgresDriverPool),
    Acquired(VmPostgresDriverConnection),
    TransactionStarted(VmPostgresDriverTransaction),
    Prepared(VmPostgresDriverPreparedStatement),
    Rows { rows: Vec<VmPostgresDriverRow> },
    AffectedRows(i64),
    Decoded(VmPostgresDecodedValue),
    Unit,
    Failed(VmPostgresFailure),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmPostgresDecodedValue {
    Null,
    String(String),
    Int(i64),
    Bool(bool),
    Json(String),
}

/// Typed value delivered back to a resumed Terlan process.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmPostgresReply {
    Pool(VmPostgresPool),
    Connection(VmPostgresConnection),
    Transaction(VmPostgresTransaction),
    PreparedStatement(VmPostgresPreparedStatement),
    Rows {
        result_set: VmPostgresResultSet,
        rows: Vec<VmPostgresRow>,
    },
    AffectedRows(i64),
    Decoded(VmPostgresDecodedValue),
    Unit,
    Error(VmPostgresFailure),
}

/// Driver controls produced by cancellation and resource cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmPostgresDriverControl {
    Cancel(RequestId),
    Rollback {
        transaction: VmPostgresTransaction,
        driver_transaction: VmPostgresDriverTransaction,
    },
    Release {
        connection: VmPostgresConnection,
        driver_connection: VmPostgresDriverConnection,
    },
    ClosePool {
        pool: VmPostgresPool,
        driver_pool: VmPostgresDriverPool,
    },
    DropPreparedStatement {
        statement: VmPostgresPreparedStatement,
        driver_statement: VmPostgresDriverPreparedStatement,
    },
    DropRow {
        row: VmPostgresRow,
        driver_row: VmPostgresDriverRow,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VmPostgresTransactionState {
    Active,
    Committed,
    RolledBack,
}

fn redact_database_message(message: &str) -> String {
    let mut redacted = message.to_string();
    for scheme in ["postgres://", "postgresql://"] {
        while let Some(start) = redacted.find(scheme) {
            let end = redacted[start..]
                .find(char::is_whitespace)
                .map_or(redacted.len(), |offset| start + offset);
            redacted.replace_range(start..end, "<redacted-postgres-url>");
        }
    }
    redacted
}
