//! Rust-native Postgres adapter boundary for `std.db.Postgres`.
//!
//! This module owns stable config validation, live execution through the
//! maintained `tokio-postgres` client, and deterministic row decoding helpers
//! for Terlan-facing values exposed through the SafeNative bridge.

use std::time::Duration;

use crate::terlan_native::json as json_adapter;
use deadpool_postgres::{
    Client as DeadpoolClient, ManagerConfig, PoolConfig, RecyclingMethod,
    Runtime as DeadpoolRuntime,
};
use tokio_postgres::types::ToSql;
use tokio_postgres::NoTls;

mod config;
mod row;

pub use config::{validate_config, Config};
use row::row_from_driver;
pub use row::{int, json, r#bool, string, Row};

/// Stable Postgres adapter error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostgresError {
    code: &'static str,
    message: String,
}

impl PostgresError {
    /// Builds a stable Postgres adapter error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    ///
    /// Output:
    /// - Error value suitable for SafeNative boundary conversion.
    ///
    /// Transformation:
    /// - Stores stable error metadata without exposing driver-specific error
    ///   values.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: Postgres adapter error.
    ///
    /// Output:
    /// - Static error code.
    ///
    /// Transformation:
    /// - Reads the code without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: Postgres adapter error.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Postgres connection pool handle.
#[derive(Clone, Debug)]
pub struct Pool {
    url: String,
    inner: Option<deadpool_postgres::Pool>,
}

impl Pool {
    /// Builds a disconnected pool placeholder inside the SafeNative crate.
    ///
    /// Inputs:
    /// - `url`: connection URL already associated with a pool contract.
    ///
    /// Output:
    /// - Pool value that carries configuration identity but owns no sockets.
    ///
    /// Transformation:
    /// - Preserves the final opaque pool value shape for dispatch and tests
    ///   without exposing a public source-level pool constructor.
    #[allow(dead_code)]
    pub(crate) fn disconnected(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            inner: None,
        }
    }

    /// Builds a live pool after adapter connection validation.
    ///
    /// Inputs:
    /// - `url`: connection URL validated by the live client path.
    /// - `inner`: maintained `deadpool-postgres` pool.
    ///
    /// Output:
    /// - Pool value carrying the resource-backed connection pool.
    ///
    /// Transformation:
    /// - Stores both stable URL identity and the native pool resource after
    ///   `connect` has proven the maintained client can check out a connection.
    fn live(url: impl Into<String>, inner: deadpool_postgres::Pool) -> Self {
        Self {
            url: url.into(),
            inner: Some(inner),
        }
    }
}

impl PartialEq for Pool {
    /// Compares pools by stable Terlan-visible identity.
    ///
    /// Inputs:
    /// - `self`: left pool handle.
    /// - `other`: right pool handle.
    ///
    /// Output:
    /// - `true` when both handles refer to the same configured URL.
    ///
    /// Transformation:
    /// - Ignores native pool internals so tests and diagnostics do not depend
    ///   on `deadpool-postgres` implementation details.
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl Eq for Pool {}

/// Transaction-scoped Postgres connection handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Connection {
    url: String,
}

/// Connects to Postgres.
///
/// Inputs:
/// - `config`: connection configuration.
///
/// Output:
/// - Live pool profile when the maintained Postgres client can connect.
/// - Stable invalid-url or connection error otherwise.
///
/// Transformation:
/// - Validates the URL, creates a maintained `deadpool-postgres` pool, checks
///   out one client to prove connectivity, and returns an opaque pool handle.
pub fn connect(config: &Config) -> Result<Pool, PostgresError> {
    validate_config(config)?;
    let runtime = runtime()?;
    let pool = build_deadpool(config)?;
    runtime.block_on(async {
        warm_deadpool(&pool, config.min_connections()).await?;
        Ok(Pool::live(config.url(), pool))
    })
}

