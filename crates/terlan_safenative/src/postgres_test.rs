use super::*;

/// Builds a pool fixture without opening a database connection.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Pool value with a stable URL.
///
/// Transformation:
/// - Constructs the private adapter shape directly inside the adjacent test
///   module so operation functions can be referenced before live connection
///   setup exists.
fn pool_fixture() -> Pool {
    Pool::disconnected("postgres://127.0.0.1:1/terlan")
}

/// Reads the optional live Postgres test URL.
///
/// Inputs:
/// - `test_name`: name used in the skip diagnostic.
///
/// Output:
/// - `Some(url)` when `TERLAN_TEST_POSTGRES_URL` is configured.
/// - `None` when live database tests should be skipped.
///
/// Transformation:
/// - Reads only the process environment and emits a human-readable skip line
///   for ordinary non-Docker unit test runs.
fn live_postgres_url(test_name: &str) -> Option<String> {
    match std::env::var("TERLAN_TEST_POSTGRES_URL") {
        Ok(url) => Some(url),
        Err(_) => {
            eprintln!("skipping {test_name}; TERLAN_TEST_POSTGRES_URL is not set");
            None
        }
    }
}

/// Builds a unique table name for one live Postgres test.
///
/// Inputs:
/// - `prefix`: descriptive table-name prefix.
///
/// Output:
/// - Table name unique enough for concurrent test processes.
///
/// Transformation:
/// - Combines the prefix with the current process id so Docker-backed tests can
///   create regular tables without colliding with each other.
fn live_table(prefix: &str) -> String {
    format!("{prefix}_{}", std::process::id())
}

/// Verifies config preserves URLs and uses stable connection diagnostics.
///
/// Inputs:
/// - Valid and invalid Postgres config URLs.
///
/// Output:
/// - Test passes when the valid but unreachable URL reaches the stable
///   maintained-driver connection boundary and the invalid URL is rejected
///   earlier.
///
/// Transformation:
/// - Exercises `connect` without requiring a live database.
#[test]
fn connect_validates_url_before_driver_connection_error() {
    let config = Config::new("postgres://127.0.0.1:1/terlan");
    assert_eq!(config.url(), "postgres://127.0.0.1:1/terlan");

    let error = connect(&config).expect_err("unreachable database should fail");
    assert_eq!(error.code(), "postgres.connect");
    assert!(error.message().contains("Postgres pool error"));

    let invalid = Config::new("mysql://localhost/terlan");
    let error = connect(&invalid).expect_err("unsupported scheme should fail");
    assert_eq!(error.code(), "postgres.invalid_url");
}

/// Verifies Postgres config defaults are conservative and explicit.
///
/// Inputs:
/// - Default config built from a URL.
///
/// Output:
/// - Test passes when URL, pool limits, and timeout defaults are stable.
///
/// Transformation:
/// - Reads config fields without opening sockets.
#[test]
fn config_defaults_are_stable() {
    let config = Config::new("postgres://localhost/terlan");

    assert_eq!(config.url(), "postgres://localhost/terlan");
    assert_eq!(config.min_connections(), 1);
    assert_eq!(config.max_connections(), 16);
    assert_eq!(config.wait_timeout_ms(), 5_000);
    assert_eq!(config.connect_timeout_ms(), 5_000);
}

/// Verifies Postgres config builder methods update pool settings.
///
/// Inputs:
/// - Config with explicit pool limits and timeouts.
///
/// Output:
/// - Test passes when the builder methods preserve URL and update only the
///   requested fields.
///
/// Transformation:
/// - Exercises the Terlan-facing config surface before it is lowered into the
///   maintained deadpool config.
#[test]
fn config_builders_set_pool_limits_and_timeouts() {
    let config = Config::new("postgres://localhost/terlan")
        .with_pool_limits(2, 8)
        .with_timeouts(250, 750);

    assert_eq!(config.url(), "postgres://localhost/terlan");
    assert_eq!(config.min_connections(), 2);
    assert_eq!(config.max_connections(), 8);
    assert_eq!(config.wait_timeout_ms(), 250);
    assert_eq!(config.connect_timeout_ms(), 750);
}

