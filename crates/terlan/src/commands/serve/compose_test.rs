use super::*;
use crate::support::test_fs;
use std::fs;
use std::path::PathBuf;

/// Creates a unique temporary Compose test directory.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path to a not-yet-existing temp directory.
///
/// Transformation:
/// - Delegates to the shared test filesystem helper with the serve-compose
///   namespace.
fn temp_dir(name: &str) -> PathBuf {
    test_fs::temp_path("serve_compose", name)
}

/// Writes one Docker Compose fixture.
///
/// Inputs:
/// - `dir`: directory where `docker-compose.yml` should be written.
/// - `body`: Docker Compose YAML body.
///
/// Output:
/// - Path to the written Compose file.
///
/// Transformation:
/// - Creates the fixture directory and writes exactly one Compose file.
fn write_compose(dir: &PathBuf, body: &str) -> PathBuf {
    fs::create_dir_all(dir).expect("create compose dir");
    let path = dir.join("docker-compose.yml");
    fs::write(&path, body).expect("write compose");
    path
}

/// Validates the generated web-profile Postgres Compose shape.
///
/// Inputs:
/// - Compose YAML matching `terlc init --profile web`.
///
/// Output:
/// - Test passes when the typed Compose parser and Terlan strict validation
///   accept the service.
///
/// Transformation:
/// - Exercises the same project-root Compose validation path used by
///   `terlc serve --check`.
#[test]
fn validate_project_compose_accepts_postgres_dev_service() {
    let dir = temp_dir("valid_postgres");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: terlan_dev
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: terlan
    ports:
      - "127.0.0.1:5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U terlan -d terlan_dev"]
      interval: 1s
      timeout: 5s
      retries: 30
"#,
    );

    validate_project_compose(&dir).expect("valid Postgres compose");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates missing Postgres services are rejected.
