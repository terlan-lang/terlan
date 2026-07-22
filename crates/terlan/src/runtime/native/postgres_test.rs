use super::json as json_adapter;
use super::postgres::test_support::disconnected_pool;
use super::postgres::*;

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
    disconnected_pool("postgres://127.0.0.1:1/terlan")
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
    assert_eq!(error.code(), "postgres.vm_driver_unavailable");
    assert!(error.message().contains("VM-owned Postgres I/O driver"));

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
///   VM-owned pool config.
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
/// - Locks the minimum NativeBoundary connection identity contract before a live
///   maintained client gets a chance to interpret adapter-specific defaults.
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

/// Verifies compatibility operations require the VM-owned driver.
///
/// Inputs:
/// - Pool fixture, SQL text, and empty JSON parameter list.
///
/// Output:
/// - Test passes when query, query_one, and execute all return the stable
///   VM-driver-unavailable code without acquiring sockets.
///
/// Transformation:
/// - Locks the backend-neutral boundary while live VM I/O remains unfinished.
#[test]
fn query_operations_return_stable_vm_driver_unavailable_error() {
    let pool = pool_fixture();
    let params = Vec::new();

    assert_eq!(
        query(&pool, "SELECT 1", &params)
            .expect_err("query unavailable")
            .code(),
        "postgres.vm_driver_unavailable"
    );
    assert_eq!(
        query_one(&pool, "SELECT 1 LIMIT 1", &params)
            .expect_err("query_one unavailable")
            .code(),
        "postgres.vm_driver_unavailable"
    );
    assert_eq!(
        execute(&pool, "CREATE TABLE users(id BIGINT)", &params)
            .expect_err("execute unavailable")
            .code(),
        "postgres.vm_driver_unavailable"
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
///   or semantic validation into the NativeBoundary proof-track adapter.
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

    assert_eq!(error.code(), "postgres.vm_driver_unavailable");
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

/// Verifies driver-owned dynamic decoding preserves concrete values and null.
#[test]
fn row_dynamic_decode_preserves_concrete_types_and_null() {
    let mut row = Row::new();
    row.put_string("name", "Ada");
    row.put_int("age", 42);
    row.put_bool("active", true);
    row.put_json("meta", json_adapter::string("ok"));
    row.put_libpq_text("nickname", 25, None)
        .expect("store null libpq value");

    assert_eq!(value(&row, "name"), Ok(DecodedValue::String("Ada".into())));
    assert_eq!(value(&row, "age"), Ok(DecodedValue::Int(42)));
    assert_eq!(value(&row, "active"), Ok(DecodedValue::Bool(true)));
    assert_eq!(
        value(&row, "meta"),
        Ok(DecodedValue::Json(json_adapter::string("ok")))
    );
    assert_eq!(value(&row, "nickname"), Ok(DecodedValue::Null));
    assert_eq!(
        value(&row, "missing")
            .expect_err("missing dynamic column")
            .code(),
        "postgres.row.missing_column"
    );
}

/// Verifies row column names stay text even when they look like atom builders.
///
/// Inputs:
/// - Row fixture with string, enum-like text, and JSON columns named after
///   unsafe Vm atom-construction functions.
///
/// Output:
/// - Test passes when every accessor uses the literal column name.
///
/// Transformation:
/// - Locks the Postgres row boundary to string-keyed decoding so database text
///   and enum labels cannot create runtime atoms.
#[test]
fn row_accessors_keep_dynamic_column_and_enum_text_as_strings() {
    let mut row = Row::new();
    row.put_string("binary_to_atom", "pending");
    row.put_string("list_to_atom", "ready");
    row.put_json(
        "meta",
        json_adapter::Json::from_serde(serde_json::json!({
            "binary_to_atom": "json-key",
            "status": "ok"
        })),
    );

    assert_eq!(string(&row, "binary_to_atom"), Ok("pending".to_string()));
    assert_eq!(string(&row, "list_to_atom"), Ok("ready".to_string()));
    assert_eq!(
        json(&row, "meta"),
        Ok(json_adapter::Json::from_serde(serde_json::json!({
            "binary_to_atom": "json-key",
            "status": "ok"
        })))
    );
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
