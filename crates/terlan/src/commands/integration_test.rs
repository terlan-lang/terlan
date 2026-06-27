use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream as StdTcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{CliCommand, CliState};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{connect_async, WebSocketStream};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 18080;
const DEFAULT_COMPOSE_SERVICE: &str = "db";
const DEFAULT_DB_USER: &str = "postgres";
const DEFAULT_DB_NAME: &str = "postgres";
const DEFAULT_DB_PASSWORD: &str = "postgres";
const DEFAULT_DB_HOST: &str = "localhost";
const DEFAULT_DB_PORT: &str = "5432";
const DEFAULT_WAIT_SECS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
struct IntegrationArgs {
    project_dir: PathBuf,
    flow_name: Option<String>,
    host: String,
    host_from_cli: bool,
    port: u16,
    port_from_cli: bool,
    compose_service: String,
    compose_service_from_cli: bool,
    skip_db: bool,
    skip_build: bool,
    migrations_dir: Option<PathBuf>,
    migrations_from_cli: bool,
    wait_secs: u64,
    wait_secs_from_cli: bool,
    http_checks: Vec<HttpCheck>,
    http_checks_from_cli: bool,
    websocket_checks: Vec<WebSocketCheck>,
    websocket_checks_from_cli: bool,
    traits: IntegrationTraits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IntegrationTraits {
    compose_db: bool,
    migrations: bool,
    web_build: bool,
    web_server: bool,
    http_checks: bool,
    websocket_checks: bool,
}

impl Default for IntegrationTraits {
    fn default() -> Self {
        Self {
            compose_db: true,
            migrations: true,
            web_build: true,
            web_server: true,
            http_checks: true,
            websocket_checks: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ManifestIntegrationFlow {
    traits: Option<IntegrationTraits>,
    host: Option<String>,
    port: Option<u16>,
    compose_service: Option<String>,
    migrations_dir: Option<PathBuf>,
    wait_secs: Option<u64>,
    http_checks: Option<Vec<HttpCheck>>,
    websocket_checks: Option<Vec<WebSocketCheck>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpCheck {
    method: String,
    path: String,
    status: u16,
    contains: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebSocketCheck {
    first_path: String,
    first_initial_contains: String,
    second_path: String,
    first_match_contains: String,
    second_match_contains: String,
    move_check: Option<WebSocketMoveCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebSocketMoveCheck {
    row: u64,
    column: u64,
    first_update_contains: String,
    second_update_contains: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpResponse {
    status: u16,
    raw: String,
}

struct ServerGuard {
    child: Child,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Runs a project-level integration test with compiler-owned process setup.
///
/// Inputs:
/// - `cmd`: `terlc integration-test` command-local arguments.
/// - `state`: global CLI state, primarily `--out-dir`.
///
/// Output:
/// - Success when the optional database, build, spawned server, and HTTP
///   checks all pass.
///
/// Transformation:
/// - Turns a web-profile project into a live runtime smoke test without
///   requiring each application to hand-roll Docker, server polling, or HTTP
///   assertions.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_integration_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    match run_integration(args, state) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn print_usage() {
    println!(
        "terlc integration-test [project-dir] [--host <host>] [--port <port>] [--compose-service <name>] [--skip-db] [--skip-build] [--migrations <dir>] [--wait-secs <seconds>] [--http-check METHOD:PATH:STATUS[:CONTAINS[:BODY]]] [--websocket-check PAIR:FIRST_PATH:FIRST_INITIAL:SECOND_PATH:FIRST_MATCH:SECOND_MATCH] [--websocket-check PAIR_MOVE:FIRST_PATH:FIRST_INITIAL:SECOND_PATH:FIRST_MATCH:SECOND_MATCH:ROW:COLUMN:FIRST_UPDATE:SECOND_UPDATE]"
    );
    println!("Use --flow <name> or [integration.default] in terlan.toml for composable integration traits.");
    println!("Global --out-dir selects the build output root; default is _build.");
}

fn parse_integration_args(args: &[String]) -> Result<IntegrationArgs, String> {
    let mut project_dir = None;
    let mut flow_name = None;
    let mut host = DEFAULT_HOST.to_string();
    let mut host_from_cli = false;
    let mut port = DEFAULT_PORT;
    let mut port_from_cli = false;
    let mut compose_service = DEFAULT_COMPOSE_SERVICE.to_string();
    let mut compose_service_from_cli = false;
    let mut skip_db = false;
    let mut skip_build = false;
    let mut migrations_dir = None;
    let mut migrations_from_cli = false;
    let mut wait_secs = DEFAULT_WAIT_SECS;
    let mut wait_secs_from_cli = false;
    let mut http_checks = Vec::new();
    let mut http_checks_from_cli = false;
    let mut websocket_checks = Vec::new();
    let mut websocket_checks_from_cli = false;
    let mut traits = IntegrationTraits::default();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                let value = require_value(args, i, "--host")?;
                host = value.to_string();
                host_from_cli = true;
                i += 2;
            }
            "--port" => {
                let value = require_value(args, i, "--port")?;
                port = value.parse::<u16>().map_err(|_| {
                    format!("terlc integration-test --port expects a u16, got `{value}`")
                })?;
                port_from_cli = true;
                i += 2;
            }
            "--flow" => {
                let value = require_value(args, i, "--flow")?;
                flow_name = Some(value.to_string());
                i += 2;
            }
            "--compose-service" => {
                let value = require_value(args, i, "--compose-service")?;
                compose_service = value.to_string();
                compose_service_from_cli = true;
                i += 2;
            }
            "--skip-db" => {
                skip_db = true;
                traits.compose_db = false;
                traits.migrations = false;
                i += 1;
            }
            "--skip-build" => {
                skip_build = true;
                traits.web_build = false;
                i += 1;
            }
            "--migrations" => {
                let value = require_value(args, i, "--migrations")?;
                migrations_dir = Some(PathBuf::from(value));
                migrations_from_cli = true;
                i += 2;
            }
            "--wait-secs" => {
                let value = require_value(args, i, "--wait-secs")?;
                wait_secs = value.parse::<u64>().map_err(|_| {
                    format!("terlc integration-test --wait-secs expects a u64, got `{value}`")
                })?;
                if wait_secs == 0 {
                    return Err(
                        "terlc integration-test --wait-secs must be greater than 0".to_string()
                    );
                }
                wait_secs_from_cli = true;
                i += 2;
            }
            "--http-check" => {
                let value = require_value(args, i, "--http-check")?;
                http_checks.push(parse_http_check(value)?);
                http_checks_from_cli = true;
                i += 2;
            }
            "--websocket-check" => {
                let value = require_value(args, i, "--websocket-check")?;
                websocket_checks.push(parse_websocket_check(value)?);
                websocket_checks_from_cli = true;
                traits.websocket_checks = true;
                i += 2;
            }
            option if option.starts_with('-') => {
                return Err(format!("unsupported integration-test option: {option}"));
            }
            path => {
                if project_dir.is_some() {
                    return Err(
                        "terlc integration-test accepts at most one project directory".to_string(),
                    );
                }
                project_dir = Some(PathBuf::from(path));
                i += 1;
            }
        }
    }

    if http_checks.is_empty() {
        http_checks.push(HttpCheck {
            method: "GET".to_string(),
            path: "/health".to_string(),
            status: 200,
            contains: Some("ok".to_string()),
            body: None,
        });
    }

    Ok(IntegrationArgs {
        project_dir: project_dir.unwrap_or_else(|| PathBuf::from(".")),
        flow_name,
        host,
        host_from_cli,
        port,
        port_from_cli,
        compose_service,
        compose_service_from_cli,
        skip_db,
        skip_build,
        migrations_dir,
        migrations_from_cli,
        wait_secs,
        wait_secs_from_cli,
        http_checks,
        http_checks_from_cli,
        websocket_checks,
        websocket_checks_from_cli,
        traits,
    })
}

fn require_value<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("terlc integration-test {option} requires a value"))
}

fn parse_http_check(value: &str) -> Result<HttpCheck, String> {
    let parts = value.splitn(5, ':').collect::<Vec<_>>();
    if parts.len() < 3 {
        return Err(format!(
            "invalid --http-check `{value}`; expected METHOD:PATH:STATUS[:CONTAINS[:BODY]]"
        ));
    }
    let method = parts[0].trim().to_ascii_uppercase();
    if method.is_empty() {
        return Err("integration HTTP check method cannot be empty".to_string());
    }
    let path = parts[1].trim().to_string();
    if !path.starts_with('/') {
        return Err(format!(
            "integration HTTP check path must start with `/`, got `{path}`"
        ));
    }
    let status = parts[2].trim().parse::<u16>().map_err(|_| {
        format!(
            "integration HTTP check status must be a u16, got `{}`",
            parts[2]
        )
    })?;
    let contains = parts
        .get(3)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let body = parts
        .get(4)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    Ok(HttpCheck {
        method,
        path,
        status,
        contains,
        body,
    })
}

fn parse_websocket_check(value: &str) -> Result<WebSocketCheck, String> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 6 && parts.len() != 10 {
        return Err(format!(
            "invalid WebSocket check `{value}`; expected PAIR:FIRST_PATH:FIRST_INITIAL:SECOND_PATH:FIRST_MATCH:SECOND_MATCH or PAIR_MOVE:FIRST_PATH:FIRST_INITIAL:SECOND_PATH:FIRST_MATCH:SECOND_MATCH:ROW:COLUMN:FIRST_UPDATE:SECOND_UPDATE"
        ));
    }
    let kind = parts[0].trim();
    if (parts.len() == 6 && kind != "PAIR") || (parts.len() == 10 && kind != "PAIR_MOVE") {
        return Err(format!(
            "invalid WebSocket check `{value}`; expected PAIR or PAIR_MOVE"
        ));
    }
    let first_path = parse_websocket_check_path(parts[1], "first")?;
    let first_initial_contains = parse_websocket_check_contains(parts[2], "first initial")?;
    let second_path = parse_websocket_check_path(parts[3], "second")?;
    let first_match_contains = parse_websocket_check_contains(parts[4], "first match")?;
    let second_match_contains = parse_websocket_check_contains(parts[5], "second match")?;
    let move_check = if parts.len() == 10 {
        let row = parts[6].trim().parse::<u64>().map_err(|_| {
            format!(
                "integration WebSocket move row must be a u64, got `{}`",
                parts[6]
            )
        })?;
        let column = parts[7].trim().parse::<u64>().map_err(|_| {
            format!(
                "integration WebSocket move column must be a u64, got `{}`",
                parts[7]
            )
        })?;
        Some(WebSocketMoveCheck {
            row,
            column,
            first_update_contains: parse_websocket_check_contains(parts[8], "first update")?,
            second_update_contains: parse_websocket_check_contains(parts[9], "second update")?,
        })
    } else {
        None
    };
    Ok(WebSocketCheck {
        first_path,
        first_initial_contains,
        second_path,
        first_match_contains,
        second_match_contains,
        move_check,
    })
}

fn parse_websocket_check_path(value: &str, label: &str) -> Result<String, String> {
    let path = value.trim().to_string();
    if !path.starts_with('/') {
        return Err(format!(
            "integration WebSocket {label} path must start with `/`, got `{path}`"
        ));
    }
    Ok(path)
}

fn parse_websocket_check_contains(value: &str, label: &str) -> Result<String, String> {
    let contains = value.trim().to_string();
    if contains.is_empty() {
        return Err(format!(
            "integration WebSocket {label} expected text cannot be empty"
        ));
    }
    Ok(contains)
}

fn run_integration(mut args: IntegrationArgs, state: CliState) -> Result<(), String> {
    let project_dir = fs::canonicalize(&args.project_dir).map_err(|error| {
        format!(
            "{}: cannot resolve integration project directory: {error}",
            args.project_dir.display()
        )
    })?;
    apply_manifest_flow(&project_dir, &mut args)?;
    let env_file = project_dir.join("config/dev.env");
    let mut app_env = read_env_file(&env_file)?;

    if args.traits.compose_db {
        normalize_database_host_port(&mut app_env)?;
        run_database_phase(&project_dir, &args, &app_env)?;
    }

    if args.traits.web_build {
        run_build_phase(&project_dir, &state)?;
    }

    if args.traits.http_checks && !args.traits.web_server {
        return Err(
            "integration trait `http-checks` requires integration trait `web-server`".to_string(),
        );
    }
    if args.traits.websocket_checks && !args.traits.web_server {
        return Err(
            "integration trait `websocket-checks` requires integration trait `web-server`"
                .to_string(),
        );
    }

    let _server = if args.traits.web_server {
        let web_root = resolve_out_dir(&project_dir, &state).join("web");
        let mut server = spawn_server(&project_dir, &web_root, &args, &app_env)?;
        wait_for_server(&mut server, &args)?;
        Some(server)
    } else {
        None
    };

    if args.traits.http_checks {
        for check in &args.http_checks {
            run_http_check(&args.host, args.port, check)?;
        }
    }
    if args.traits.websocket_checks {
        if args.websocket_checks.is_empty() {
            return Err(
                "integration trait `websocket-checks` requires at least one websocket_checks entry"
                    .to_string(),
            );
        }
        for check in &args.websocket_checks {
            run_websocket_check(&args.host, args.port, check)?;
        }
    }

    println!("integration: all checks passed");
    Ok(())
}

fn apply_manifest_flow(project_dir: &PathBuf, args: &mut IntegrationArgs) -> Result<(), String> {
    let manifest_path = project_dir.join("terlan.toml");
    if !manifest_path.exists() {
        if let Some(flow_name) = &args.flow_name {
            return Err(format!(
                "{}: cannot load integration flow `{flow_name}` because terlan.toml does not exist",
                project_dir.display()
            ));
        }
        return Ok(());
    }
    let flow_name = args.flow_name.as_deref().unwrap_or("default");
    let required = args.flow_name.is_some();
    let Some(flow) = read_manifest_integration_flow(&manifest_path, flow_name)? else {
        if required {
            return Err(format!(
                "{}: missing [integration.{flow_name}] flow",
                manifest_path.display()
            ));
        }
        return Ok(());
    };
    if let Some(traits) = flow.traits {
        args.traits = traits;
        args.skip_db = !args.traits.compose_db;
        args.skip_build = !args.traits.web_build;
    }
    if !args.host_from_cli {
        if let Some(host) = flow.host {
            args.host = host;
        }
    }
    if !args.port_from_cli {
        if let Some(port) = flow.port {
            args.port = port;
        }
    }
    if !args.compose_service_from_cli {
        if let Some(compose_service) = flow.compose_service {
            args.compose_service = compose_service;
        }
    }
    if !args.migrations_from_cli {
        args.migrations_dir = flow.migrations_dir;
    }
    if !args.wait_secs_from_cli {
        if let Some(wait_secs) = flow.wait_secs {
            args.wait_secs = wait_secs;
        }
    }
    if !args.http_checks_from_cli {
        if let Some(http_checks) = flow.http_checks {
            args.http_checks = http_checks;
        }
    }
    if !args.websocket_checks_from_cli {
        if let Some(websocket_checks) = flow.websocket_checks {
            args.websocket_checks = websocket_checks;
        }
    }
    Ok(())
}

fn read_manifest_integration_flow(
    path: &PathBuf,
    flow_name: &str,
) -> Result<Option<ManifestIntegrationFlow>, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("{}: cannot read project manifest: {error}", path.display()))?;
    parse_manifest_integration_flow(&source, path, flow_name)
}

