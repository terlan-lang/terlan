use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time;

/// Shared reload subscriber registry.
///
/// Inputs:
/// - Held by the HTTP listener and server-sent-events connections.
///
/// Output:
/// - Mutable list of reload event subscribers.
///
/// Transformation:
/// - Gives the watcher backend a narrow broadcast target so future
///   Oxc/Rsbuild/Rspack watch integration can replace polling without changing
///   HTTP routing.
pub(super) type ReloadHub = Arc<Mutex<Vec<mpsc::UnboundedSender<u64>>>>;

/// Reload watch backend selected for `terlc serve`.
///
/// Inputs:
/// - Chosen by the compiler release policy, not by user-facing source syntax.
///
/// Output:
/// - Internal backend discriminator for the live-reload watcher.
///
/// Transformation:
/// - Makes the 0.0.4 compatibility decision explicit: local serving uses a
///   Terlan-owned polling backend over generated `_build/web` files, while
///   Oxc/Rsbuild/Rspack remain the checked boundaries for future asset graph
///   orchestration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReloadWatchBackend {
    PollCompatibility,
}

impl ReloadWatchBackend {
    /// Returns the selected 0.0.4 watch backend.
    ///
    /// Inputs:
    /// - Current compiler release policy.
    ///
    /// Output:
    /// - Active watch backend for `terlc serve`.
    ///
    /// Transformation:
    /// - Keeps the compatibility shim explicit so changing to a future
    ///   Oxc/Rsbuild/Rspack backend requires one intentional policy edit.
    pub(super) fn selected() -> Self {
        Self::PollCompatibility
    }

    /// Returns a stable backend identifier.
    ///
    /// Inputs:
    /// - `self`: selected watch backend.
    ///
    /// Output:
    /// - Stable backend name for diagnostics and tests.
    ///
    /// Transformation:
    /// - Maps enum variants to release-facing text without exposing Rust enum
    ///   names as the server contract.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::PollCompatibility => "poll-compatibility",
        }
    }

    /// Describes why the backend is allowed for 0.0.4.
    ///
    /// Inputs:
    /// - `self`: selected watch backend.
    ///
    /// Output:
    /// - Stable policy text used by tests and future diagnostics.
    ///
    /// Transformation:
    /// - Records the Oxc-first/Rsbuild/Rspack-fallback decision in code beside
    ///   the watcher implementation, while keeping the detailed decision in
    ///   the roadmap capability record.
    pub(super) fn policy(self) -> &'static str {
        match self {
            Self::PollCompatibility => {
                "Oxc has no live-reload owner; Rsbuild/Rspack is reserved for asset graph orchestration; polling is accepted for generated _build/web files"
            }
        }
    }
}

/// Starts the current reload watcher backend.
///
/// Inputs:
/// - `web_root`: package directory to watch.
/// - `poll_ms`: temporary polling interval in milliseconds.
/// - `reload_hub`: shared reload subscriber registry.
///
/// Output:
/// - Detached Tokio task handle.
///
/// Transformation:
/// - Isolates the temporary polling backend behind one function so the later
///   Oxc/Rsbuild/Rspack watch boundary replaces this module instead of the HTTP
///   server request path.
pub(super) fn spawn_reload_watcher(
    web_root: PathBuf,
    poll_ms: u64,
    reload_hub: ReloadHub,
) -> tokio::task::JoinHandle<()> {
    let backend = ReloadWatchBackend::selected();
    let _policy = backend.policy();
    match backend {
        ReloadWatchBackend::PollCompatibility => tokio::spawn(watch_web_package_for_reload(
            web_root,
            Duration::from_millis(poll_ms),
            reload_hub,
        )),
    }
}

/// Watches the packaged web directory and broadcasts reload events.
///
/// Inputs:
/// - `web_root`: package directory to watch.
/// - `poll_interval`: temporary polling interval.
/// - `reload_hub`: shared reload subscriber registry.
///
/// Output:
/// - Future that runs until the process exits.
///
/// Transformation:
/// - Computes a deterministic package snapshot, polls for changes, and emits
///   monotonically increasing reload versions to connected SSE clients. This is
///   the temporary compatibility backend until the selected
///   Oxc/Rsbuild/Rspack watch boundary is wired.
async fn watch_web_package_for_reload(
    web_root: PathBuf,
    poll_interval: Duration,
    reload_hub: ReloadHub,
) {
    let mut last_snapshot = match web_package_snapshot(&web_root) {
        Ok(snapshot) => snapshot,
        Err(message) => {
            eprintln!("{message}");
            0
        }
    };
    let mut version = 0_u64;
    let mut ticker = time::interval(poll_interval);

    loop {
        ticker.tick().await;
        match web_package_snapshot(&web_root) {
            Ok(snapshot) if snapshot != last_snapshot => {
                last_snapshot = snapshot;
                version = version.saturating_add(1);
                broadcast_reload(&reload_hub, version);
            }
            Ok(_) => {}
            Err(message) => eprintln!("{message}"),
        }
    }
}

/// Broadcasts one reload event to connected clients.
///
/// Inputs:
/// - `reload_hub`: shared reload subscriber registry.
/// - `version`: monotonically increasing reload version.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Sends the reload version to every connected SSE client and removes
///   disconnected senders.
pub(super) fn broadcast_reload(reload_hub: &ReloadHub, version: u64) {
    if let Ok(mut subscribers) = reload_hub.lock() {
        subscribers.retain(|subscriber| subscriber.send(version).is_ok());
    }
}

/// Computes a package snapshot for the temporary reload watcher.
///
/// Inputs:
/// - `web_root`: package directory to inspect.
///
/// Output:
/// - Deterministic hash of package-relative file paths and file bytes.
///
/// Transformation:
/// - Recursively lists files under the package root, sorts them, reads each
///   file, and hashes path plus content. Directory metadata is ignored so only
///   served artifact changes trigger reloads.
pub(super) fn web_package_snapshot(web_root: &Path) -> Result<u64, String> {
    let mut files = Vec::new();
    collect_package_files(web_root, web_root, &mut files)?;
    files.sort();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for relative_path in files {
        relative_path.hash(&mut hasher);
        let file_path = web_root.join(&relative_path);
        let bytes = fs::read(&file_path).map_err(|err| {
            format!(
                "error[serve_watch]: failed to read watched package file `{}`: {err}",
                file_path.display()
            )
        })?;
        bytes.hash(&mut hasher);
    }
    Ok(hasher.finish())
}

/// Collects package-relative files for reload snapshotting.
///
/// Inputs:
/// - `web_root`: package root.
/// - `dir`: current directory being scanned.
/// - `files`: output list of package-relative files.
///
/// Output:
/// - `Ok(())` when all reachable package files have been collected.
///
/// Transformation:
/// - Recursively scans directories, converts files to package-relative paths,
///   and rejects filesystem errors with stable serve-watch diagnostics.
fn collect_package_files(
    web_root: &Path,
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|err| {
        format!(
            "error[serve_watch]: failed to read watched package directory `{}`: {err}",
            dir.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|err| {
            format!(
                "error[serve_watch]: failed to inspect watched package directory `{}`: {err}",
                dir.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_package_files(web_root, &path, files)?;
        } else if path.is_file() {
            let relative = path.strip_prefix(web_root).map_err(|err| {
                format!(
                    "error[serve_watch]: failed to relativize watched package file `{}`: {err}",
                    path.display()
                )
            })?;
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "watch_test.rs"]
mod watch_test;
