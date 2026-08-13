use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use docker_compose_types::{Compose, Environment, HealthcheckTest, Ports, Service};

/// Required Postgres service name for generated web-profile Compose files.
const POSTGRES_SERVICE_NAME: &str = "postgres";

/// Maximum time Docker Compose may wait for the Postgres healthcheck.
const POSTGRES_READY_TIMEOUT_SECONDS: &str = "60";

/// Maximum number of recent service log lines collected after failed startup.
const COMPOSE_LOG_TAIL_LINES: &str = "200";

/// Maximum number of redacted service-log characters rendered in a diagnostic.
const MAX_COMPOSE_LOG_EXCERPT_CHARS: usize = 4_096;

/// Markers that make an entire service-log line unsafe to render.
const SENSITIVE_LOG_MARKERS: &[&str] = &[
    "api_key",
    "apikey",
    "authorization",
    "cookie",
    "password",
    "postgres://",
    "postgresql://",
    "secret",
    "token",
];

/// Docker Compose file names accepted by Terlan dev validation.
const COMPOSE_FILE_NAMES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

/// Ownership classification for one declared development dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DependencyOwnership {
    Inactive,
    External,
    Owned(PathBuf),
}

/// Scoped lifecycle for dependencies admitted by one Terlan command.
///
/// Existing Compose containers are classified as external and preserved.
/// Containers created by this session are removed when the command scope ends.
#[derive(Debug)]
pub(crate) struct DevDependencySession {
    ownership: DependencyOwnership,
    finished: bool,
}

impl DevDependencySession {
    fn inactive() -> Self {
        Self {
            ownership: DependencyOwnership::Inactive,
            finished: false,
        }
    }

    fn external() -> Self {
        Self {
            ownership: DependencyOwnership::External,
            finished: false,
        }
    }

    fn owned(compose_path: PathBuf) -> Self {
        Self {
            ownership: DependencyOwnership::Owned(compose_path),
            finished: false,
        }
    }

    fn shutdown_command(&self) -> Option<ComposeCommand> {
        match &self.ownership {
            DependencyOwnership::Owned(path) => Some(docker_compose_remove_command(path)),
            DependencyOwnership::Inactive | DependencyOwnership::External => None,
        }
    }

    /// Completes this session and reports owned-container removal failures.
    pub(crate) fn finish(mut self) -> Result<(), String> {
        let command = self.shutdown_command();
        self.finished = true;
        match command {
            Some(command) => run_compose_remove(&command),
            None => Ok(()),
        }
    }
}

impl Drop for DevDependencySession {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let Some(command) = self.shutdown_command() else {
            return;
        };
        if let Err(message) = run_compose_remove(&command) {
            eprintln!("{message}");
        }
    }
}

