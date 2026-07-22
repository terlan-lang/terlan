//! Safe VM-facing adapter over the generated `libpq` C ABI crate.

use std::time::Duration;

use terlan_libpq::{
    CAbiError, Connection, ConnectionInterest, ConnectionPoller, ConnectionReadiness, QueryResult,
};

use crate::terlan_native::json as json_adapter;

use super::{PostgresError, Row};

const LIBPQ_NO_RESULT: i32 = 1;

/// One nonblocking connection transition requested by libpq.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConnectPoll {
    Read,
    Write,
    Ready,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DriverIoInterest {
    Drive,
    Read,
    Write,
}

#[derive(Clone, Copy)]
pub(crate) struct DriverReadinessSource<'a> {
    pub(crate) key: u64,
    pub(crate) connection: &'a DriverConnection,
    pub(crate) interest: DriverIoInterest,
}

#[derive(Clone, Debug)]
pub(crate) struct DriverReadinessPoller {
    inner: ConnectionPoller,
}

impl DriverReadinessPoller {
    pub(crate) fn new() -> Result<Self, PostgresError> {
        ConnectionPoller::new()
            .map(|inner| Self { inner })
            .map_err(readiness_error)
    }

    pub(crate) fn wait(
        &self,
        sources: &[DriverReadinessSource<'_>],
        timeout: Option<Duration>,
    ) -> Result<Vec<u64>, PostgresError> {
        let native = sources
            .iter()
            .map(|source| ConnectionReadiness {
                key: source.key,
                connection: &source.connection.inner,
                interest: match source.interest {
                    DriverIoInterest::Drive => ConnectionInterest::Drive,
                    DriverIoInterest::Read => ConnectionInterest::Read,
                    DriverIoInterest::Write => ConnectionInterest::Write,
                },
            })
            .collect::<Vec<_>>();
        self.inner.wait(&native, timeout).map_err(readiness_error)
    }
}

/// Safe, thread-confined libpq connection owned by the VM driver.
pub(crate) struct DriverConnection {
    inner: Connection,
}

impl std::fmt::Debug for DriverConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DriverConnection")
            .finish_non_exhaustive()
    }
}

impl DriverConnection {
    pub(crate) fn start(url: &str) -> Result<Self, PostgresError> {
        Connection::start(url)
            .map(|inner| Self { inner })
            .map_err(|error| map_error("postgres.connect.start", error))
    }

    pub(crate) fn socket(&self) -> Result<i64, PostgresError> {
        self.inner
            .socket()
            .map_err(|error| map_error("postgres.connect.socket", error))
    }

    pub(crate) fn poll_connect(&mut self) -> Result<ConnectPoll, PostgresError> {
        match self.inner.poll_connect() {
            Ok(1) => Ok(ConnectPoll::Read),
            Ok(2) => Ok(ConnectPoll::Write),
            Ok(3) => Ok(ConnectPoll::Ready),
            Ok(4) => Ok(ConnectPoll::Active),
            Ok(state) => Err(PostgresError::new(
                "postgres.connect.poll_state",
                format!("libpq returned unknown connection poll state {state}."),
            )),
            Err(error) => Err(self.driver_error("postgres.connect", error)),
        }
    }

    pub(crate) fn send_query(
        &mut self,
        sql: &str,
        parameters: &[json_adapter::Json],
    ) -> Result<(), PostgresError> {
        self.inner
            .clear_parameters()
            .map_err(|error| map_error("postgres.parameters.clear", error))?;
        for parameter in parameters {
            match parameter.as_serde() {
                serde_json::Value::Null => self
                    .inner
                    .push_null()
                    .map_err(|error| map_error("postgres.parameters.bind", error))?,
                value => {
                    let text = parameter_text(value)?;
                    self.inner
                        .push_text(&text)
                        .map_err(|error| map_error("postgres.parameters.bind", error))?;
                }
            }
        }
        self.inner
            .send_query(sql)
            .map_err(|error| self.driver_error("postgres.query.send", error))
    }

    pub(crate) fn send_batch(&mut self, sql: &str) -> Result<(), PostgresError> {
        self.inner
            .send_batch(sql)
            .map_err(|error| self.driver_error("postgres.batch.send", error))
    }

    pub(crate) fn consume_input(&mut self) -> Result<(), PostgresError> {
        self.inner
            .consume_input()
            .map_err(|error| self.driver_error("postgres.query.read", error))
    }

    pub(crate) fn is_busy(&self) -> Result<bool, PostgresError> {
        self.inner
            .is_busy()
            .map_err(|error| map_error("postgres.query.busy", error))
    }

    pub(crate) fn next_result(&mut self) -> Result<Option<DriverResult>, PostgresError> {
        match self.inner.next_result() {
            Ok(inner) => Ok(Some(DriverResult { inner })),
            Err(error) if error.status == LIBPQ_NO_RESULT => Ok(None),
            Err(error) => Err(self.driver_error("postgres.query.result", error)),
        }
    }

