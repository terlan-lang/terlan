use std::cell::{Cell, RefCell};

use super::*;
use crate::runtime::vm::postgres::{
    VmPostgresConnectConfig, VmPostgresConnection, VmPostgresPool, VmPostgresQueryTarget,
    VmPostgresRow, VmPostgresTransaction,
};
use crate::runtime::vm::process::VmProcessId;
use crate::terlan_native_boundary::request::RequestId;

#[derive(Debug, Default)]
struct FixtureBackend {
    fail_connect: Cell<bool>,
    fail_commit: Cell<bool>,
    fail_rollback_once: Cell<bool>,
    observed_parameters: RefCell<Vec<serde_json::Value>>,
}

#[derive(Debug)]
struct FixtureConnection {
    transaction_active: bool,
}

impl VmPostgresDriverBackend for FixtureBackend {
    type Pool = usize;
    type Connection = FixtureConnection;
    type PreparedStatement = String;
    type Row = postgres::Row;

    fn connect(&self, config: &postgres::Config) -> Result<Self::Pool, postgres::PostgresError> {
        if self.fail_connect.get() {
            return Err(postgres::PostgresError::new(
                "postgres.connect",
                "could not reach postgres://alice:secret@fixture/database",
            ));
        }
        postgres::validate_config(config)?;
        Ok(config.max_connections())
    }

    fn acquire(&self, pool: &Self::Pool) -> Result<Self::Connection, postgres::PostgresError> {
        if *pool == 0 {
            return Err(postgres::PostgresError::new(
                "postgres.pool.stale",
                "fixture pool is closed",
            ));
        }
        Ok(FixtureConnection {
            transaction_active: false,
        })
    }

    fn query_pool(
        &self,
        _pool: &Self::Pool,
        _sql: &str,
        parameters: &[json::Json],
        one: bool,
    ) -> Result<Vec<Self::Row>, postgres::PostgresError> {
        self.observe_parameters(parameters);
        Ok(fixture_rows(if one { 1 } else { 2 }))
    }

    fn query_connection(
        &self,
        connection: &Self::Connection,
        _sql: &str,
        parameters: &[json::Json],
        one: bool,
    ) -> Result<Vec<Self::Row>, postgres::PostgresError> {
        self.observe_parameters(parameters);
        if !connection.transaction_active {
            return Err(postgres::PostgresError::new(
                "postgres.transaction.inactive",
                "fixture transaction is not active",
            ));
        }
        Ok(fixture_rows(if one { 1 } else { 2 }))
    }

    fn execute_pool(
        &self,
        _pool: &Self::Pool,
        _sql: &str,
        parameters: &[json::Json],
    ) -> Result<i64, postgres::PostgresError> {
        self.observe_parameters(parameters);
        Ok(3)
    }

    fn execute_connection(
        &self,
        connection: &Self::Connection,
        _sql: &str,
        parameters: &[json::Json],
    ) -> Result<i64, postgres::PostgresError> {
        self.observe_parameters(parameters);
        if connection.transaction_active {
            Ok(4)
        } else {
            Err(postgres::PostgresError::new(
                "postgres.transaction.inactive",
                "fixture transaction is not active",
            ))
        }
    }

    fn begin(&self, connection: &mut Self::Connection) -> Result<(), postgres::PostgresError> {
        if connection.transaction_active {
            return Err(postgres::PostgresError::new(
                "postgres.transaction.active",
                "fixture transaction is already active",
            ));
        }
        connection.transaction_active = true;
        Ok(())
    }

    fn commit(&self, connection: &mut Self::Connection) -> Result<(), postgres::PostgresError> {
        if self.fail_commit.get() {
            return Err(postgres::PostgresError::new(
                "postgres.transaction.commit",
                "fixture commit failed",
            ));
        }
        finish_fixture_transaction(connection)
    }

    fn rollback(&self, connection: &mut Self::Connection) -> Result<(), postgres::PostgresError> {
        if self.fail_rollback_once.replace(false) {
            return Err(postgres::PostgresError::new(
                "postgres.transaction.rollback",
                "fixture rollback failed",
            ));
        }
        finish_fixture_transaction(connection)
    }

    fn prepare(
        &self,
        connection: &Self::Connection,
        sql: &str,
    ) -> Result<Self::PreparedStatement, postgres::PostgresError> {
        if !connection.transaction_active {
            return Err(postgres::PostgresError::new(
                "postgres.transaction.inactive",
                "fixture transaction is not active",
            ));
        }
        Ok(sql.to_string())
    }

