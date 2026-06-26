//! Postgres adapter boundary for `std.db.Postgres`.
//!
//! This module captures the SafeNative Postgres contract: stable config
//! validation, live execution through the maintained `tokio-postgres` client,
//! and deterministic row decoding helpers for Terlan-facing values.

use std::{collections::BTreeMap, time::Duration};

use crate::json as json_adapter;
use deadpool_postgres::{
    Client as DeadpoolClient, ManagerConfig, PoolConfig, RecyclingMethod,
    Runtime as DeadpoolRuntime,
};
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::{NoTls, Row as DriverRow};

const DEFAULT_MIN_CONNECTIONS: usize = 1;
const DEFAULT_MAX_CONNECTIONS: usize = 16;
const DEFAULT_WAIT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5_000;

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

/// Postgres connection configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    url: String,
    min_connections: usize,
    max_connections: usize,
    wait_timeout_ms: u64,
    connect_timeout_ms: u64,
}

impl Config {
    /// Builds a Postgres connection configuration.
    ///
    /// Inputs:
    /// - `url`: connection URL text.
    ///
    /// Output:
    /// - Config value preserving the supplied URL.
    ///
    /// Transformation:
    /// - Stores user configuration without opening sockets or validating
    ///   credentials.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            min_connections: DEFAULT_MIN_CONNECTIONS,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            wait_timeout_ms: DEFAULT_WAIT_TIMEOUT_MS,
            connect_timeout_ms: DEFAULT_CONNECT_TIMEOUT_MS,
        }
    }

    /// Returns a config with explicit pool size limits.
    ///
    /// Inputs:
    /// - `self`: existing config.
    /// - `min_connections`: connections warmed during `connect`.
    /// - `max_connections`: maximum checked-out/idle connections owned by the
    ///   maintained pool.
    ///
    /// Output:
    /// - Updated config value.
    ///
    /// Transformation:
    /// - Replaces only the pool size limits; validation happens before connect
    ///   so invalid values produce stable Terlan diagnostics.
    pub fn with_pool_limits(mut self, min_connections: usize, max_connections: usize) -> Self {
        self.min_connections = min_connections;
        self.max_connections = max_connections;
        self
    }

    /// Returns a config with explicit pool wait and connect timeouts.
    ///
    /// Inputs:
    /// - `self`: existing config.
    /// - `wait_timeout_ms`: maximum time to wait for a pool slot.
    /// - `connect_timeout_ms`: maximum time to create a new Postgres
    ///   connection.
    ///
    /// Output:
    /// - Updated config value.
    ///
    /// Transformation:
    /// - Replaces timeout values while preserving the connection URL and pool
    ///   size limits.
    pub fn with_timeouts(mut self, wait_timeout_ms: u64, connect_timeout_ms: u64) -> Self {
        self.wait_timeout_ms = wait_timeout_ms;
        self.connect_timeout_ms = connect_timeout_ms;
        self
    }

    /// Returns the configured URL.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Borrowed URL text.
    ///
    /// Transformation:
    /// - Reads the URL without allocation or mutation.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the configured minimum warmed connection count.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Minimum connection count warmed by `connect`.
    ///
    /// Transformation:
    /// - Reads the field without allocation or mutation.
    pub fn min_connections(&self) -> usize {
        self.min_connections
    }

    /// Returns the configured maximum pool size.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Maximum connection count accepted by the maintained pool.
    ///
    /// Transformation:
    /// - Reads the field without allocation or mutation.
    pub fn max_connections(&self) -> usize {
        self.max_connections
    }

    /// Returns the configured pool wait timeout in milliseconds.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Wait timeout in milliseconds.
    ///
    /// Transformation:
    /// - Reads the field without allocation or mutation.
    pub fn wait_timeout_ms(&self) -> u64 {
        self.wait_timeout_ms
    }

    /// Returns the configured connection creation timeout in milliseconds.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Connect timeout in milliseconds.
    ///
    /// Transformation:
    /// - Reads the field without allocation or mutation.
    pub fn connect_timeout_ms(&self) -> u64 {
        self.connect_timeout_ms
    }
}