    pub(crate) fn abort(&mut self) -> Result<(), PostgresError> {
        self.inner
            .abort()
            .map_err(|error| map_error("postgres.cancel", error))
    }

    pub(crate) fn result_error(&mut self) -> PostgresError {
        self.driver_error(
            "postgres.query.result_status",
            CAbiError {
                operation: "postgres.libpq.result.status",
                status: -1,
            },
        )
    }

    fn driver_error(&mut self, code: &'static str, fallback: CAbiError) -> PostgresError {
        let message = self
            .inner
            .error_bytes()
            .ok()
            .and_then(|bytes| decode_bytes(&bytes).ok())
            .filter(|message| !message.trim().is_empty())
            .unwrap_or_else(|| format!("libpq status {}", fallback.status));
        PostgresError::new(code, format!("Postgres driver error: {message}"))
    }
}

/// Owned query result whose values are copied before disposal.
pub(crate) struct DriverResult {
    inner: QueryResult,
}

impl DriverResult {
    pub(crate) fn status(&self) -> Result<i64, PostgresError> {
        self.inner
            .status()
            .map_err(|error| map_error("postgres.result.status", error))
    }

    pub(crate) fn affected_rows(&self) -> Result<i64, PostgresError> {
        self.inner
            .affected_rows()
            .map_err(|error| map_error("postgres.result.affected_rows", error))
    }

    pub(crate) fn rows(mut self) -> Result<Vec<Row>, PostgresError> {
        let row_count = usize_from(
            self.inner
                .row_count()
                .map_err(|error| map_error("postgres.result.row_count", error))?,
            "row count",
        )?;
        let column_count = usize_from(
            self.inner
                .column_count()
                .map_err(|error| map_error("postgres.result.column_count", error))?,
            "column count",
        )?;
        let mut names = Vec::with_capacity(column_count);
        let mut oids = Vec::with_capacity(column_count);
        for column in 0..column_count {
            let column = i64::try_from(column).map_err(count_error)?;
            self.inner
                .select_column_name(column)
                .map_err(|error| map_error("postgres.result.column_name", error))?;
            names.push(decode_bytes(&self.inner.value_bytes().map_err(
                |error| map_error("postgres.result.column_name", error),
            )?)?);
            oids.push(
                self.inner
                    .column_oid(column)
                    .map_err(|error| map_error("postgres.result.column_oid", error))?,
            );
        }

        let mut rows = Vec::with_capacity(row_count);
        for row_index in 0..row_count {
            let mut row = Row::new();
            for column_index in 0..column_count {
                self.inner
                    .select_value(
                        i64::try_from(row_index).map_err(count_error)?,
                        i64::try_from(column_index).map_err(count_error)?,
                    )
                    .map_err(|error| map_error("postgres.result.value", error))?;
                let value = if self
                    .inner
                    .value_is_null()
                    .map_err(|error| map_error("postgres.result.value_null", error))?
                {
                    None
                } else {
                    Some(decode_bytes(&self.inner.value_bytes().map_err(
                        |error| map_error("postgres.result.value", error),
                    )?)?)
                };
                row.put_libpq_text(&names[column_index], oids[column_index], value.as_deref())?;
            }
            rows.push(row);
        }
        Ok(rows)
    }
}

fn parameter_text(value: &serde_json::Value) -> Result<String, PostgresError> {
    match value {
        serde_json::Value::String(value) => Ok(value.clone()),
        serde_json::Value::Bool(value) => Ok(value.to_string()),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => serde_json::to_string(value)
            .map_err(|error| {
                PostgresError::new(
                    "postgres.parameters.json",
                    format!("Could not encode Postgres JSON parameter: {error}."),
                )
            }),
        serde_json::Value::Null => unreachable!("null parameters have a dedicated binding"),
    }
}

fn readiness_error(error: terlan_libpq::ReadinessError) -> PostgresError {
    PostgresError::new(error.operation, error.message)
}

fn decode_bytes(bytes: &[i64]) -> Result<String, PostgresError> {
    let bytes = bytes
        .iter()
        .map(|byte| {
            u8::try_from(*byte).map_err(|_| {
                PostgresError::new(
                    "postgres.driver.bytes",
                    "libpq returned a byte outside the unsigned 8-bit range.",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|error| {
        PostgresError::new(
            "postgres.driver.utf8",
            format!("libpq returned invalid UTF-8 text: {error}."),
        )
    })
}

fn usize_from(value: i64, label: &str) -> Result<usize, PostgresError> {
    usize::try_from(value).map_err(|error| {
        PostgresError::new(
            "postgres.result.count",
            format!("Postgres {label} is invalid: {error}."),
        )
    })
}

fn count_error(error: std::num::TryFromIntError) -> PostgresError {
    PostgresError::new(
        "postgres.result.count",
        format!("Postgres result index is invalid: {error}."),
    )
}

fn map_error(code: &'static str, error: CAbiError) -> PostgresError {
    PostgresError::new(
        code,
        format!(
            "libpq adapter operation failed with status {}.",
            error.status
        ),
    )
}

#[cfg(test)]
#[path = "libpq_test.rs"]
mod libpq_test;
