use super::model::{ProjectServerProfile, ProjectServerTls, ProjectServerTlsProvider};
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
    assert_eq!(parsed.artifact, ProjectArtifactKind::TerlanVm);
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
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\", \"lib\"]\nartifact = \"terlan-vm\"\n",
            &manifest_path(),
        )
        .expect("manifest should parse");

    assert_eq!(parsed.package.name, "demo");
    assert_eq!(parsed.package.version, "0.0.1");
    assert_eq!(parsed.source_roots, vec!["src", "lib"]);
    assert_eq!(parsed.artifact, ProjectArtifactKind::TerlanVm);
}

#[test]
fn project_manifest_parses_script_aliases() {
    let parsed = parse_project_manifest(
        "\
[package]
name = \"demo\"
version = \"0.0.1\"

[scripts]
seed = \"scripts/SeedDatabase.terl\"
db.reset = \"scripts/db/Reset.terl\"
",
        &manifest_path(),
    )
    .expect("manifest should parse script aliases");

    assert_eq!(
        parsed.scripts,
        vec![
            ProjectScript {
                name: "seed".to_string(),
                path: "scripts/SeedDatabase.terl".to_string(),
            },
            ProjectScript {
                name: "db.reset".to_string(),
                path: "scripts/db/Reset.terl".to_string(),
            },
        ]
    );
}

#[test]
fn project_manifest_rejects_unsafe_script_path() {
    let err = parse_project_manifest(
        "\
[package]
name = \"demo\"
version = \"0.0.1\"

[scripts]
seed = \"../SeedDatabase.terl\"
",
        &manifest_path(),
    )
    .expect_err("manifest should reject unsafe script path");

    assert!(err.contains("cannot use current-directory or parent traversal"));
}

#[test]
fn project_manifest_rejects_legacy_beam_thin_artifact_kind() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"beam-thin\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject legacy beam-thin artifact kind");

    assert!(err.contains("unsupported [build] artifact `beam-thin`"));
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
fn project_manifest_parses_server_production_profile() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server]\nprofile = \"production\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse production server profile");

    assert_eq!(
        parsed.server_profile,
        Some(ProjectServerProfile::Production)
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
fn project_manifest_rejects_production_internal_server_tls() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[server]\nprofile = \"production\"\n\n[server.tls]\nmode = \"internal\"\nserver_name = \"localhost\"\ntrust_local = true\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject production internal server tls config");

    assert!(err.contains("[server] profile production cannot use [server.tls] mode internal"));
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
    assert!(
        !err.contains("beam-thin"),
        "public artifact diagnostics must not advertise the legacy VM artifact: {err}"
    );
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
fn adversarial_project_manifest_rejects_empty_source_root_list() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = []\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject empty source root lists");

    assert!(err.contains("project manifest string array cannot be empty"));
}

#[test]
fn adversarial_project_manifest_rejects_absolute_source_root() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"/tmp/generated\"]\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject absolute source roots");

    assert!(err.contains("source_root `/tmp/generated` must be package-relative"));
}

#[test]
fn adversarial_project_manifest_rejects_parent_traversal_source_root() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"../sibling\"]\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject source roots that escape the package");

    assert!(
        err.contains("source_root `../sibling` cannot use current-directory or parent traversal")
    );
}

#[test]
fn adversarial_project_manifest_rejects_current_directory_source_root() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\".\"]\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject ambiguous current-directory source root");

    assert!(err.contains("source_root `.` cannot use current-directory or parent traversal"));
}

#[test]
fn adversarial_project_manifest_rejects_duplicate_source_roots() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\", \"src\"]\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject duplicate source roots");

    assert!(err.contains("source_root `src` is declared more than once"));
}

#[test]
fn adversarial_project_manifest_rejects_whitespace_padded_source_root() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\" src\"]\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject whitespace-padded source roots");

    assert!(err.contains("source_root ` src` cannot contain leading or trailing whitespace"));
}