/// Verifies config validation is available without opening an adapter.
///
/// Inputs:
/// - Supported and unsupported database URLs.
///
/// Output:
/// - Test passes when supported Postgres schemes validate and unsupported
///   schemes return the stable invalid-url code.
///
/// Transformation:
/// - Exercises the config-only validation boundary used by CLI command parsing
///   before live migration execution is wired.
#[test]
fn validate_config_checks_url_scheme_without_opening_sockets() {
    assert_eq!(
        validate_config(&Config::new("postgresql://localhost/terlan")),
        Ok(())
    );

    let error = validate_config(&Config::new("sqlite://local.db")).expect_err("unsupported scheme");
    assert_eq!(error.code(), "postgres.invalid_url");
}

/// Verifies config validation rejects incomplete Postgres connection identity.
///
/// Inputs:
/// - Postgres URLs missing a host or database name.
///
/// Output:
/// - Test passes when each incomplete URL returns the stable invalid-url code.
///
/// Transformation:
/// - Locks the minimum SafeNative connection identity contract before a live
///   Rust/Tokio client gets a chance to interpret adapter-specific defaults.
#[test]
fn validate_config_requires_host_and_database_name() {
    let missing_host =
        validate_config(&Config::new("postgres:///terlan")).expect_err("host is required");
    assert_eq!(missing_host.code(), "postgres.invalid_url");
    assert!(missing_host.message().contains("host"));

    let missing_database = validate_config(&Config::new("postgres://localhost"))
        .expect_err("database name is required");
    assert_eq!(missing_database.code(), "postgres.invalid_url");
    assert!(missing_database.message().contains("database name"));
}

/// Verifies invalid URL diagnostics do not leak credentials.
///
/// Inputs:
/// - Credential-bearing Postgres URL with an incomplete database identity.
///
/// Output:
/// - Test passes when the stable diagnostic omits the original password.
///
/// Transformation:
/// - Exercises the config-only validation boundary with a secret-bearing URL
///   so future live adapter diagnostics keep the same redaction behavior.
#[test]
fn validate_config_does_not_echo_passwords_in_errors() {
    let error = validate_config(&Config::new("postgres://user:secret@localhost"))
        .expect_err("missing database should fail");

    assert_eq!(error.code(), "postgres.invalid_url");
    assert!(!error.message().contains("secret"));
    assert!(!error.message().contains("user:secret"));
}

/// Verifies pool config validation rejects unusable limits.
///
/// Inputs:
/// - Postgres configs with invalid pool sizes and timeouts.
///
/// Output:
/// - Test passes when each invalid config returns the stable pool config code.
///
/// Transformation:
/// - Validates Terlan-facing settings before any maintained pool resource is
///   created.
#[test]
fn validate_config_rejects_invalid_pool_settings() {
    for config in [
        Config::new("postgres://localhost/terlan").with_pool_limits(0, 1),
        Config::new("postgres://localhost/terlan").with_pool_limits(2, 1),
        Config::new("postgres://localhost/terlan").with_pool_limits(1, 0),
        Config::new("postgres://localhost/terlan").with_timeouts(0, 1),
        Config::new("postgres://localhost/terlan").with_timeouts(1, 0),
    ] {
        assert_eq!(
            validate_config(&config)
                .expect_err("invalid pool setting should fail")
                .code(),
            "postgres.pool.config"
        );
    }
}

/// Verifies query operations expose stable maintained-driver errors.
///
/// Inputs:
/// - Pool fixture, SQL text, and empty JSON parameter list.
///
/// Output:
/// - Test passes when `query`, `query_one`, and `execute` all return the same
///   stable connection error before a live database is configured.
///
/// Transformation:
/// - Locks the operation surface through the maintained Rust/Tokio client path
///   without requiring a live database for ordinary unit tests.
#[test]
fn query_operations_return_stable_driver_connection_error() {
    let pool = pool_fixture();
    let params = Vec::new();

    assert_eq!(
        query(&pool, "SELECT 1", &params)
            .expect_err("query unavailable")
            .code(),
        "postgres.connect"
    );
    assert_eq!(
        query_one(&pool, "SELECT 1 LIMIT 1", &params)
            .expect_err("query_one unavailable")
            .code(),
        "postgres.connect"
    );
    assert_eq!(
        execute(&pool, "CREATE TABLE users(id BIGINT)", &params)
            .expect_err("execute unavailable")
            .code(),
        "postgres.connect"
    );
}

