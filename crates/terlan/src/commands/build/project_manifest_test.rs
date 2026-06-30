use super::model::{ProjectServerTls, ProjectServerTlsProvider};
use super::*;
use std::path::PathBuf;

/// Returns a stable synthetic manifest path for parser tests.
///
/// Inputs:
/// - No inputs.
///
/// Output:
/// - Path used in parser diagnostics.
///
/// Transformation:
/// - Builds a path without touching the filesystem.
fn manifest_path() -> PathBuf {
    PathBuf::from("terlan.toml")
}

#[test]
fn project_manifest_parses_package_name_with_default_source_root() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse");

    assert_eq!(parsed.package.name, "demo");
    assert_eq!(parsed.package.version, "0.0.1");
    assert_eq!(parsed.package.namespace, None);
    assert_eq!(parsed.source_roots, vec!["src"]);
    assert_eq!(parsed.artifact, ProjectArtifactKind::BeamThin);
}

#[test]
fn project_manifest_parses_package_namespace() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"std-native-polars\"\nversion = \"0.0.4\"\nnamespace = \"std.native.polars\"\n",
            &manifest_path(),
        )
        .expect("manifest should parse package namespace");

    assert_eq!(parsed.package.name, "std-native-polars");
    assert_eq!(
        parsed.package.namespace.as_deref(),
        Some("std.native.polars")
    );
}

#[test]
fn project_manifest_rejects_invalid_package_namespace() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\nnamespace = \"std.Native\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject invalid package namespace");

    assert!(err.contains("namespace `std.Native` segments must start"));
}

#[test]
fn project_manifest_parses_explicit_source_roots() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\", \"lib\"]\nartifact = \"beam-thin\"\n",
            &manifest_path(),
        )
        .expect("manifest should parse");

    assert_eq!(parsed.package.name, "demo");
    assert_eq!(parsed.package.version, "0.0.1");
    assert_eq!(parsed.source_roots, vec!["src", "lib"]);
    assert_eq!(parsed.artifact, ProjectArtifactKind::BeamThin);
}

#[test]
fn project_manifest_parses_library_artifact_kind() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"library\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse library artifact kind");

    assert_eq!(parsed.artifact, ProjectArtifactKind::Library);
}

#[test]
fn project_manifest_wasm_parses_core_target_metadata() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasm-core\"\n\n[target.wasm]\nprofile = \"core\"\nexports = [\"main.Math.add\"]\nvalidation_engine = \"wasmtime\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse wasm core target metadata");

    assert_eq!(parsed.artifact, ProjectArtifactKind::WasmCore);
    assert_eq!(
        parsed.wasm_target,
        Some(ProjectWasmTarget {
            profile: ProjectWasmProfile::Core,
            exports: vec!["main.Math.add".to_string()],
            bridge: None,
            capabilities: Vec::new(),
            world: None,
            validation_engine: Some("wasmtime".to_string()),
        })
    );
    assert_eq!(parsed.wasi_target, None);
}

#[test]
fn project_manifest_wasm_parses_browser_target_metadata() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasm-browser\"\n\n[target.wasm]\nprofile = \"browser\"\nexports = [\"app.TodoList\", \"app.TodoStore\"]\nbridge = \"generated-js\"\ncapabilities = [\"browser.console\", \"browser.scope\", \"browser.fetch\"]\nvalidation_engine = \"browser-playwright\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse wasm browser target metadata");

    assert_eq!(parsed.artifact, ProjectArtifactKind::WasmBrowser);
    assert_eq!(
        parsed.wasm_target,
        Some(ProjectWasmTarget {
            profile: ProjectWasmProfile::Browser,
            exports: vec!["app.TodoList".to_string(), "app.TodoStore".to_string()],
            bridge: Some("generated-js".to_string()),
            capabilities: vec![
                "browser.console".to_string(),
                "browser.scope".to_string(),
                "browser.fetch".to_string(),
            ],
            world: None,
            validation_engine: Some("browser-playwright".to_string()),
        })
    );
}