/// Validates a Postgres connection configuration.
///
/// Inputs:
/// - `config`: Postgres config value to validate.
///
/// Output:
/// - `Ok(())` when the URL uses a supported Postgres scheme.
/// - Stable invalid-url error otherwise.
///
/// Transformation:
/// - Reuses the adapter URL-scheme validator without opening sockets or
///   checking credentials.
pub fn validate_config(config: &Config) -> Result<(), PostgresError> {
    validate_postgres_url(config.url())?;
    validate_pool_config(config)
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

/// Postgres row value used by row-decoding helpers.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    values: BTreeMap<String, PostgresValue>,
}

impl Row {
    /// Builds an empty row.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Row with no columns.
    ///
    /// Transformation:
    /// - Initializes deterministic map-backed row storage for adapter tests
    ///   before live database rows are wired in.
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Inserts one string column.
    ///
    /// Inputs:
    /// - `self`: mutable row fixture.
    /// - `name`: column name.
    /// - `value`: column value.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Stores the value under the supplied name for later typed decoding.
    pub fn put_string(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values
            .insert(name.into(), PostgresValue::String(value.into()));
    }

    /// Inserts one integer column.
    ///
    /// Inputs:
    /// - `self`: mutable row fixture.
    /// - `name`: column name.
    /// - `value`: column value.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Stores the value under the supplied name for later typed decoding.
    pub fn put_int(&mut self, name: impl Into<String>, value: i64) {
        self.values.insert(name.into(), PostgresValue::Int(value));
    }

    /// Inserts one boolean column.
    ///
    /// Inputs:
    /// - `self`: mutable row fixture.
    /// - `name`: column name.
    /// - `value`: column value.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Stores the value under the supplied name for later typed decoding.
    pub fn put_bool(&mut self, name: impl Into<String>, value: bool) {
        self.values.insert(name.into(), PostgresValue::Bool(value));
    }

    /// Inserts one JSON column.
    ///
    /// Inputs:
    /// - `self`: mutable row fixture.
    /// - `name`: column name.
    /// - `value`: JSON column value.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Stores the value under the supplied name for later typed decoding.
    pub fn put_json(&mut self, name: impl Into<String>, value: json_adapter::Json) {
        self.values.insert(name.into(), PostgresValue::Json(value));
    }
}

impl Default for Row {
    /// Builds the default row value.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Empty row.
    ///
    /// Transformation:
    /// - Delegates to `Row::new`.
    fn default() -> Self {
        Self::new()
    }
}

/// Typed Postgres column value.
#[derive(Clone, Debug, PartialEq)]
enum PostgresValue {
    String(String),
    Int(i64),
    Bool(bool),
    Json(json_adapter::Json),
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

/// Reads a string column by name.
///
/// Inputs:
/// - `row`: Postgres row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(value)` when the column is present and is a string.
/// - Stable error for missing columns or type mismatches.
///
/// Transformation:
/// - Decodes the fixture/native row value through the same typed accessor
///   surface exposed by `std.db.Postgres.Row.string`.
pub fn string(row: &Row, name: &str) -> Result<String, PostgresError> {
    match row.values.get(name) {
        Some(PostgresValue::String(value)) => Ok(value.clone()),
        Some(value) => Err(type_error(name, "String", value.kind())),
        None => Err(missing_column(name)),
    }
}

/// Reads an integer column by name.
///
/// Inputs:
/// - `row`: Postgres row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(value)` when the column is present and is an integer.
/// - Stable error for missing columns or type mismatches.
///
/// Transformation:
/// - Decodes the fixture/native row value through the same typed accessor
///   surface exposed by `std.db.Postgres.Row.int`.
pub fn int(row: &Row, name: &str) -> Result<i64, PostgresError> {
    match row.values.get(name) {
        Some(PostgresValue::Int(value)) => Ok(*value),
        Some(value) => Err(type_error(name, "Int", value.kind())),
        None => Err(missing_column(name)),
    }
}

/// Reads a boolean column by name.
///
/// Inputs:
/// - `row`: Postgres row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(value)` when the column is present and is a boolean.
/// - Stable error for missing columns or type mismatches.
///
/// Transformation:
/// - Decodes the fixture/native row value through the same typed accessor
///   surface exposed by `std.db.Postgres.Row.bool`.
pub fn r#bool(row: &Row, name: &str) -> Result<bool, PostgresError> {
    match row.values.get(name) {
        Some(PostgresValue::Bool(value)) => Ok(*value),
        Some(value) => Err(type_error(name, "Bool", value.kind())),
        None => Err(missing_column(name)),
    }
}