/// Verifies query operations reject empty SQL before adapter dispatch.
///
/// Inputs:
/// - Pool fixture, whitespace SQL text, and empty JSON parameter list.
///
/// Output:
/// - Test passes when `query`, `query_one`, and `execute` all return the
///   stable empty-SQL error before the unavailable-adapter boundary.
///
/// Transformation:
/// - Locks a minimal transport-boundary guard without introducing SQL parsing
///   or semantic validation into the SafeNative proof-track adapter.
#[test]
fn query_operations_reject_empty_sql_before_adapter_dispatch() {
    let pool = pool_fixture();
    let params = Vec::new();

    assert_eq!(
        query(&pool, "   ", &params)
            .expect_err("empty query")
            .code(),
        "postgres.sql.empty"
    );
    assert_eq!(
        query_one(&pool, "\n\t", &params)
            .expect_err("empty query_one")
            .code(),
        "postgres.sql.empty"
    );
    assert_eq!(
        execute(&pool, "", &params)
            .expect_err("empty execute")
            .code(),
        "postgres.sql.empty"
    );
}

/// Verifies transaction preserves the callback-shaped API boundary.
///
/// Inputs:
/// - Pool fixture and transaction callback.
///
/// Output:
/// - Test passes when `transaction` returns a stable connection error without
///   a live database.
///
/// Transformation:
/// - References the transaction operation through the maintained client path
///   without fabricating commit/rollback behavior.
#[test]
fn transaction_returns_stable_driver_connection_error() {
    let pool = pool_fixture();

    let error = transaction(&pool, |_connection| Ok(7)).expect_err("transaction unavailable");

    assert_eq!(error.code(), "postgres.connect");
}

