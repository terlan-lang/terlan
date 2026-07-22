use std::fs;
use std::process::ExitCode;

use super::run;
use super::test_support::{remove_dir, temp_db_dir};
use crate::runtime::vm::postgres::{VmPostgresDecodedValue, VmPostgresTransaction};
use crate::runtime::vm::postgres_command::VmPostgresCommandClient;
use crate::terlan_native::postgres;
use crate::CliCommand;

const LIVE_POSTGRES_SKIP_MESSAGE: &str =
    "skipping live Postgres migration lifecycle: TERLAN_TEST_POSTGRES_URL is not configured";
const CREATE_LIVE_USERS_SQL: &str = include_str!("testdata/20260619123000_create_live_users.sql");
const ADD_LIVE_USER_EMAIL_SQL: &str =
    include_str!("testdata/20260619123100_add_live_user_email.sql");
const FAIL_ATOMICALLY_SQL: &str = include_str!("testdata/20260619123200_fail_atomically.sql");

fn live_postgres_url_or_skip_message(
    url: Result<String, std::env::VarError>,
) -> Result<String, &'static str> {
    url.map_err(|_| LIVE_POSTGRES_SKIP_MESSAGE)
}

#[test]
fn live_postgres_url_reports_stable_skip_message_when_unconfigured() {
    let error = live_postgres_url_or_skip_message(Err(std::env::VarError::NotPresent))
        .expect_err("missing live database URL should skip");

    assert_eq!(error, LIVE_POSTGRES_SKIP_MESSAGE);
}

#[test]
fn run_db_migration_lifecycle_against_live_postgres_when_configured() {
    let database_url =
        match live_postgres_url_or_skip_message(std::env::var("TERLAN_TEST_POSTGRES_URL")) {
            Ok(url) => url,
            Err(message) => {
                println!("{message}");
                return;
            }
        };
    run_db_migration_lifecycle(&database_url);
}

#[test]
#[ignore = "requires a local Docker daemon"]
fn run_db_migration_and_snapshot_lifecycle_against_docker_postgres() {
    let fixture =
        crate::runtime::vm::postgres::libpq_worker::libpq_docker_gate_test::DockerPostgres::start()
            .expect("start Docker Postgres fixture");
    run_db_migration_lifecycle(&fixture.url("terlan"));
}

fn run_db_migration_lifecycle(database_url: &str) {
    let directory = temp_db_dir("run_db_migration_lifecycle_against_live_postgres_when_configured");
    fs::write(
        directory.join("20260619123000_create_live_users.sql"),
        CREATE_LIVE_USERS_SQL,
    )
    .expect("write first migration");

    assert_eq!(
        run_db_command("rebuild", database_url, &directory, true),
        ExitCode::SUCCESS
    );
    assert_eq!(
        run_db_command("status", database_url, &directory, false),
        ExitCode::SUCCESS
    );

    fs::write(
        directory.join("20260619123100_add_live_user_email.sql"),
        ADD_LIVE_USER_EMAIL_SQL,
    )
    .expect("write second migration");

    let config = postgres::Config::new(database_url.to_string());
    let mut lock_client =
        VmPostgresCommandClient::connect(&config).expect("connect migration lock owner");
    let lock_transaction = lock_client.begin().expect("begin migration lock owner");
    assert_eq!(
        acquire_test_migration_lock(&mut lock_client, lock_transaction),
        VmPostgresDecodedValue::Bool(true)
    );
    assert_eq!(
        run_db_command("migrate", database_url, &directory, false),
        ExitCode::from(1)
    );
    lock_client
        .finish_transaction(lock_transaction, false)
        .expect("release migration lock owner");

    assert_eq!(
        run_db_command("migrate", database_url, &directory, false),
        ExitCode::SUCCESS
    );
    prove_failed_migration_is_atomic(database_url, &config, &directory);
    assert_eq!(
        run_db_command("status", database_url, &directory, false),
        ExitCode::SUCCESS
    );

    run_schema_snapshot_lifecycle(database_url, &config, &directory);
    remove_dir(&directory);
}

fn prove_failed_migration_is_atomic(
    database_url: &str,
    config: &postgres::Config,
    directory: &std::path::Path,
) {
    let failing_migration = directory.join("20260619123200_fail_atomically.sql");
    fs::write(&failing_migration, FAIL_ATOMICALLY_SQL).expect("write failing migration");
    assert_eq!(
        run_db_command("migrate", database_url, directory, false),
        ExitCode::from(1)
    );

    let mut client =
        VmPostgresCommandClient::connect(config).expect("connect after failed migration");
    let transaction = client.begin().expect("begin after failed migration");
    assert_eq!(
        acquire_test_migration_lock(&mut client, transaction),
        VmPostgresDecodedValue::Bool(true)
    );
    client
        .finish_transaction(transaction, false)
        .expect("release post-failure migration lock");
    let relation_row = client
        .query_one(
            "SELECT to_regclass('public.incomplete_migration')::text AS relation;",
            Vec::new(),
        )
        .expect("query failed migration relation")
        .expect("failed migration relation row");
    assert_eq!(
        client
            .decode_dynamic(relation_row, "relation")
            .expect("decode failed migration relation"),
        VmPostgresDecodedValue::Null
    );
    fs::remove_file(failing_migration).expect("remove failing migration");
}

fn run_schema_snapshot_lifecycle(
    database_url: &str,
    config: &postgres::Config,
    directory: &std::path::Path,
) {
    let snapshot_path = directory.join("artifacts/schema.snapshot.json");
    let snapshot_args = vec![
        "snapshot".to_string(),
        "--database-url".to_string(),
        database_url.to_string(),
        "--output".to_string(),
        snapshot_path.display().to_string(),
        directory.display().to_string(),
    ];
    assert_eq!(run_cli(snapshot_args.clone()), ExitCode::SUCCESS);
    let mut check_args = snapshot_args.clone();
    check_args.insert(1, "--check".to_string());
    assert_eq!(run_cli(check_args), ExitCode::SUCCESS);

    let mut client = VmPostgresCommandClient::connect(config).expect("connect for schema drift");
    client
        .batch_execute("ALTER TABLE live_users ADD COLUMN drift_probe boolean;")
        .expect("introduce schema drift");
    let mut drift_args = snapshot_args;
    drift_args.insert(1, "--check".to_string());
    assert_eq!(run_cli(drift_args), ExitCode::from(1));
}

fn run_db_command(
    command: &str,
    database_url: &str,
    directory: &std::path::Path,
    development: bool,
) -> ExitCode {
    let mut args = vec![command.to_string()];
    if development {
        args.push("--dev".to_string());
        args.push("--confirm".to_string());
    }
    args.extend([
        "--database-url".to_string(),
        database_url.to_string(),
        directory.display().to_string(),
    ]);
    run_cli(args)
}

fn run_cli(args: Vec<String>) -> ExitCode {
    run(CliCommand {
        verb: Some("db".to_string()),
        args,
    })
}

fn acquire_test_migration_lock(
    client: &mut VmPostgresCommandClient,
    transaction: VmPostgresTransaction,
) -> VmPostgresDecodedValue {
    let row = client
        .query_one_transaction(
            transaction,
            super::execution::MIGRATION_LOCK_SQL,
            Vec::new(),
        )
        .expect("query migration lock")
        .expect("migration lock row");
    client
        .decode_dynamic(row, "acquired")
        .expect("decode migration lock")
}
