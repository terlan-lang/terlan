use super::*;

fn fixture(source: &str) -> (tempfile::TempDir, WebRouteSourceArtifact) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("src/app/WebSocket.terl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, source).unwrap();
    let artifact = WebRouteSourceArtifact {
        module: "app.WebSocket".into(),
        source_path: path.to_string_lossy().into_owned(),
        manifest_path: Some("src/app/WebSocket.terl".into()),
    };
    (root, artifact)
}

fn discover(artifact: &WebRouteSourceArtifact) -> Result<WebRouteManifestRows, String> {
    routes::discover_web_route_manifest_from_sources(std::slice::from_ref(artifact))
}

#[test]
fn websocket_metadata_executes_computed_values_exact_imports_and_source_edits() {
    let source = r#"module app.WebSocket.
import app.first.Protocol as Selected.
pub route(): String ->
    let prefix = Selected.prefix();
    prefix + suffix().
suffix(): String -> "/socket".
pub protocol(): String -> Selected.name() + ".v1".
"#;
    let (root, artifact) = fixture(source);
    for (provider, prefix, protocol) in [("first", "/api", "chat"), ("second", "/wrong", "wrong")] {
        let path = root
            .path()
            .join(format!("src/app/{provider}/Protocol.terl"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("module app.{provider}.Protocol.\npub prefix(): String -> \"{prefix}\".\npub name(): String -> \"{protocol}\".\n")).unwrap();
    }
    let first = discover(&artifact).unwrap();
    assert_eq!(first.websockets.len(), 1);
    assert_eq!(first.websockets[0].module, "app.WebSocket");
    assert_eq!(first.websockets[0].route, "/api/socket");
    assert_eq!(first.websockets[0].protocol, "chat.v1");
    assert_eq!(
        first.websockets[0].source.as_ref().unwrap().path,
        "src/app/WebSocket.terl"
    );
    assert_eq!(first, discover(&artifact).unwrap());
    fs::write(&artifact.source_path, source.replace("/socket", "/live")).unwrap();
    assert_eq!(
        discover(&artifact).unwrap().websockets[0].route,
        "/api/live"
    );
}

#[test]
fn websocket_metadata_rejects_missing_private_wrong_arity_and_nonstrings() {
    for (declarations, expected) in [
        (
            "pub route(): String -> \"/ws\".",
            "missing public `app.WebSocket.protocol/0`",
        ),
        (
            "pub protocol(): String -> \"chat\".",
            "missing public `app.WebSocket.route/0`",
        ),
        (
            "route(): String -> \"/ws\".\npub protocol(): String -> \"chat\".",
            "missing public `app.WebSocket.route/0`",
        ),
        (
            "pub route(value: String): String -> value.\npub protocol(): String -> \"chat\".",
            "missing public `app.WebSocket.route/0`",
        ),
        (
            "pub route(): Int -> 1.\npub protocol(): String -> \"chat\".",
            "must return String",
        ),
        (
            "pub route(): String -> \"/ws\".\npub protocol(): Int -> 1.",
            "must return String",
        ),
        (
            "pub route(): String -> \"/ws\".\npub protocol(): String -> \"\".",
            "cannot be empty",
        ),
        (
            "pub route(): String -> \"/ws\".\npub protocol(): String -> \"chat room\".",
            "invalid protocol",
        ),
        (
            "pub route(): String -> \"not-a-path\".\npub protocol(): String -> \"chat\".",
            "route",
        ),
    ] {
        let (_root, artifact) = fixture(&format!("module app.WebSocket.\n{declarations}"));
        let error = discover(&artifact).unwrap_err();
        assert!(error.contains(expected), "expected {expected}: {error}");
    }
}

#[test]
fn websocket_metadata_does_not_guess_unimported_module_suffixes() {
    let (root, artifact) = fixture("module app.WebSocket.\npub route(): String -> Protocol.route().\npub protocol(): String -> \"chat\".");
    fs::write(
        root.path().join("src/app/Protocol.terl"),
        "module app.Protocol.\npub route(): String -> \"/wrong\".",
    )
    .unwrap();
    let error = discover(&artifact).unwrap_err();
    assert!(error.contains("compilation failed"), "{error}");
}

#[test]
fn websocket_metadata_denies_external_services_in_either_export() {
    for function in ["route", "protocol"] {
        let source = format!(
            r#"module app.WebSocket.
import std.core.Option.
import type std.core.Option.
@compiler.native {{std.http.session.lookup}}
lookup(identity: String): Option[String].
pub route(): String -> {}.
pub protocol(): String -> {}.
"#,
            if function == "route" {
                "Option.with_default(lookup(\"identity\"), \"/ws\")"
            } else {
                "\"/ws\""
            },
            if function == "protocol" {
                "Option.with_default(lookup(\"identity\"), \"chat\")"
            } else {
                "\"chat\""
            }
        );
        let (_root, artifact) = fixture(&source);
        let error = discover(&artifact).unwrap_err();
        assert!(
            error.contains("source_eval.external_wait") && error.contains("Capability"),
            "{error}"
        );
        assert!(
            error.contains(&format!("app.WebSocket.{function}/0")),
            "{error}"
        );
    }
}

#[test]
fn websocket_metadata_ignores_nonmetadata_modules_and_rejects_false_identity() {
    let (_root, mut artifact) = fixture("module app.WebSocket.\npub value(): Int -> 1.");
    assert!(discover(&artifact).unwrap().websockets.is_empty());
    fs::write(&artifact.source_path, "module app.WebSocket.\npub route(): String -> \"/ws\".\npub protocol(): String -> \"chat\".").unwrap();
    artifact.module = "other.WebSocket".into();
    assert!(discover(&artifact)
        .unwrap_err()
        .contains("expected module `other.WebSocket`"));
}