/// Runs a SQL query and returns all rows.
///
/// Inputs:
/// - `pool`: Postgres pool handle.
/// - `sql`: SQL text.
/// - `params`: JSON-encoded parameter values.
///
/// Output:
/// - All rows returned by the maintained Postgres client.
/// - Stable SQL, connection, query, or row-decoding error otherwise.
///
/// Transformation:
/// - Checks out a client from the native pool, binds JSON-backed parameters,
///   runs the query, and decodes driver rows into Terlan-facing row values.
pub fn query(
    pool: &Pool,
    sql: &str,
    params: &[json_adapter::Json],
) -> Result<Vec<Row>, PostgresError> {
    validate_sql_text(sql)?;
    let runtime = runtime()?;
    runtime.block_on(async {
        let client = pool_client(pool).await?;
        let param_values = postgres_params(params)?;
        let param_refs = postgres_param_refs(&param_values);
        let rows = client
            .query(sql, &param_refs)
            .await
            .map_err(driver_error("postgres.query"))?;
        rows.iter().map(row_from_driver).collect()
    })
}

/// Runs a SQL query and returns at most one row.
///
/// Inputs:
/// - `pool`: Postgres pool handle.
/// - `sql`: SQL text.
/// - `params`: JSON-encoded parameter values.
///
/// Output:
/// - Optional row returned by the maintained Postgres client.
/// - Stable SQL, connection, query, or row-decoding error otherwise.
///
/// Transformation:
/// - Checks out a client from the native pool, binds JSON-backed parameters,
///   runs a single-row query, and decodes the row when present.
pub fn query_one(
    pool: &Pool,
    sql: &str,
    params: &[json_adapter::Json],
) -> Result<Option<Row>, PostgresError> {
    validate_sql_text(sql)?;
    let runtime = runtime()?;
    runtime.block_on(async {
        let client = pool_client(pool).await?;
        let param_values = postgres_params(params)?;
        let param_refs = postgres_param_refs(&param_values);
        let row = client
            .query_opt(sql, &param_refs)
            .await
            .map_err(driver_error("postgres.query"))?;
        row.as_ref().map(row_from_driver).transpose()
    })
}

/// Runs a SQL command.
///
/// Inputs:
/// - `pool`: Postgres pool handle.
/// - `sql`: SQL text.
/// - `params`: JSON-encoded parameter values.
///
/// Output:
/// - Affected-row count returned by the maintained Postgres client.
/// - Stable SQL, connection, execute, or count-conversion error otherwise.
///
/// Transformation:
/// - Checks out a client from the native pool, binds JSON-backed parameters,
///   executes the command, and converts the affected-row count to Terlan `Int`.
pub fn execute(
    pool: &Pool,
    sql: &str,
    params: &[json_adapter::Json],
) -> Result<i64, PostgresError> {
    validate_sql_text(sql)?;
    let runtime = runtime()?;
    runtime.block_on(async {
        let client = pool_client(pool).await?;
        let param_values = postgres_params(params)?;
        let param_refs = postgres_param_refs(&param_values);
        let affected = client
            .execute(sql, &param_refs)
            .await
            .map_err(driver_error("postgres.execute"))?;
        i64::try_from(affected).map_err(|error| {
            PostgresError::new(
                "postgres.execute.count",
                format!("Postgres affected-row count does not fit in Terlan Int: {error}."),
            )
        })
    })
}

/// Runs one SQL batch without parameters.
///
/// Inputs:
/// - `pool`: Postgres pool handle.
/// - `sql`: SQL batch text.
///
/// Output:
/// - `Ok(())` when the maintained Postgres client accepts the batch.
/// - Stable SQL, connection, or execution error otherwise.
///
/// Transformation:
/// - Checks out a client from the native pool and delegates multi-statement
///   batch execution to `tokio-postgres` instead of shelling out to database
///   tools or hand-rolling protocol behavior.
pub fn batch_execute(pool: &Pool, sql: &str) -> Result<(), PostgresError> {
    validate_sql_text(sql)?;
    let runtime = runtime()?;
    runtime.block_on(async {
        let client = pool_client(pool).await?;
        client
            .batch_execute(sql)
            .await
            .map_err(driver_error("postgres.batch_execute"))
    })
}

