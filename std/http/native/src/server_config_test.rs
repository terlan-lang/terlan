use super::*;

fn resolve_at(
    root: &Path,
    manifest: &str,
    env: &[(&str, &str)],
    cli: ServeOverrides,
) -> Result<EffectiveServeConfig, ServiceError> {
    resolve(
        ServeManifest::parse(manifest).expect("manifest"),
        env.iter()
            .map(|(name, value)| (name.to_string(), value.to_string())),
        cli,
        root.to_path_buf(),
        Some(root),
        4,
    )
}

#[test]
fn defaults_and_fingerprint_are_deterministic_without_host_environment() {
    let root = tempfile::tempdir().unwrap();
    let config = resolve_at(root.path(), "", &[], ServeOverrides::default()).unwrap();
    assert_eq!(config.host, DEFAULT_SERVE_HOST);
    assert_eq!(config.port, DEFAULT_SERVE_PORT);
    assert_eq!(config.poll_ms, DEFAULT_POLL_MS);
    assert_eq!(config.max_body_bytes, DEFAULT_MAX_BODY_BYTES);
    assert_eq!(config.handler_pool_size, 4);
    assert_eq!(config.protocol, ServeProtocol::Http1);
    assert_eq!(config.telemetry, ServeTelemetry::Basic);
    assert_eq!(config.log_format, ServeLogFormat::Text);
    assert_eq!(
        config.certificate_cache,
        root.path().join(".terlan/certificates")
    );
    assert_eq!(config.sources.len(), 19);
    assert!(config.sources.values().all(|source| source == "default"));
    let replay = resolve_at(root.path(), "", &[], ServeOverrides::default()).unwrap();
    assert_eq!(config, replay);
    let mut unsigned = config.clone();
    unsigned.fingerprint.clear();
    let digest = Sha256::digest(serde_json::to_vec(&unsigned).unwrap());
    let digest = bytes::Bytes::copy_from_slice(&digest);
    assert_eq!(config.fingerprint, format!("sha256:{digest:x}"));
    let changed = resolve_at(
        root.path(),
        "[serve]\nport = 0",
        &[],
        ServeOverrides::default(),
    )
    .unwrap();
    assert_eq!(changed.port, 0);
    assert_ne!(config.fingerprint, changed.fingerprint);
}

const ENV: &[(&str, &str)] = &[
    ("TERLAN_SERVE_HOST", "127.0.0.2"),
    ("TERLAN_SERVE_PORT", "3200"),
    ("TERLAN_SERVE_PROTOCOL", "http1"),
    ("TERLAN_SERVE_ALLOW_PUBLIC", "true"),
    ("TERLAN_SERVE_POLL_MS", "20"),
    ("TERLAN_SERVE_MAX_CONNECTIONS", "100"),
    ("TERLAN_SERVE_MAX_REQUEST_BYTES", "4096"),
    ("TERLAN_SERVE_MAX_BODY_BYTES", "2048"),
    ("TERLAN_SERVE_MAX_HEADER_BYTES", "1024"),
    ("TERLAN_SERVE_REQUEST_TIMEOUT_MS", "200"),
    ("TERLAN_SERVE_IDLE_TIMEOUT_MS", "300"),
    ("TERLAN_SERVE_QUEUE_CAPACITY", "50"),
    ("TERLAN_SERVE_HANDLER_POOL_SIZE", "10"),
    ("TERLAN_SERVE_TELEMETRY", "off"),
    ("TERLAN_SERVE_LOG_FORMAT", "json"),
    ("TERLAN_SERVE_SHUTDOWN_GRACE_MS", "400"),
    ("TERLAN_SERVE_CERTIFICATE_CACHE", "certificates"),
];

