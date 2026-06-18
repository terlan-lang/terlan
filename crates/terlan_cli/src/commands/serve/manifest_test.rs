use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates a unique temporary manifest test directory.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path to a not-yet-existing directory under the system temp directory.
///
/// Transformation:
/// - Combines process id and current nanoseconds to avoid collisions between
///   parallel test runs.
fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("timestamp")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan_serve_manifest_{name}_{}_{}",
        std::process::id(),
        nanos
    ))
}

/// Writes a browser manifest fixture with one index and one asset.
///
/// Inputs:
/// - `web_root`: target package root.
///
/// Output:
/// - Filesystem fixture containing manifest-declared index and asset files.
///
/// Transformation:
/// - Creates the minimal manifest shape needed by manifest static routing tests
///   without invoking the full browser build pipeline.
fn write_manifest_package(web_root: &Path) {
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create dirs");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(web_root.join("unlisted.txt"), "not routed\n").expect("write unlisted file");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write app asset");
    fs::write(web_root.join("assets/hello.txt"), "hello asset\n").expect("write static asset");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    },
    {
      "module": "",
      "kind": "static-asset",
      "source_relative_path": "assets/hello.txt",
      "web_relative_path": "assets/hello.txt",
      "fingerprint": 2
    }
  ]
}
"#,
    )
    .expect("write manifest");
}

#[test]
fn manifest_static_file_for_request_matches_index_and_assets() {
    let dir = temp_dir("matches_index_assets");
    let web_root = dir.join("web");
    write_manifest_package(&web_root);

    let root_index = manifest_static_file_for_request(&web_root, "/").expect("root index");
    let explicit_index =
        manifest_static_file_for_request(&web_root, "/index.html").expect("explicit index");
    let asset = manifest_static_file_for_request(&web_root, "/assets/js/modules/app.js")
        .expect("manifest asset");
    let static_asset =
        manifest_static_file_for_request(&web_root, "/assets/hello.txt").expect("static asset");

    assert_eq!(root_index, web_root.join("index.html"));
    assert_eq!(explicit_index, web_root.join("index.html"));
    assert_eq!(asset, web_root.join("assets/js/modules/app.js"));
    assert_eq!(static_asset, web_root.join("assets/hello.txt"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_static_file_for_request_ignores_unlisted_files() {
    let dir = temp_dir("ignores_unlisted");
    let web_root = dir.join("web");
    write_manifest_package(&web_root);

    let unlisted = manifest_static_file_for_request(&web_root, "/unlisted.txt");

    assert!(unlisted.is_none());
    fs::remove_dir_all(dir).expect("cleanup");
}
