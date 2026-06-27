use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
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
///   filesystem watch integration independent from HTTP routing.
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
/// - Detached Tokio task handle.
///
/// Transformation:
/// - Isolates maintained filesystem watcher setup behind one function so the
///   HTTP server request path does not own watch integration details.
pub(super) fn spawn_reload_watcher(
    web_root: PathBuf,
    poll_ms: u64,
    reload_hub: ReloadHub,
) -> tokio::task::JoinHandle<()> {
    let backend = ReloadWatchBackend::selected();
    let _policy = backend.policy();
    match backend {
        ReloadWatchBackend::Notify => tokio::spawn(watch_web_package_for_reload(
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
async fn watch_web_package_for_reload(
    web_root: PathBuf,
    debounce_interval: Duration,
    reload_hub: ReloadHub,
) {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut watcher = match RecommendedWatcher::new(
        move |result: notify::Result<Event>| match result {
            Ok(event) if should_reload_for_event(&event) => {
                let _ = event_tx.send(());
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

    let mut version = 0_u64;
    let debounce_interval = debounce_interval.max(Duration::from_millis(1));

    while event_rx.recv().await.is_some() {
        time::sleep(debounce_interval).await;
        while event_rx.try_recv().is_ok() {}
        version = version.saturating_add(1);
        broadcast_reload(&reload_hub, version);
    }
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
mod watch_test;