#[test]
fn project_manifest_wasm_parses_wasi_cli_target_metadata() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasi-cli\"\n\n[target.wasi]\nprofile = \"cli\"\nworld = \"wasi:cli/command\"\ncapabilities = [\"stdio\", \"args\", \"env\", \"filesystem.read\"]\nvalidation_engine = \"wasmtime\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse wasi cli target metadata");

    assert_eq!(parsed.artifact, ProjectArtifactKind::WasiCli);
    assert_eq!(
        parsed.wasi_target,
        Some(ProjectWasiTarget {
            profile: ProjectWasiProfile::Cli,
            world: Some("wasi:cli/command".to_string()),
            capabilities: vec![
                "stdio".to_string(),
                "args".to_string(),
                "env".to_string(),
                "filesystem.read".to_string(),
            ],
            validation_engine: Some("wasmtime".to_string()),
        })
    );
    assert_eq!(parsed.wasm_target, None);
}

#[test]
fn project_manifest_wasm_rejects_artifact_without_target_section() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasm-core\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject wasm artifact without target section");

    assert!(err.contains("[build] artifact `wasm-core` requires [target.wasm]"));
}

#[test]
fn project_manifest_wasm_rejects_mismatched_wasi_profile() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasi-http\"\n\n[target.wasi]\nprofile = \"cli\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject mismatched wasi target profile");

    assert!(err.contains("[build] artifact `wasi-http` does not match [target.wasi] profile `cli`"));
}

#[test]
fn project_manifest_parses_web_assets_config() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[web.assets]\ndirectory = \"assets\"\npublic_path = \"/assets\"\ninline_limit = 8192\nrsbuild_config = \"rsbuild.config.mjs\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse web asset config");

    assert_eq!(
        parsed.web_assets,
        Some(ProjectWebAssets {
            directory: "assets".to_string(),
            public_path: Some("/assets".to_string()),
            inline_limit: Some(8192),
            rsbuild_config: Some("rsbuild.config.mjs".to_string()),
        })
    );
}

#[test]
fn project_manifest_accepts_integration_flow_config() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[integration.default]\ntraits = [\"compose-db\", \"web-build\", \"web-server\", \"http-checks\", \"websocket-checks\"]\nhttp_checks = [\"GET:/health:200:ok\"]\nwebsocket_checks = [\"PAIR:/ws?player=Ada:lobby_waiting:/ws?player=Grace:match_found:match_found\"]\n",
        &manifest_path(),
    )
    .expect("manifest should accept integration flow config");

    assert_eq!(parsed.package.name, "demo");
}

#[test]
fn project_manifest_parses_server_tls_manual_config() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server.tls]\nmode = \"manual\"\ncert = \"cert.pem\"\nkey = \"key.pem\"\npassphrase_env = \"TERLAN_TLS_PASSPHRASE\"\nca = \"ca.pem\"\nserver_name = \"localhost\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse server tls manual config");

    assert_eq!(
        parsed.server_tls,
        Some(ProjectServerTls {
            mode: ProjectServerTlsMode::Manual,
            domains: Vec::new(),
            email: None,
            primary_provider: None,
            fallback_provider: None,
            cert: Some("cert.pem".to_string()),
            key: Some("key.pem".to_string()),
            passphrase_env: Some("TERLAN_TLS_PASSPHRASE".to_string()),
            ca: Some("ca.pem".to_string()),
            server_name: Some("localhost".to_string()),
            trust_local: None,
        })
    );
}

#[test]
fn project_manifest_accepts_absent_server_tls_config() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse without server tls config");

    assert_eq!(parsed.server_tls, None);
}

#[test]
fn project_manifest_rejects_server_tls_without_mode() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server.tls]\ncert = \"cert.pem\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject server tls without mode");

    assert!(err.contains("[server.tls] requires mode"));
}

#[test]
fn project_manifest_parses_server_tls_auto_config() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server.tls]\nmode = \"auto\"\ndomains = [\"example.com\"]\nemail = \"admin@example.com\"\nprimary_provider = \"letsencrypt\"\nfallback_provider = \"zerossl\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse server tls auto config");

    assert_eq!(
        parsed.server_tls,
        Some(ProjectServerTls {
            mode: ProjectServerTlsMode::Auto,
            domains: vec!["example.com".to_string()],
            email: Some("admin@example.com".to_string()),
            primary_provider: Some(ProjectServerTlsProvider::LetsEncrypt),
            fallback_provider: Some(ProjectServerTlsProvider::ZeroSsl),
            cert: None,
            key: None,
            passphrase_env: None,
            ca: None,
            server_name: None,
            trust_local: None,
        })
    );
}