fn parse_manifest_integration_flow(
    source: &str,
    path: &PathBuf,
    flow_name: &str,
) -> Result<Option<ManifestIntegrationFlow>, String> {
    let target_section = format!("integration.{flow_name}");
    let mut in_target = false;
    let mut found = false;
    let mut flow = ManifestIntegrationFlow::default();

    for (index, raw_line) in source.lines().enumerate() {
        let line_no = index + 1;
        let line = strip_manifest_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            let section = line
                .strip_prefix('[')
                .and_then(|inner| inner.strip_suffix(']'))
                .ok_or_else(|| {
                    format!(
                        "{}:{}: malformed project manifest section",
                        path.display(),
                        line_no
                    )
                })?
                .trim()
                .to_string();
            in_target = section == target_section;
            found |= in_target;
            continue;
        }
        if !in_target {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            format!(
                "{}:{}: expected KEY=VALUE in [integration.{flow_name}]",
                path.display(),
                line_no
            )
        })?;
        match key.trim() {
            "traits" => {
                flow.traits = Some(parse_integration_traits(
                    &parse_manifest_string_array(value.trim(), path, line_no)?,
                    path,
                    line_no,
                )?);
            }
            "host" => flow.host = Some(parse_manifest_string(value.trim(), path, line_no)?),
            "port" => {
                flow.port = Some(value.trim().parse::<u16>().map_err(|_| {
                    format!(
                        "{}:{}: [integration.{flow_name}] port expects a u16",
                        path.display(),
                        line_no
                    )
                })?);
            }
            "compose_service" => {
                flow.compose_service = Some(parse_manifest_string(value.trim(), path, line_no)?);
            }
            "migrations" => {
                flow.migrations_dir = Some(PathBuf::from(parse_manifest_string(
                    value.trim(),
                    path,
                    line_no,
                )?));
            }
            "wait_secs" => {
                let wait_secs = value.trim().parse::<u64>().map_err(|_| {
                    format!(
                        "{}:{}: [integration.{flow_name}] wait_secs expects a u64",
                        path.display(),
                        line_no
                    )
                })?;
                if wait_secs == 0 {
                    return Err(format!(
                        "{}:{}: [integration.{flow_name}] wait_secs must be greater than 0",
                        path.display(),
                        line_no
                    ));
                }
                flow.wait_secs = Some(wait_secs);
            }
            "http_checks" => {
                let checks = parse_manifest_string_array(value.trim(), path, line_no)?
                    .iter()
                    .map(|check| parse_http_check(check))
                    .collect::<Result<Vec<_>, _>>()?;
                flow.http_checks = Some(checks);
            }
            "websocket_checks" => {
                let checks = parse_manifest_string_array(value.trim(), path, line_no)?
                    .iter()
                    .map(|check| parse_websocket_check(check))
                    .collect::<Result<Vec<_>, _>>()?;
                flow.websocket_checks = Some(checks);
            }
            other => {
                return Err(format!(
                    "{}:{}: unsupported [integration.{flow_name}] key `{other}`",
                    path.display(),
                    line_no
                ));
            }
        }
    }

    Ok(found.then_some(flow))
}

