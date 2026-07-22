use super::*;

#[test]
fn command_client_rejects_invalid_config_before_worker_dispatch() {
    let error = VmPostgresCommandClient::connect(&postgres::Config::new("http://localhost/db"))
        .expect_err("invalid scheme must fail");

    assert_eq!(
        error,
        "error[postgres.invalid_url]: Postgres connection URL scheme `http` is not supported."
    );
}

#[test]
fn command_client_reports_unreachable_database_without_leaking_url() {
    let url = "postgres://secret:never-print@127.0.0.1:1/terlan";
    let mut client =
        VmPostgresCommandClient::connect(&postgres::Config::new(url).with_timeouts(100, 100))
            .expect("pool registration is lazy");

    let error = client
        .batch_execute("SELECT 1")
        .expect_err("unreachable database must fail");

    assert!(error.starts_with("error[postgres.connect]:"));
    assert!(!error.contains(url));
    assert!(!error.contains("never-print"));
}

#[test]
fn command_client_rejects_empty_batch_before_opening_a_socket() {
    let mut client = VmPostgresCommandClient::connect(
        &postgres::Config::new("postgres://localhost/terlan").with_timeouts(100, 100),
    )
    .expect("pool registration is lazy");

    let error = client
        .batch_execute("  \n")
        .expect_err("empty batch must fail validation");

    assert_eq!(
        error,
        "error[postgres.sql.empty]: Postgres SQL text must not be empty"
    );
}