/// Finalizes an optional dependency session without hiding cleanup failures.
pub(crate) fn finish_dependency_session(
    session: Option<DevDependencySession>,
    outcome: ExitCode,
) -> ExitCode {
    let Some(session) = session else {
        return outcome;
    };
    match session.finish() {
        Ok(()) => outcome,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Validates the Docker Compose file in a project root when one exists.
///
/// Inputs:
/// - `project_root`: directory that owns `terlan.toml`.
///
/// Output:
/// - `Ok(())` when no Compose file exists or when the first supported Compose
///   file validates.
/// - `Err(String)` when Compose parsing or required service validation fails.
///
/// Transformation:
/// - Uses `docker-compose-types` plus Serde YAML deserialization to parse the
///   Compose model, then applies only Terlan's strict web-profile Postgres
///   service contract.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
pub(crate) fn validate_project_compose(project_root: &Path) -> Result<(), String> {
    let Some(path) = project_compose_path(project_root) else {
        return Ok(());
    };
    validate_docker_compose_file(&path)
}

/// Starts validated project-owned Docker Compose dependencies for dev serve.
///
/// Inputs:
/// - `project_root`: directory that may contain a Docker Compose file.
///
/// Output:
/// - A scoped session when no Compose file exists or Docker Compose reports
///   the Postgres service healthy within the bounded readiness timeout.
/// - `Err(String)` when Compose validation fails, Docker is unavailable, or
///   dependency startup fails.
///
/// Transformation:
/// - Reuses Terlan's strict Compose validation, classifies any existing
///   container as external, then invokes Docker only for the declared
///   `postgres` dependency. The returned session preserves external containers
///   and removes only a container created by this invocation.
pub(crate) fn start_project_dependencies(
    project_root: &Path,
) -> Result<DevDependencySession, String> {
    let Some(path) = project_compose_path(project_root) else {
        return Ok(DevDependencySession::inactive());
    };
    validate_docker_compose_file(&path)?;
    let existed_before_start = run_compose_inspect(&docker_compose_inspect_command(&path))?;
    run_compose_up(
        &docker_compose_up_command(&path),
        &docker_compose_logs_command(&path),
    )?;
    Ok(classify_dependency_ownership(existed_before_start, path))
}

/// Classifies the service after a pre-start ownership probe.
///
/// A stopped container is deliberately considered external because the probe
/// uses `docker compose ps --all`. Starting it may repair a stale development
/// service, but Terlan must not remove a container it did not create.
fn classify_dependency_ownership(
    existed_before_start: bool,
    compose_path: PathBuf,
) -> DevDependencySession {
    if existed_before_start {
        DevDependencySession::external()
    } else {
        DevDependencySession::owned(compose_path)
    }
}

/// Starts dependencies declared by the project owning one command input path.
///
/// Inputs:
/// - `path`: project-relative or absolute command input, such as
///   `db/migrations`.
///
/// Output:
/// - A scoped session when no owning project or Compose file exists, or
///   dependencies become ready.
/// - `Err(String)` when project discovery or dependency startup fails.
///
/// Transformation:
/// - Resolves relative inputs against the current directory, walks ancestors
///   to the nearest `terlan.toml`, and delegates to the same typed Compose
///   validation and startup path used by `terlc serve`.
pub(crate) fn start_project_dependencies_for_path(
    path: &Path,
) -> Result<DevDependencySession, String> {
    let Some(project_root) = project_root_for_path(path)? else {
        return Ok(DevDependencySession::inactive());
    };
    start_project_dependencies(&project_root)
}

/// Finds the nearest Terlan project that owns one command input path.
fn project_root_for_path(path: &Path) -> Result<Option<PathBuf>, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                format!(
                    "error[dev_dependency.project_discovery]: cannot resolve current directory: {error}"
                )
            })?
            .join(path)
    };
    let search_from = if absolute.is_file() {
        absolute.parent().unwrap_or(&absolute)
    } else {
        absolute.as_path()
    };
    Ok(search_from
        .ancestors()
        .find(|ancestor| ancestor.join("terlan.toml").is_file())
        .map(Path::to_path_buf))
}

/// Validates one Docker Compose file for Terlan's web dev profile.
///
/// Inputs:
/// - `path`: Compose file path.
///
/// Output:
/// - `Ok(())` when the Compose file parses and contains the required Postgres
///   service contract.
/// - `Err(String)` for read, parse, or contract validation failures.
///
/// Transformation:
/// - Reads YAML text, deserializes it into the typed Compose model supplied by
///   `docker-compose-types`, then validates only the service details Terlan
///   needs to rely on during local web/Postgres development.
fn validate_docker_compose_file(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "error[serve_package]: cannot read Docker Compose file `{}`: {error}",
            path.display()
        )
    })?;
    let compose = serde_yaml::from_str::<Compose>(&text).map_err(|error| {
        format!(
            "error[serve_package]: malformed Docker Compose file `{}`: {error}",
            path.display()
        )
    })?;
    validate_postgres_service(&compose, path)
}

