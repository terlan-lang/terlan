use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

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
///   filesystem watch integration independent from HTTP routing.
pub(super) type ReloadHub = Arc<Mutex<Vec<mpsc::Sender<u64>>>>;

/// Reload watch backend selected for `terlc serve`.
///
/// Inputs:
/// - Chosen by the compiler release policy, not by user-facing source syntax.
///
/// Output:
/// - Internal backend discriminator for the live-reload watcher.
///
/// Transformation:
/// - Makes the maintained filesystem watcher explicit while keeping the
///   backend selection isolated from HTTP routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReloadWatchBackend {
    Notify,
}

impl ReloadWatchBackend {
    /// Returns the selected watch backend.
    ///
    /// Inputs:
    /// - Current compiler release policy.
    ///
    /// Output:
    /// - Active watch backend for `terlc serve`.
    ///
    /// Transformation:
    /// - Keeps watcher selection explicit so future asset graph integration
    ///   requires one intentional policy edit.
    pub(super) fn selected() -> Self {
        Self::Notify
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
            Self::Notify => "notify",
        }
    }

    /// Describes why the backend is allowed for local development serving.
    ///
    /// Inputs:
    /// - `self`: selected watch backend.
    ///
    /// Output:
    /// - Stable policy text used by tests and future diagnostics.
    ///
    /// Transformation:
    /// - Records that local serve relies on a maintained filesystem watcher
    ///   over generated `_build/web` artifacts.
    pub(super) fn policy(self) -> &'static str {
        match self {
            Self::Notify => {
                "notify watches generated _build/web files recursively for local live reload"
            }
        }
    }
}

/// Starts the current reload watcher backend.
///
/// Inputs:
/// - `web_root`: package directory to watch.
/// - `poll_ms`: debounce interval in milliseconds.
/// - `reload_hub`: shared reload subscriber registry.
///
/// Output:
/// - Detached thread handle.
///
/// Transformation:
/// - Isolates maintained filesystem watcher setup behind one function so the
///   HTTP server request path does not own watch integration details.
pub(super) fn spawn_reload_watcher(
    web_root: PathBuf,
    poll_ms: u64,
    reload_hub: ReloadHub,
) -> thread::JoinHandle<()> {
    let backend = ReloadWatchBackend::selected();
    let _policy = backend.policy();
    match backend {
        ReloadWatchBackend::Notify => thread::spawn(move || {
            watch_web_package_for_reload(web_root, Duration::from_millis(poll_ms), reload_hub)
        }),
    }
}

/// Watches the packaged web directory and broadcasts reload events.
///
/// Inputs:
/// - `web_root`: package directory to watch.
/// - `debounce_interval`: debounce interval for coalescing filesystem events.
/// - `reload_hub`: shared reload subscriber registry.
///
/// Output:
/// - Future that runs until the process exits.
///
/// Transformation:
/// - Uses the maintained `notify` watcher recursively over the generated web
///   package, coalesces event bursts, and emits monotonically increasing reload
///   versions to connected SSE clients.
fn watch_web_package_for_reload(
    web_root: PathBuf,
    debounce_interval: Duration,
    reload_hub: ReloadHub,
) {
    let (event_tx, event_rx) = mpsc::channel();
    let manifest_path = web_root.join("manifest.json");
    let source_paths = super::manifest::web_package_handler_source_paths(&web_root)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let source_directories = source_paths
        .iter()
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect::<BTreeSet<_>>();
    let watched_sources = Arc::new(source_paths);
    let cache_root = web_root.clone();
    let mut watcher = match RecommendedWatcher::new(
        move |result: notify::Result<Event>| match result {
            Ok(event) if should_reload_for_event(&event) => {
                if event.paths.iter().any(|path| path == &manifest_path) {
                    super::manifest::invalidate_web_manifest_cache(&cache_root);
                }
                let _ = event_tx.send(event.paths);
            }
            Ok(_) => {}
            Err(err) => eprintln!("error[serve_watch]: failed to watch package changes: {err}"),
        },
        Config::default(),
    ) {
        Ok(watcher) => watcher,
        Err(err) => {
            eprintln!("error[serve_watch]: failed to create package watcher: {err}");
            return;
        }
    };
    if let Err(err) = watcher.watch(&web_root, RecursiveMode::Recursive) {
        eprintln!(
            "error[serve_watch]: failed to watch package directory `{}`: {err}",
            web_root.display()
        );
        return;
    }
    for directory in source_directories {
        if let Err(err) = watcher.watch(&directory, RecursiveMode::NonRecursive) {
            eprintln!(
                "error[serve_watch]: failed to watch handler source directory `{}`: {err}",
                directory.display()
            );
        }
    }
    if let Some(project_root) = super::manifest::adjacent_project_root(&web_root) {
        for directory in ["src", "templates", "styles", ".terlan/generated-bindings"] {
            let path = project_root.join(directory);
            if path.is_dir() {
                if let Err(err) = watcher.watch(&path, RecursiveMode::Recursive) {
                    eprintln!(
                        "error[serve_watch]: failed to watch development input `{}`: {err}",
                        path.display()
                    );
                }
            }
        }
        for file in ["terlan.toml", "terlan.lock", "package.json"] {
            let path = project_root.join(file);
            if path.is_file() {
                if let Err(err) = watcher.watch(&path, RecursiveMode::NonRecursive) {
                    eprintln!(
                        "error[serve_watch]: failed to watch package input `{}`: {err}",
                        path.display()
                    );
                }
            }
        }
    }

    let mut version = 0_u64;
    let debounce_interval = debounce_interval.max(Duration::from_millis(1));

    while let Ok(paths) = event_rx.recv() {
        let mut changed_paths = paths.into_iter().collect::<BTreeSet<_>>();
        thread::sleep(debounce_interval);
        while let Ok(followup_paths) = event_rx.try_recv() {
            changed_paths.extend(followup_paths);
        }
        let compiler_paths = changed_paths
            .iter()
            .filter(|path| is_compiler_reload_input(path, &watched_sources))
            .cloned()
            .collect::<Vec<_>>();
        if !compiler_paths.is_empty()
            && stage_source_replacement(&web_root, &compiler_paths).is_err()
        {
            continue;
        }
        version = version.saturating_add(1);
        broadcast_reload(&reload_hub, version);
    }
}

fn stage_source_replacement(web_root: &Path, changed_paths: &[PathBuf]) -> Result<(), ()> {
    match super::handler_cache::stage_source_generation(web_root, changed_paths) {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!("{error}");
            Err(())
        }
    }
}

fn is_compiler_reload_input(path: &Path, handler_sources: &BTreeSet<PathBuf>) -> bool {
    handler_sources.contains(path)
        || path.extension().and_then(|extension| extension.to_str()) == Some("terl")
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                matches!(name, "terlan.toml" | "terlan.lock" | "package.json")
                    || name.ends_with(".binding.json")
            })
}

/// Returns whether one filesystem event should trigger live reload.
///
/// Inputs:
/// - `event`: notify event emitted for the watched package tree.
///
/// Output:
/// - `true` for create, modify, remove, and generic events.
///
/// Transformation:
/// - Filters out pure access notifications while preserving all artifact
///   changes that can affect served output.
fn should_reload_for_event(event: &Event) -> bool {
    match event.kind {
        EventKind::Access(_) => false,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => true,
        EventKind::Other => true,
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

#[cfg(test)]
#[path = "watch_test.rs"]
#[cfg(test)]
mod watch_test;
