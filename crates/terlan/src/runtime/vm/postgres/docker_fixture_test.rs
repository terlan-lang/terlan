//! Shared Docker database fixture for driver, migration, and server gates.

use std::{
    process::{Command, Output},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const POSTGRES_IMAGE: &str = "postgres:16-alpine";
pub(super) const POSTGRES_USER: &str = "terlan";
pub(super) const POSTGRES_PASSWORD: &str = "terlan";
pub(super) const POSTGRES_DATABASE: &str = "terlan";

/// Disposable local database retained for the duration of a live gate.
pub(crate) struct DockerPostgres {
    name: String,
    port: u16,
}

impl DockerPostgres {
    pub(crate) fn start() -> Result<Self, String> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
            .as_nanos();
        let name = format!("terlan-postgres-gate-{}-{unique}", std::process::id());
        let output = Command::new("docker")
            .args([
                "run",
                "--detach",
                "--rm",
                "--name",
                &name,
                "--env",
                &format!("POSTGRES_USER={POSTGRES_USER}"),
                "--env",
                &format!("POSTGRES_PASSWORD={POSTGRES_PASSWORD}"),
                "--env",
                &format!("POSTGRES_DB={POSTGRES_DATABASE}"),
                "--publish",
                "127.0.0.1::5432",
                POSTGRES_IMAGE,
            ])
            .output()
            .map_err(|error| format!("failed to launch Docker: {error}"))?;
        require_success("docker run", &output)?;

        let port = match published_port(&name) {
            Ok(port) => port,
            Err(error) => {
                remove_container(&name);
                return Err(error);
            }
        };
        let fixture = Self { name, port };
        fixture.wait_until_ready()?;
        Ok(fixture)
    }

    pub(crate) fn url(&self, password: &str) -> String {
        format!(
            "postgres://{POSTGRES_USER}:{password}@127.0.0.1:{}/{POSTGRES_DATABASE}",
            self.port
        )
    }

    fn wait_until_ready(&self) -> Result<(), String> {
        for _ in 0..300 {
            let ready = Command::new("docker")
                .args([
                    "exec",
                    &self.name,
                    "pg_isready",
                    "--host",
                    "127.0.0.1",
                    "--username",
                    POSTGRES_USER,
                    "--dbname",
                    POSTGRES_DATABASE,
                ])
                .output()
                .map_err(|error| format!("failed to poll Docker Postgres readiness: {error}"))?;
            if ready.status.success() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        let logs = Command::new("docker")
            .args(["logs", &self.name])
            .output()
            .map_err(|error| format!("failed to read Docker Postgres logs: {error}"))?;
        Err(format!(
            "Docker Postgres did not become ready:\n{}",
            String::from_utf8_lossy(&logs.stderr)
        ))
    }
}

impl Drop for DockerPostgres {
    fn drop(&mut self) {
        remove_container(&self.name);
    }
}

fn remove_container(name: &str) {
    let _ignored = Command::new("docker")
        .args(["rm", "--force", name])
        .output();
}

fn published_port(name: &str) -> Result<u16, String> {
    let output = Command::new("docker")
        .args(["port", name, "5432/tcp"])
        .output()
        .map_err(|error| format!("failed to inspect Docker Postgres port: {error}"))?;
    require_success("docker port", &output)?;
    let address = String::from_utf8(output.stdout)
        .map_err(|error| format!("Docker returned a non-UTF-8 published port: {error}"))?;
    address
        .trim()
        .rsplit(':')
        .next()
        .ok_or_else(|| format!("Docker returned malformed published port `{address}`"))?
        .parse::<u16>()
        .map_err(|error| format!("Docker returned invalid published port `{address}`: {error}"))
}

fn require_success(operation: &str, output: &Output) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{operation} failed with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}