#[test]
fn project_manifest_parses_script_entrypoints() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[scripts]\nseed-db = \"scripts/SeedDatabase.terl\"\nreports.daily = \"scripts/reports/Daily.terl\"\n",
        &manifest_path(),
    )
    .expect("manifest should parse script entrypoints");

    assert_eq!(
        parsed.scripts,
        vec![
            ProjectScript {
                name: "seed-db".to_string(),
                path: "scripts/SeedDatabase.terl".to_string(),
            },
            ProjectScript {
                name: "reports.daily".to_string(),
                path: "scripts/reports/Daily.terl".to_string(),
            },
        ]
    );
}

#[test]
fn adversarial_project_manifest_rejects_duplicate_script_aliases() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[scripts]\nseed = \"scripts/Seed.terl\"\nseed = \"scripts/Other.terl\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject duplicate script aliases");

    assert!(err.contains("[scripts] alias `seed` is declared more than once"));
}

#[test]
fn adversarial_project_manifest_rejects_invalid_script_alias() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[scripts]\nSeed = \"scripts/Seed.terl\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject invalid script aliases");

    assert!(err.contains("[scripts] alias `Seed` may contain only lowercase ASCII"));
}

#[test]
fn adversarial_project_manifest_rejects_absolute_script_path() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[scripts]\nseed = \"/tmp/Seed.terl\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject absolute script paths");

    assert!(err.contains("[scripts] path `/tmp/Seed.terl` must be package-relative"));
}

#[test]
fn adversarial_project_manifest_rejects_parent_traversal_script_path() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[scripts]\nseed = \"../Seed.terl\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject script paths that escape the package");

    assert!(err.contains(
        "[scripts] path `../Seed.terl` cannot use current-directory or parent traversal"
    ));
}

#[test]
fn adversarial_project_manifest_rejects_non_terlan_script_path() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[scripts]\nseed = \"scripts/Seed.txt\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject non-Terlan script paths");

    assert!(err.contains("[scripts] path `scripts/Seed.txt` must point to a .terl file"));
}

#[test]
fn project_manifest_accepts_reserved_empty_dependency_sections() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[dependencies]\n\n[target.js.dependencies]\n\n[target.rust.dependencies]\n",
            &manifest_path(),
        )
        .expect("manifest should accept reserved dependency section boundaries");

    assert_eq!(parsed.package.name, "demo");
    assert_eq!(parsed.package.version, "0.0.1");
    assert_eq!(parsed.artifact, ProjectArtifactKind::TerlanVm);
    assert!(parsed.dependencies.is_empty());
}

#[test]
fn project_manifest_parses_package_publication_metadata() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"terlan-polars\"\nversion = \"0.1.0\"\nnamespace = \"polars\"\ndescription = \"Polars DataFrames for Terlan\"\nlicense = \"MIT\"\nrepository = \"https://github.com/terlan-lang/terlan-polars\"\ncompiler = \">= 0.0.7\"\nlinks = [\"https://terlan.org\", \"https://pola.rs\"]\n",
        &manifest_path(),
    )
    .expect("manifest should accept publication metadata");

    assert_eq!(
        parsed.package.description.as_deref(),
        Some("Polars DataFrames for Terlan")
    );
    assert_eq!(parsed.package.license.as_deref(), Some("MIT"));
    assert_eq!(
        parsed.package.repository.as_deref(),
        Some("https://github.com/terlan-lang/terlan-polars")
    );
    assert_eq!(parsed.package.compiler.as_deref(), Some(">= 0.0.7"));
    assert_eq!(
        parsed.package.links,
        vec![
            "https://terlan.org".to_string(),
            "https://pola.rs".to_string()
        ]
    );
}

#[test]
fn project_manifest_rejects_empty_or_duplicate_publication_metadata() {
    let empty = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\nlicense = \"\"\n",
        &manifest_path(),
    )
    .expect_err("empty publication field should fail");
    assert!(empty.contains("[package] `license` cannot be empty"));

    let duplicate = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\nlinks = [\"https://terlan.org\", \"https://terlan.org\"]\n",
        &manifest_path(),
    )
    .expect_err("duplicate package link should fail");
    assert!(duplicate.contains("[package] `links` contains duplicate"));
}

