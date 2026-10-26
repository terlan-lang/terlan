use super::*;
use crate::commands::integration_test::manifest_and_arguments::parse_integration_args;
use crate::support::test_fs::TestDirectory;

#[test]
fn missing_migration_directory_does_not_run_database_commands() {
    let project = TestDirectory::new("integration", "absent_sql");
    let args = parse_integration_args(&[]).unwrap();
    run_with_executor(project.path(), &args, &BTreeMap::new(), |_| {
        panic!("absent SQL directory must not execute migrations")
    })
    .unwrap();
}

#[test]
fn migrations_use_the_native_database_command_and_selected_directory() {
    let project = TestDirectory::new("integration", "native_migrations");
    let selected = project.path().join("database schema");
    std::fs::create_dir_all(&selected).unwrap();
    let args = parse_integration_args(&["--migrations".into(), "database schema".into()]).unwrap();
    let env = BTreeMap::from([(
        "TERLAN_DATABASE_URL".into(),
        "postgresql://localhost/example".into(),
    )]);
    let mut executed = false;
    run_with_executor(project.path(), &args, &env, |command| {
        executed = true;
        assert_eq!(command.verb.as_deref(), Some("db"));
        assert_eq!(
            command.args,
            [
                "migrate",
                "--database-url",
                "postgresql://localhost/example",
                selected.to_str().unwrap(),
            ]
        );
        ExitCode::SUCCESS
    })
    .unwrap();
    assert!(executed);
}

#[test]
fn configured_missing_directories_and_files_fail_before_execution() {
    let project = TestDirectory::new("integration", "invalid_sql_directory");
    let args = parse_integration_args(&["--migrations".into(), "configured".into()]).unwrap();
    for is_file in [false, true] {
        if is_file {
            std::fs::write(project.path().join("configured"), "not a directory").unwrap();
        }
        assert!(
            run_with_executor(project.path(), &args, &BTreeMap::new(), |_| {
                panic!("invalid SQL directory must not execute migrations")
            })
            .is_err()
        );
    }
}

#[test]
fn migration_failures_fail_integration_without_disclosing_credentials() {
    let project = TestDirectory::new("integration", "failed_sql");
    std::fs::create_dir(project.path().join("sql")).unwrap();
    let args = parse_integration_args(&[]).unwrap();
    let env = BTreeMap::from([("POSTGRES_PASSWORD".into(), "private-secret".into())]);
    let failure = run_with_executor(project.path(), &args, &env, |_| ExitCode::from(1))
        .unwrap_err()
        .to_string();
    assert!(failure.contains("Terlan SQL migrations failed"));
    assert!(!failure.contains("private-secret"));
}

#[test]
fn invalid_targets_are_rejected_by_the_real_database_command() {
    let project = TestDirectory::new("integration", "invalid_sql_target");
    std::fs::create_dir(project.path().join("sql")).unwrap();
    let args = parse_integration_args(&[]).unwrap();
    let env = BTreeMap::from([("TERLAN_DATABASE_URL".into(), "invalid-target".into())]);
    assert!(
        crate::commands::integration_test::phase_execution::run_database_phase(
            project.path(),
            &args,
            &env
        )
        .is_err()
    );
}

#[test]
fn postgres_environment_values_are_encoded_as_url_components() {
    let env = BTreeMap::from([
        ("POSTGRES_HOST".into(), "[::1]".into()),
        ("POSTGRES_PORT".into(), "5433".into()),
        ("POSTGRES_USER".into(), "alice@work".into()),
        ("POSTGRES_PASSWORD".into(), "p@ss%2F word/secret".into()),
        ("POSTGRES_DB".into(), "app/main".into()),
    ]);
    let text = database_url(&env).unwrap();
    let url = url::Url::parse(&text).unwrap();
    assert_eq!(
        url.host(),
        Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
    );
    assert_eq!(url.port(), Some(5433));
    assert_eq!(url.username(), "alice%40work");
    assert_eq!(url.password(), Some("p%40ss%252F%20word%2Fsecret"));
    assert_eq!(url.path(), "/app%2Fmain");
    assert_eq!(url.query(), None);
}

#[test]
fn invalid_ports_fail_before_execution() {
    for value in ["0", "65536", "not-a-port"] {
        let env = BTreeMap::from([("POSTGRES_PORT".into(), value.into())]);
        assert!(database_url(&env).is_err(), "port {value}");
    }
}

#[test]
fn explicit_database_urls_follow_the_application_precedence() {
    let mut env = BTreeMap::from([
        (
            "TERLAN_DATABASE_URL".into(),
            "postgresql://localhost/terlan".into(),
        ),
        (
            "DATABASE_URL".into(),
            "postgresql://localhost/fallback".into(),
        ),
    ]);
    assert_eq!(database_url(&env).unwrap(), "postgresql://localhost/terlan");
    env.insert("TERLAN_DATABASE_URL".into(), String::new());
    assert_eq!(
        database_url(&env).unwrap(),
        "postgresql://localhost/fallback"
    );
}

#[test]
fn empty_url_variables_use_the_postgres_environment() {
    let env = BTreeMap::from([
        ("TERLAN_DATABASE_URL".into(), String::new()),
        ("DATABASE_URL".into(), "  ".into()),
        ("POSTGRES_DB".into(), "configured_database".into()),
    ]);
    let url = url::Url::parse(&database_url(&env).unwrap()).unwrap();
    assert_eq!(url.path(), "/configured_database");
}