/// Reads a JSON column by name.
///
/// Inputs:
/// - `row`: Postgres row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(value)` when the column is present and is JSON.
/// - Stable error for missing columns or type mismatches.
///
/// Transformation:
/// - Decodes the fixture/native row value through the same typed accessor
///   surface exposed by `std.db.Postgres.Row.json`.
pub fn json(row: &Row, name: &str) -> Result<json_adapter::Json, PostgresError> {
    match row.values.get(name) {
        Some(PostgresValue::Json(value)) => Ok(value.clone()),
        Some(value) => Err(type_error(name, "Json", value.kind())),
        None => Err(missing_column(name)),
    }
}

impl PostgresValue {
    /// Returns the stable Terlan type name for this column value.
    ///
    /// Inputs:
    /// - `self`: stored column value.
    ///
    /// Output:
    /// - Source-visible type name used in diagnostics.
    ///
    /// Transformation:
    /// - Maps internal row variants to public names without exposing backend
    ///   row storage.
    fn kind(&self) -> &'static str {
        match self {
            Self::String(_) => "String",
            Self::Int(_) => "Int",
            Self::Bool(_) => "Bool",
            Self::Json(_) => "Json",
        }
    }
}

/// Validates that a URL uses a supported Postgres scheme.
///
/// Inputs:
/// - `url`: connection URL text.
///
/// Output:
/// - `Ok(())` for complete `postgres://` or `postgresql://` URLs.
/// - Stable invalid-url error for parse errors, unsupported schemes, or
///   incomplete connection identity.
///
/// Transformation:
/// - Delegates URL parsing to the existing `url` crate and checks only the
///   minimal connection contract before pool construction. Diagnostics
///   intentionally avoid echoing the original URL because it may contain
///   credentials.
fn validate_postgres_url(url: &str) -> Result<(), PostgresError> {
    let parsed = url::Url::parse(url).map_err(|error| {
        PostgresError::new(
            "postgres.invalid_url",
            format!("Postgres connection URL is invalid: {error}."),
        )
    })?;
    match parsed.scheme() {
        "postgres" | "postgresql" => validate_postgres_url_identity(&parsed),
        other => Err(PostgresError::new(
            "postgres.invalid_url",
            format!("Postgres connection URL scheme `{other}` is not supported."),
        )),
    }
}

/// Validates the non-secret identity parts of a parsed Postgres URL.
///
/// Inputs:
/// - `url`: parsed Postgres URL.
///
/// Output:
/// - `Ok(())` when host and database name are present.
/// - Stable invalid-url error for incomplete connection identity.
///
/// Transformation:
/// - Checks only host and database path. Username, password, query options, and
///   fragments are ignored here so the proof-track adapter does not duplicate
///   driver-owned connection validation or leak credentials.
fn validate_postgres_url_identity(url: &url::Url) -> Result<(), PostgresError> {
    if url.host_str().is_none_or(str::is_empty) {
        return Err(PostgresError::new(
            "postgres.invalid_url",
            "Postgres connection URL must include a host.",
        ));
    }

    let database = url.path().trim_start_matches('/');
    if database.is_empty() {
        return Err(PostgresError::new(
            "postgres.invalid_url",
            "Postgres connection URL must include a database name.",
        ));
    }

    Ok(())
}

/// Validates pool sizing and timeout settings.
///
/// Inputs:
/// - `config`: Postgres config containing pool limits.
///
/// Output:
/// - `Ok(())` when limits can be represented by the maintained pool.
/// - Stable pool config error otherwise.
///
/// Transformation:
/// - Rejects invalid Terlan-facing pool settings before constructing
///   `deadpool-postgres` resources.
fn validate_pool_config(config: &Config) -> Result<(), PostgresError> {
    if config.max_connections() == 0 {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres max_connections must be greater than zero.",
        ));
    }
    if config.min_connections() == 0 {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres min_connections must be greater than zero.",
        ));
    }
    if config.min_connections() > config.max_connections() {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres min_connections must not exceed max_connections.",
        ));
    }
    if config.wait_timeout_ms() == 0 {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres wait_timeout_ms must be greater than zero.",
        ));
    }
    if config.connect_timeout_ms() == 0 {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres connect_timeout_ms must be greater than zero.",
        ));
    }
    Ok(())
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