#[test]
fn project_manifest_parses_dependency_source_metadata() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[dependencies]\nlocal_utils = { path = \"../local_utils\" }\nremote_utils = { git = \"https://github.com/terlan-lang/utils\", rev = \"a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4\" }\n\n[target.js.dependencies]\nzod = { npm = \"zod\", version = \"3.25.0\" }\n\n[target.rust.dependencies]\nserde = { cargo = \"serde\", version = \"1.0.0\" }\n",
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
            alias: "remote_utils".to_string(),
            scope: ProjectDependencyScope::Local,
            source: ProjectDependencySource::Git {
                url: "https://github.com/terlan-lang/utils".to_string(),
                rev: "a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4".to_string()
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
fn project_manifest_rejects_git_dependency_without_rev() {
    let err = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[dependencies]\nutils = { git = \"https://github.com/terlan-lang/utils\" }\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject unpinned git dependency");

    assert!(err.contains("[dependencies] entries must use"));
    assert!(err.contains("rev"));
}

#[test]
fn project_manifest_rejects_abbreviated_or_symbolic_git_revisions() {
    for rev in ["a1b2c3d4", "main", "v0.1.0"] {
        let source = format!(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[dependencies]\nutils = {{ git = \"https://github.com/terlan-lang/utils\", rev = \"{rev}\" }}\n"
        );
        let err = parse_project_manifest(&source, &manifest_path())
            .expect_err("manifest should reject non-immutable Git revision");

        assert!(err.contains("full 40- or 64-character hexadecimal commit id"));
        assert!(err.contains(rev));
    }
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
fn project_manifest_rejects_legacy_target_package_metadata() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.erlang.package]\nadapter = \"rebar3-compatible\"\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject legacy target package metadata");

    assert!(err.contains("unsupported project manifest section `target.erlang.package`"));
}

#[test]
fn project_manifest_parses_native_rust_helper_metadata() {
    let parsed = parse_project_manifest(
            "[package]\nname = \"terlan-polars\"\nversion = \"0.1.0\"\n\n[native.rust]\ncrate = \"terlan_polars_native\"\npath = \"native\"\nhelper = \"terlan-polars-native-boundary\"\nhelper_env = \"TERLAN_NATIVE_BOUNDARY_HELPER_PATH\"\nfeatures = [\"real-polars\"]\n",
            &manifest_path(),
        )
        .expect("manifest should parse native Rust helper metadata");

    assert_eq!(
        parsed.native_rust,
        Some(ProjectNativeRust {
            crate_name: "terlan_polars_native".to_string(),
            path: "native".to_string(),
            helper: "terlan-polars-native-boundary".to_string(),
            helper_env: "TERLAN_NATIVE_BOUNDARY_HELPER_PATH".to_string(),
            features: vec!["real-polars".to_string()],
        })
    );
}

#[test]
fn project_manifest_rejects_partial_native_rust_helper_metadata() {
    let err = parse_project_manifest(
        "[package]\nname = \"terlan-polars\"\nversion = \"0.1.0\"\n\n[native.rust]\ncrate = \"terlan_polars_native\"\npath = \"native\"\nhelper = \"terlan-polars-native-boundary\"\n",
        &manifest_path(),
    )
    .expect_err("manifest should reject partial native Rust helper metadata");

    assert!(err.contains("[native.rust] requires `helper_env`"));
}

#[test]
fn project_manifest_rejects_unsupported_legacy_target_package_metadata() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.erlang.package]\nadapter = \"rebar3-plugin\"\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject unsupported legacy target package metadata");

    assert!(err.contains("unsupported project manifest section `target.erlang.package`"));
}

#[test]
fn project_manifest_rejects_registry_dependency_in_local_scope() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[dependencies]\ncowboy = { hex = \"cowboy\", version = \"2.12.0\" }\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject registry dependency in local scope");

    assert!(
        err.contains("[dependencies] entries must use { path = \"...\" }"),
        "{err}"
    );
}

#[test]
fn project_manifest_rejects_wrong_target_dependency_source() {
    let err = parse_project_manifest(
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.erlang.dependencies]\nzod = { npm = \"zod\", version = \"3.25.0\" }\n",
            &manifest_path(),
        )
        .expect_err("manifest should reject wrong target dependency source");

    assert!(err.contains("unsupported project manifest section `target.erlang.dependencies`"));
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
