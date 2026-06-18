use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;

/// Creates a unique temporary watcher test directory.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path to a not-yet-existing directory under the system temp directory.
///
/// Transformation:
/// - Combines process id and current nanoseconds so package snapshot tests can
///   run in parallel without sharing state.
fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("timestamp")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan_serve_watch_{name}_{}_{}",
        std::process::id(),
        nanos
    ))
}

/// Writes a minimal package fixture for watcher tests.
///
/// Inputs:
/// - `web_root`: target package root.
///
/// Output:
/// - Filesystem fixture with a nested JavaScript asset.
///
/// Transformation:
/// - Creates enough package content for deterministic snapshot hashing without
///   depending on the browser build pipeline.
fn write_watched_package(web_root: &Path) {
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
}

#[test]
fn reload_watch_backend_documents_polling_compatibility_policy() {
    let backend = ReloadWatchBackend::selected();

    assert_eq!(backend, ReloadWatchBackend::PollCompatibility);
    assert_eq!(backend.name(), "poll-compatibility");
    assert!(backend.policy().contains("Oxc has no live-reload owner"));
    assert!(backend.policy().contains("Rsbuild/Rspack is reserved"));
    assert!(backend.policy().contains("_build/web"));
}

#[test]
fn web_package_snapshot_changes_when_asset_content_changes() {
    let dir = temp_dir("snapshot_changes");
    let web_root = dir.join("web");
    write_watched_package(&web_root);
    let first = web_package_snapshot(&web_root).expect("first snapshot");

    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 2;\n",
    )
    .expect("change js asset");
    let second = web_package_snapshot(&web_root).expect("second snapshot");

    assert_ne!(first, second);
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn broadcast_reload_removes_disconnected_subscribers() {
    let hub = Arc::new(Mutex::new(Vec::new()));
    let (connected_tx, mut connected_rx) = mpsc::unbounded_channel();
    let (dropped_tx, dropped_rx) = mpsc::unbounded_channel::<u64>();
    drop(dropped_rx);
    hub.lock().expect("hub").push(connected_tx);
    hub.lock().expect("hub").push(dropped_tx);

    broadcast_reload(&hub, 7);

    assert_eq!(connected_rx.try_recv().expect("reload event"), 7);
    assert_eq!(hub.lock().expect("hub").len(), 1);
}
