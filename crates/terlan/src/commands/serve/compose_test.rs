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
///   `docker compose -f <file> up -d postgres`.
///
/// Transformation:
/// - Checks command construction without requiring Docker or starting a
///   container during unit tests.
#[test]
fn docker_compose_up_command_targets_postgres_service_only() {
    let path = PathBuf::from("/tmp/demo/docker-compose.yml");

    let command = docker_compose_up_command(&path);

    assert_eq!(command.program, "docker");
    assert_eq!(
        command.args,
        vec![
            "compose",
            "-f",
            "/tmp/demo/docker-compose.yml",
            "up",
            "-d",
            "postgres"
        ]
    );
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
fn start_project_compose_dependencies_ignores_missing_compose() {
    let dir = temp_dir("no_compose_startup");
    fs::create_dir_all(&dir).expect("create temp project");

    start_project_compose_dependencies(&dir).expect("missing compose is a no-op");

    fs::remove_dir_all(dir).expect("cleanup");
}
