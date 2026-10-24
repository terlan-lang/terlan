//! VM packages execute returned router declarations and preserve their graph owner.

use super::*;
use crate::commands::serve::handle_vm_stream_http1_request;

#[test]
fn vm_service_discovery_preserves_separate_websocket_metadata() {
    let root = tempfile::tempdir().unwrap();
    let source_path = root.path().join("app/WebSocket.terl");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::write(&source_path, "module app.WebSocket.\npub route(): String -> \"/socket\".\npub protocol(): String -> \"chat.v1\".\n").unwrap();
    let artifact = WebRouteSourceArtifact {
        module: "app.WebSocket".into(),
        source_path: source_path.to_string_lossy().into_owned(),
        manifest_path: Some("src/app/WebSocket.terl".into()),
    };
    let rows = routes::discover_web_route_manifest_from_sources(&[artifact]).unwrap();
    assert!(rows.handlers.is_empty());
    assert_eq!(rows.websockets.len(), 1);
    assert_eq!(rows.websockets[0].route, "/socket");
    assert_eq!(rows.websockets[0].protocol, "chat.v1");
}

#[test]
fn vm_service_routes_execute_computed_imported_and_captured_handlers_with_middleware() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("src/app");
    fs::create_dir_all(&app).unwrap();
    fs::write(root.path().join("terlan.toml"), "[package]\nname = \"source_routes\"\nversion = \"1.0.0\"\nnamespace = \"app\"\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n").unwrap();
    fs::write(
        app.join("Pages.terl"),
        r#"module app.Pages.
import app.Rendering.
import type std.http.Request.{Request as Incoming}.
import type std.http.Response.{Response as Outgoing}.
pub home(_request: Incoming): Outgoing -> Rendering.body("imported").
"#,
    )
    .unwrap();
    fs::write(app.join("Rendering.terl"), "module app.Rendering.\nimport std.http.Response.\nimport type std.http.Response.{Response as Outgoing}.\npub body(text: String): Outgoing -> Response.text(text).\n").unwrap();
    fs::write(app.join("Model.terl"), "module app.Model.\npub type Request = Int.\npub type Response = String.\npub convert(value: Request): Response -> \"unrelated\".\n").unwrap();
    let source = r#"module app.Http.
import app.Pages.{home as start}.
import std.http.Router as Routing.
import std.http.{Response, Sse, WebSocket}.
import type std.http.Router.Router.
pub router(): Router ->
    let prefix = "/v1";
    let _unused = Routing.new().get("/ghost", start);
    Routing.new().get("/imported", start)
        .group(prefix, (child) -> child.get("/hello", (_request) -> Response.text(prefix)))
        .map_response((_request, response) -> response.with_header("X-Owner", "router"))
        .sse(prefix + "/events", Sse.endpoint(2, 256))
        .websocket(prefix + "/socket", WebSocket.endpoint(2, 256)).
"#;
    let path = app.join("Http.terl");
    fs::write(&path, source).unwrap();
    let mut artifacts = Vec::new();
    for name in ["Http", "Pages", "Rendering", "Model"] {
        if let Some(mut artifact) =
            crate::commands::build::js_source_classification::web_route_source_artifact_from_file(
                &app.join(format!("{name}.terl")),
            )
            .unwrap()
        {
            artifact.manifest_path = Some(format!("src/app/{name}.terl"));
            artifacts.push(artifact);
        }
    }
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].module, "app.Http");
    let web = write_vm_service_package(
        root.path(),
        &root.path().join("_build"),
        &["src".into()],
        &artifacts,
        None,
        false,
    )
    .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(web.join("manifest.json")).unwrap()).unwrap();
    let handlers = manifest["handlers"].as_array().unwrap();
    assert_eq!(handlers.len(), 2);
    for handler in handlers {
        assert_eq!(handler["module"], "app.Http");
        assert_eq!(handler["source"]["path"], "src/app/Http.terl");
        assert_ne!(handler["route"], "/ghost");
    }
    assert_eq!(manifest["sse"][0]["route"], "/v1/events");
    assert_eq!(manifest["websockets"][0]["route"], "/v1/socket");
    assert_eq!(manifest["websockets"][0]["protocol"], "");
    for (path, body) in [("/imported", "imported"), ("/v1/hello", "/v1")] {
        let wire = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let response =
            String::from_utf8(handle_vm_stream_http1_request(&web, wire.as_bytes()).unwrap())
                .unwrap();
        assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
        let (headers, actual) = response.split_once("\r\n\r\n").unwrap();
        assert!(
            headers.to_ascii_lowercase().contains("\r\nx-owner: router"),
            "{response}"
        );
        assert_eq!(actual, body);
    }
}

#[test]
fn vm_service_route_discovery_rejects_identity_mismatch_and_nonrouter_values() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    fs::create_dir_all(&app).unwrap();
    let path = app.join("Http.terl");
    fs::write(&path, "module app.Http.\npub router(): Int -> 1.").unwrap();
    let artifact = WebRouteSourceArtifact {
        module: "app.Http".into(),
        source_path: path.to_string_lossy().into_owned(),
        manifest_path: Some("app/Http.terl".into()),
    };
    let error = routes::discover_web_route_manifest_from_sources(&[artifact]).unwrap_err();
    assert!(error.contains("expected Router record"), "{error}");
    fs::write(&path, "module app.Http.\nimport std.http.Router.\npub router(): std.http.Router.Router -> Router.new().").unwrap();
    let artifact = WebRouteSourceArtifact {
        module: "app.Wrong".into(),
        source_path: path.to_string_lossy().into_owned(),
        manifest_path: Some("app/Http.terl".into()),
    };
    let error = routes::discover_web_route_manifest_from_sources(&[artifact]).unwrap_err();
    assert!(error.contains("expected module `app.Wrong`"), "{error}");
}