/// Converts one driver row into the SafeNative row shape.
///
/// Inputs:
/// - `row`: row returned by `tokio-postgres`.
///
/// Output:
/// - SafeNative row with supported typed column values.
/// - Stable row error for unsupported column types.
///
/// Transformation:
/// - Reads column metadata from the maintained driver and copies supported
///   values into Terlan's backend-neutral row representation.
fn row_from_driver(row: &DriverRow) -> Result<Row, PostgresError> {
    let mut output = Row::new();
    for column in row.columns() {
        put_column_from_driver(row, column.name(), column.type_(), &mut output)?;
    }
    Ok(output)
}

/// Copies one supported driver column into a SafeNative row.
///
/// Inputs:
/// - `row`: driver row containing the column.
/// - `name`: column name.
/// - `ty`: Postgres column type.
/// - `output`: row being populated.
///
/// Output:
/// - `Ok(())` when the column is supported.
/// - Stable row error for unsupported or undecodable values.
///
/// Transformation:
/// - Converts driver-specific values into the limited typed row value set
///   currently exposed by `std.db.Postgres`.
fn put_column_from_driver(
    row: &DriverRow,
    name: &str,
    ty: &Type,
    output: &mut Row,
) -> Result<(), PostgresError> {
    match *ty {
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => {
            if let Some(value) = try_get_optional::<String>(row, name)? {
                output.put_string(name, value);
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::INT8 => {
            if let Some(value) = try_get_optional::<i64>(row, name)? {
                output.put_int(name, value);
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::INT4 => {
            if let Some(value) = try_get_optional::<i32>(row, name)? {
                output.put_int(name, i64::from(value));
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::INT2 => {
            if let Some(value) = try_get_optional::<i16>(row, name)? {
                output.put_int(name, i64::from(value));
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::BOOL => {
            if let Some(value) = try_get_optional::<bool>(row, name)? {
                output.put_bool(name, value);
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::JSON | Type::JSONB => {
            let value = try_get_optional::<serde_json::Value>(row, name)?
                .unwrap_or(serde_json::Value::Null);
            output.put_json(name, json_adapter::Json::from_serde(value));
            Ok(())
        }
        _ => Err(PostgresError::new(
            "postgres.row.unsupported_type",
            format!(
                "Postgres row column `{name}` has unsupported type `{}`.",
                ty.name()
            ),
        )),
    }
}

/// Reads one nullable column value from a driver row.
///
/// Inputs:
/// - `row`: driver row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(Some(value))` when a non-null value is present.
/// - `Ok(None)` for SQL null.
/// - Stable row error when the driver cannot decode the value as `T`.
///
/// Transformation:
/// - Delegates decoding to `tokio-postgres` and erases driver diagnostics into
///   Terlan's stable Postgres error envelope.
fn try_get_optional<T>(row: &DriverRow, name: &str) -> Result<Option<T>, PostgresError>
where
    for<'a> T: tokio_postgres::types::FromSql<'a>,
{
    row.try_get::<_, Option<T>>(name).map_err(|error| {
        PostgresError::new(
            "postgres.row.decode",
            format!("Could not decode Postgres row column `{name}`: {error}."),
        )
    })
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

/// Builds a missing-column error.
///
/// Inputs:
/// - `name`: missing column name.
///
/// Output:
/// - Stable missing-column error.
///
/// Transformation:
/// - Converts row lookup absence into portable row-decoding diagnostics.
fn missing_column(name: &str) -> PostgresError {
    PostgresError::new(
        "postgres.row.missing_column",
        format!("Postgres row does not contain column `{name}`."),
    )
}

/// Builds a type-mismatch error.
///
/// Inputs:
/// - `name`: column name.
/// - `expected`: expected Terlan type name.
/// - `actual`: actual Terlan type name.
///
/// Output:
/// - Stable row type-mismatch error.
///
/// Transformation:
/// - Converts a stored row variant mismatch into portable row-decoding
///   diagnostics.
fn type_error(name: &str, expected: &str, actual: &str) -> PostgresError {
    PostgresError::new(
        "postgres.row.type",
        format!("Postgres row column `{name}` is {actual}, expected {expected}."),
    )
}

#[cfg(test)]
#[path = "postgres_test.rs"]
mod postgres_test;