fn parse_integration_traits(
    values: &[String],
    path: &PathBuf,
    line_no: usize,
) -> Result<IntegrationTraits, String> {
    let mut traits = IntegrationTraits {
        compose_db: false,
        migrations: false,
        web_build: false,
        web_server: false,
        http_checks: false,
        websocket_checks: false,
    };
    for value in values {
        match value.as_str() {
            "compose-db" => traits.compose_db = true,
            "migrations" => traits.migrations = true,
            "web-build" => traits.web_build = true,
            "web-server" => traits.web_server = true,
            "http-checks" => traits.http_checks = true,
            "websocket-checks" => traits.websocket_checks = true,
            other => {
                return Err(format!(
                    "{}:{}: unsupported integration trait `{other}`; supported traits: compose-db, migrations, web-build, web-server, http-checks, websocket-checks",
                    path.display(),
                    line_no
                ));
            }
        }
    }
    if traits.migrations && !traits.compose_db {
        return Err(format!(
            "{}:{}: integration trait `migrations` requires `compose-db`",
            path.display(),
            line_no
        ));
    }
    if traits.http_checks && !traits.web_server {
        return Err(format!(
            "{}:{}: integration trait `http-checks` requires `web-server`",
            path.display(),
            line_no
        ));
    }
    if traits.websocket_checks && !traits.web_server {
        return Err(format!(
            "{}:{}: integration trait `websocket-checks` requires `web-server`",
            path.display(),
            line_no
        ));
    }
    Ok(traits)
}

