//! Deployment routes come from returned source values, not builder-shaped syntax.

use super::*;
use crate::commands::build::project_manifest::read_project_manifest;
use serde_json::Value;

const IMPORTS: &str = "import std.http.{Router, Response}.\nimport type std.http.Router.Router.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\n";

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src/app")).unwrap();
    fs::write(root.path().join("terlan.toml"), "[package]\nname = \"app\"\nversion = \"1.0.0\"\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n").unwrap();
    write_source(root.path(), "Main", "pub main(): Int -> 0.");
    root
}

fn write_source(root: &Path, module: &str, body: &str) {
    fs::write(
        root.join(format!("src/app/{module}.terl")),
        format!("module app.{module}.\n{body}"),
    )
    .unwrap();
}

fn plan(root: &Path) -> Result<Value, String> {
    let manifest = read_project_manifest(&root.join("terlan.toml"))?;
    serde_json::to_value(build_semantic_deploy_plan(root, &manifest)?)
        .map_err(|error| error.to_string())
}

#[test]
fn deployment_evaluates_returned_routes_and_imported_handlers() {
    let root = project();
    write_source(
        root.path(),
        "Pages",
        &format!(
            "{IMPORTS}
pub home(_request: Request): Response -> Response.text(\"home\").
pub make(): Router ->
    let _unused = Router.new().get(\"/phantom\", home);
    let prefix = \"/api\";
    Router.new().get(\"/\", home).group(prefix, (child) ->
        child.get(\"/users/:id\", home).get(\"/captured\", (_request) -> Response.text(prefix))).
"
        ),
    );
    // No Router import at this entrypoint: declaration discovery is not an
    // import-name heuristic, and the helper's source owns all route behavior.
    write_source(
        root.path(),
        "Http",
        "import app.Pages.\npub router(): std.http.Router.Router -> Pages.make().",
    );
    let first = plan(root.path()).unwrap();
    let routes = first["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 3);
    assert_eq!(routes[0]["path"], "/");
    assert_eq!(routes[1]["path"], "/api/captured");
    assert_eq!(routes[2]["path"], "/api/users/:id");
    for route in routes {
        if route["path"] == "/api/captured" {
            let handler = route["handler"].as_str().unwrap();
            assert!(handler.starts_with("app.Http.closure_") && handler.ends_with("/1"));
        } else {
            assert_eq!(route["handler"], "app.Pages.home/1");
        }
        assert_eq!(route["source"]["module"], "app.Http");
        assert_eq!(route["source"]["path"], "src/app/Http.terl");
    }
    assert_eq!(first["sources"].as_array().unwrap().len(), 3);
    assert_eq!(plan(root.path()).unwrap(), first);
    assert!(!root.path().join(".terlan").exists());
}

#[test]
fn deployment_evaluates_grouped_fallback_and_source_edits() {
    let root = project();
    let source = format!(
        "{IMPORTS}
pub home(_request: Request): Response -> Response.text(\"home\").
pub router(): Router ->
    let prefix = \"/admin\";
    Router.new().group(prefix, (child) -> child.fallback(home)).
"
    );
    write_source(root.path(), "Http", &source);
    let first = plan(root.path()).unwrap();
    let routes = first["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 7);
    assert_eq!(
        routes
            .iter()
            .map(|route| route["method"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"]
    );
    for route in routes {
        assert_eq!(route["path"], "/admin/*");
        assert_eq!(route["handler"], "app.Http.home/1");
    }
    write_source(
        root.path(),
        "Http",
        &source.replace("\"/admin\"", "\"/staff\""),
    );
    let changed = plan(root.path()).unwrap();
    assert_ne!(first["sources"], changed["sources"]);
    assert!(changed["routes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|route| route["path"] == "/staff/*"));
}

#[test]
fn deployment_rejects_conflicting_router_modules_and_invalid_descriptors() {
    let root = project();
    let source = format!("{IMPORTS}\npub home(_request: Request): Response -> Response.text(\"ok\").\npub router(): Router -> Router.new().get(\"/same\", home).");
    write_source(root.path(), "Http", &source);
    write_source(root.path(), "Other", &source);
    let error = plan(root.path()).unwrap_err();
    assert!(
        error.contains("deploy.route_conflict") && error.contains("GET /same"),
        "{error}"
    );
    fs::remove_file(root.path().join("src/app/Other.terl")).unwrap();
    write_source(root.path(), "Http", "pub router(): Int -> 42.");
    let error = plan(root.path()).unwrap_err();
    assert!(
        error.contains("deploy.route_evaluation") && error.contains("expected Router record"),
        "{error}"
    );
}

#[test]
fn deployment_does_not_accept_discarded_health_routes_or_invoke_other_arities() {
    let root = project();
    let path = root.path().join("terlan.toml");
    let manifest = fs::read_to_string(&path).unwrap() + "\n[deploy.health]\npath = \"/health\"\n";
    fs::write(&path, manifest).unwrap();
    write_source(
        root.path(),
        "Http",
        &format!(
            "{IMPORTS}
pub home(_request: Request): Response -> Response.text(\"ok\").
pub router(): Router ->
    let _discarded = Router.new().get(\"/health\", home);
    Router.new().
"
        ),
    );
    let error = plan(root.path()).unwrap_err();
    assert!(error.contains("deploy.health_route"), "{error}");
    write_source(root.path(), "Http", "pub router(value: Int): Int -> value.");
    let error = plan(root.path()).unwrap_err();
    assert!(error.contains("deploy.health_route"), "{error}");
}
