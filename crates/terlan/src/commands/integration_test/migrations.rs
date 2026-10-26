//! Integration database setup through Terlan's SQL migration engine.

use super::manifest_and_arguments::{
    IntegrationArgs, DEFAULT_DB_HOST, DEFAULT_DB_NAME, DEFAULT_DB_PASSWORD, DEFAULT_DB_PORT,
    DEFAULT_DB_USER,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

pub(super) fn run_migrations_if_present(
    project_dir: &Path,
    args: &IntegrationArgs,
    app_env: &BTreeMap<String, String>,
) -> Result<(), BoundaryError> {
    run_with_executor(project_dir, args, app_env, crate::commands::db::run)
}

fn migration_error(message: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::CommandExecution,
        "apply integration SQL migrations",
        message,
    )
}

fn run_with_executor(
    project_dir: &Path,
    args: &IntegrationArgs,
    app_env: &BTreeMap<String, String>,
    execute: impl FnOnce(crate::CliCommand) -> ExitCode,
) -> Result<(), BoundaryError> {
    let directory = project_dir.join(args.migrations_dir.as_deref().unwrap_or(Path::new("sql")));
    if !directory.exists() {
        if args.migrations_dir.is_some() {
            return Err(migration_error(format!(
                "SQL migration directory does not exist: {}",
                directory.display()
            )));
        }
        return Ok(());
    }
    if !directory.is_dir() {
        return Err(migration_error(format!(
            "SQL migration path is not a directory: {}",
            directory.display()
        )));
    }
    let database_url = database_url(app_env)?;
    let command = crate::CliCommand {
        verb: Some("db".into()),
        args: vec![
            "migrate".into(),
            "--database-url".into(),
            database_url,
            directory.to_string_lossy().into_owned(),
        ],
    };
    if execute(command) != ExitCode::SUCCESS {
        return Err(migration_error(format!(
            "Terlan SQL migrations failed for {}",
            directory.display()
        )));
    }
    println!("integration: Terlan SQL migrations applied");
    Ok(())
}

pub(super) fn database_url(app_env: &BTreeMap<String, String>) -> Result<String, BoundaryError> {
    for key in ["TERLAN_DATABASE_URL", "DATABASE_URL"] {
        if let Some(value) = app_env.get(key).filter(|value| !value.trim().is_empty()) {
            return Ok(value.clone());
        }
    }
    let value = |key, default| app_env.get(key).map(String::as_str).unwrap_or(default);
    let mut url = url::Url::parse("postgresql://localhost")
        .map_err(|_| migration_error("cannot initialize database URL"))?;
    url.set_host(Some(value("POSTGRES_HOST", DEFAULT_DB_HOST)))
        .map_err(|_| migration_error("invalid POSTGRES_HOST"))?;
    url.set_username(&value("POSTGRES_USER", DEFAULT_DB_USER).replace('%', "%25"))
        .map_err(|_| migration_error("invalid POSTGRES_USER"))?;
    url.set_password(Some(
        &value("POSTGRES_PASSWORD", DEFAULT_DB_PASSWORD).replace('%', "%25"),
    ))
    .map_err(|_| migration_error("invalid POSTGRES_PASSWORD"))?;
    let port = value("POSTGRES_PORT", DEFAULT_DB_PORT)
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| migration_error("POSTGRES_PORT must be between 1 and 65535"))?;
    url.set_port(Some(port))
        .map_err(|_| migration_error("invalid POSTGRES_PORT"))?;
    let database = value("POSTGRES_DB", DEFAULT_DB_NAME);
    if database.is_empty() {
        return Err(migration_error("POSTGRES_DB must not be empty"));
    }
    url.path_segments_mut()
        .map_err(|_| migration_error("invalid database URL"))?
        .clear()
        .push(database);
    Ok(url.into())
}

#[cfg(test)]
#[path = "migrations_test.rs"]
mod tests;