fn parse_manifest_string(value: &str, path: &PathBuf, line_no: usize) -> Result<String, String> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .map(unescape_manifest_string)
        .ok_or_else(|| {
            format!(
                "{}:{}: expected quoted string value",
                path.display(),
                line_no
            )
        })
}

fn parse_manifest_string_array(
    value: &str,
    path: &PathBuf,
    line_no: usize,
) -> Result<Vec<String>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| {
            format!(
                "{}:{}: expected quoted string array",
                path.display(),
                line_no
            )
        })?;
    let mut values = Vec::new();
    let mut chars = inner.char_indices().peekable();
    while let Some((_, ch)) = chars.peek().copied() {
        if ch.is_whitespace() || ch == ',' {
            chars.next();
            continue;
        }
        if ch != '"' {
            return Err(format!(
                "{}:{}: expected quoted string array item",
                path.display(),
                line_no
            ));
        }
        chars.next();
        let mut item = String::new();
        let mut escaped = false;
        let mut closed = false;
        for (_, ch) in chars.by_ref() {
            if escaped {
                item.push(match ch {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '"' => '"',
                    '\\' => '\\',
                    other => other,
                });
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => {
                    closed = true;
                    break;
                }
                other => item.push(other),
            }
        }
        if !closed {
            return Err(format!(
                "{}:{}: unterminated quoted string array item",
                path.display(),
                line_no
            ));
        }
        values.push(item);
    }
    Ok(values)
}

fn unescape_manifest_string(value: &str) -> String {
    let mut output = String::new();
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            output.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => other,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            output.push(ch);
        }
    }
    output
}

fn strip_manifest_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

