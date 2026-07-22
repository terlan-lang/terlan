//! Backend-neutral Postgres boundary for `std.db.Postgres`.
//!
//! This module owns stable configuration validation, opaque driver values,
//! and deterministic row decoding. Live socket execution belongs to the VM
//! Postgres driver worker; this compatibility surface must not create an
//! independent async runtime.

use crate::terlan_native::json as json_adapter;

#[path = "postgres/config.rs"]
mod config;
#[path = "postgres/libpq.rs"]
pub(crate) mod libpq;
#[path = "postgres/row.rs"]
mod row;
#[cfg(test)]
#[path = "postgres/postgres_test_support_test.rs"]
pub(crate) mod test_support;

pub use config::{validate_config, Config};
pub use row::{int, json, r#bool, string, Row};
pub(crate) use row::{value, DecodedValue};

/// Driver status recorded in database evidence reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DriverProvenance {
    pub(crate) client_crate: &'static str,
    pub(crate) client_version: &'static str,
    pub(crate) pool_crate: &'static str,
    pub(crate) pool_version: &'static str,
    pub(crate) runtime: &'static str,
}

/// Returns the current VM-owned Postgres driver provenance.
pub(crate) const fn driver_provenance() -> DriverProvenance {
    DriverProvenance {
        client_crate: "libpq",
        client_version: "14+",
        pool_crate: "terlan-vm",
        pool_version: "0.0.7",
        runtime: "terlan-vm",
    }
}

/// Stable Postgres adapter error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostgresError {
    code: &'static str,
    message: String,
}

impl PostgresError {
    /// Builds a stable, driver-independent Postgres error.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Returns the stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the redacted human-readable error message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Opaque Postgres pool placeholder owned by a VM driver worker.
#[derive(Clone)]
pub struct Pool {
    url: String,
}

impl std::fmt::Debug for Pool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Pool")
            .field("url", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl PartialEq for Pool {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl Eq for Pool {}

/// Compatibility value for the retired callback transaction API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Connection;

fn vm_driver_required() -> PostgresError {
    PostgresError::new(
        "postgres.vm_driver_unavailable",
        "Postgres execution requires the VM-owned Postgres I/O driver.",
    )
}

/// Validates configuration before VM-owned pool creation.
pub fn connect(config: &Config) -> Result<Pool, PostgresError> {
    validate_config(config)?;
    Err(vm_driver_required())
}

/// Validates a pool query before VM-owned driver dispatch.
pub fn query(
    _pool: &Pool,
    sql: &str,
    _params: &[json_adapter::Json],
) -> Result<Vec<Row>, PostgresError> {
    validate_sql_text(sql)?;
    Err(vm_driver_required())
}

/// Validates a single-row query before VM-owned driver dispatch.
pub fn query_one(
    _pool: &Pool,
    sql: &str,
    _params: &[json_adapter::Json],
) -> Result<Option<Row>, PostgresError> {
    validate_sql_text(sql)?;
    Err(vm_driver_required())
}

/// Validates a pool command before VM-owned driver dispatch.
pub fn execute(
    _pool: &Pool,
    sql: &str,
    _params: &[json_adapter::Json],
) -> Result<i64, PostgresError> {
    validate_sql_text(sql)?;
    Err(vm_driver_required())
}

/// Validates a SQL batch before VM-owned driver dispatch.
pub fn batch_execute(_pool: &Pool, sql: &str) -> Result<(), PostgresError> {
    validate_sql_text(sql)?;
    Err(vm_driver_required())
}

/// Rejects callback transactions because transaction lifetime is VM-owned.
pub fn transaction<T>(
    _pool: &Pool,
    _body: impl FnOnce(&Connection) -> Result<T, PostgresError>,
) -> Result<T, PostgresError> {
    Err(vm_driver_required())
}

/// Performs the boundary's minimal nonempty-SQL validation.
fn validate_sql_text(sql: &str) -> Result<(), PostgresError> {
    if sql.trim().is_empty() {
        return Err(PostgresError::new(
            "postgres.sql.empty",
            "Postgres SQL text must not be empty.",
        ));
    }
    Ok(())
}
