use super::manifest_and_arguments::{
    HttpCheck, HttpResponse, IntegrationArgs, ServerGuard, DEFAULT_DB_HOST, DEFAULT_DB_NAME,
    DEFAULT_DB_PASSWORD, DEFAULT_DB_PORT, DEFAULT_DB_USER,
};

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream as StdTcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::CliState;

#[path = "phase_execution_websocket.rs"]
mod websocket_checks;
pub(super) use websocket_checks::run_websocket_check;

pub(super) fn read_env_file(path: &Path) -> Result<BTreeMap<String, String>, String> {
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

pub(super) fn trim_env_value(value: &str) -> &str {
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

pub(super) fn run_database_phase(
    project_dir: &Path,
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

pub(super) fn reset_compose_dependencies(
    project_dir: &Path,
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

pub(super) fn normalize_database_host_port(
    app_env: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    normalize_database_host_port_with(app_env, port_is_available, free_local_port)
}

pub(super) fn configure_database_url(app_env: &mut BTreeMap<String, String>) -> Result<(), String> {
    if app_env
        .get("TERLAN_DATABASE_URL")
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(());
    }
    if app_env
        .get("DATABASE_URL")
        .is_some_and(|value| !value.trim().is_empty())
    {
        app_env.remove("TERLAN_DATABASE_URL");
        return Ok(());
    }

    let host = env_value(app_env, "POSTGRES_HOST", DEFAULT_DB_HOST);
    let user = env_value(app_env, "POSTGRES_USER", DEFAULT_DB_USER);
    let password = env_value(app_env, "POSTGRES_PASSWORD", DEFAULT_DB_PASSWORD);
    let database = env_value(app_env, "POSTGRES_DB", DEFAULT_DB_NAME);
    let port = env_value(app_env, "POSTGRES_PORT", DEFAULT_DB_PORT)
        .parse::<u16>()
        .map_err(|error| format!("invalid POSTGRES_PORT for database URL: {error}"))?;

    let mut url = url::Url::parse("postgresql://localhost")
        .map_err(|error| format!("cannot initialize database URL: {error}"))?;
    url.set_username(user)
        .map_err(|()| "invalid POSTGRES_USER for database URL".to_string())?;
    url.set_password(Some(password))
        .map_err(|()| "invalid POSTGRES_PASSWORD for database URL".to_string())?;
    url.set_host(Some(host))
        .map_err(|error| format!("invalid POSTGRES_HOST for database URL: {error}"))?;
    url.set_port(Some(port))
        .map_err(|()| "invalid POSTGRES_PORT for database URL".to_string())?;
    url.set_path(database);
    app_env.insert("TERLAN_DATABASE_URL".to_string(), url.into());
    Ok(())
}

pub(super) fn normalize_database_host_port_with(
    app_env: &mut BTreeMap<String, String>,
    port_is_available: impl Fn(u16) -> bool,
    free_local_port: impl Fn() -> Result<u16, String>,
) -> Result<(), String> {
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

pub(super) fn port_is_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub(super) fn free_local_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("cannot allocate a free local port: {error}"))?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|error| format!("cannot inspect allocated local port: {error}"))
}

pub(super) fn wait_for_database(
    project_dir: &Path,
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

pub(super) fn run_database_query(
    project_dir: &Path,
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
    Err(format!(
        "database service `{}` did not accept SELECT 1 within {} seconds",
        args.compose_service, args.wait_secs
    ))
}

pub(super) fn run_migrations_if_present(
    project_dir: &Path,
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

pub(super) fn command_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

pub(super) fn postgres_dsn(app_env: &BTreeMap<String, String>) -> String {
    format!(
        "host={} user={} password={} dbname={} port={} sslmode=disable",
        env_value(app_env, "POSTGRES_HOST", DEFAULT_DB_HOST),
        env_value(app_env, "POSTGRES_USER", DEFAULT_DB_USER),
        env_value(app_env, "POSTGRES_PASSWORD", DEFAULT_DB_PASSWORD),
        env_value(app_env, "POSTGRES_DB", DEFAULT_DB_NAME),
        env_value(app_env, "POSTGRES_PORT", DEFAULT_DB_PORT)
    )
}

pub(super) fn env_value<'a>(
    app_env: &'a BTreeMap<String, String>,
    key: &str,
    default: &'a str,
) -> &'a str {
    app_env.get(key).map(String::as_str).unwrap_or(default)
}

pub(super) fn run_build_phase(project_dir: &Path, state: &CliState) -> Result<(), String> {
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

    println!("integration: building VM target");
    run_command(
        project_dir,
        &terlc,
        &[
            "build",
            "--target",
            "terlan-vm",
            "--out-dir",
            out_arg.as_str(),
        ],
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

pub(super) fn resolve_out_dir(project_dir: &Path, state: &CliState) -> PathBuf {
    if state.out_dir.is_absolute() {
        state.out_dir.clone()
    } else {
        project_dir.join(&state.out_dir)
    }
}

pub(super) fn current_terlc() -> Result<String, String> {
    let path = std::env::current_exe()
        .map_err(|error| format!("cannot resolve current terlc executable: {error}"))?;
    Ok(path.to_string_lossy().to_string())
}

pub(super) fn spawn_server(
    project_dir: &Path,
    web_root: &Path,
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
        .env("TERLAN_SERVE_RUNTIME_ONLY", "1")
        .env("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES", "1")
        .spawn()
        .map_err(|error| format!("cannot start integration server: {error}"))?;
    Ok(ServerGuard { child })
}

pub(super) fn wait_for_server(
    server: &mut ServerGuard,
    args: &IntegrationArgs,
) -> Result<(), String> {
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

pub(super) fn run_http_check(host: &str, port: u16, check: &HttpCheck) -> Result<(), String> {
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

pub(super) fn http_request(
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

pub(super) fn parse_http_response(response: &str) -> Result<HttpResponse, String> {
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

pub(super) fn run_command(project_dir: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    run_command_with_env(project_dir, &BTreeMap::new(), program, args)
}

pub(super) fn run_command_with_env(
    project_dir: &Path,
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

pub(super) fn command_label(program: &str, args: &[&str]) -> String {
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
        let check = parse_http_check("POST:/_login:200:session_id:username=ada&password=secret")
            .expect("http check");
        assert_eq!(check.method, "POST");
        assert_eq!(check.path, "/_login");
        assert_eq!(check.status, 200);
        assert_eq!(check.contains.as_deref(), Some("session_id"));
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
        let flow = parse_manifest_integration_flow(manifest, Path::new("terlan.toml"), "default")
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
        assert_eq!(check.restore_check, None);
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
        assert_eq!(check.restore_check, None);
    }

    #[test]
    fn parses_websocket_pair_restore_check() {
        let check = parse_websocket_check(
            "PAIR_RESTORE:/ws?player=Ada:lobby_waiting:/ws?player=Grace:match_found:match_found:0:0:room_update:+:/ws?room_id=room-1&player_id=player-2:room_joined:+",
        )
        .expect("websocket restore check");

        assert!(check.move_check.is_some());
        let restore = check.restore_check.expect("restore check");
        assert_eq!(restore.path, "/ws?room_id=room-1&player_id=player-2");
        assert_eq!(restore.entry_contains, "room_joined");
        assert_eq!(restore.view_contains, "+");
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
        let error = parse_manifest_integration_flow(manifest, Path::new("terlan.toml"), "default")
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
        let error = parse_manifest_integration_flow(manifest, Path::new("terlan.toml"), "default")
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
        let port = 55_432;
        let mut env = BTreeMap::from([("POSTGRES_PORT".to_string(), port.to_string())]);
        normalize_database_host_port_with(
            &mut env,
            |candidate| candidate == port,
            || Err("replacement should not be allocated".to_string()),
        )
        .expect("normalize");
        assert_eq!(env.get("POSTGRES_PORT"), Some(&port.to_string()));
    }

    #[test]
    fn normalize_database_host_port_replaces_busy_port() {
        let port = 55_432;
        let replacement = 55_433;
        let mut env = BTreeMap::from([("POSTGRES_PORT".to_string(), port.to_string())]);
        normalize_database_host_port_with(&mut env, |_| false, || Ok(replacement))
            .expect("normalize");
        assert_eq!(env.get("POSTGRES_PORT"), Some(&replacement.to_string()));
    }

    #[test]
    fn configure_database_url_uses_normalized_postgres_environment() {
        let mut env = BTreeMap::from([
            ("POSTGRES_HOST".to_string(), "127.0.0.1".to_string()),
            ("POSTGRES_PORT".to_string(), "55433".to_string()),
            ("POSTGRES_USER".to_string(), "test user".to_string()),
            ("POSTGRES_PASSWORD".to_string(), "p@ss/word".to_string()),
            ("POSTGRES_DB".to_string(), "test db".to_string()),
        ]);

        configure_database_url(&mut env).expect("configure database URL");

        assert_eq!(
            env.get("TERLAN_DATABASE_URL").map(String::as_str),
            Some("postgresql://test%20user:p%40ss%2Fword@127.0.0.1:55433/test%20db")
        );
    }

    #[test]
    fn configure_database_url_preserves_explicit_database_url() {
        let database_url = "postgresql://external.example/demo";
        let mut env = BTreeMap::from([
            ("TERLAN_DATABASE_URL".to_string(), String::new()),
            ("DATABASE_URL".to_string(), database_url.to_string()),
        ]);

        configure_database_url(&mut env).expect("preserve database URL");

        assert!(!env.contains_key("TERLAN_DATABASE_URL"));
        assert_eq!(
            env.get("DATABASE_URL").map(String::as_str),
            Some(database_url)
        );
    }

    #[test]
    fn parses_http_status() {
        let response = parse_http_response("HTTP/1.1 201 Created\r\n\r\n{}").expect("response");
        assert_eq!(response.status, 201);
        assert!(response.raw.contains("Created"));
    }
}
#[cfg(test)]
use super::manifest_and_arguments::{
    parse_http_check, parse_integration_args, parse_manifest_integration_flow,
    parse_websocket_check, DEFAULT_HOST, DEFAULT_PORT,
};