    fn decode(
        &self,
        row: &Self::Row,
        column: &str,
        expected: VmPostgresDecodeType,
    ) -> Result<VmPostgresDecodedValue, postgres::PostgresError> {
        match expected {
            VmPostgresDecodeType::Dynamic => match postgres::value(row, column)? {
                postgres::DecodedValue::Null => Ok(VmPostgresDecodedValue::Null),
                postgres::DecodedValue::String(value) => Ok(VmPostgresDecodedValue::String(value)),
                postgres::DecodedValue::Int(value) => Ok(VmPostgresDecodedValue::Int(value)),
                postgres::DecodedValue::Bool(value) => Ok(VmPostgresDecodedValue::Bool(value)),
                postgres::DecodedValue::Json(value) => {
                    Ok(VmPostgresDecodedValue::Json(value.as_serde().to_string()))
                }
            },
            VmPostgresDecodeType::String => {
                postgres::string(row, column).map(VmPostgresDecodedValue::String)
            }
            VmPostgresDecodeType::Int => {
                postgres::int(row, column).map(VmPostgresDecodedValue::Int)
            }
            VmPostgresDecodeType::Bool => {
                postgres::r#bool(row, column).map(VmPostgresDecodedValue::Bool)
            }
            VmPostgresDecodeType::Json => postgres::json(row, column)
                .map(|value| VmPostgresDecodedValue::Json(value.as_serde().to_string())),
        }
    }
}

impl FixtureBackend {
    fn observe_parameters(&self, parameters: &[json::Json]) {
        self.observed_parameters.replace(
            parameters
                .iter()
                .map(|parameter| parameter.as_serde().clone())
                .collect(),
        );
    }
}

fn finish_fixture_transaction(
    connection: &mut FixtureConnection,
) -> Result<(), postgres::PostgresError> {
    if !connection.transaction_active {
        return Err(postgres::PostgresError::new(
            "postgres.transaction.inactive",
            "fixture transaction is not active",
        ));
    }
    connection.transaction_active = false;
    Ok(())
}

fn fixture_rows(count: usize) -> Vec<postgres::Row> {
    (0..count)
        .map(|index| {
            let mut row = postgres::Row::new();
            row.put_int("id", index as i64 + 1);
            row.put_string("name", format!("row-{index}"));
            row.put_bool("active", true);
            row.put_json(
                "payload",
                json::Json::from_serde(serde_json::json!({"ok": true})),
            );
            row.put_libpq_text("nickname", 25, None)
                .expect("store null fixture value");
            row
        })
        .collect()
}

fn config() -> VmPostgresConnectConfig {
    VmPostgresConnectConfig::new(
        postgres::Config::new("postgres://alice:secret@fixture/database").with_pool_limits(1, 4),
    )
    .expect("fixture config")
}

fn request(id: u64, operation: VmPostgresDriverOperation) -> VmPostgresDriverRequest {
    VmPostgresDriverRequest {
        request_id: RequestId { value: id },
        owner: VmProcessId::from_raw_for_test(9),
        operation,
    }
}