#[test]
fn project_manifest_rejects_server_tls_auto_without_domains() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server.tls]\nmode = \"auto\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject automatic TLS without domains");

    assert!(err.contains("mode auto requires domains"));
}

/// Verifies automatic TLS rejects fields owned by manual/internal modes.
///
/// Inputs:
/// - Project manifest with `mode = "auto"` plus a local CA field.
///
/// Output:
/// - Test passes when parser rejects the mixed-mode TLS configuration.
///
/// Transformation:
/// - Locks ACME mode as provider/domain metadata only, so future rustls/ACME
///   serving does not inherit contradictory certificate-source config.
#[test]
fn project_manifest_rejects_server_tls_auto_manual_or_internal_fields() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server.tls]\nmode = \"auto\"\ndomains = [\"example.com\"]\nca = \"ca.pem\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject automatic TLS with manual/internal fields");

    assert!(err.contains("mode auto cannot set manual or internal TLS fields"));
}

#[test]
fn project_manifest_rejects_server_tls_manual_without_key() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server.tls]\nmode = \"manual\"\ncert = \"cert.pem\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject incomplete manual server tls config");

    assert!(err.contains("mode manual requires cert and key"));
}

#[test]
fn project_manifest_parses_server_tls_internal_config() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server.tls]\nmode = \"internal\"\nserver_name = \"localhost\"\ntrust_local = true\n",
        &manifest_path(),
    )
    .expect("manifest should parse server tls internal config");

    assert_eq!(
        parsed.server_tls,
        Some(ProjectServerTls {
            mode: ProjectServerTlsMode::Internal,
            domains: Vec::new(),
            email: None,
            primary_provider: None,
            fallback_provider: None,
            cert: None,
            key: None,
            passphrase_env: None,
            ca: None,
            server_name: Some("localhost".to_string()),
            trust_local: Some(true),
        })
    );
}

#[test]
fn project_manifest_rejects_server_tls_internal_with_public_fields() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server.tls]\nmode = \"internal\"\ndomains = [\"example.com\"]\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject internal TLS with public fields");

    assert!(err.contains("mode internal cannot set public or manual TLS fields"));
}

#[test]
fn project_manifest_rejects_server_tls_manual_acme_provider() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server.tls]\nmode = \"manual\"\ncert = \"cert.pem\"\nkey = \"key.pem\"\nprimary_provider = \"letsencrypt\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject manual tls ACME provider");

    assert!(err.contains("mode manual cannot set ACME providers"));
}

#[test]
fn project_manifest_accepts_absent_web_assets_config() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse without web asset config");

    assert_eq!(parsed.web_assets, None);
}

#[test]
fn project_manifest_rejects_incomplete_web_assets_config() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[web.assets]\ninline_limit = 8192\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject incomplete web asset config");

    assert!(err.contains("[web.assets] requires directory"));
}

#[test]
fn project_manifest_rejects_invalid_web_assets_inline_limit() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[web.assets]\ndirectory = \"assets\"\ninline_limit = \"8192\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject string inline limit");

    assert!(err.contains("non-negative integer"));
}

#[test]
fn project_manifest_rejects_missing_package_name() {
    let err = parse_project_manifest("[package]\nversion = \"0.0.1\"\n", &manifest_path())
        .expect_err("manifest should reject missing package name");

    assert!(err.contains("requires [package] name"));
}

#[test]
fn project_manifest_rejects_missing_package_version() {
    let err = parse_project_manifest("[package]\nname = \"demo\"\n", &manifest_path())
        .expect_err("manifest should reject missing package version");

    assert!(err.contains("requires [package] version"));
}

