use super::*;
use crate::commands::serve::args::{ServeCliOverrides, ServeHandlerRuntime};
use crate::support::test_fs;

fn fixture(name: &str) -> (PathBuf, ServeArgs) {
    let root = test_fs::temp_path("serve-config", name);
    let web_root = root.join("_build/web");
    fs::create_dir_all(&web_root).expect("create web root");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"config_test\"\nversion = \"0.0.1\"\n",
    )
    .expect("write manifest");
    let args = ServeArgs {
        web_root,
        host: DEFAULT_SERVE_HOST.to_string(),
        port: DEFAULT_SERVE_PORT,
        poll_ms: DEFAULT_POLL_MS,
        max_body_bytes: crate::commands::serve::args::DEFAULT_MAX_BODY_BYTES,
        handler_runtime: ServeHandlerRuntime::Static,
        check_only: true,
        overrides: ServeCliOverrides::default(),
    };
    (root, args)
}

#[test]
fn cli_adapter_forwards_only_explicit_values_to_package_config() {
    let (_root, mut args) = fixture("cli-adapter");
    args.host = "127.0.0.9".into();
    args.port = 9999;
    args.poll_ms = 77;
    let implicit = cli_overrides(&args);
    assert_eq!(implicit.host, None);
    assert_eq!(implicit.port, None);
    assert_eq!(implicit.poll_ms, None);
    assert_eq!(implicit.allow_public, None);

    args.overrides = ServeCliOverrides {
        host: true,
        port: true,
        poll_ms: true,
        protocol: Some("http1".into()),
        allow_public: true,
        max_connections: Some(100),
        max_request_bytes: Some(4096),
        max_body_bytes: Some(2048),
        max_header_bytes: Some(1024),
        request_timeout_ms: Some(200),
        idle_timeout_ms: Some(300),
        queue_capacity: Some(50),
        handler_pool_size: Some(10),
        shutdown_grace_ms: Some(400),
        telemetry: Some("off".into()),
        log_format: Some("json".into()),
    };
    let config = resolve_effective_serve_config_with_env(&args, []).unwrap();
    assert_eq!(config.host, args.host);
    assert_eq!(config.port, args.port);
    assert_eq!(config.poll_ms, args.poll_ms);
    assert_eq!(config.max_connections, 100);
    assert_eq!(config.max_request_bytes, 4096);
    assert_eq!(config.max_body_bytes, 2048);
    assert_eq!(config.max_header_bytes, 1024);
    assert_eq!(config.request_timeout_ms, 200);
    assert_eq!(config.idle_timeout_ms, 300);
    assert_eq!(config.queue_capacity, 50);
    assert_eq!(config.handler_pool_size, 10);
    assert_eq!(config.shutdown_grace_ms, 400);
    assert!(config.allow_public);
    assert_eq!(config.protocol, server_config::ServeProtocol::Http1);
    assert_eq!(config.telemetry, server_config::ServeTelemetry::Off);
    assert_eq!(config.log_format, server_config::ServeLogFormat::Json);
    assert_eq!(
        config
            .sources
            .values()
            .filter(|value| *value == "cli")
            .count(),
        16
    );
    assert_eq!(config.sources["certificate_cache"], "default");
}

#[test]
fn effective_config_precedence_is_default_manifest_environment_cli() {
    let (root, mut args) = fixture("precedence");
    fs::write(
        root.join("terlan.toml"),
        "[serve]\nhost = \"127.0.0.2\"\nport = 3100\nqueue_capacity = 50\n",
    )
    .expect("write overrides");
    args.host = "127.0.0.4".to_string();
    args.overrides.host = true;
    let config = resolve_effective_serve_config_with_env(
        &args,
        [
            ("TERLAN_SERVE_HOST".to_string(), "127.0.0.3".to_string()),
            ("TERLAN_SERVE_PORT".to_string(), "3200".to_string()),
        ],
    )
    .expect("resolve config");
    assert_eq!(config.host, "127.0.0.4");
    assert_eq!(config.port, 3200);
    assert_eq!(config.queue_capacity, 50);
    assert_eq!(config.sources["host"], "cli");
    assert_eq!(config.sources["port"], "environment");
    assert_eq!(config.sources["queue_capacity"], "manifest");
}