/// Verifies live Postgres query execution when a test database is configured.
///
/// Inputs:
/// - `TERLAN_TEST_POSTGRES_URL`: optional Postgres URL supplied by Docker gate.
///
/// Output:
/// - Test is skipped when no URL is configured.
/// - Test passes when connect/query/query_one/execute/transaction all execute
///   through the maintained Rust/Tokio Postgres client.
///
/// Transformation:
/// - Converts the Docker-provided URL into a live SafeNative pool profile and
///   verifies typed row decoding for string, integer, boolean, and JSON values.
#[test]
fn live_postgres_query_execute_and_transaction_roundtrip_when_configured() {
    let Some(url) = live_postgres_url("live Postgres adapter roundtrip test") else {
        return;
    };
    let pool = connect(
        &Config::new(url)
            .with_pool_limits(2, 4)
            .with_timeouts(1_000, 1_000),
    )
    .expect("live Postgres connect should succeed");
    let status = pool
        .inner
        .as_ref()
        .map(deadpool_postgres::Pool::status)
        .expect("live pool should expose status");
    assert_eq!(status.max_size, 4);
    assert!(status.size >= 2);
    assert!(status.available >= 2);
    let params = Vec::new();
    let table = live_table("terlan_live_check");

    execute(&pool, &format!("DROP TABLE IF EXISTS {table}"), &params)
        .expect("test table cleanup should succeed");
    execute(
        &pool,
        &format!("CREATE TABLE {table}(id BIGINT, name TEXT, active BOOL, meta JSONB)"),
        &params,
    )
    .expect("temp table creation should succeed");
    let affected = execute(
        &pool,
        &format!("INSERT INTO {table}(id, name, active, meta) VALUES (1, 'Ada', true, '{{\"ok\":true}}'::jsonb)"),
        &params,
    )
    .expect("insert should succeed");
    assert_eq!(affected, 1);

    let rows = query(
        &pool,
        &format!("SELECT id, name, active, meta FROM {table} ORDER BY id"),
        &params,
    )
    .expect("query should succeed");
    assert_eq!(rows.len(), 1);
    assert_eq!(int(&rows[0], "id"), Ok(1));
    assert_eq!(string(&rows[0], "name"), Ok(String::from("Ada")));
    assert_eq!(r#bool(&rows[0], "active"), Ok(true));
    assert_eq!(
        json(&rows[0], "meta"),
        Ok(json_adapter::Json::from_serde(
            serde_json::json!({"ok": true})
        ))
    );

    let row = query_one(
        &pool,
        &format!("SELECT id, name, active, meta FROM {table} LIMIT 1"),
        &params,
    )
    .expect("query_one should succeed")
    .expect("query_one should return one row");
    assert_eq!(string(&row, "name"), Ok(String::from("Ada")));

    let value = transaction(&pool, |_connection| Ok(42)).expect("transaction should commit");
    assert_eq!(value, 42);

    execute(&pool, &format!("DROP TABLE IF EXISTS {table}"), &params)
        .expect("test table cleanup should succeed");
}

/// Verifies live parameter binding and single-row absence handling.
///
/// Inputs:
/// - `TERLAN_TEST_POSTGRES_URL`: optional Postgres URL supplied by Docker gate.
///
/// Output:
/// - Test is skipped when no URL is configured.
/// - Test passes when scalar and JSON parameters bind through
///   `tokio-postgres`, and `query_one` returns `None` for empty result sets.
///
/// Transformation:
/// - Inserts a row using positional parameters instead of interpolated SQL,
///   then decodes the row through Terlan-facing accessors.
#[test]
fn live_postgres_binds_params_and_returns_none_for_missing_query_one() {
    let Some(url) = live_postgres_url("live Postgres parameter binding test") else {
        return;
    };
    let pool = connect(&Config::new(url)).expect("live Postgres connect should succeed");
    let table = live_table("terlan_live_params");
    let no_params = Vec::new();

    execute(&pool, &format!("DROP TABLE IF EXISTS {table}"), &no_params)
        .expect("test table cleanup should succeed");
    execute(
        &pool,
        &format!("CREATE TABLE {table}(id BIGINT, name TEXT, active BOOL, meta JSONB)"),
        &no_params,
    )
    .expect("test table creation should succeed");

    let params = vec![
        json_adapter::int(7),
        json_adapter::string("Grace"),
        json_adapter::r#bool(false),
        json_adapter::Json::from_serde(serde_json::json!({"source": "param"})),
    ];
    let affected = execute(
        &pool,
        &format!("INSERT INTO {table}(id, name, active, meta) VALUES ($1, $2, $3, $4::jsonb)"),
        &params,
    )
    .expect("parameterized insert should succeed");
    assert_eq!(affected, 1);

    let row = query_one(
        &pool,
        &format!("SELECT id, name, active, meta FROM {table} WHERE id = $1"),
        &[json_adapter::int(7)],
    )
    .expect("parameterized query_one should succeed")
    .expect("inserted row should be present");
    assert_eq!(int(&row, "id"), Ok(7));
    assert_eq!(string(&row, "name"), Ok(String::from("Grace")));
    assert_eq!(r#bool(&row, "active"), Ok(false));
    assert_eq!(
        json(&row, "meta"),
        Ok(json_adapter::Json::from_serde(
            serde_json::json!({"source": "param"})
        ))
    );

    let missing = query_one(
        &pool,
        &format!("SELECT id FROM {table} WHERE id = $1"),
        &[json_adapter::int(999)],
    )
    .expect("missing query_one should succeed");
    assert_eq!(missing, None);

    execute(&pool, &format!("DROP TABLE IF EXISTS {table}"), &no_params)
        .expect("test table cleanup should succeed");
}

/// Verifies transaction rollback errors do not poison the pool.
///
/// Inputs:
/// - `TERLAN_TEST_POSTGRES_URL`: optional Postgres URL supplied by Docker gate.
///
/// Output:
/// - Test is skipped when no URL is configured.
/// - Test passes when a transaction callback error is returned unchanged and
///   the pool can serve a later query.
///
/// Transformation:
/// - Forces the callback error branch and then checks out the pool through a
///   normal query to prove rollback cleanup returned the connection.
#[test]
fn live_postgres_transaction_callback_error_rolls_back_and_pool_remains_usable() {
    let Some(url) = live_postgres_url("live Postgres transaction rollback test") else {
        return;
    };
    let pool = connect(&Config::new(url)).expect("live Postgres connect should succeed");

    let error = transaction::<i64>(&pool, |_connection| {
        Err(PostgresError::new(
            "postgres.test.rollback",
            "forced rollback",
        ))
    })
    .expect_err("forced transaction error should be returned");
    assert_eq!(error.code(), "postgres.test.rollback");

    let row = query_one(&pool, "SELECT 1::BIGINT AS value", &[])
        .expect("pool should remain usable after rollback")
        .expect("query should return one row");
    assert_eq!(int(&row, "value"), Ok(1));
}

/// Verifies pool wait timeouts are surfaced with stable diagnostics.
///
/// Inputs:
/// - `TERLAN_TEST_POSTGRES_URL`: optional Postgres URL supplied by Docker gate.
///
/// Output:
/// - Test is skipped when no URL is configured.
/// - Test passes when a max-size-one pool times out while its only client is
///   held by another checkout.
///
/// Transformation:
/// - Uses the maintained deadpool checkout path directly inside the adjacent
///   test module to prove Terlan maps pool exhaustion to `postgres.connect`.
#[test]
fn live_postgres_pool_wait_timeout_is_stable_when_exhausted() {
    let Some(url) = live_postgres_url("live Postgres pool timeout test") else {
        return;
    };
    let pool = connect(
        &Config::new(url)
            .with_pool_limits(1, 1)
            .with_timeouts(1, 1_000),
    )
    .expect("live Postgres connect should succeed");

    let runtime = runtime().expect("test runtime should start");
    runtime.block_on(async {
        let inner = pool
            .inner
            .as_ref()
            .expect("live pool should own native pool");
        let held_client = inner.get().await.expect("first checkout should succeed");
        let error = pool_client(&pool)
            .await
            .expect_err("second checkout should timeout");
        assert_eq!(error.code(), "postgres.connect");
        assert!(error
            .message()
            .contains("Timeout occurred while waiting for a slot"));
        drop(held_client);
    });
}

/// Verifies unsupported live row column types return stable row errors.
///
/// Inputs:
/// - `TERLAN_TEST_POSTGRES_URL`: optional Postgres URL supplied by Docker gate.
///
/// Output:
/// - Test is skipped when no URL is configured.
/// - Test passes when a live query returning an unsupported float column uses
///   the stable unsupported-type code.
///
/// Transformation:
/// - Lets Postgres produce a real row type that the current Terlan row surface
///   intentionally does not expose yet.
#[test]
fn live_postgres_unsupported_row_type_returns_stable_error() {
    let Some(url) = live_postgres_url("live Postgres unsupported row type test") else {
        return;
    };
    let pool = connect(&Config::new(url)).expect("live Postgres connect should succeed");

    let error = query(&pool, "SELECT 1.5::DOUBLE PRECISION AS value", &[])
        .expect_err("unsupported row type should fail");
    assert_eq!(error.code(), "postgres.row.unsupported_type");
    assert!(error.message().contains("value"));
}

/// Verifies row typed accessors decode matching column values.
///
/// Inputs:
/// - Row fixture with string, integer, boolean, and JSON columns.
///
/// Output:
/// - Test passes when each accessor returns the expected typed value.
///
/// Transformation:
/// - Exercises the dependency-light row decoding contract before live database
///   rows are wired into the adapter.
#[test]
fn row_accessors_decode_matching_values() {
    let mut row = Row::new();
    row.put_string("name", "Ada");
    row.put_int("age", 42);
    row.put_bool("active", true);
    row.put_json("meta", json_adapter::string("ok"));

    assert_eq!(string(&row, "name"), Ok("Ada".to_string()));
    assert_eq!(int(&row, "age"), Ok(42));
    assert_eq!(r#bool(&row, "active"), Ok(true));
    assert_eq!(json(&row, "meta"), Ok(json_adapter::string("ok")));
}

/// Verifies row typed accessors reject missing and mismatched columns.
///
/// Inputs:
/// - Row fixture with one integer column.
///
/// Output:
/// - Test passes when missing and type-mismatched lookups return stable error
///   codes.
///
/// Transformation:
/// - Locks row decoding diagnostics independently from any future database
///   driver error format.
#[test]
fn row_accessors_report_missing_and_type_errors() {
    let mut row = Row::new();
    row.put_int("age", 42);

    assert_eq!(
        string(&row, "missing").expect_err("missing column").code(),
        "postgres.row.missing_column"
    );
    assert_eq!(
        string(&row, "age").expect_err("type mismatch").code(),
        "postgres.row.type"
    );
}