#[test]
fn project_manifest_rejects_invalid_package_name() {
    let err = parse_project_manifest(
        "[package]\nname = \"Demo\"\nversion = \"0.0.1\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject invalid package name");

    assert!(err.contains("must start with a lowercase ASCII letter"));
}

#[test]
fn project_manifest_rejects_invalid_package_version() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.1\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject invalid package version");

    assert!(err.contains("major.minor.patch"));
}

#[test]
fn project_manifest_rejects_unsupported_artifact_kind() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"beam-standalone\"\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject unsupported artifact kind");

    assert!(err.contains("unsupported [build] artifact `beam-standalone`"));
}

/// Verifies manifest parsing rejects empty source-root entries.
///
/// Inputs:
/// - A manifest with `source_roots = ["src", ""]`.
///
/// Output:
/// - Test passes when parsing returns a source-root diagnostic.
///
/// Transformation:
/// - Exercises an adversarial project layout shape that could otherwise create
///   accidental repository-root traversal during build discovery.
#[test]
fn adversarial_project_manifest_rejects_empty_source_root_entries() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\", \"\"]\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject empty source root entries");

    assert!(err.contains("source_roots cannot contain empty entries"));
}

#[test]
fn project_manifest_accepts_reserved_empty_dependency_sections() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[dependencies]\n\n[target.erlang.dependencies]\n\n[target.js.dependencies]\n\n[target.rust.dependencies]\n",
            &manifest_path(),
        )
        .expect("manifest should accept reserved dependency section boundaries");

    assert_eq!(parsed.package.name, "demo");
    assert_eq!(parsed.package.version, "0.0.1");
    assert_eq!(parsed.artifact, ProjectArtifactKind::BeamThin);
    assert!(parsed.dependencies.is_empty());
}

#[test]
fn project_manifest_parses_dependency_source_metadata() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[dependencies]\nlocal_utils = { path = \"../local_utils\" }\n\n[target.erlang.dependencies]\ncowboy = { hex = \"cowboy\", version = \"2.12.0\" }\n\n[target.js.dependencies]\nzod = { npm = \"zod\", version = \"3.25.0\" }\n\n[target.rust.dependencies]\nserde = { cargo = \"serde\", version = \"1.0.0\" }\n",
            &manifest_path(),
        )
        .expect("manifest should parse dependency metadata");

    assert_eq!(parsed.dependencies.len(), 4);
    assert_eq!(
        parsed.dependencies[0],
        ProjectDependency {
            alias: "local_utils".to_string(),
            scope: ProjectDependencyScope::Local,
            source: ProjectDependencySource::Path {
                path: "../local_utils".to_string()
            },
        }
    );
    assert_eq!(
        parsed.dependencies[1],
        ProjectDependency {
            alias: "cowboy".to_string(),
            scope: ProjectDependencyScope::Target(ProjectTarget::Erlang),
            source: ProjectDependencySource::Hex {
                package: "cowboy".to_string(),
                version: "2.12.0".to_string()
            },
        }
    );
    assert_eq!(
        parsed.dependencies[2],
        ProjectDependency {
            alias: "zod".to_string(),
            scope: ProjectDependencyScope::Target(ProjectTarget::Js),
            source: ProjectDependencySource::Npm {
                package: "zod".to_string(),
                version: "3.25.0".to_string()
            },
        }
    );
    assert_eq!(
        parsed.dependencies[3],
        ProjectDependency {
            alias: "serde".to_string(),
            scope: ProjectDependencyScope::Target(ProjectTarget::Rust),
            source: ProjectDependencySource::Cargo {
                package: "serde".to_string(),
                version: "1.0.0".to_string(),
                features: Vec::new()
            },
        }
    );
}

#[test]
fn project_manifest_parses_rust_dependency_feature_metadata() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.rust.dependencies]\npolars = { cargo = \"polars\", version = \"0.54.4\", features = [\"lazy\", \"csv\", \"strings\"] }\n",
            &manifest_path(),
        )
        .expect("manifest should parse Rust dependency feature metadata");

    assert_eq!(
        parsed.dependencies[0],
        ProjectDependency {
            alias: "polars".to_string(),
            scope: ProjectDependencyScope::Target(ProjectTarget::Rust),
            source: ProjectDependencySource::Cargo {
                package: "polars".to_string(),
                version: "0.54.4".to_string(),
                features: vec!["lazy".to_string(), "csv".to_string(), "strings".to_string()]
            },
        }
    );
}