fn read_env_file(path: &PathBuf) -> Result<BTreeMap<String, String>, String> {
    let mut env = BTreeMap::new();
    if !path.exists() {
        return Ok(env);
    }
    let source = fs::read_to_string(path)
        .map_err(|error| format!("{}: cannot read env file: {error}", path.display()))?;
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(format!(
                "{}:{}: expected KEY=VALUE",
                path.display(),
                line_index + 1
            ));
        };
        env.insert(
            key.trim().to_string(),
            trim_env_value(value.trim()).to_string(),
        );
    }
    Ok(env)
}

fn trim_env_value(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn run_database_phase(
    project_dir: &PathBuf,
    args: &IntegrationArgs,
    app_env: &BTreeMap<String, String>,
) -> Result<(), String> {
    let compose_file = project_dir.join("docker-compose.yml");
    if !compose_file.exists() {
        println!("integration: no docker-compose.yml found; skipping database startup");
        return Ok(());
    }

    println!(
        "integration: starting Docker Compose service `{}`",
        args.compose_service
    );
    reset_compose_dependencies(project_dir, app_env)?;
    run_command_with_env(
        project_dir,
        app_env,
        "docker",
        &["compose", "up", "-d", args.compose_service.as_str()],
    )?;

    wait_for_database(project_dir, args, app_env)?;
    run_database_query(project_dir, args, app_env)?;
    if args.traits.migrations {
        run_migrations_if_present(project_dir, args, app_env)?;
    }
    Ok(())
}

fn reset_compose_dependencies(
    project_dir: &PathBuf,
    app_env: &BTreeMap<String, String>,
) -> Result<(), String> {
    println!("integration: resetting Docker Compose dependencies");
    run_command_with_env(
        project_dir,
        app_env,
        "docker",
        &["compose", "down", "-v", "--remove-orphans"],
    )
}

fn normalize_database_host_port(app_env: &mut BTreeMap<String, String>) -> Result<(), String> {
    let configured = env_value(app_env, "POSTGRES_PORT", DEFAULT_DB_PORT);
    let Ok(configured_port) = configured.parse::<u16>() else {
        return Ok(());
    };
    if port_is_available(configured_port) {
        return Ok(());
    }
    let replacement = free_local_port()?;
    app_env.insert("POSTGRES_PORT".to_string(), replacement.to_string());
    println!(
        "integration: POSTGRES_PORT {configured_port} is busy; using {replacement} for Docker Compose"
    );
    Ok(())
}

fn port_is_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn free_local_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("cannot allocate a free local port: {error}"))?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|error| format!("cannot inspect allocated local port: {error}"))
}

fn wait_for_database(
    project_dir: &PathBuf,
    args: &IntegrationArgs,
    app_env: &BTreeMap<String, String>,
) -> Result<(), String> {
    let user = env_value(app_env, "POSTGRES_USER", DEFAULT_DB_USER);
    let db = env_value(app_env, "POSTGRES_DB", DEFAULT_DB_NAME);
    let deadline = Instant::now() + Duration::from_secs(args.wait_secs);
    while Instant::now() < deadline {
        let status = Command::new("docker")
            .args([
                "compose",
                "exec",
                "-T",
                args.compose_service.as_str(),
                "pg_isready",
                "-U",
                user,
                "-d",
                db,
            ])
            .current_dir(project_dir)
            .envs(app_env)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if matches!(status, Ok(status) if status.success()) {
            println!("integration: database is ready");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "database service `{}` did not become ready within {} seconds",
        args.compose_service, args.wait_secs
    ))
}

fn run_database_query(
    project_dir: &PathBuf,
    args: &IntegrationArgs,
    app_env: &BTreeMap<String, String>,
) -> Result<(), String> {
    let user = env_value(app_env, "POSTGRES_USER", DEFAULT_DB_USER);
    let db = env_value(app_env, "POSTGRES_DB", DEFAULT_DB_NAME);
    let deadline = Instant::now() + Duration::from_secs(args.wait_secs);
    while Instant::now() < deadline {
        let status = Command::new("docker")
            .args([
                "compose",
                "exec",
                "-T",
                args.compose_service.as_str(),
                "psql",
                "-U",
                user,
                "-d",
                db,
                "-c",
                "SELECT 1",
            ])
            .current_dir(project_dir)
            .envs(app_env)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if matches!(status, Ok(status) if status.success()) {
            println!("integration: database query check passed");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    return Err(format!(
        "database service `{}` did not accept SELECT 1 within {} seconds",
        args.compose_service, args.wait_secs
    ));
}

fn run_migrations_if_present(
    project_dir: &PathBuf,
    args: &IntegrationArgs,
    app_env: &BTreeMap<String, String>,
) -> Result<(), String> {
    let migrations_dir = args
        .migrations_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("sql"));
    let migrations_dir = project_dir.join(migrations_dir);
    if !migrations_dir.exists() {
        return Ok(());
    }
    if !command_exists("goose") {
        println!(
            "integration: {} exists but goose is not installed; skipping migrations",
            migrations_dir.display()
        );
        return Ok(());
    }

    let dsn = postgres_dsn(app_env);
    let dir = migrations_dir.to_string_lossy().to_string();
    run_command_with_env(
        project_dir,
        app_env,
        "goose",
        &[
            "-v",
            "-allow-missing",
            "-dir",
            dir.as_str(),
            "postgres",
            dsn.as_str(),
            "up",
        ],
    )?;
    println!("integration: migrations applied");
    Ok(())
}

fn command_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn postgres_dsn(app_env: &BTreeMap<String, String>) -> String {
    format!(
        "host={} user={} password={} dbname={} port={} sslmode=disable",
        env_value(app_env, "POSTGRES_HOST", DEFAULT_DB_HOST),
        env_value(app_env, "POSTGRES_USER", DEFAULT_DB_USER),
        env_value(app_env, "POSTGRES_PASSWORD", DEFAULT_DB_PASSWORD),
        env_value(app_env, "POSTGRES_DB", DEFAULT_DB_NAME),
        env_value(app_env, "POSTGRES_PORT", DEFAULT_DB_PORT)
    )
}

fn env_value<'a>(app_env: &'a BTreeMap<String, String>, key: &str, default: &'a str) -> &'a str {
    app_env.get(key).map(String::as_str).unwrap_or(default)
}

fn run_build_phase(project_dir: &PathBuf, state: &CliState) -> Result<(), String> {
    let out_dir = resolve_out_dir(project_dir, state);
    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).map_err(|error| {
            format!(
                "{}: cannot remove previous integration build output: {error}",
                out_dir.display()
            )
        })?;
    }
    let out_arg = out_dir.to_string_lossy().to_string();
    let terlc = current_terlc()?;

    println!("integration: building Erlang target");
    run_command(
        project_dir,
        &terlc,
        &["build", "--target", "erlang", "--out-dir", out_arg.as_str()],
    )?;
    println!("integration: building browser target");
    run_command(
        project_dir,
        &terlc,
        &[
            "build",
            "--target",
            "js.browser",
            "--out-dir",
            out_arg.as_str(),
        ],
    )?;
    println!("integration: validating web package");
    let web_dir = out_dir.join("web").to_string_lossy().to_string();
    run_command(project_dir, &terlc, &["serve", web_dir.as_str(), "--check"])?;
    Ok(())
}