/// Finds a project-root Docker Compose file.
///
/// Inputs:
/// - `project_root`: directory containing the project's `terlan.toml`.
///
/// Output:
/// - First supported Compose path if it exists.
/// - `None` when no project Compose file exists.
///
/// Transformation:
/// - Checks only the project root. Compose convenience remains outside the
///   compiler; Terlan validates the project-owned file it can reason about.
fn project_compose_path(project_root: &Path) -> Option<PathBuf> {
    COMPOSE_FILE_NAMES
        .iter()
        .map(|name| project_root.join(name))
        .find(|candidate| candidate.is_file())
}

/// Command model for the narrow Docker Compose startup action.
///
/// Inputs:
/// - Built from a validated Docker Compose file path.
///
/// Output:
/// - Program and argument vector used by `std::process::Command`.
///
/// Transformation:
/// - Keeps command rendering testable without executing Docker in unit tests.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ComposeCommand {
    program: String,
    args: Vec<String>,
}

/// Shell-free process result used by production execution and unit fixtures.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ComposeCommandOutput {
    success: bool,
    status: String,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Builds the Docker Compose command for the Postgres development service.
///
/// Inputs:
/// - `compose_path`: path to the project-owned Compose file.
///
/// Output:
/// - Command specification for a detached Postgres startup with bounded
///   healthcheck readiness waiting.
///
/// Transformation:
/// - Converts the Compose path to a string once and keeps the supported Docker
///   operation narrow and auditable.
fn docker_compose_up_command(compose_path: &Path) -> ComposeCommand {
    ComposeCommand {
        program: "docker".to_string(),
        args: vec![
            "compose".to_string(),
            "-f".to_string(),
            compose_path.display().to_string(),
            "up".to_string(),
            "-d".to_string(),
            "--no-recreate".to_string(),
            "--wait".to_string(),
            "--wait-timeout".to_string(),
            POSTGRES_READY_TIMEOUT_SECONDS.to_string(),
            POSTGRES_SERVICE_NAME.to_string(),
        ],
    }
}

/// Builds the read-only command used to classify pre-existing containers.
fn docker_compose_inspect_command(compose_path: &Path) -> ComposeCommand {
    ComposeCommand {
        program: "docker".to_string(),
        args: vec![
            "compose".to_string(),
            "-f".to_string(),
            compose_path.display().to_string(),
            "ps".to_string(),
            "--all".to_string(),
            "--quiet".to_string(),
            POSTGRES_SERVICE_NAME.to_string(),
        ],
    }
}

/// Builds the command used to remove only a session-owned container.
fn docker_compose_remove_command(compose_path: &Path) -> ComposeCommand {
    ComposeCommand {
        program: "docker".to_string(),
        args: vec![
            "compose".to_string(),
            "-f".to_string(),
            compose_path.display().to_string(),
            "rm".to_string(),
            "--stop".to_string(),
            "--force".to_string(),
            POSTGRES_SERVICE_NAME.to_string(),
        ],
    }
}

/// Builds bounded service-log collection for failed dependency readiness.
fn docker_compose_logs_command(compose_path: &Path) -> ComposeCommand {
    ComposeCommand {
        program: "docker".to_string(),
        args: vec![
            "compose".to_string(),
            "-f".to_string(),
            compose_path.display().to_string(),
            "logs".to_string(),
            "--no-color".to_string(),
            "--tail".to_string(),
            COMPOSE_LOG_TAIL_LINES.to_string(),
            POSTGRES_SERVICE_NAME.to_string(),
        ],
    }
}

/// Runs one Docker Compose startup command.
///
/// Inputs:
/// - `command`: command model created by `docker_compose_up_command`.
///
/// Output:
/// - `Ok(())` when Docker exits successfully after the service is healthy.
/// - `Err(String)` with a stable `error[serve_compose]` diagnostic otherwise.
///
/// Transformation:
/// - Executes Docker without a shell, captures output for diagnostics, and
///   keeps all generic Docker behavior outside Terlan.
fn run_compose_up(command: &ComposeCommand, logs_command: &ComposeCommand) -> Result<(), String> {
    run_compose_up_with(command, logs_command, execute_compose_command)
}