#[test]
fn project_manifest_parses_erlang_package_adapter_metadata() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.erlang.package]\nadapter = \"rebar3-compatible\"\n",
            &manifest_path(),
        )
        .expect("manifest should parse Erlang package adapter metadata");

    assert_eq!(
        parsed.erlang_package_adapter,
        Some(ProjectErlangPackageAdapter::Rebar3Compatible)
    );
}

#[test]
fn project_manifest_parses_native_rust_helper_metadata() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"terlan-polars\"\nversion = \"0.1.0\"\n\n[native.rust]\ncrate = \"terlan_polars_native\"\npath = \"native\"\nhelper = \"terlan-polars-safe-native\"\nhelper_env = \"TERLAN_SAFE_NATIVE_PATH\"\nfeatures = [\"real-polars\"]\n",
            &manifest_path(),
        )
        .expect("manifest should parse native Rust helper metadata");

    assert_eq!(
        parsed.native_rust,
        Some(ProjectNativeRust {
            crate_name: "terlan_polars_native".to_string(),
            path: "native".to_string(),
            helper: "terlan-polars-safe-native".to_string(),
            helper_env: "TERLAN_SAFE_NATIVE_PATH".to_string(),
            features: vec!["real-polars".to_string()],
        })
    );
}

#[test]
fn project_manifest_rejects_partial_native_rust_helper_metadata() {
    let err = parse_project_manifest(
        "[package]\nname = \"terlan-polars\"\nversion = \"0.1.0\"\n\n[native.rust]\ncrate = \"terlan_polars_native\"\npath = \"native\"\nhelper = \"terlan-polars-safe-native\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject partial native Rust helper metadata");

    assert!(err.contains("[native.rust] requires `helper_env`"));
}

#[test]
fn project_manifest_rejects_unsupported_erlang_package_adapter() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.erlang.package]\nadapter = \"rebar3-plugin\"\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject unsupported Erlang package adapter");

    assert!(err.contains("unsupported [target.erlang.package] adapter `rebar3-plugin`"));
}

#[test]
fn project_manifest_rejects_registry_dependency_in_local_scope() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[dependencies]\ncowboy = { hex = \"cowboy\", version = \"2.12.0\" }\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject registry dependency in local scope");

    assert!(err.contains("[dependencies] entries must use exactly"));
}

#[test]
fn project_manifest_rejects_wrong_target_dependency_source() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.erlang.dependencies]\nzod = { npm = \"zod\", version = \"3.25.0\" }\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject wrong target dependency source");

    assert!(err.contains("{ hex = \"...\", version = \"...\" }"));
}

/// Verifies target dependency entries cannot mix multiple package managers.
///
/// Inputs:
/// - A Rust-target dependency declaring both Cargo and npm source keys.
///
/// Output:
/// - Test passes when parsing returns the exact-source-shape diagnostic.
///
/// Transformation:
/// - Guards dependency resolution from accepting ambiguous cross-ecosystem
///   source metadata in one dependency entry.
#[test]
fn adversarial_project_manifest_rejects_mixed_dependency_sources() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.rust.dependencies]\nserde = { cargo = \"serde\", npm = \"serde\", version = \"1.0.0\" }\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject mixed dependency source keys");

    assert!(err.contains("{ cargo = \"...\", version = \"...\" }"));
}

#[test]
fn project_manifest_rejects_dependency_without_version() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.rust.dependencies]\nserde = { cargo = \"serde\" }\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject dependency without version");

    assert!(err.contains("{ cargo = \"...\", version = \"...\" }"));
}

#[test]
fn project_manifest_rejects_unsupported_section() {
    let err = parse_project_manifest("[workspace]\nfoo = \"bar\"\n", &manifest_path())
        .expect_err("manifest should reject unsupported section");

    assert!(err.contains("unsupported project manifest section `workspace`"));
}