fn resolve_out_dir(project_dir: &PathBuf, state: &CliState) -> PathBuf {
    if state.out_dir.is_absolute() {
        state.out_dir.clone()
    } else {
        project_dir.join(&state.out_dir)
    }
}

fn current_terlc() -> Result<String, String> {
    let path = std::env::current_exe()
        .map_err(|error| format!("cannot resolve current terlc executable: {error}"))?;
    Ok(path.to_string_lossy().to_string())
}

fn spawn_server(
    project_dir: &PathBuf,
    web_root: &PathBuf,
    args: &IntegrationArgs,
    app_env: &BTreeMap<String, String>,
) -> Result<ServerGuard, String> {
    if !web_root.exists() {
        return Err(format!(
            "{}: web package does not exist; run without --skip-build or pass --out-dir",
            web_root.display()
        ));
    }
    let terlc = current_terlc()?;
    println!(
        "integration: starting server on http://{}:{}",
        args.host, args.port
    );
    let web_root_arg = web_root.to_string_lossy().to_string();
    let port_arg = args.port.to_string();
    let child = Command::new(terlc)
        .args([
            "serve",
            web_root_arg.as_str(),
            "--host",
            args.host.as_str(),
            "--port",
            port_arg.as_str(),
        ])
        .current_dir(project_dir)
        .envs(app_env)
        .spawn()
        .map_err(|error| format!("cannot start integration server: {error}"))?;
    Ok(ServerGuard { child })
}

fn wait_for_server(server: &mut ServerGuard, args: &IntegrationArgs) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(args.wait_secs);
    while Instant::now() < deadline {
        if let Some(status) = server
            .child
            .try_wait()
            .map_err(|error| format!("cannot inspect integration server: {error}"))?
        {
            return Err(format!(
                "integration server exited early with status {status}"
            ));
        }
        if http_request(&args.host, args.port, "GET", "/", None).is_ok() {
            println!("integration: server is ready");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "server did not become ready within {} seconds",
        args.wait_secs
    ))
}

fn run_http_check(host: &str, port: u16, check: &HttpCheck) -> Result<(), String> {
    let response = http_request(
        host,
        port,
        check.method.as_str(),
        check.path.as_str(),
        check.body.as_deref(),
    )?;
    if response.status != check.status {
        return Err(format!(
            "{} {} expected HTTP {}, got HTTP {}",
            check.method, check.path, check.status, response.status
        ));
    }
    if let Some(expected) = &check.contains {
        if !response.raw.contains(expected) {
            return Err(format!(
                "{} {} expected response to contain `{}`",
                check.method, check.path, expected
            ));
        }
    }
    println!(
        "integration: {} {} -> HTTP {}",
        check.method, check.path, check.status
    );
    Ok(())
}

fn run_websocket_check(host: &str, port: u16, check: &WebSocketCheck) -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("cannot create WebSocket integration runtime: {error}"))?;
    runtime.block_on(run_websocket_pair_check(host, port, check))
}

