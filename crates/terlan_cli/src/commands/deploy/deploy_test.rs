use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::*;

/// Creates a unique temporary deploy-command directory.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path under the process temp directory.
///
/// Transformation:
/// - Combines process id and nanosecond timestamp to avoid fixture collisions.
fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("timestamp")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan_deploy_{name}_{}_{}",
        std::process::id(),
        nanos
    ))
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
fn build_deploy_plan_projects_manifest_capabilities() {
    let manifest = crate::commands::build::project_manifest::parse_project_manifest(
        r#"[package]
name = "demo"
version = "0.0.1"
namespace = "demo.cloud"

[build]
source_roots = ["src", "lib"]
artifact = "beam-thin"

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

[target.erlang.dependencies]
cowboy = { hex = "cowboy", version = "2.12.0" }

[target.js.dependencies]
zod = { npm = "zod", version = "3.25.0" }

[target.rust.dependencies]
serde = { cargo = "serde", version = "1.0.0", features = ["derive"] }

[target.erlang.package]
adapter = "rebar3-compatible"
"#,
        &PathBuf::from("terlan.toml"),
    )
    .expect("parse manifest");

    let plan = build_deploy_plan(&manifest);
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
    assert_eq!(json["build"]["erlang_package_adapter"], "rebar3-compatible");
    assert_eq!(
        json["capabilities"],
        serde_json::json!([
            "dependency.local",
            "dependency.target.erlang",
            "dependency.target.js",
            "dependency.target.rust",
            "http.tls",
            "runtime.beam",
            "target.erlang.package",
            "web.assets",
            "web.rsbuild"
        ])
    );
    assert_eq!(json["web_assets"]["directory"], "assets");
    assert_eq!(json["server_tls"]["mode"], "manual");
    assert_eq!(json["dependencies"][0]["alias"], "shared");
    assert_eq!(json["dependencies"][0]["source"]["kind"], "path");
    assert_eq!(json["dependencies"][3]["alias"], "serde");
    assert_eq!(
        json["dependencies"][3]["source"]["features"],
        serde_json::json!(["derive"])
    );
}

#[test]
fn write_deploy_plan_writes_cloud_json_artifact() {
    let root = temp_dir("write_plan");
    let project_dir = root.join("app");
    let out_dir = root.join("build");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        project_dir.join(PROJECT_MANIFEST_FILE),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("write manifest");

    let path = write_deploy_plan(&project_dir, &out_dir).expect("write deploy plan");
    assert_eq!(path, out_dir.join("cloud").join(DEPLOY_PLAN_FILE));

    let json: Value = serde_json::from_str(&fs::read_to_string(&path).expect("read deploy plan"))
        .expect("parse deploy plan");
    assert_eq!(json["schema"], DEPLOY_PLAN_SCHEMA);
    assert_eq!(json["package"]["name"], "demo");
    assert_eq!(json["build"]["artifact"], "beam-thin");

    fs::remove_dir_all(root).expect("remove test root");
}