fn run_compose_up_with(
    command: &ComposeCommand,
    logs_command: &ComposeCommand,
    mut execute: impl FnMut(&ComposeCommand) -> std::io::Result<ComposeCommandOutput>,
) -> Result<(), String> {
    let output = execute(command).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "error[dev_dependency.docker_missing]: Docker is required for declared project dependencies but the `docker` executable was not found".to_string()
        } else {
            format!(
                "error[dev_dependency.start_failed]: failed to start and await Docker Compose dependency `{POSTGRES_SERVICE_NAME}`: {error}"
            )
        }
    })?;
    if output.success {
        return Ok(());
    }

    let detail = redact_compose_logs(&combined_compose_output(&output));
    let service_logs = collect_compose_logs(logs_command, &mut execute);
    Err(format!(
        "error[dev_dependency.readiness_failed]: Docker Compose dependency `{POSTGRES_SERVICE_NAME}` failed to become ready with status {}: {detail}; redacted service logs: {service_logs}",
        output.status
    ))
}

fn execute_compose_command(command: &ComposeCommand) -> std::io::Result<ComposeCommandOutput> {
    Command::new(&command.program)
        .args(&command.args)
        .output()
        .map(|output| ComposeCommandOutput {
            success: output.status.success(),
            status: output.status.to_string(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
}

/// Executes the ownership probe before startup.
fn run_compose_inspect(command: &ComposeCommand) -> Result<bool, String> {
    let output = run_checked_compose_command(
        command,
        "dev_dependency.ownership_failed",
        "inspect declared Docker Compose dependency ownership",
    )?;
    Ok(compose_container_exists(&output.stdout))
}

/// Removes one dependency container proven to be session-owned.
fn run_compose_remove(command: &ComposeCommand) -> Result<(), String> {
    run_compose_remove_with(command, execute_compose_command)
}

fn run_compose_remove_with(
    command: &ComposeCommand,
    execute: impl FnMut(&ComposeCommand) -> std::io::Result<ComposeCommandOutput>,
) -> Result<(), String> {
    run_checked_compose_command_with(
        command,
        "dev_dependency.stop_failed",
        "remove session-owned Docker Compose dependency",
        execute,
    )?;
    Ok(())
}

/// Executes one non-startup Compose command without exposing command arguments.
fn run_checked_compose_command(
    command: &ComposeCommand,
    failure_code: &str,
    action: &str,
) -> Result<ComposeCommandOutput, String> {
    run_checked_compose_command_with(command, failure_code, action, execute_compose_command)
}

fn run_checked_compose_command_with(
    command: &ComposeCommand,
    failure_code: &str,
    action: &str,
    mut execute: impl FnMut(&ComposeCommand) -> std::io::Result<ComposeCommandOutput>,
) -> Result<ComposeCommandOutput, String> {
    let output = execute(command).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "error[dev_dependency.docker_missing]: Docker is required for declared project dependencies but the `docker` executable was not found".to_string()
        } else {
            format!("error[{failure_code}]: failed to {action}: {error}")
        }
    })?;
    if output.success {
        Ok(output)
    } else {
        Err(format!(
            "error[{failure_code}]: failed to {action}: Docker exited with status {}",
            output.status
        ))
    }
}

/// Returns whether Compose reported any existing container identifier.
fn compose_container_exists(stdout: &[u8]) -> bool {
    String::from_utf8_lossy(stdout)
        .lines()
        .any(|line| !line.trim().is_empty())
}

