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
fn project_manifest_parses_web_assets_config() {
    let parsed = parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[web.assets]\ndirectory = \"assets\"\npublic_path = \"/assets\"\ninline_limit = 8192\n",
        &manifest_path(),
    )
    .expect("manifest should parse web asset config");

    assert_eq!(
        parsed.web_assets,
        Some(ProjectWebAssets {
            directory: "assets".to_string(),
            public_path: Some("/assets".to_string()),
            inline_limit: Some(8192),
        })
    );
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