/// Runs a transaction body.
///
/// Inputs:
/// - `pool`: Postgres pool handle.
/// - `body`: transaction callback.
///
/// Output:
/// - Callback result after a successful commit.
/// - Stable connection, transaction, or callback error otherwise.
///
/// Transformation:
/// - Checks out a client from the native pool, starts a transaction, runs the
///   callback, commits on success, and rolls back on callback failure.
pub fn transaction<T>(
    pool: &Pool,
    body: impl FnOnce(&Connection) -> Result<T, PostgresError>,
) -> Result<T, PostgresError> {
    let runtime = runtime()?;
    runtime.block_on(async {
        let client = pool_client(pool).await?;
        client
            .batch_execute("BEGIN")
            .await
            .map_err(driver_error("postgres.transaction.begin"))?;
        let connection = Connection {
            url: pool.url.clone(),
        };
        match body(&connection) {
            Ok(value) => {
                client
                    .batch_execute("COMMIT")
                    .await
                    .map_err(driver_error("postgres.transaction.commit"))?;
                Ok(value)
            }
            Err(error) => {
                let _rollback_result = client.batch_execute("ROLLBACK").await;
                Err(error)
            }
        }
    })
}

/// Validates SQL text before it reaches the live Postgres adapter.
///
/// Inputs:
/// - `sql`: SQL text supplied by `std.db.Postgres` or generated SQL wrappers.
///
/// Output:
/// - `Ok(())` when the SQL contains non-whitespace text.
/// - Stable invalid-SQL error when the text is empty.
///
/// Transformation:
/// - Performs only a minimal transport-boundary guard. It intentionally does
///   not parse SQL or validate SQL semantics; that remains owned by Postgres
///   and the chosen Rust client/parser path.
fn validate_sql_text(sql: &str) -> Result<(), PostgresError> {
    if sql.trim().is_empty() {
        return Err(PostgresError::new(
            "postgres.sql.empty",
            "Postgres SQL text must not be empty.",
        ));
    }
    Ok(())
}

/// Builds the maintained Postgres pool for one Terlan pool config.
///
/// Inputs:
/// - `config`: validated Postgres connection and pool configuration.
///
/// Output:
/// - `deadpool-postgres` pool that can grow up to its configured maximum.
/// - Stable pool creation error if driver config cannot be built.
///
/// Transformation:
/// - Converts Terlan's stable URL config into the maintained pool config and
///   maps max-size and timeout settings into `deadpool-postgres`.
fn build_deadpool(config: &Config) -> Result<deadpool_postgres::Pool, PostgresError> {
    let mut driver_config = deadpool_postgres::Config::new();
    driver_config.url = Some(config.url().to_string());
    driver_config.connect_timeout = Some(Duration::from_millis(config.connect_timeout_ms()));
    driver_config.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Verified,
    });
    let mut pool_config = PoolConfig::new(config.max_connections());
    pool_config.timeouts.wait = Some(Duration::from_millis(config.wait_timeout_ms()));
    pool_config.timeouts.create = Some(Duration::from_millis(config.connect_timeout_ms()));
    driver_config.pool = Some(pool_config);
    driver_config
        .create_pool(Some(DeadpoolRuntime::Tokio1), NoTls)
        .map_err(|error| {
            PostgresError::new(
                "postgres.pool.create",
                format!("Could not create Postgres connection pool: {error}."),
            )
        })
}

/// Warms a newly-created Postgres pool.
///
/// Inputs:
/// - `pool`: maintained Postgres pool.
/// - `min_connections`: number of connections to create immediately.
///
/// Output:
/// - `Ok(())` after the requested minimum number of connections can be checked
///   out.
/// - Stable connection error when any checkout fails.
///
/// Transformation:
/// - Holds each checked-out client until the requested count is reached, then
///   drops them together so the pool retains warm idle connections for reuse.
async fn warm_deadpool(
    pool: &deadpool_postgres::Pool,
    min_connections: usize,
) -> Result<(), PostgresError> {
    let mut clients = Vec::with_capacity(min_connections);
    for _ in 0..min_connections {
        clients.push(pool.get().await.map_err(pool_error("postgres.connect"))?);
    }
    drop(clients);
    Ok(())
}

/// Checks out one client from a live Postgres pool.
///
/// Inputs:
/// - `pool`: Terlan Postgres pool handle.
///
/// Output:
/// - Checked-out deadpool client.
/// - Stable connection error for disconnected test handles or pool checkout
///   failures.
///
/// Transformation:
/// - Hides the native resource pool behind Terlan's stable diagnostics and
///   lets `deadpool-postgres` expand the pool under concurrent demand.
async fn pool_client(pool: &Pool) -> Result<DeadpoolClient, PostgresError> {
    let Some(inner) = &pool.inner else {
        return Err(PostgresError::new(
            "postgres.connect",
            "Postgres pool is not connected.",
        ));
    };
    inner.get().await.map_err(pool_error("postgres.connect"))
}