async fn run_websocket_pair_check(
    host: &str,
    port: u16,
    check: &WebSocketCheck,
) -> Result<(), String> {
    let first_url = websocket_url(host, port, &check.first_path);
    let second_url = websocket_url(host, port, &check.second_path);
    let (mut first_socket, _) = connect_async(first_url.as_str())
        .await
        .map_err(|error| format!("cannot connect WebSocket {first_url}: {error}"))?;
    let first_initial = next_websocket_text(&mut first_socket).await?;
    require_websocket_contains(
        "first initial",
        &first_url,
        &first_initial,
        &check.first_initial_contains,
    )?;

    let (mut second_socket, _) = connect_async(second_url.as_str())
        .await
        .map_err(|error| format!("cannot connect WebSocket {second_url}: {error}"))?;
    let second_match = next_websocket_text(&mut second_socket).await?;
    let first_match = next_websocket_text(&mut first_socket).await?;
    require_websocket_contains(
        "first match",
        &first_url,
        &first_match,
        &check.first_match_contains,
    )?;
    require_websocket_contains(
        "second match",
        &second_url,
        &second_match,
        &check.second_match_contains,
    )?;
    if let Some(move_check) = &check.move_check {
        let message = format!(
            r#"{{"type":"move","row":{},"column":{}}}"#,
            move_check.row, move_check.column
        );
        first_socket
            .send(Message::Text(message.into()))
            .await
            .map_err(|error| format!("cannot send WebSocket move to {first_url}: {error}"))?;
        let first_update = next_websocket_text(&mut first_socket).await?;
        let second_update = next_websocket_text(&mut second_socket).await?;
        require_websocket_contains(
            "first update",
            &first_url,
            &first_update,
            &move_check.first_update_contains,
        )?;
        require_websocket_contains(
            "second update",
            &second_url,
            &second_update,
            &move_check.second_update_contains,
        )?;
    }
    let _ = first_socket.close(None).await;
    let _ = second_socket.close(None).await;

    if check.move_check.is_some() {
        println!(
            "integration: WS PAIR_MOVE {} + {} -> matched and moved",
            check.first_path, check.second_path
        );
    } else {
        println!(
            "integration: WS PAIR {} + {} -> matched",
            check.first_path, check.second_path
        );
    }
    Ok(())
}

fn websocket_url(host: &str, port: u16, path: &str) -> String {
    format!("ws://{host}:{port}{path}")
}

async fn next_websocket_text<S>(socket: &mut WebSocketStream<S>) -> Result<String, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let frame = timeout(Duration::from_secs(5), socket.next())
        .await
        .map_err(|_| "timed out waiting for WebSocket message".to_string())?;
    match frame {
        Some(Ok(Message::Text(text))) => Ok(text.to_string()),
        Some(Ok(Message::Binary(bytes))) => String::from_utf8(bytes.to_vec())
            .map_err(|error| format!("WebSocket binary message was not UTF-8: {error}")),
        Some(Ok(Message::Close(_))) => Err("WebSocket closed before expected message".to_string()),
        Some(Ok(other)) => Err(format!("unexpected WebSocket message: {other:?}")),
        Some(Err(error)) => Err(format!("cannot read WebSocket message: {error}")),
        None => Err("WebSocket ended before expected message".to_string()),
    }
}

fn require_websocket_contains(
    label: &str,
    url: &str,
    actual: &str,
    expected: &str,
) -> Result<(), String> {
    if actual.contains(expected) {
        return Ok(());
    }
    Err(format!(
        "WebSocket {label} message from {url} expected to contain `{expected}`, got `{actual}`"
    ))
}