#[test]
fn worker_binds_opaque_resources_and_keeps_transaction_on_one_connection() {
    let mut worker = VmPostgresDriverWorker::new(FixtureBackend::default());
    let driver_pool = match worker.execute(request(1, VmPostgresDriverOperation::Connect(config())))
    {
        VmPostgresDriverCompletion::Connected(pool) => pool,
        completion => panic!("expected connected completion, found {completion:?}"),
    };
    let driver_connection = match worker.execute(request(
        2,
        VmPostgresDriverOperation::Acquire {
            pool: VmPostgresPool(10),
            driver_pool,
        },
    )) {
        VmPostgresDriverCompletion::Acquired(connection) => connection,
        completion => panic!("expected acquired completion, found {completion:?}"),
    };
    let driver_transaction = match worker.execute(request(
        3,
        VmPostgresDriverOperation::Begin {
            connection: VmPostgresConnection(11),
            driver_connection,
        },
    )) {
        VmPostgresDriverCompletion::TransactionStarted(transaction) => transaction,
        completion => panic!("expected transaction completion, found {completion:?}"),
    };

    let row = match worker.execute(request(
        4,
        VmPostgresDriverOperation::Query {
            target: VmPostgresQueryTarget::Transaction(VmPostgresTransaction(12)),
            driver_target: VmPostgresDriverQueryTarget::Transaction(driver_transaction),
            sql: "SELECT id, name, active FROM rows".to_string(),
            parameters: Vec::new(),
            one: true,
        },
    )) {
        VmPostgresDriverCompletion::Rows { rows } => {
            assert_eq!(rows.len(), 1);
            rows[0]
        }
        completion => panic!("expected rows completion, found {completion:?}"),
    };
    assert_eq!(
        worker.execute(request(
            5,
            VmPostgresDriverOperation::Decode {
                row: VmPostgresRow(13),
                driver_row: row,
                column: "id".to_string(),
                expected: VmPostgresDecodeType::Dynamic,
            },
        )),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::Int(1))
    );
    assert_eq!(
        worker.execute(request(
            50,
            VmPostgresDriverOperation::Decode {
                row: VmPostgresRow(13),
                driver_row: row,
                column: "payload".to_string(),
                expected: VmPostgresDecodeType::Dynamic,
            },
        )),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::Json(
            "{\"ok\":true}".to_string()
        ))
    );
    assert_eq!(
        worker.execute(request(
            51,
            VmPostgresDriverOperation::Decode {
                row: VmPostgresRow(13),
                driver_row: row,
                column: "nickname".to_string(),
                expected: VmPostgresDecodeType::Dynamic,
            },
        )),
        VmPostgresDriverCompletion::Decoded(VmPostgresDecodedValue::Null)
    );
    assert!(matches!(
        worker.execute(request(
            6,
            VmPostgresDriverOperation::Prepare {
                connection: VmPostgresConnection(11),
                driver_connection,
                sql: "SELECT id FROM rows".to_string(),
                parameter_count: 0,
            },
        )),
        VmPostgresDriverCompletion::Prepared(_)
    ));
    assert_eq!(
        worker.execute(request(
            7,
            VmPostgresDriverOperation::Execute {
                target: VmPostgresQueryTarget::Transaction(VmPostgresTransaction(12)),
                driver_target: VmPostgresDriverQueryTarget::Transaction(driver_transaction),
                sql: "UPDATE rows SET active = true".to_string(),
                parameters: Vec::new(),
            },
        )),
        VmPostgresDriverCompletion::AffectedRows(4)
    );
    assert_eq!(
        worker.execute(request(
            8,
            VmPostgresDriverOperation::Commit {
                transaction: VmPostgresTransaction(12),
                driver_transaction,
            },
        )),
        VmPostgresDriverCompletion::Unit
    );
    assert_eq!(worker.pools.len(), 1);
    assert_eq!(worker.connections.len(), 1);
    assert!(worker.transactions.is_empty());
    assert_eq!(worker.prepared.len(), 1);
    assert_eq!(worker.rows.len(), 1);
    assert!(!format!("{worker:?}").contains("secret"));
}

#[test]
fn worker_transports_parameters_and_rejects_cancelled_and_stale_requests_without_leaking_secrets() {
    let backend = FixtureBackend::default();
    backend.fail_connect.set(true);
    let mut worker = VmPostgresDriverWorker::new(backend);
    let failed = worker.execute(request(1, VmPostgresDriverOperation::Connect(config())));
    assert!(matches!(
        failed,
        VmPostgresDriverCompletion::Failed(ref error)
            if error.code == "postgres.connect"
                && !error.message.contains("alice")
                && !error.message.contains("secret")
    ));

    worker
        .apply_control(VmPostgresDriverControl::Cancel(RequestId { value: 2 }))
        .expect("queue cancellation");
    assert!(matches!(
        worker.execute(request(2, VmPostgresDriverOperation::Connect(config()))),
        VmPostgresDriverCompletion::Failed(ref error) if error.code == "postgres.cancelled"
    ));

    worker.backend.fail_connect.set(false);
    let driver_pool = match worker.execute(request(3, VmPostgresDriverOperation::Connect(config())))
    {
        VmPostgresDriverCompletion::Connected(pool) => pool,
        completion => panic!("expected pool completion, found {completion:?}"),
    };
    let parameter = json::Json::from_serde(serde_json::json!("parameter-secret"));
    let operation = VmPostgresDriverOperation::Query {
        target: VmPostgresQueryTarget::Pool(VmPostgresPool(10)),
        driver_target: VmPostgresDriverQueryTarget::Pool(driver_pool),
        sql: "SELECT $1".to_string(),
        parameters: vec![parameter],
        one: true,
    };
    let debug = format!("{operation:?}");
    assert!(debug.contains("parameter_count: 1"));
    assert!(!debug.contains("parameter-secret"));
    assert!(matches!(
        worker.execute(request(4, operation)),
        VmPostgresDriverCompletion::Rows { ref rows } if rows.len() == 1
    ));
    assert_eq!(
        *worker.backend.observed_parameters.borrow(),
        [serde_json::json!("parameter-secret")]
    );
    assert!(worker
        .apply_control(VmPostgresDriverControl::Release {
            connection: VmPostgresConnection(11),
            driver_connection: VmPostgresDriverConnection(999),
        })
        .is_err());
}