/// Builds a Tokio runtime for one blocking SafeNative call.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Tokio runtime for executing maintained async Postgres client calls.
/// - Stable runtime error if the runtime cannot be constructed.
///
/// Transformation:
/// - Keeps the public SafeNative adapter API synchronous while isolating the
///   async runtime detail inside the Rust/Tokio adapter boundary.
fn runtime() -> Result<tokio::runtime::Runtime, PostgresError> {
    tokio::runtime::Runtime::new().map_err(|error| {
        PostgresError::new(
            "postgres.runtime",
            format!("Could not start the Postgres Tokio runtime: {error}."),
        )
    })
}

/// Converts JSON parameters into Postgres driver parameters.
///
/// Inputs:
/// - `params`: SafeNative JSON parameters supplied by `std.db.Postgres`.
///
/// Output:
/// - Boxed values implementing `ToSql`.
/// - Stable parameter error for unsupported JSON numbers.
///
/// Transformation:
/// - Maps JSON scalars to native Postgres scalar parameters and keeps JSON
///   arrays/objects as `jsonb`-capable serde values through `tokio-postgres`.
fn postgres_params(
    params: &[json_adapter::Json],
) -> Result<Vec<Box<dyn ToSql + Sync>>, PostgresError> {
    params
        .iter()
        .map(postgres_param)
        .collect::<Result<Vec<_>, _>>()
}

/// Converts one JSON parameter into a Postgres driver parameter.
///
/// Inputs:
/// - `param`: JSON parameter value.
///
/// Output:
/// - Boxed `ToSql` value.
/// - Stable parameter error for unsupported numeric forms.
///
/// Transformation:
/// - Preserves scalar shape where possible and delegates JSON object/array
///   serialization to the maintained `tokio-postgres` serde integration.
fn postgres_param(param: &json_adapter::Json) -> Result<Box<dyn ToSql + Sync>, PostgresError> {
    match param.as_serde() {
        serde_json::Value::Null => Ok(Box::new(Option::<String>::None)),
        serde_json::Value::Bool(value) => Ok(Box::new(*value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Box::new(value))
            } else if let Some(value) = value.as_f64() {
                Ok(Box::new(value))
            } else {
                Err(PostgresError::new(
                    "postgres.param.number",
                    "Postgres parameter number is outside the supported Terlan numeric range.",
                ))
            }
        }
        serde_json::Value::String(value) => Ok(Box::new(value.clone())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Ok(Box::new(param.as_serde().clone()))
        }
    }
}

/// Borrows boxed Postgres parameters as driver parameter references.
///
/// Inputs:
/// - `params`: boxed `ToSql` parameter values.
///
/// Output:
/// - Borrowed parameter references in the same order.
///
/// Transformation:
/// - Converts owned boxes into the slice shape expected by `tokio-postgres`
///   without copying parameter values.
fn postgres_param_refs(params: &[Box<dyn ToSql + Sync>]) -> Vec<&(dyn ToSql + Sync)> {
    params
        .iter()
        .map(|param| param.as_ref() as &(dyn ToSql + Sync))
        .collect()
}

/// Builds a closure that maps driver errors into stable adapter errors.
///
/// Inputs:
/// - `code`: stable machine-readable error code for the operation.
///
/// Output:
/// - Closure suitable for `map_err`.
///
/// Transformation:
/// - Captures only the stable code and converts driver text into the portable
///   Postgres adapter error shape.
fn driver_error(code: &'static str) -> impl Fn(tokio_postgres::Error) -> PostgresError {
    move |error| PostgresError::new(code, format!("Postgres driver error: {error}."))
}

/// Builds a closure that maps pool checkout errors into stable adapter errors.
///
/// Inputs:
/// - `code`: stable machine-readable error code for the operation.
///
/// Output:
/// - Closure suitable for `map_err`.
///
/// Transformation:
/// - Captures only the stable code and converts `deadpool-postgres` checkout
///   diagnostics into the portable Postgres adapter error shape.
fn pool_error(code: &'static str) -> impl Fn(deadpool_postgres::PoolError) -> PostgresError {
    move |error| PostgresError::new(code, format!("Postgres pool error: {error}."))
}

#[cfg(test)]
#[path = "postgres_test.rs"]
mod postgres_test;