///
/// Inputs:
/// - Compose YAML with a non-Postgres service.
///
/// Output:
/// - Test passes when validation returns a stable missing-service diagnostic.
///
/// Transformation:
/// - Confirms Terlan validates required dev services instead of treating any
///   syntactically valid Compose file as sufficient.
#[test]
fn validate_project_compose_rejects_missing_postgres_service() {
    let dir = temp_dir("missing_postgres");
    write_compose(
        &dir,
        r#"services:
  redis:
    image: redis:7-alpine
"#,
    );

    let error = validate_project_compose(&dir).expect_err("missing postgres should fail");

    assert!(error.contains("must define service `postgres`"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates missing Postgres healthchecks are rejected.
///
/// Inputs:
/// - Compose YAML with a Postgres service but no healthcheck.
///
/// Output:
/// - Test passes when validation returns a stable healthcheck diagnostic.
///
/// Transformation:
/// - Locks the readiness-signal requirement needed by future `terlc serve`
///   dependency validation.
#[test]
fn validate_project_compose_rejects_postgres_without_healthcheck() {
    let dir = temp_dir("missing_healthcheck");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: terlan_dev
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: terlan
    ports:
      - "127.0.0.1:5432:5432"
"#,
    );

    let error = validate_project_compose(&dir).expect_err("missing healthcheck should fail");

    assert!(error.contains("must define a healthcheck"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates broad Postgres host bindings are rejected.
///
/// Inputs:
/// - Compose YAML that publishes Postgres as `5432:5432`.
///
/// Output:
/// - Test passes when validation requires an explicit loopback host binding.
///
/// Transformation:
/// - Locks the web-profile dev dependency rule that local Postgres should not
///   be exposed on every host interface by default.
#[test]
fn validate_project_compose_rejects_public_postgres_port_binding() {
    let dir = temp_dir("public_postgres_port");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: terlan_dev
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: terlan
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U terlan -d terlan_dev"]
"#,
    );

    let error = validate_project_compose(&dir).expect_err("public binding should fail");

    assert!(error.contains("must publish container port 5432 on loopback"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates long-form loopback port syntax is accepted.
///
/// Inputs:
/// - Compose YAML using long-form `ports` entries with `host_ip`.
///
/// Output:
/// - Test passes when the typed Compose model validates the port contract.
///
/// Transformation:
/// - Confirms Terlan's strict check is not tied to the scaffold's short syntax
///   and still relies on the typed `docker-compose-types` port model.
#[test]
fn validate_project_compose_accepts_long_loopback_postgres_port() {
    let dir = temp_dir("long_loopback_postgres_port");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: terlan_dev
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: terlan
    ports:
      - target: 5432
        published: 5432
        host_ip: 127.0.0.1
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U terlan -d terlan_dev"]
"#,
    );

    validate_project_compose(&dir).expect("long-form loopback port");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates list-form Postgres environment syntax is accepted.
///
/// Inputs:
/// - Compose YAML using `KEY=value` environment list entries.
///
/// Output:
/// - Test passes when the typed Compose model and Terlan validator accept the
///   required Postgres environment keys.
///
/// Transformation:
/// - Locks support for Compose's list environment form without weakening the
///   required `POSTGRES_*` key contract.
#[test]
fn validate_project_compose_accepts_list_form_postgres_environment() {
    let dir = temp_dir("list_form_environment");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      - POSTGRES_DB=terlan_dev
      - POSTGRES_USER=terlan
      - POSTGRES_PASSWORD=terlan
    ports:
      - "localhost:5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U terlan -d terlan_dev"]
"#,
    );

    validate_project_compose(&dir).expect("list-form environment");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates empty map-form Postgres environment values are rejected.
///
/// Inputs:
/// - Compose YAML with a blank `POSTGRES_PASSWORD` value.
///
/// Output:
/// - Test passes when validation rejects the empty required value.
///
/// Transformation:
/// - Keeps the web-profile database contract useful before Docker startup by
///   rejecting configuration that cannot identify the generated dev database.
#[test]
fn validate_project_compose_rejects_empty_map_form_postgres_environment() {
    let dir = temp_dir("empty_map_form_environment");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: terlan_dev
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: ""
    ports:
      - "127.0.0.1:5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U terlan -d terlan_dev"]
"#,
    );

    let error = validate_project_compose(&dir).expect_err("empty password should fail");

    assert!(error.contains("must set non-empty `POSTGRES_PASSWORD`"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates empty list-form Postgres environment values are rejected.
///
/// Inputs:
/// - Compose YAML with a `KEY=` environment list entry.
///
/// Output:
/// - Test passes when validation rejects the empty required value.
///
/// Transformation:
/// - Applies the same non-empty Postgres contract to Compose's list
///   environment syntax.
#[test]
fn validate_project_compose_rejects_empty_list_form_postgres_environment() {
    let dir = temp_dir("empty_list_form_environment");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      - POSTGRES_DB=terlan_dev
      - POSTGRES_USER=terlan
      - POSTGRES_PASSWORD=
    ports:
      - "127.0.0.1:5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U terlan -d terlan_dev"]
"#,
    );

    let error = validate_project_compose(&dir).expect_err("empty password should fail");

    assert!(error.contains("must set non-empty `POSTGRES_PASSWORD`"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates disabled Postgres healthchecks are rejected.
///
/// Inputs:
/// - Compose YAML with a Postgres service and explicitly disabled healthcheck.
///
/// Output:
/// - Test passes when validation reports the stable disabled-healthcheck
///   diagnostic.
///
/// Transformation:
/// - Keeps future dependency startup from accepting services that cannot expose
///   a readiness signal to `terlc serve`.
#[test]
fn validate_project_compose_rejects_disabled_postgres_healthcheck() {
    let dir = temp_dir("disabled_healthcheck");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: terlan_dev
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: terlan
    ports:
      - "127.0.0.1:5432:5432"
    healthcheck:
      disable: true
"#,
    );

    let error = validate_project_compose(&dir).expect_err("disabled healthcheck should fail");

    assert!(error.contains("healthcheck must not be disabled"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates commandless Postgres healthchecks are rejected.
///
/// Inputs:
/// - Compose YAML with healthcheck timing fields but no `test`.
///
/// Output:
/// - Test passes when validation reports a missing healthcheck command.
///
/// Transformation:
/// - Ensures readiness metadata can actually be waited on before `terlc serve`
///   starts depending on the generated Postgres service.
#[test]
fn validate_project_compose_rejects_postgres_healthcheck_without_test() {
    let dir = temp_dir("healthcheck_without_test");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: terlan_dev
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: terlan
    ports:
      - "127.0.0.1:5432:5432"
    healthcheck:
      interval: 1s
      timeout: 5s
      retries: 30
"#,
    );

    let error = validate_project_compose(&dir).expect_err("commandless healthcheck should fail");

    assert!(error.contains("healthcheck must define a non-empty test command"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates Compose's disabled `NONE` healthcheck marker is rejected.
///
/// Inputs:
/// - Compose YAML with `healthcheck.test` set to `NONE`.
///
/// Output:
/// - Test passes when validation reports a missing enabled healthcheck command.
///
/// Transformation:
/// - Rejects an alternate disabled-healthcheck spelling in addition to
///   `disable: true`.
#[test]
fn validate_project_compose_rejects_postgres_healthcheck_none_test() {
    let dir = temp_dir("healthcheck_none_test");
    write_compose(
        &dir,
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: terlan_dev
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: terlan
    ports:
      - "127.0.0.1:5432:5432"
    healthcheck:
      test: ["NONE"]
"#,
    );

    let error = validate_project_compose(&dir).expect_err("NONE healthcheck should fail");

    assert!(error.contains("healthcheck must define a non-empty test command"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Validates malformed Compose files report parser diagnostics.
///
/// Inputs:
/// - Invalid Docker Compose YAML text.
///
/// Output:
/// - Test passes when validation reports a malformed Compose diagnostic.
///
/// Transformation:
/// - Confirms YAML parsing remains owned by the Rust Compose/YAML stack while
///   Terlan wraps the error in a serve-package diagnostic.
#[test]
fn validate_project_compose_rejects_malformed_yaml() {
    let dir = temp_dir("malformed");
    write_compose(&dir, "services:\n  postgres: [");

    let error = validate_project_compose(&dir).expect_err("malformed compose should fail");

    assert!(error.contains("malformed Docker Compose file"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies Docker startup is deliberately constrained to Postgres.
///
/// Inputs:
/// - A project-owned Compose file path.
///
/// Output:
/// - Test passes when the rendered command is exactly
///   `docker compose -f <file> up -d --no-recreate --wait --wait-timeout 60 postgres`.
///
/// Transformation:
/// - Checks command construction without requiring Docker or starting a
///   container during unit tests.
#[test]
fn docker_compose_up_command_targets_postgres_service_only() {
    let path = PathBuf::from("/tmp/demo/docker-compose.yml");

    let command = docker_compose_up_command(&path);
    let logs_command = docker_compose_logs_command(&path);

    assert_eq!(command.program, "docker");
    assert_eq!(
        command.args,
        vec![
            "compose",
            "-f",
            "/tmp/demo/docker-compose.yml",
            "up",
            "-d",
            "--no-recreate",
            "--wait",
            "--wait-timeout",
            "60",
            "postgres"
        ]
    );
    assert_eq!(logs_command.program, "docker");
    assert_eq!(
        logs_command.args,
        vec![
            "compose",
            "-f",
            "/tmp/demo/docker-compose.yml",
            "logs",
            "--no-color",
            "--tail",
            "200",
            "postgres"
        ]
    );
}

#[test]
fn docker_compose_ownership_commands_are_narrow_and_project_scoped() {
    let path = PathBuf::from("/tmp/demo/docker-compose.yml");

    let inspect = docker_compose_inspect_command(&path);
    let remove = docker_compose_remove_command(&path);

    assert_eq!(
        inspect.args,
        vec![
            "compose",
            "-f",
            "/tmp/demo/docker-compose.yml",
            "ps",
            "--all",
            "--quiet",
            "postgres"
        ]
    );
    assert_eq!(
        remove.args,
        vec![
            "compose",
            "-f",
            "/tmp/demo/docker-compose.yml",
            "rm",
            "--stop",
            "--force",
            "postgres"
        ]
    );
}

#[test]
fn compose_container_identity_rejects_empty_and_accepts_nonempty_output() {
    assert!(!compose_container_exists(b"\n  \r\n"));
    assert!(compose_container_exists(b"\nabc123\n"));
}

#[test]
fn external_dependency_session_never_produces_shutdown_command() {
    let session = DevDependencySession::external();

    assert_eq!(session.ownership, DependencyOwnership::External);
    assert_eq!(session.shutdown_command(), None);
}

#[test]
fn owned_dependency_session_removes_only_its_postgres_container() {
    let path = PathBuf::from("/tmp/demo/docker-compose.yml");
    let session = DevDependencySession::owned(path);

    let command = session
        .shutdown_command()
        .expect("owned dependency must have shutdown command");

    assert_eq!(
        command,
        docker_compose_remove_command(Path::new("/tmp/demo/docker-compose.yml"))
    );
    std::mem::forget(session);
}

#[test]
fn stale_container_is_reused_as_external_and_preserved() {
    let session =
        classify_dependency_ownership(true, PathBuf::from("/tmp/demo/docker-compose.yml"));

    assert_eq!(session.ownership, DependencyOwnership::External);
    assert!(session.shutdown_command().is_none());
}

#[test]
fn owned_dependency_cleanup_preserves_stale_postgres_volumes() {
    let command = docker_compose_remove_command(Path::new("/tmp/demo/docker-compose.yml"));

    assert!(!command
        .args
        .iter()
        .any(|arg| arg == "--volumes" || arg == "-v"));
}

#[test]
fn external_dependency_finalization_preserves_success() {
    let outcome = finish_dependency_session(
        Some(DevDependencySession::external()),
        std::process::ExitCode::SUCCESS,
    );

    assert_eq!(outcome, std::process::ExitCode::SUCCESS);
}

#[test]
fn owned_dependency_removal_failure_is_typed_and_redacted() {
    let command = docker_compose_remove_command(Path::new("/tmp/demo/docker-compose.yml"));

    let error = run_compose_remove_with(&command, |_command| {
        Ok(ComposeCommandOutput {
            success: false,
            status: "exit status: 17".to_string(),
            stdout: b"POSTGRES_PASSWORD=hunter2".to_vec(),
            stderr: b"/private/project/compose.yml".to_vec(),
        })
    })
    .expect_err("owned dependency removal must fail closed");

    assert!(error.starts_with("error[dev_dependency.stop_failed]:"));
    assert!(error.contains("exit status: 17"));
    assert!(!error.contains("hunter2"));
    assert!(!error.contains("/private/project"));
}

/// Verifies dependency startup is optional for standalone web packages.
///
/// Inputs:
/// - A project directory with no Compose file.
///
/// Output:
/// - Test passes when startup returns success without invoking Docker.
///
/// Transformation:
/// - Locks the rule that Docker-aware serving only applies to projects that
///   declare a Compose dependency contract.
#[test]
fn start_project_dependencies_ignores_missing_compose() {
    let dir = temp_dir("no_compose_startup");
    fs::create_dir_all(&dir).expect("create temp project");

    start_project_dependencies(&dir).expect("missing compose is a no-op");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn project_root_for_path_finds_nearest_manifest_from_migrations() {
    let root = temp_dir("project_root_from_migrations");
    let migrations = root.join("db/migrations");
    fs::create_dir_all(&migrations).expect("create migrations");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
    )
    .expect("write manifest");

    assert_eq!(
        project_root_for_path(&migrations).expect("project discovery"),
        Some(root.clone())
    );

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn run_compose_up_reports_missing_docker_with_typed_diagnostic() {
    let command = ComposeCommand {
        program: "/definitely/missing/terlan-docker".to_string(),
        args: Vec::new(),
    };
    let logs_command = ComposeCommand {
        program: "docker".to_string(),
        args: Vec::new(),
    };

    let error = run_compose_up(&command, &logs_command).expect_err("missing Docker must fail");

    assert!(error.starts_with("error[dev_dependency.docker_missing]:"));
    assert!(!error.contains("/definitely/missing/terlan-docker"));
}

#[test]
fn run_compose_up_collects_bounded_redacted_service_logs() {
    let compose_path = PathBuf::from("/tmp/demo/docker-compose.yml");
    let up_command = docker_compose_up_command(&compose_path);
    let logs_command = docker_compose_logs_command(&compose_path);
    let long_line = "x".repeat(MAX_COMPOSE_LOG_EXCERPT_CHARS + 100);
    let mut calls = 0;

    let error = run_compose_up_with(&up_command, &logs_command, |command| {
        calls += 1;
        if command.args.contains(&"up".to_string()) {
            Ok(ComposeCommandOutput {
                success: false,
                status: "exit status: 1".to_string(),
                stdout: Vec::new(),
                stderr: b"TOKEN=up-secret".to_vec(),
            })
        } else {
            Ok(ComposeCommandOutput {
                success: true,
                status: "exit status: 0".to_string(),
                stdout: format!(
                    "database is starting\nPOSTGRES_PASSWORD=hunter2\nunsafe\x1bcontrol\n{long_line}"
                )
                .into_bytes(),
                stderr: Vec::new(),
            })
        }
    })
    .expect_err("failed readiness must collect logs");

    assert_eq!(calls, 2);
    assert!(error.starts_with("error[dev_dependency.readiness_failed]:"));
    assert!(error.contains("database is starting"));
    assert!(error.contains("[redacted sensitive log line]"));
    assert!(error.contains("[redacted control-bearing log line]"));
    assert!(error.contains("[truncated]"));
    assert!(!error.contains("up-secret"));
    assert!(!error.contains("hunter2"));
}

#[test]
fn run_compose_up_preserves_primary_failure_when_logs_are_unavailable() {
    let compose_path = PathBuf::from("/tmp/demo/docker-compose.yml");
    let up_command = docker_compose_up_command(&compose_path);
    let logs_command = docker_compose_logs_command(&compose_path);
    let mut calls = 0;

    let error = run_compose_up_with(&up_command, &logs_command, |_command| {
        calls += 1;
        if calls == 1 {
            Ok(ComposeCommandOutput {
                success: false,
                status: "exit status: 1".to_string(),
                stdout: b"healthcheck failed".to_vec(),
                stderr: Vec::new(),
            })
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "logs denied",
            ))
        }
    })
    .expect_err("readiness failure must remain primary");

    assert_eq!(calls, 2);
    assert!(error.contains("healthcheck failed"));
    assert!(error.contains("(service logs unavailable)"));
    assert!(!error.contains("logs denied"));
}
