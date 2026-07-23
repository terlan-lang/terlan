use super::*;
use crate::commands::serve::manifest::invalidate_web_manifest_cache;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

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
    assert!(Arc::ptr_eq(&first, &second));

    invalidate_web_manifest_cache(&root);
    let reloaded = match manifest_route_for_request(&root, "POST", "/api/bench") {
        Some(MatchedWebPackageRoute::Handler(handler)) => handler,
        _ => panic!("reloaded dynamic route"),
    };
    assert!(!Arc::ptr_eq(&first, &reloaded));

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
