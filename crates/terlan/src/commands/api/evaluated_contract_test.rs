use super::*;
use std::fs;

fn fixture(source: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("src/app/Http.terl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, source).unwrap();
    (root, path)
}

const SOURCE: &str = r#"module app.Http.
import std.http.{Router, Response}.
import type std.http.Request.Request.
import type std.http.Response.Response.
import type std.http.Router.Router.

pub home(_request: Request): Response -> Response.text("home").

pub router(): Router ->
    let prefix = "/api";
    let segment = "users";
    Router.new()
        .get("/", home)
        .group(prefix, (child) ->
            child.get("/" + segment, (_request) -> Response.text(segment)))
        .fallback(home).
"#;

#[test]
fn api_discovery_evaluates_computed_groups_closures_and_source_changes() {
    let (root, path) = fixture(SOURCE);
    let workspace = root.path().join("build");
    let first = from_router_file(&path, &workspace, "Service", "1").unwrap();
    assert_eq!(first.routes.len(), 9);
    let home = first.routes.iter().find(|route| route.path == "/").unwrap();
    assert_eq!(home.handler, "home");
    let users = first
        .routes
        .iter()
        .find(|route| route.path == "/api/users")
        .unwrap();
    assert!(users.handler.starts_with("closure_"), "{}", users.handler);
    let repeat = from_router_file(&path, &workspace, "Service", "1").unwrap();
    assert_eq!(first, repeat);
    fs::write(&path, SOURCE.replace("\"/api\"", "\"/v2\"")).unwrap();
    let changed = from_router_file(&path, &workspace, "Service", "1").unwrap();
    assert!(changed.routes.iter().any(|route| route.path == "/v2/users"));
    assert!(!changed
        .routes
        .iter()
        .any(|route| route.path == "/api/users"));
}

#[test]
fn api_discovery_rejects_missing_source_and_malformed_syntax() {
    let (root, path) = fixture("pub router(");
    let error = from_router_file(&path, root.path(), "Service", "1").unwrap_err();
    assert!(error.contains("cannot parse source"), "{error}");
    fs::remove_file(&path).unwrap();
    let error = from_router_file(&path, root.path(), "Service", "1").unwrap_err();
    assert!(error.contains("cannot read"), "{error}");
}

#[test]
fn api_discovery_resolves_imports_with_relative_source_and_independent_output() {
    let cwd = std::env::current_dir().unwrap();
    let root = tempfile::Builder::new()
        .prefix(".api-discovery-")
        .tempdir_in(&cwd)
        .unwrap();
    let path = root.path().join("src/app/Http.terl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path.with_file_name("Pages.terl"),
        r#"module app.Pages.
import std.http.Response.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub home(_request: Request): Response -> Response.text("imported").
"#,
    )
    .unwrap();
    let source = SOURCE
        .replace(
            "import std.http.{Router, Response}.",
            "import std.http.{Router, Response}.\nimport app.Pages.",
        )
        .replace(
            "pub home(_request: Request): Response -> Response.text(\"home\").",
            "",
        )
        .replace(", home)", ", Pages.home)")
        .replace(".fallback(home)", ".fallback(Pages.home)");
    fs::write(&path, source).unwrap();
    let output = tempfile::tempdir().unwrap();
    let contract = from_router_file(
        path.strip_prefix(&cwd).unwrap(),
        output.path(),
        "Service",
        "1",
    )
    .unwrap();
    assert_eq!(
        contract
            .routes
            .iter()
            .find(|route| route.path == "/")
            .unwrap()
            .handler,
        "app.Pages.home"
    );
}

#[test]
fn api_discovery_rejects_nonrouters_duplicates_and_channel_schemas() {
    for (source, expected) in [
        (SOURCE.replace("pub router()", "router()"), "missing public"),
        (
            SOURCE.replace(".get(\"/\", home)", ".get(\"/\", home).get(\"/\", home)"),
            "duplicate VM HTTP route",
        ),
        (
            "module app.Http.\npub router(): Int -> 1.".into(),
            "expected Router record",
        ),
        (
            SOURCE
                .replace(
                    "import std.http.{Router, Response}.",
                    "import std.http.{Router, Response, Sse}.",
                )
                .replace(".fallback(home)", ".sse(\"/events\", Sse.endpoint(2, 256))"),
            "api_emit.channel_schema",
        ),
    ] {
        let (root, path) = fixture(&source);
        let error =
            from_router_file(&path, &root.path().join("build"), "Service", "1").unwrap_err();
        assert!(error.contains(expected), "expected {expected}, got {error}");
    }
}

#[test]
fn api_discovery_does_not_grant_session_services() {
    let source = SOURCE.replace("import std.http.{Router, Response}.", "import std.http.{Router, Response}.\nimport type std.core.Option.")
        .replace("let prefix = \"/api\";", "let prefix = std.core.Option.with_default(lookup(\"identity\"), \"/api\");")
        + "\n@compiler.native {std.http.session.lookup}\nlookup(identity: String): Option[String].\n";
    let (root, path) = fixture(&source);
    let error = from_router_file(&path, &root.path().join("build"), "Service", "1").unwrap_err();
    assert!(
        error.contains("api_emit.external_wait") && error.contains("Capability"),
        "{error}"
    );
}