#[test]
fn effective_config_rejects_unsafe_public_default_before_socket_startup() {
    let (_root, args) = fixture("public-bind");
    let error = resolve_effective_serve_config_with_env(
        &args,
        [("TERLAN_SERVE_HOST".to_string(), "0.0.0.0".to_string())],
    )
    .expect_err("public default must be rejected");
    assert!(error.contains("error[serve.config.public_bind]"));
}

#[test]
fn effective_config_accepts_explicit_public_bind_and_tracks_its_origin() {
    let (_root, args) = fixture("public-opt-in");
    let config = resolve_effective_serve_config_with_env(
        &args,
        [
            ("TERLAN_SERVE_HOST".to_string(), "0.0.0.0".to_string()),
            ("TERLAN_SERVE_ALLOW_PUBLIC".to_string(), "true".to_string()),
        ],
    )
    .expect("explicit public bind");
    assert!(config.allow_public);
    assert_eq!(config.sources["allow_public"], "environment");
}

#[test]
fn effective_config_rejects_malformed_and_ambiguous_values() {
    let (root, args) = fixture("malformed");
    fs::write(root.join("terlan.toml"), "[serve]\nunknown_limit = 4\n")
        .expect("write malformed config");
    let error = resolve_effective_serve_config_with_env(&args, []).expect_err("unknown key");
    assert!(error.contains("unknown field"));

    fs::write(
        root.join("terlan.toml"),
        "[serve]\nmax_request_bytes = 100\nmax_body_bytes = 101\n",
    )
    .expect("write invalid limits");
    let error = resolve_effective_serve_config_with_env(&args, []).expect_err("ambiguous limits");
    assert!(error.contains("max_body_bytes cannot exceed max_request_bytes"));
}

#[test]
fn effective_config_rejects_unsupported_protocol_and_bad_environment() {
    let (_root, args) = fixture("protocol");
    let error = resolve_effective_serve_config_with_env(
        &args,
        [("TERLAN_SERVE_PROTOCOL".to_string(), "http3".to_string())],
    )
    .expect_err("unsupported protocol");
    assert!(error.contains("unsupported protocol `http3`"));
    let error = resolve_effective_serve_config_with_env(
        &args,
        [(
            "TERLAN_SERVE_ALLOW_PUBLIC".to_string(),
            "perhaps".to_string(),
        )],
    )
    .expect_err("bad boolean");
    assert!(error.contains("expects true or false"));
}

#[test]
fn effective_config_fingerprint_and_artifact_are_replay_stable() {
    let (root, args) = fixture("fingerprint");
    let first = resolve_effective_serve_config_with_env(&args, []).expect("first config");
    let second = resolve_effective_serve_config_with_env(&args, []).expect("second config");
    assert_eq!(first.fingerprint, second.fingerprint);
    assert!(first.fingerprint.starts_with("sha256:"));
    let path = write_effective_serve_config(&first, &args.web_root).expect("write artifact");
    assert_eq!(
        path,
        root.join("build/artifacts/serve-effective-config.json")
    );
    let artifact: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("read artifact")).expect("parse artifact");
    assert_eq!(artifact["schema"], SERVE_CONFIG_SCHEMA);
    assert_eq!(artifact["fingerprint"], first.fingerprint);
}

#[test]
fn effective_config_rejects_path_escape_and_zero_pressure_limits() {
    let (root, args) = fixture("paths");
    fs::write(
        root.join("terlan.toml"),
        "[serve]\ncertificate_cache = \"../outside\"\n",
    )
    .expect("write path escape");
    let error = resolve_effective_serve_config_with_env(&args, []).expect_err("path escape");
    assert!(error.contains("cannot escape the project root"));

    fs::write(root.join("terlan.toml"), "[serve]\nqueue_capacity = 0\n").expect("write zero queue");
    let error = resolve_effective_serve_config_with_env(&args, []).expect_err("zero queue");
    assert!(error.contains("queue_capacity must be greater than zero"));
}
