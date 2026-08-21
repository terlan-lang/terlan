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
        DeployArgs::Plan(args) => {
            assert_eq!(args.project_dir, PathBuf::from("."));
            assert_eq!(args.schema, DeployPlanSchema::V2);
        }
        _ => panic!("expected deploy plan args"),
    }
}

#[test]
fn parse_deploy_args_accepts_plan_project_dir() {
    match parse_deploy_args(&["plan".to_string(), "app".to_string()]) {
        DeployArgs::Plan(args) => {
            assert_eq!(args.project_dir, PathBuf::from("app"));
            assert_eq!(args.schema, DeployPlanSchema::V2);
        }
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
fn parse_deploy_args_accepts_explicit_v1_compatibility_schema() {
    match parse_deploy_args(&[
        "plan".to_string(),
        "fixture".to_string(),
        "--schema".to_string(),
        "v1".to_string(),
    ]) {
        DeployArgs::Plan(args) => {
            assert_eq!(args.project_dir, PathBuf::from("fixture"));
            assert_eq!(args.schema, DeployPlanSchema::V1);
        }
        _ => panic!("expected deploy plan args"),
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

    let path = write_deploy_plan(
        &DeployPlanArgs {
            project_dir: project_dir.clone(),
            schema: DeployPlanSchema::V1,
        },
        &out_dir,
    )
    .expect("write deploy plan");
    assert_eq!(path, out_dir.join("cloud").join(DEPLOY_PLAN_FILE));

    let json: Value = serde_json::from_str(&fs::read_to_string(&path).expect("read deploy plan"))
        .expect("parse deploy plan");
    assert_eq!(json["schema"], DEPLOY_PLAN_SCHEMA);
    assert_eq!(json["package"]["name"], "demo");
    assert_eq!(json["build"]["artifact"], "terlan-vm");

    fs::remove_dir_all(root).expect("remove test root");
}

#[test]
fn write_semantic_deploy_plan_emits_routes_sources_migrations_and_inspection() {
    let root = temp_dir("write_semantic_plan");
    let project_dir = root.join("registry");
    let out_dir = root.join("build");
    fs::create_dir_all(project_dir.join("src/registry")).expect("create source dir");
    fs::create_dir_all(project_dir.join("priv/migrations")).expect("create migration dir");
    fs::write(
        project_dir.join(PROJECT_MANIFEST_FILE),
        r#"[package]
name = "registry"
version = "0.1.0"

[build]
source_roots = ["src"]
artifact = "terlan-vm"

[deploy]
environment = ["PORT", "DATABASE_URL"]
secrets = ["DATABASE_URL"]
migrations = ["priv/migrations/001_initial.sql"]
outbound_network = ["objects.example.test:443"]
rollback = "migration-compatible"

[deploy.health]
path = "/health"
interval_secs = 10
timeout_secs = 2

[deploy.resources]
cpu_millis = 500
memory_mb = 384
processes = 2
"#,
    )
    .expect("write manifest");
    fs::write(
        project_dir.join("src/registry/Main.terl"),
        "module registry.Main.\n\npub main(): Int -> 0.\n",
    )
    .expect("write main source");
    fs::write(
        project_dir.join("src/registry/Http.terl"),
        r#"module registry.Http.

import std.http.Router.
import std.http.Response.
import type std.http.Request.Request.
import type std.http.Response.Response.
import type std.http.Router.Router.

pub router(): Router ->
    let router = Router.get(Router.new(), "/health", health);
    router.get("/packages/:name", package).

pub health(_request: Request): Response -> Response.text("ok").
pub package(_request: Request): Response -> Response.text("package").
"#,
    )
    .expect("write router source");
    fs::write(
        project_dir.join("priv/migrations/001_initial.sql"),
        "create table packages (name text primary key);\n",
    )
    .expect("write migration");

    let path = write_deploy_plan(
        &DeployPlanArgs {
            project_dir: project_dir.clone(),
            schema: DeployPlanSchema::V2,
        },
        &out_dir,
    )
    .expect("write semantic deploy plan");
    let bytes = fs::read(&path).expect("read semantic plan");
    let json: Value = serde_json::from_slice(&bytes).expect("parse semantic plan");

    assert_eq!(json["schema"], SEMANTIC_DEPLOY_PLAN_SCHEMA);
    assert_eq!(json["release"]["id"], "registry@0.1.0");
    assert_eq!(json["target"]["runtime"], "terlan-vm");
    assert_eq!(
        json["services"][0]["process"]["entrypoint"],
        "registry.Main.main/0"
    );
    assert_eq!(json["services"][0]["process"]["count"], 2);
    assert_eq!(json["routes"][0]["path"], "/health");
    assert_eq!(json["routes"][0]["handler"], "registry.Http.health/1");
    assert_eq!(
        json["routes"][0]["source"]["path"],
        "src/registry/Http.terl"
    );
    assert_eq!(json["configuration"]["secrets"][0]["name"], "DATABASE_URL");
    assert!(json["configuration"]["secrets"][0]
        .as_object()
        .expect("secret reference")
        .get("value")
        .is_none());
    assert_eq!(json["migrations"][0]["id"], "001_initial");
    assert_eq!(json["migrations"][0]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(json["resources"]["memory_mb"], 384);
    assert_eq!(json["outbound_network"][0], "objects.example.test:443");
    assert_eq!(json["rollback"]["policy"], "migration-compatible");
    assert_eq!(json["rollback"]["automatic"], true);
    assert_eq!(json["sources"].as_array().expect("sources").len(), 2);
    let inspection =
        fs::read_to_string(out_dir.join("cloud/deploy-plan.txt")).expect("read inspection view");
    assert!(inspection.contains("Release: registry@0.1.0"));
    assert!(inspection.contains("GET /health -> registry.Http.health/1"));
    assert!(!inspection.contains("postgres://"));

    let first = bytes;
    write_deploy_plan(
        &DeployPlanArgs {
            project_dir: project_dir.clone(),
            schema: DeployPlanSchema::V2,
        },
        &out_dir,
    )
    .expect("rewrite semantic deploy plan");
    assert_eq!(first, fs::read(&path).expect("read repeated plan"));

    fs::remove_dir_all(root).expect("remove test root");
}

#[test]
fn semantic_deploy_plan_rejects_missing_health_handler_and_unsupported_target() {
    let root = temp_dir("semantic_rejections");
    let project_dir = root.join("app");
    fs::create_dir_all(project_dir.join("src/demo")).expect("create source dir");
    fs::write(
        project_dir.join("src/demo/Main.terl"),
        "module demo.Main.\n\npub main(): Int -> 0.\n",
    )
    .expect("write source");
    fs::write(
        project_dir.join(PROJECT_MANIFEST_FILE),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[deploy.health]\npath = \"/health\"\n",
    )
    .expect("write health manifest");
    let manifest = read_project_manifest(&project_dir.join(PROJECT_MANIFEST_FILE))
        .expect("parse health manifest");
    let Err(health_error) = build_semantic_deploy_plan(&project_dir, &manifest) else {
        panic!("missing health route should fail");
    };
    assert!(health_error.contains("error[deploy.health_route]"));

    fs::write(
        project_dir.join(PROJECT_MANIFEST_FILE),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"library\"\n",
    )
    .expect("write unsupported target manifest");
    let manifest = read_project_manifest(&project_dir.join(PROJECT_MANIFEST_FILE))
        .expect("parse target manifest");
    let Err(target_error) = build_semantic_deploy_plan(&project_dir, &manifest) else {
        panic!("unsupported deploy target should fail");
    };
    assert!(target_error.contains("error[deploy.target_capability]"));

    fs::remove_dir_all(root).expect("remove test root");
}