fn collect_compose_logs(
    command: &ComposeCommand,
    execute: &mut impl FnMut(&ComposeCommand) -> std::io::Result<ComposeCommandOutput>,
) -> String {
    let Ok(output) = execute(command) else {
        return "(service logs unavailable)".to_string();
    };
    if !output.success {
        return format!("(service logs unavailable with status {})", output.status);
    }
    redact_compose_logs(&combined_compose_output(&output))
}

fn combined_compose_output(output: &ComposeCommandOutput) -> Vec<u8> {
    let mut combined = Vec::with_capacity(output.stdout.len() + output.stderr.len() + 1);
    combined.extend_from_slice(&output.stdout);
    if !output.stdout.is_empty() && !output.stderr.is_empty() {
        combined.push(b'\n');
    }
    combined.extend_from_slice(&output.stderr);
    combined
}

fn redact_compose_logs(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut redacted = String::new();
    for line in text.lines() {
        if !redacted.is_empty() {
            redacted.push_str(" | ");
        }
        let lowercase = line.to_ascii_lowercase();
        if SENSITIVE_LOG_MARKERS
            .iter()
            .any(|marker| lowercase.contains(marker))
        {
            redacted.push_str("[redacted sensitive log line]");
        } else if line.chars().any(char::is_control) {
            redacted.push_str("[redacted control-bearing log line]");
        } else {
            redacted.push_str(line.trim());
        }
    }

    if redacted.is_empty() {
        return "(no service logs returned)".to_string();
    }

    let mut excerpt = redacted
        .chars()
        .take(MAX_COMPOSE_LOG_EXCERPT_CHARS)
        .collect::<String>();
    if redacted.chars().count() > MAX_COMPOSE_LOG_EXCERPT_CHARS {
        excerpt.push_str(" [truncated]");
    }
    excerpt
}

/// Validates the required Postgres service contract.
///
/// Inputs:
/// - `compose`: typed Docker Compose model.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when service `postgres` uses a Postgres image, exposes port
///   `5432`, declares required `POSTGRES_*` environment keys, and has a
///   healthcheck.
/// - `Err(String)` when any required detail is absent or malformed.
///
/// Transformation:
/// - Reads only typed Compose fields; it does not inspect untyped YAML maps or
///   start Docker services.
fn validate_postgres_service(compose: &Compose, path: &Path) -> Result<(), String> {
    let service = compose
        .services
        .0
        .get(POSTGRES_SERVICE_NAME)
        .and_then(Option::as_ref)
        .ok_or_else(|| {
            format!(
                "error[serve_package]: Docker Compose file `{}` must define service `{POSTGRES_SERVICE_NAME}` for web-profile Postgres validation",
                path.display()
            )
        })?;
    validate_postgres_image(service, path)?;
    validate_postgres_environment(service, path)?;
    validate_postgres_port(service, path)?;
    validate_postgres_healthcheck(service, path)
}

/// Validates the Postgres service image.
///
/// Inputs:
/// - `service`: typed Compose service named `postgres`.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when the image is `postgres` or starts with `postgres:`.
/// - `Err(String)` when the image is absent or not a Postgres image.
///
/// Transformation:
/// - Narrows Compose image text to the generated web-profile dependency
///   contract.
fn validate_postgres_image(service: &Service, path: &Path) -> Result<(), String> {
    let Some(image) = service.image.as_deref() else {
        return Err(format!(
            "error[serve_package]: Docker Compose file `{}` service `{POSTGRES_SERVICE_NAME}` must set image",
            path.display()
        ));
    };
    if image != "postgres" && !image.starts_with("postgres:") {
        return Err(format!(
            "error[serve_package]: Docker Compose file `{}` service `{POSTGRES_SERVICE_NAME}` must use a postgres image, got `{image}`",
            path.display()
        ));
    }
    Ok(())
}

