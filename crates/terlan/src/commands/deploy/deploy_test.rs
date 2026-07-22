use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::*;
use crate::support::test_fs;

/// Creates a unique temporary deploy-command directory.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path under the process temp directory.
///
/// Transformation:
/// - Delegates to the shared test filesystem helper with the deploy namespace.
fn temp_dir(name: &str) -> PathBuf {
    test_fs::temp_path("deploy", name)
}

#[test]
fn parse_deploy_args_accepts_plan_default_project_dir() {
    match parse_deploy_args(&["plan".to_string()]) {
        DeployArgs::Plan(args) => assert_eq!(args.project_dir, PathBuf::from(".")),
        _ => panic!("expected deploy plan args"),
    }
}

#[test]
fn parse_deploy_args_accepts_plan_project_dir() {
    match parse_deploy_args(&["plan".to_string(), "app".to_string()]) {
        DeployArgs::Plan(args) => assert_eq!(args.project_dir, PathBuf::from("app")),
        _ => panic!("expected deploy plan args"),
    }
}

#[test]
fn parse_deploy_args_rejects_extra_plan_operands() {
    match parse_deploy_args(&["plan".to_string(), "app".to_string(), "extra".to_string()]) {
        DeployArgs::Error(err) => assert!(err.contains("at most one project directory")),
        _ => panic!("expected deploy plan usage error"),
    }
}

#[test]
fn build_deploy_plan_defaults_to_terlan_vm_runtime_capability() {
    let manifest = crate::commands::build::project_manifest::parse_project_manifest(
        r#"[package]
name = "demo"
version = "0.0.1"
"#,
        &PathBuf::from("terlan.toml"),
    )
    .expect("parse manifest");

    let plan = build_deploy_plan(&manifest).expect("build deploy plan");
    let json = serde_json::to_value(&plan).expect("serialize deploy plan");

    assert_eq!(json["build"]["artifact"], "terlan-vm");
    assert_eq!(
        json["capabilities"],
        serde_json::json!(["runtime.terlan-vm"])
    );
}

#[test]
fn build_deploy_plan_projects_manifest_capabilities() {
    let manifest = crate::commands::build::project_manifest::parse_project_manifest(
        r#"[package]
name = "demo"
version = "0.0.1"
namespace = "demo.cloud"

[build]
source_roots = ["src", "lib"]
artifact = "terlan-vm"

[web.assets]
directory = "assets"
public_path = "/assets"
inline_limit = 2048
rsbuild_config = "rsbuild.config.mjs"

[server.tls]
mode = "manual"
cert = "cert.pem"
key = "key.pem"
server_name = "localhost"

[dependencies]
shared = { path = "../shared" }

[target.js.dependencies]
zod = { npm = "zod", version = "3.25.0" }

[target.rust.dependencies]
serde = { cargo = "serde", version = "1.0.0", features = ["derive"] }
"#,
        &PathBuf::from("terlan.toml"),
    )
    .expect("parse manifest");

    let plan = build_deploy_plan(&manifest).expect("build deploy plan");
    let json = serde_json::to_value(&plan).expect("serialize deploy plan");

    assert_eq!(json["schema"], DEPLOY_PLAN_SCHEMA);
    assert_eq!(json["generated_by"]["tool"], "terlc");
    assert_eq!(json["generated_by"]["experimental"], true);
    assert_eq!(json["package"]["name"], "demo");
    assert_eq!(json["package"]["namespace"], "demo.cloud");
    assert_eq!(
        json["build"]["source_roots"],
        serde_json::json!(["src", "lib"])
    );
    assert!(json["build"]
        .as_object()
        .expect("build object")
        .get("package_adapter")
        .is_none());
    assert_eq!(
        json["capabilities"],
        serde_json::json!([
            "dependency.local",
            "dependency.target.js",
            "dependency.target.rust",
            "http.tls",
            "runtime.terlan-vm",
            "web.assets",
            "web.rsbuild"
        ])
    );
    assert_eq!(json["web_assets"]["directory"], "assets");
    assert_eq!(json["server_tls"]["mode"], "manual");
    assert_eq!(json["dependencies"][0]["alias"], "shared");
    assert_eq!(json["dependencies"][0]["source"]["kind"], "path");
    assert_eq!(json["dependencies"][2]["alias"], "serde");
    assert_eq!(
        json["dependencies"][2]["source"]["features"],
        serde_json::json!(["derive"])
    );
}

#[test]
fn build_deploy_plan_rejects_legacy_beam_artifact() {
    let err = crate::commands::build::project_manifest::parse_project_manifest(
        r#"[package]
name = "demo"
version = "0.0.1"

[build]
artifact = "beam-thin"
"#,
        &PathBuf::from("terlan.toml"),
    )
    .expect_err("legacy beam artifact should fail while parsing manifest");

    assert!(err.contains("unsupported [build] artifact `beam-thin`"));
}

#[test]
fn build_deploy_plan_rejects_legacy_erlang_dependencies() {
    let err = crate::commands::build::project_manifest::parse_project_manifest(
        r#"[package]
name = "demo"
version = "0.0.1"

[target.erlang.dependencies]
cowboy = { hex = "cowboy", version = "2.12.0" }
"#,
        &PathBuf::from("terlan.toml"),
    )
    .expect_err("legacy erlang dependency should fail while parsing manifest");

    assert!(err.contains("unsupported project manifest section `target.erlang.dependencies`"));
}

#[test]
fn build_deploy_plan_rejects_legacy_target_package_metadata() {
    let err = crate::commands::build::project_manifest::parse_project_manifest(
        r#"[package]
name = "demo"
version = "0.0.1"

[target.erlang.package]
adapter = "rebar3-compatible"
"#,
        &PathBuf::from("terlan.toml"),
    )
    .expect_err("legacy erlang package should fail while parsing manifest");

    assert!(err.contains("unsupported project manifest section `target.erlang.package`"));
}

#[test]
fn write_deploy_plan_writes_cloud_json_artifact() {
    let root = temp_dir("write_plan");
    let project_dir = root.join("app");
    let out_dir = root.join("build");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        project_dir.join(PROJECT_MANIFEST_FILE),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .expect("write manifest");

    let path = write_deploy_plan(&project_dir, &out_dir).expect("write deploy plan");
    assert_eq!(path, out_dir.join("cloud").join(DEPLOY_PLAN_FILE));

    let json: Value = serde_json::from_str(&fs::read_to_string(&path).expect("read deploy plan"))
        .expect("parse deploy plan");
    assert_eq!(json["schema"], DEPLOY_PLAN_SCHEMA);
    assert_eq!(json["package"]["name"], "demo");
    assert_eq!(json["build"]["artifact"], "terlan-vm");

    fs::remove_dir_all(root).expect("remove test root");
}