#[test]
fn all_fields_obey_manifest_environment_cli_precedence_and_provenance() {
    let root = tempfile::tempdir().unwrap();
    let manifest = "[serve]\nhost = '127.0.0.5'\nport = 3100\n\
                    [server]\nprofile = 'test'\n[server.tls]\nmode = 'manual'";
    let environment = resolve_at(root.path(), manifest, ENV, ServeOverrides::default()).unwrap();
    assert_eq!(environment.host, "127.0.0.2");
    assert_eq!(environment.port, 3200);
    assert_eq!(environment.poll_ms, 20);
    assert_eq!(environment.max_connections, 100);
    assert_eq!(environment.max_request_bytes, 4096);
    assert_eq!(environment.max_body_bytes, 2048);
    assert_eq!(environment.max_header_bytes, 1024);
    assert_eq!(environment.request_timeout_ms, 200);
    assert_eq!(environment.idle_timeout_ms, 300);
    assert_eq!(environment.queue_capacity, 50);
    assert_eq!(environment.handler_pool_size, 10);
    assert_eq!(environment.shutdown_grace_ms, 400);
    assert!(environment.allow_public);
    assert_eq!(environment.telemetry, ServeTelemetry::Off);
    assert_eq!(environment.log_format, ServeLogFormat::Json);
    assert_eq!(environment.profile, "test");
    assert_eq!(environment.tls_mode, "manual");
    assert_eq!(environment.sources["profile"], "manifest");
    assert_eq!(environment.sources["tls_mode"], "manifest");
    for (key, _) in ENV {
        assert_eq!(
            environment.sources[&key.strip_prefix("TERLAN_SERVE_").unwrap().to_lowercase()],
            "environment"
        );
    }
    let fields = "host = '127.0.0.3'\nport = 3300\nprotocol = 'http1'\n\
        allow_public = false\npoll_ms = 30\nmax_connections = 200\n\
        max_request_bytes = 8192\nmax_body_bytes = 4096\nmax_header_bytes = 2048\n\
        request_timeout_ms = 500\nidle_timeout_ms = 600\nqueue_capacity = 100\n\
        handler_pool_size = 20\nshutdown_grace_ms = 700\ntelemetry = 'full'\n\
        log_format = 'text'\ncertificate_cache = 'cli-certificates'";
    let cli: ServeOverrides = basic_toml::from_str(fields).unwrap();
    let effective = resolve_at(root.path(), manifest, ENV, cli).unwrap();
    let from_manifest = resolve_at(
        root.path(),
        &format!("[serve]\n{fields}"),
        &[],
        ServeOverrides::default(),
    )
    .unwrap();
    let mut expected = from_manifest;
    expected.profile.clone_from(&effective.profile);
    expected.tls_mode.clone_from(&effective.tls_mode);
    expected.fingerprint.clone_from(&effective.fingerprint);
    expected.sources.clone_from(&effective.sources);
    assert_eq!(effective, expected);
    for (key, _) in ENV {
        assert_eq!(
            effective.sources[&key.strip_prefix("TERLAN_SERVE_").unwrap().to_lowercase()],
            "cli"
        );
    }
    assert_eq!(effective.telemetry, ServeTelemetry::Full);
    assert_ne!(effective.fingerprint, environment.fingerprint);
}