/// Validates required Postgres environment keys.
///
/// Inputs:
/// - `service`: typed Compose service named `postgres`.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when `POSTGRES_DB`, `POSTGRES_USER`, and `POSTGRES_PASSWORD`
///   are present with non-empty values.
/// - `Err(String)` when required keys are absent or empty.
///
/// Transformation:
/// - Accepts Compose's map and list environment forms while validating only
///   the required dev-service keys and non-empty values.
fn validate_postgres_environment(service: &Service, path: &Path) -> Result<(), String> {
    for key in ["POSTGRES_DB", "POSTGRES_USER", "POSTGRES_PASSWORD"] {
        let Some(value) = environment_value_for_key(&service.environment, key) else {
            return Err(missing_or_empty_postgres_env(path, key));
        };
        if value.trim().is_empty() {
            return Err(missing_or_empty_postgres_env(path, key));
        }
    }
    Ok(())
}

/// Returns one Compose environment value by key.
///
/// Inputs:
/// - `environment`: typed Compose environment block.
/// - `key`: key to search for.
///
/// Output:
/// - `Some(value)` when the key is present with a concrete value.
/// - `None` when the key is absent or map-form value is intentionally unset.
///
/// Transformation:
/// - Normalizes Compose map and list environment forms into string values for
///   Terlan's small required-Postgres-key validation.
fn environment_value_for_key(environment: &Environment, key: &str) -> Option<String> {
    match environment {
        Environment::List(values) => values.iter().find_map(|value| {
            let (name, raw_value) = value.split_once('=')?;
            (name == key).then(|| raw_value.to_string())
        }),
        Environment::KvPair(values) => values
            .get(key)
            .and_then(|value| value.as_ref())
            .map(ToString::to_string),
    }
}

/// Builds the required Postgres environment diagnostic.
///
/// Inputs:
/// - `path`: source Compose path.
/// - `key`: required environment key.
///
/// Output:
/// - Stable serve-package diagnostic text.
///
/// Transformation:
/// - Centralizes missing and empty key wording so map and list environment
///   forms produce the same user-facing error.
fn missing_or_empty_postgres_env(path: &Path, key: &str) -> String {
    format!(
        "error[serve_package]: Docker Compose file `{}` service `{POSTGRES_SERVICE_NAME}` must set non-empty `{key}`",
        path.display()
    )
}

/// Validates Postgres port exposure.
///
/// Inputs:
/// - `service`: typed Compose service named `postgres`.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when the service publishes container port `5432` on a loopback
///   host address.
/// - `Err(String)` when the service does not expose the expected port.
///
/// Transformation:
/// - Accepts both short and long Compose port syntax through typed Compose
///   port variants while rejecting broad host bindings such as `5432:5432`.
fn validate_postgres_port(service: &Service, path: &Path) -> Result<(), String> {
    if ports_publish_postgres_on_loopback(&service.ports) {
        return Ok(());
    }
    Err(format!(
        "error[serve_package]: Docker Compose file `{}` service `{POSTGRES_SERVICE_NAME}` must publish container port 5432 on loopback, for example `127.0.0.1:5432:5432`",
        path.display()
    ))
}

/// Returns whether one Compose port block publishes Postgres on loopback.
///
/// Inputs:
/// - `ports`: typed Compose port declaration.
///
/// Output:
/// - `true` when container port `5432` is host-published through loopback.
/// - `false` otherwise.
///
/// Transformation:
/// - Handles short string syntax such as `127.0.0.1:5432:5432` and long syntax
///   where `target`, `published`, and `host_ip` are typed fields.
fn ports_publish_postgres_on_loopback(ports: &Ports) -> bool {
    match ports {
        Ports::Short(values) => values
            .iter()
            .any(|value| short_port_publishes_postgres(value)),
        Ports::Long(values) => values.iter().any(|port| {
            port.target == 5432
                && port.published.is_some()
                && port
                    .host_ip
                    .as_deref()
                    .is_some_and(is_loopback_compose_host)
        }),
    }
}