#[test]
fn worker_cleanup_controls_drop_every_native_resource_exactly_once() {
    let mut worker = VmPostgresDriverWorker::new(FixtureBackend::default());
    let driver_pool = match worker.execute(request(1, VmPostgresDriverOperation::Connect(config())))
    {
        VmPostgresDriverCompletion::Connected(pool) => pool,
        completion => panic!("expected connected completion, found {completion:?}"),
    };
    let driver_connection = match worker.execute(request(
        2,
        VmPostgresDriverOperation::Acquire {
            pool: VmPostgresPool(10),
            driver_pool,
        },
    )) {
        VmPostgresDriverCompletion::Acquired(connection) => connection,
        completion => panic!("expected acquired completion, found {completion:?}"),
    };
    let driver_transaction = match worker.execute(request(
        3,
        VmPostgresDriverOperation::Begin {
            connection: VmPostgresConnection(11),
            driver_connection,
        },
    )) {
        VmPostgresDriverCompletion::TransactionStarted(transaction) => transaction,
        completion => panic!("expected transaction completion, found {completion:?}"),
    };
    worker
        .apply_control(VmPostgresDriverControl::Rollback {
            transaction: VmPostgresTransaction(12),
            driver_transaction,
        })
        .expect("rollback");
    let duplicate_release = worker
        .apply_control(VmPostgresDriverControl::Release {
            connection: VmPostgresConnection(11),
            driver_connection,
        })
        .expect_err("rollback cleanup already destroyed the connection");
    assert_eq!(duplicate_release.code, "postgres.connection.stale");
    worker
        .apply_control(VmPostgresDriverControl::ClosePool {
            pool: VmPostgresPool(10),
            driver_pool,
        })
        .expect("close pool");
    assert!(worker.pools.is_empty());
    assert!(worker.connections.is_empty());
    assert!(worker.connection_pools.is_empty());
    assert!(worker.transactions.is_empty());
    assert!(worker
        .apply_control(VmPostgresDriverControl::ClosePool {
            pool: VmPostgresPool(10),
            driver_pool,
        })
        .is_err());
}

#[test]
fn failed_commit_retains_transaction_for_explicit_rollback() {
    let mut worker = VmPostgresDriverWorker::new(FixtureBackend::default());
    let driver_pool = match worker.execute(request(1, VmPostgresDriverOperation::Connect(config())))
    {
        VmPostgresDriverCompletion::Connected(pool) => pool,
        completion => panic!("expected connected completion, found {completion:?}"),
    };
    let driver_connection = match worker.execute(request(
        2,
        VmPostgresDriverOperation::Acquire {
            pool: VmPostgresPool(10),
            driver_pool,
        },
    )) {
        VmPostgresDriverCompletion::Acquired(connection) => connection,
        completion => panic!("expected acquired completion, found {completion:?}"),
    };
    let driver_transaction = match worker.execute(request(
        3,
        VmPostgresDriverOperation::Begin {
            connection: VmPostgresConnection(11),
            driver_connection,
        },
    )) {
        VmPostgresDriverCompletion::TransactionStarted(transaction) => transaction,
        completion => panic!("expected transaction completion, found {completion:?}"),
    };
    worker.backend.fail_commit.set(true);
    assert!(matches!(
        worker.execute(request(
            4,
            VmPostgresDriverOperation::Commit {
                transaction: VmPostgresTransaction(12),
                driver_transaction,
            },
        )),
        VmPostgresDriverCompletion::Failed(ref error)
            if error.code == "postgres.driver.transaction"
    ));
    assert!(worker.transactions.contains_key(&driver_transaction));
    worker.backend.fail_commit.set(false);
    worker
        .apply_control(VmPostgresDriverControl::Rollback {
            transaction: VmPostgresTransaction(12),
            driver_transaction,
        })
        .expect("rollback remains possible");
    assert!(worker.transactions.is_empty());
}

#[cfg(test)]
#[path = "worker_cleanup_isolation_test.rs"]
#[cfg(test)]
mod worker_cleanup_isolation_test;