#[test]
fn malformed_environment_is_rejected_even_when_cli_would_override_it() {
    let root = tempfile::tempdir().unwrap();
    for (name, _) in ENV.iter().filter(|(name, _)| {
        name.ends_with("_MS")
            || name.ends_with("_BYTES")
            || name.ends_with("_SIZE")
            || name.ends_with("_CONNECTIONS")
            || name.ends_with("_CAPACITY")
            || *name == "TERLAN_SERVE_PORT"
    }) {
        for value in ["-1", "1.5", "18446744073709551616", "", " 10"] {
            let error = resolve_at(root.path(), "", &[(name, value)], ServeOverrides::default())
                .unwrap_err();
            assert!(
                error.to_string().contains("expects an unsigned integer"),
                "{name}: {error}"
            );
        }
    }
    for (name, value, message) in [
        ("TERLAN_SERVE_PORT", "65536", "exceeds u16"),
        ("TERLAN_SERVE_ALLOW_PUBLIC", "yes", "expects true or false"),
    ] {
        let error = resolve_at(
            root.path(),
            "",
            &[(name, value)],
            ServeOverrides {
                port: Some(3000),
                allow_public: Some(true),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
    for (value, expected) in [("true", true), ("1", true), ("false", false), ("0", false)] {
        let config = resolve_at(
            root.path(),
            "",
            &[("TERLAN_SERVE_ALLOW_PUBLIC", value)],
            ServeOverrides::default(),
        )
        .unwrap();
        assert_eq!(config.allow_public, expected);
    }
}

#[test]
fn limits_and_cross_field_constraints_fail_before_serving() {
    let root = tempfile::tempdir().unwrap();
    for field in [
        "poll_ms",
        "max_connections",
        "max_request_bytes",
        "max_body_bytes",
        "max_header_bytes",
        "request_timeout_ms",
        "idle_timeout_ms",
        "queue_capacity",
        "handler_pool_size",
        "shutdown_grace_ms",
    ] {
        let error = resolve_at(
            root.path(),
            &format!("[serve]\n{field} = 0"),
            &[],
            ServeOverrides::default(),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("{field} must be greater than zero")),
            "{error}"
        );
    }
    for (fields, message) in [
        (
            "max_request_bytes = 100\nmax_body_bytes = 101",
            "max_body_bytes cannot exceed",
        ),
        (
            "max_request_bytes = 100\nmax_body_bytes = 100\nmax_header_bytes = 100",
            "max_header_bytes must be smaller",
        ),
        ("queue_capacity = 16385", "queue_capacity cannot exceed"),
        (
            "handler_pool_size = 16385",
            "handler_pool_size cannot exceed",
        ),
    ] {
        let error = resolve_at(
            root.path(),
            &format!("[serve]\n{fields}"),
            &[],
            ServeOverrides::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
    let equal = resolve_at(
        root.path(),
        "[serve]\nmax_body_bytes = 10485760\nqueue_capacity = 16384\nhandler_pool_size = 16384",
        &[],
        ServeOverrides::default(),
    )
    .unwrap();
    assert_eq!(equal.max_body_bytes, equal.max_request_bytes);
    assert_eq!(equal.queue_capacity, equal.max_connections);
    assert_eq!(equal.handler_pool_size, equal.max_connections);
}

#[test]
fn public_bind_and_tls_profile_policies_are_package_owned() {
    let root = tempfile::tempdir().unwrap();
    for host in [
        "0.0.0.0",
        "::",
        "[::]",
        "0:0:0:0:0:0:0:0",
        "[0:0:0:0:0:0:0:0]",
    ] {
        let error = resolve_at(
            root.path(),
            "",
            &[("TERLAN_SERVE_HOST", host)],
            ServeOverrides::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("serve.config.public_bind"));
        let config = resolve_at(
            root.path(),
            "",
            &[("TERLAN_SERVE_HOST", host)],
            ServeOverrides {
                allow_public: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(config.host, host);
    }
    for host in ["127.0.0.1", "::1", "[::1]", "0:0:0:0:0:0:0:1", "localhost"] {
        let config = resolve_at(
            root.path(),
            "",
            &[("TERLAN_SERVE_HOST", host)],
            ServeOverrides::default(),
        )
        .unwrap();
        assert_eq!(config.host, host);
        assert!(!config.allow_public);
    }
    for profile in ["development", "test", "staging", "production"] {
        for mode in ["plain", "auto", "manual", "internal"] {
            let manifest =
                format!("[server]\nprofile = '{profile}'\n[server.tls]\nmode = '{mode}'");
            let result = resolve_at(root.path(), &manifest, &[], ServeOverrides::default());
            if profile == "production" && mode == "internal" {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("production cannot use internal TLS"));
            } else {
                let config = result.unwrap();
                assert_eq!(config.profile, profile);
                assert_eq!(config.tls_mode, mode);
            }
        }
    }
}

#[test]
fn unknown_values_and_manifest_fields_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    for (source, message) in [
        ("[serve]\nhost = ' '", "host cannot be empty"),
        ("[serve]\nprotocol = 'http3'", "unsupported protocol"),
        (
            "[serve]\ntelemetry = 'verbose'",
            "unsupported telemetry mode",
        ),
        ("[serve]\nlog_format = 'xml'", "unsupported log format"),
        ("[server]\nprofile = 'unknown'", "unsupported profile"),
        ("[server.tls]\nmode = 'unknown'", "unsupported TLS mode"),
    ] {
        let error = resolve_at(root.path(), source, &[], ServeOverrides::default()).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
    for source in [
        "[serve]\nunknown = 1",
        "[serve]\nport = -1",
        "[serve]\nport = 65536",
        "[serve]\nport = '3000'",
        "[serve]\nport = 1\nport = 2",
    ] {
        assert!(ServeManifest::parse(source).is_err(), "{source}");
    }
    assert!(ServeManifest::parse(
        "[package]\nname = 'app'\n[server]\nother = 1\n[server.tls]\ncert = 'cert.pem'"
    )
    .is_ok());
}

#[test]
fn assets_and_certificate_paths_are_validated_without_creating_files() {
    let root = tempfile::tempdir().unwrap();
    let error = resolve_at(
        &root.path().join("missing"),
        "",
        &[],
        ServeOverrides::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("is not a directory"));
    let error = resolve_at(
        root.path(),
        "[serve]\ncertificate_cache = '../outside'",
        &[],
        ServeOverrides::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot escape the project root"));
    assert_eq!(
        resolve_project_path(None, PathBuf::from("local")).unwrap(),
        PathBuf::from("local")
    );
    assert_eq!(
        resolve_project_path(Some(root.path()), root.path().to_path_buf()).unwrap(),
        root.path()
    );
    assert_eq!(
        resolve_project_path(Some(root.path()), PathBuf::from("local")).unwrap(),
        root.path().join("local")
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn non_utf8_certificate_path_cannot_produce_an_unreplayable_fingerprint() {
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let mut config = MutableConfig::defaults(root.path(), 1);
    config.certificate_cache = PathBuf::from(std::ffi::OsString::from_vec(vec![0xff]));
    let error = finish(config, root.path().to_path_buf(), None).unwrap_err();
    assert!(error.to_string().contains("encode effective config"));
}
