use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream as StdTcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{CliCommand, CliState};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

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