/// Returns whether a short Compose port string publishes Postgres safely.
///
/// Inputs:
/// - `value`: short Compose port string.
///
/// Output:
/// - `true` for values such as `127.0.0.1:5432:5432`.
/// - `false` for unbound or broad values such as `5432` and `5432:5432`.
///
/// Transformation:
/// - Removes an optional protocol suffix, splits the host/published/target
///   shape, and accepts only loopback host bindings for container port `5432`.
fn short_port_publishes_postgres(value: &str) -> bool {
    let without_protocol = value
        .split_once('/')
        .map_or(value, |(port, _protocol)| port);
    let parts = without_protocol.split(':').collect::<Vec<_>>();
    if parts.len() < 3 {
        return false;
    }
    let Some(target) = parts.last() else {
        return false;
    };
    target.trim() == "5432" && is_loopback_compose_host(parts[0].trim())
}

/// Returns whether a Compose host binding names a loopback interface.
///
/// Inputs:
/// - `host`: host binding from short or long Compose port syntax.
///
/// Output:
/// - `true` for currently supported loopback bindings.
/// - `false` for empty, wildcard, or remote host bindings.
///
/// Transformation:
/// - Keeps Terlan's dev database binding rule small and explicit. Additional
///   loopback spellings can be admitted here without changing callers.
fn is_loopback_compose_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Validates the Postgres healthcheck.
///
/// Inputs:
/// - `service`: typed Compose service named `postgres`.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when the service has an enabled, non-empty healthcheck command.
/// - `Err(String)` when no healthcheck is present, disabled, or commandless.
///
/// Transformation:
/// - Requires Compose health metadata so future `terlc serve` dependency
///   validation can wait on a known readiness signal.
fn validate_postgres_healthcheck(service: &Service, path: &Path) -> Result<(), String> {
    let Some(healthcheck) = &service.healthcheck else {
        return Err(format!(
            "error[serve_package]: Docker Compose file `{}` service `{POSTGRES_SERVICE_NAME}` must define a healthcheck",
            path.display()
        ));
    };
    if healthcheck.disable {
        return Err(format!(
            "error[serve_package]: Docker Compose file `{}` service `{POSTGRES_SERVICE_NAME}` healthcheck must not be disabled",
            path.display()
        ));
    }
    if !healthcheck_has_enabled_test(healthcheck.test.as_ref()) {
        return Err(format!(
            "error[serve_package]: Docker Compose file `{}` service `{POSTGRES_SERVICE_NAME}` healthcheck must define a non-empty test command",
            path.display()
        ));
    }
    Ok(())
}

/// Returns whether a Compose healthcheck test is an enabled command.
///
/// Inputs:
/// - `test`: optional typed healthcheck test value.
///
/// Output:
/// - `true` when the test contains executable command text.
/// - `false` when the test is absent, empty, or uses Compose's `NONE` marker.
///
/// Transformation:
/// - Normalizes single-string and list-form Compose healthcheck commands into
///   the minimal readiness contract Terlan needs before dependency startup.
fn healthcheck_has_enabled_test(test: Option<&HealthcheckTest>) -> bool {
    match test {
        Some(HealthcheckTest::Single(command)) => healthcheck_command_is_enabled(command),
        Some(HealthcheckTest::Multiple(parts)) => {
            parts.iter().any(|part| !part.trim().is_empty())
                && parts
                    .first()
                    .is_some_and(|first| healthcheck_command_is_enabled(first))
        }
        None => false,
    }
}

/// Returns whether one healthcheck command marker is enabled.
///
/// Inputs:
/// - `command`: command marker or command text.
///
/// Output:
/// - `true` when the command text is non-empty and not `NONE`.
///
/// Transformation:
/// - Applies Docker Compose's conventional disabled marker without parsing
///   shell syntax or command arguments.
fn healthcheck_command_is_enabled(command: &str) -> bool {
    let trimmed = command.trim();
    !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("NONE")
}

#[cfg(test)]
#[path = "serve/compose_test.rs"]
#[cfg(test)]
mod compose_test;
