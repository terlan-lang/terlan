use super::*;
use crate::commands::serve::manifest::invalidate_web_manifest_cache;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn positional_route_arguments_follow_declared_scalar_types() {
    assert_eq!(
        route_param_argument("/api/{id:Int}/{enabled:Bool}/:name", "id", "42")
            .expect("integer capture"),
        ReplValue::Int(42)
    );
    assert_eq!(
        route_param_argument("/api/{id:Int}/{enabled:Bool}/:name", "enabled", "true")
            .expect("boolean capture"),
        ReplValue::Bool(true)
    );
    assert_eq!(
        route_param_argument("/api/{id:Int}/{enabled:Bool}/:name", "name", "terlan")
            .expect("string capture"),
        ReplValue::String("terlan".to_string())
    );
}

#[test]
fn positional_route_arguments_reject_invalid_typed_values() {
    let error = route_param_argument("/api/{id:Int}", "id", "not-an-int")
        .expect_err("invalid integer capture");
    assert!(
        error.to_string().contains("error[serve.route_param]"),
        "{error}"
    );
}

fn route_fixture() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan-route-cache-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("fixture root");
    fs::write(root.join("index.html"), "fixture").expect("fixture index");
    fs::write(
        root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "index": "index.html",
  "assets": [],
  "handlers": [{
    "method": "POST",
    "route": "/api/bench",
    "module": "app.Api",
    "function": "handle",
    "arity": 1,
    "source": {"path": "src/app/Api.terl", "line": 1, "column": 1}
  }]
}"#,
    )
    .expect("fixture manifest");
    root
}

#[test]
fn repeated_simple_route_reuses_handler_and_reload_replaces_generation() {
    let root = route_fixture();
    let first = match manifest_route_for_request(&root, "POST", "/api/bench") {
        Some(MatchedWebPackageRoute::Handler(handler)) => handler,
        _ => panic!("dynamic route"),
    };
    let second = match manifest_route_for_request(&root, "POST", "/api/bench") {
        Some(MatchedWebPackageRoute::Handler(handler)) => handler,
        _ => panic!("cached dynamic route"),
    };
    assert!(Rc::ptr_eq(&first, &second));

    invalidate_web_manifest_cache(&root);
    let reloaded = match manifest_route_for_request(&root, "POST", "/api/bench") {
        Some(MatchedWebPackageRoute::Handler(handler)) => handler,
        _ => panic!("reloaded dynamic route"),
    };
    assert!(!Rc::ptr_eq(&first, &reloaded));

    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn repeated_simple_route_miss_is_cached_for_the_manifest_generation() {
    let root = route_fixture();
    assert!(manifest_route_for_request(&root, "GET", "/missing").is_none());
    let cached_manifest = LAST_SIMPLE_HANDLER_ROUTE.with(|cached| {
        let cached = cached.borrow();
        let cached = cached.as_ref().expect("negative route cache");
        assert_eq!(cached.method, "GET");
        assert_eq!(cached.request_path, "/missing");
        assert!(cached.matched.is_none());
        Arc::clone(&cached.manifest)
    });

    assert!(manifest_route_for_request(&root, "GET", "/missing").is_none());
    invalidate_web_manifest_cache(&root);
    assert!(manifest_route_for_request(&root, "GET", "/missing").is_none());
    LAST_SIMPLE_HANDLER_ROUTE.with(|cached| {
        let cached = cached.borrow();
        let cached = cached.as_ref().expect("reloaded negative route cache");
        assert!(!Arc::ptr_eq(&cached_manifest, &cached.manifest));
    });

    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn static_resolution_uses_the_route_manifest_snapshot() {
    let root = route_fixture();
    let path = match manifest_route_for_request(&root, "GET", "/") {
        Some(MatchedWebPackageRoute::StaticFile(path)) => path,
        _ => panic!("static route"),
    };
    assert_eq!(path, root.join("index.html"));

    invalidate_web_manifest_cache(&root);
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn mixed_manifest_dispatch_retains_endpoint_kind_and_file_boundary() {
    let root = route_fixture();
    fs::write(root.join("report.txt"), "report").unwrap();
    let mut manifest: WebPackageManifest =
        serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
    manifest.handlers = serde_json::from_value(serde_json::json!([
        {"method":"GET", "route":"/*", "module":"app.Web", "function":"fallback", "arity":1},
        {"method":"GET", "route":"/users/{id:Int}", "module":"app.Web", "function":"user", "arity":1},
        {"method":"HEAD", "route":"/events/*", "module":"app.Web", "function":"head", "arity":1}
    ])).unwrap();
    manifest.sse = serde_json::from_value(serde_json::json!([
        {"module":"app.Events", "route":"/events/live", "source":{"path":"src/app/Events.terl", "line":1, "column":1}}
    ])).unwrap();
    manifest.static_responses = serde_json::from_value(serde_json::json!([
        {"method":"GET", "route":"/users/me", "status":200, "content_type":"text/plain", "body":"me"}
    ])).unwrap();
    manifest.file_responses = serde_json::from_value(serde_json::json!([
        {"method":"GET", "route":"/download", "path":"report.txt", "status":200},
        {"method":"GET", "route":"/unsafe", "path":"../outside.txt", "status":200}
    ]))
    .unwrap();
    let manifest = Arc::new(manifest);
    let select = |method, path| manifest_route_for_loaded_manifest(&root, method, path, &manifest);
    match select("GET", "/users/42") {
        Some(MatchedWebPackageRoute::Handler(matched)) => {
            assert_eq!(matched.handler.function, "user");
            assert_eq!(matched.params, [("id".into(), "42".into())]);
        }
        _ => panic!("typed dynamic handler"),
    }
    match select("HEAD", "/users/me") {
        Some(MatchedWebPackageRoute::StaticResponse(response)) => assert_eq!(response.body, "me"),
        _ => panic!("static response outranks wildcard handler"),
    }
    match select("GET", "/events/live") {
        Some(MatchedWebPackageRoute::Sse(endpoint)) => assert_eq!(endpoint.module, "app.Events"),
        _ => panic!("SSE outranks wildcard handler"),
    }
    match select("HEAD", "/events/live") {
        Some(MatchedWebPackageRoute::Handler(matched)) => {
            assert_eq!(matched.handler.function, "head")
        }
        _ => panic!("explicit HEAD outranks GET SSE"),
    }
    match select("GET", "/download") {
        Some(MatchedWebPackageRoute::FileResponse(response, path)) => {
            assert_eq!(response.path, "report.txt");
            assert_eq!(path, root.join("report.txt"));
        }
        _ => panic!("file response outranks wildcard handler"),
    }
    assert!(select("GET", "/unsafe").is_none());
    fs::remove_file(root.join("report.txt")).unwrap();
    assert!(select("GET", "/download").is_none());
    assert!(select("POST", "/events/live").is_none());
    assert!(matches!(
        select("GET", "/"),
        Some(MatchedWebPackageRoute::StaticFile(_))
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn websocket_reservation_precedes_static_assets_for_handshake_rejection() {
    let root = route_fixture();
    let mut manifest: WebPackageManifest =
        serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
    manifest.websockets = serde_json::from_value(serde_json::json!([
        {"route":"/index.html", "protocol":"chat"}
    ]))
    .unwrap();
    let manifest = Arc::new(manifest);
    for method in ["GET", "HEAD", "POST"] {
        assert!(matches!(
            manifest_route_for_loaded_manifest(&root, method, "/index.html", &manifest),
            Some(MatchedWebPackageRoute::WebSocket(_))
        ));
    }
    fs::remove_dir_all(root).unwrap();
}