fn http_request(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<HttpResponse, String> {
    let mut stream = StdTcpStream::connect((host, port))
        .map_err(|error| format!("cannot connect to {host}:{port}: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("cannot set HTTP read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("cannot set HTTP write timeout: {error}"))?;

    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\nContent-Type: application/x-www-form-urlencoded\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("cannot write HTTP request: {error}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("cannot read HTTP response: {error}"))?;
    parse_http_response(&response)
}

fn parse_http_response(response: &str) -> Result<HttpResponse, String> {
    let status_line = response
        .lines()
        .next()
        .ok_or_else(|| "empty HTTP response".to_string())?;
    let mut parts = status_line.split_whitespace();
    let _version = parts
        .next()
        .ok_or_else(|| format!("malformed HTTP status line `{status_line}`"))?;
    let status = parts
        .next()
        .ok_or_else(|| format!("malformed HTTP status line `{status_line}`"))?
        .parse::<u16>()
        .map_err(|_| format!("malformed HTTP status line `{status_line}`"))?;
    Ok(HttpResponse {
        status,
        raw: response.to_string(),
    })
}

fn run_command(project_dir: &PathBuf, program: &str, args: &[&str]) -> Result<(), String> {
    run_command_with_env(project_dir, &BTreeMap::new(), program, args)
}

fn run_command_with_env(
    project_dir: &PathBuf,
    env: &BTreeMap<String, String>,
    program: &str,
    args: &[&str],
) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(project_dir)
        .envs(env)
        .status()
        .map_err(|error| format!("cannot run `{}`: {error}", command_label(program, args)))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{}` exited with {status}",
            command_label(program, args)
        ))
    }
}

fn command_label(program: &str, args: &[&str]) -> String {
    let mut label = program.to_string();
    for arg in args {
        label.push(' ');
        label.push_str(arg);
    }
    label
}

#[cfg(test)]
mod integration_test_test {
    use super::*;

    #[test]
    fn parses_default_args() {
        let args = parse_integration_args(&[]).expect("default integration args");
        assert_eq!(args.project_dir, PathBuf::from("."));
        assert_eq!(args.host, DEFAULT_HOST);
        assert_eq!(args.port, DEFAULT_PORT);
        assert_eq!(args.http_checks.len(), 1);
        assert_eq!(args.http_checks[0].path, "/health");
    }

    #[test]
    fn parses_http_check_with_body() {
        let check =
            parse_http_check("POST:/_login:200:battleship_session:username=ada&password=secret")
                .expect("http check");
        assert_eq!(check.method, "POST");
        assert_eq!(check.path, "/_login");
        assert_eq!(check.status, 200);
        assert_eq!(check.contains.as_deref(), Some("battleship_session"));
        assert_eq!(check.body.as_deref(), Some("username=ada&password=secret"));
    }

    #[test]
    fn parses_manifest_integration_flow() {
        let manifest = r#"
[package]
name = "demo"
version = "0.0.1"

[integration.default]
traits = ["compose-db", "migrations", "web-build", "web-server", "http-checks"]
host = "127.0.0.1"
port = 19090
compose_service = "postgres"
migrations = "sql"
wait_secs = 12
http_checks = ["GET:/health:200:ok", "POST:/_login:400:invalid_request"]
websocket_checks = ["PAIR:/ws?player=Ada&board=%5B%5B%22X%22%5D%5D:lobby_waiting:/ws?player=Grace&board=%5B%5B%22X%22%5D%5D:match_found:match_found"]
"#;
        let flow =
            parse_manifest_integration_flow(manifest, &PathBuf::from("terlan.toml"), "default")
                .expect("flow parse")
                .expect("flow");

        assert_eq!(flow.port, Some(19090));
        assert_eq!(flow.compose_service.as_deref(), Some("postgres"));
        assert_eq!(flow.migrations_dir, Some(PathBuf::from("sql")));
        assert_eq!(flow.wait_secs, Some(12));
        assert_eq!(flow.http_checks.expect("checks").len(), 2);
        assert_eq!(flow.websocket_checks.expect("websocket checks").len(), 1);
        assert!(flow.traits.expect("traits").http_checks);
    }

    #[test]
    fn parses_websocket_pair_check() {
        let check = parse_websocket_check(
            "PAIR:/ws?player=Ada&board=%5B%5B%22X%22%5D%5D:lobby_waiting:/ws?player=Grace&board=%5B%5B%22X%22%5D%5D:match_found:match_found",
        )
        .expect("websocket check");

        assert_eq!(check.first_path, "/ws?player=Ada&board=%5B%5B%22X%22%5D%5D");
        assert_eq!(check.first_initial_contains, "lobby_waiting");
        assert_eq!(
            check.second_path,
            "/ws?player=Grace&board=%5B%5B%22X%22%5D%5D"
        );
        assert_eq!(check.first_match_contains, "match_found");
        assert_eq!(check.second_match_contains, "match_found");
        assert_eq!(check.move_check, None);
    }

    #[test]
    fn parses_websocket_pair_move_check() {
        let check = parse_websocket_check(
            "PAIR_MOVE:/ws?player=Ada&board=%5B%5B%220%22%5D%5D:lobby_waiting:/ws?player=Grace&board=%5B%5B%220%22%2C%221%22%5D%5D:match_found:match_found:0:0:opponent_board:+",
        )
        .expect("websocket move check");

        assert_eq!(check.first_path, "/ws?player=Ada&board=%5B%5B%220%22%5D%5D");
        assert_eq!(
            check.second_path,
            "/ws?player=Grace&board=%5B%5B%220%22%2C%221%22%5D%5D"
        );
        let move_check = check.move_check.expect("move check");
        assert_eq!(move_check.row, 0);
        assert_eq!(move_check.column, 0);
        assert_eq!(move_check.first_update_contains, "opponent_board");
        assert_eq!(move_check.second_update_contains, "+");
    }

    #[test]
    fn manifest_integration_flow_rejects_uncomposable_traits() {
        let manifest = r#"
[package]
name = "demo"
version = "0.0.1"

[integration.default]
traits = ["http-checks"]
"#;
        let error =
            parse_manifest_integration_flow(manifest, &PathBuf::from("terlan.toml"), "default")
                .expect_err("flow should reject http checks without server");

        assert!(error.contains("requires `web-server`"));
    }

    #[test]
    fn manifest_integration_flow_rejects_uncomposable_websocket_traits() {
        let manifest = r#"
[package]
name = "demo"
version = "0.0.1"

[integration.default]
traits = ["websocket-checks"]
"#;
        let error =
            parse_manifest_integration_flow(manifest, &PathBuf::from("terlan.toml"), "default")
                .expect_err("flow should reject websocket checks without server");

        assert!(error.contains("requires `web-server`"));
    }

    #[test]
    fn rejects_http_check_without_absolute_path() {
        let error = parse_http_check("GET:health:200:ok").expect_err("relative path rejected");
        assert!(error.contains("must start"));
    }

    #[test]
    fn rejects_websocket_check_without_absolute_path() {
        let error = parse_websocket_check(
            "PAIR:ws?player=Ada:lobby_waiting:/ws?player=Grace:match_found:match_found",
        )
        .expect_err("relative path rejected");
        assert!(error.contains("must start"));
    }

    #[test]
    fn parses_env_file_values() {
        assert_eq!(trim_env_value("\"quoted\""), "quoted");
        assert_eq!(trim_env_value("'quoted'"), "quoted");
        assert_eq!(trim_env_value("plain"), "plain");
    }

    #[test]
    fn normalize_database_host_port_preserves_available_port() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener");
        let port = listener.local_addr().expect("address").port();
        drop(listener);

        let mut env = BTreeMap::from([("POSTGRES_PORT".to_string(), port.to_string())]);
        normalize_database_host_port(&mut env).expect("normalize");
        assert_eq!(env.get("POSTGRES_PORT"), Some(&port.to_string()));
    }

    #[test]
    fn normalize_database_host_port_replaces_busy_port() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener");
        let port = listener.local_addr().expect("address").port();

        let mut env = BTreeMap::from([("POSTGRES_PORT".to_string(), port.to_string())]);
        normalize_database_host_port(&mut env).expect("normalize");
        assert_ne!(env.get("POSTGRES_PORT"), Some(&port.to_string()));
    }

    #[test]
    fn parses_http_status() {
        let response = parse_http_response("HTTP/1.1 201 Created\r\n\r\n{}").expect("response");
        assert_eq!(response.status, 201);
        assert!(response.raw.contains("Created"));
    }
}
