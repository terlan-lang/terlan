#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};

use super::process::VmProcessId;

#[cfg(test)]
#[path = "package_transport_test.rs"]
mod package_transport_test;

/// VM-owned package download handle.
///
/// Inputs: opaque runtime id. Output: stable download handle. Transformation:
/// represents a package transfer without exposing host client state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VmPackageDownload {
    id: u64,
}

/// Runtime-visible package download wake intent.
///
/// Inputs: blocked process and download handle. Output: scheduler wake intent.
/// Transformation: keeps transfer readiness independent from host async
/// runtimes or HTTP client internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmPackageDownloadWake {
    Chunk {
        process: VmProcessId,
        download: VmPackageDownload,
    },
    Complete {
        process: VmProcessId,
        download: VmPackageDownload,
    },
}

/// Runtime-visible package download state.
///
/// Inputs: one download handle. Output: inspectable transfer pressure and
/// lifecycle state. Transformation: exposes progress without leaking native
/// package-registry transport handles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPackageDownloadInfo {
    pub(crate) owner: String,
    pub(crate) url: String,
    pub(crate) queued_chunks: usize,
    pub(crate) queued_bytes: usize,
    pub(crate) queue_limit: usize,
    pub(crate) waiting_receivers: usize,
    pub(crate) complete: bool,
    pub(crate) cancelled: bool,
}

/// Package download event returned to actor code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmPackageDownloadEvent {
    Chunk(Vec<u8>),
    Complete,
}

/// VM-owned package download transport registry.
///
/// Inputs:
/// - Start, enqueue, receive, park, complete, cancel, and owner-cleanup
///   operations.
///
/// Output:
/// - Deterministic package download handles, wakeups, and stable diagnostics.
///
/// Transformation:
/// - Models package transfer readiness and backpressure inside the VM while
///   leaving actual TLS/HTTP bytes to maintained Rust clients behind a typed
///   NativeBoundary adapter.
#[derive(Debug, Default)]
pub(crate) struct VmPackageDownloadRuntime {
    next_download: u64,
    downloads: HashMap<u64, PackageDownloadState>,
}

#[derive(Debug)]
struct PackageDownloadState {
    owner: String,
    url: String,
    queue: VecDeque<Vec<u8>>,
    queue_limit: usize,
    waiters: VecDeque<VmProcessId>,
    complete: bool,
    cancelled: bool,
    complete_delivered: bool,
}

impl VmPackageDownloadRuntime {
    /// Creates an empty package download transport registry.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Starts a VM-owned package download with bounded chunk buffering.
    pub(crate) fn start_download(
        &mut self,
        url: impl Into<String>,
        owner: impl Into<String>,
        queue_limit: usize,
    ) -> Result<VmPackageDownload, String> {
        let url = url.into();
        if url.trim().is_empty() {
            return Err("VM package download URL cannot be empty".to_string());
        }
        if queue_limit == 0 {
            return Err("VM package download queue limit must be greater than 0".to_string());
        }

        self.next_download = self.next_download.saturating_add(1);
        let download = VmPackageDownload {
            id: self.next_download,
        };
        self.downloads.insert(
            download.id,
            PackageDownloadState {
                owner: owner.into(),
                url,
                queue: VecDeque::new(),
                queue_limit,
                waiters: VecDeque::new(),
                complete: false,
                cancelled: false,
                complete_delivered: false,
            },
        );
        Ok(download)
    }

    /// Enqueues one received package chunk and returns receiver wake intents.
    pub(crate) fn enqueue_chunk(
        &mut self,
        download: VmPackageDownload,
        bytes: Vec<u8>,
    ) -> Result<Vec<VmPackageDownloadWake>, String> {
        if bytes.is_empty() {
            return Err("VM package download chunk cannot be empty".to_string());
        }
        let state = self.download_mut(download)?;
        if state.complete {
            return Err("VM package download is complete".to_string());
        }
        if state.queue.len() >= state.queue_limit {
            return Err("VM package download chunk queue is full".to_string());
        }
        state.queue.push_back(bytes);
        Ok(state
            .waiters
            .drain(..)
            .map(|process| VmPackageDownloadWake::Chunk { process, download })
            .collect())
    }

    /// Marks the download complete and wakes blocked receivers.
    pub(crate) fn finish_download(
        &mut self,
        download: VmPackageDownload,
    ) -> Result<Vec<VmPackageDownloadWake>, String> {
        let state = self.download_mut(download)?;
        state.complete = true;
        Ok(state
            .waiters
            .drain(..)
            .map(|process| VmPackageDownloadWake::Complete { process, download })
            .collect())
    }

    /// Receives the next chunk or completion event.
    pub(crate) fn receive_next(
        &mut self,
        download: VmPackageDownload,
    ) -> Result<Option<VmPackageDownloadEvent>, String> {
        let state = self.download_mut(download)?;
        if let Some(bytes) = state.queue.pop_front() {
            return Ok(Some(VmPackageDownloadEvent::Chunk(bytes)));
        }
        if state.complete && !state.complete_delivered {
            state.complete_delivered = true;
            return Ok(Some(VmPackageDownloadEvent::Complete));
        }
        Ok(None)
    }

    /// Parks a process until a chunk or completion event is available.
    pub(crate) fn park_receive(
        &mut self,
        download: VmPackageDownload,
        process: VmProcessId,
    ) -> Result<bool, String> {
        let state = self.download_mut(download)?;
        if !state.queue.is_empty() || (state.complete && !state.complete_delivered) {
            return Ok(false);
        }
        if !state.waiters.contains(&process) {
            state.waiters.push_back(process);
        }
        Ok(true)
    }

    /// Cancels one package download and clears pending transfer state.
    pub(crate) fn cancel_download(&mut self, download: VmPackageDownload) -> Result<(), String> {
        let state = self.download_mut(download)?;
        state.cancelled = true;
        state.queue.clear();
        state.waiters.clear();
        Ok(())
    }

    /// Cancels all package downloads owned by one actor.
    pub(crate) fn cancel_owner_downloads(&mut self, owner: &str) -> Vec<VmPackageDownload> {
        let downloads = self
            .downloads
            .iter()
            .filter_map(|(id, state)| {
                (!state.cancelled && state.owner == owner).then_some(VmPackageDownload { id: *id })
            })
            .collect::<Vec<_>>();
        for download in &downloads {
            let _ = self.cancel_download(*download);
        }
        downloads
    }

    /// Returns an inspectable download snapshot.
    pub(crate) fn inspect_download(
        &self,
        download: VmPackageDownload,
    ) -> Result<VmPackageDownloadInfo, String> {
        let state = self.download(download)?;
        Ok(VmPackageDownloadInfo {
            owner: state.owner.clone(),
            url: state.url.clone(),
            queued_chunks: state.queue.len(),
            queued_bytes: state.queue.iter().map(Vec::len).sum(),
            queue_limit: state.queue_limit,
            waiting_receivers: state.waiters.len(),
            complete: state.complete,
            cancelled: state.cancelled,
        })
    }

    fn download(&self, download: VmPackageDownload) -> Result<&PackageDownloadState, String> {
        let state = self
            .downloads
            .get(&download.id)
            .ok_or_else(|| "VM package download was not found".to_string())?;
        if state.cancelled {
            return Err("VM package download is cancelled".to_string());
        }
        Ok(state)
    }

    fn download_mut(
        &mut self,
        download: VmPackageDownload,
    ) -> Result<&mut PackageDownloadState, String> {
        let state = self
            .downloads
            .get_mut(&download.id)
            .ok_or_else(|| "VM package download was not found".to_string())?;
        if state.cancelled {
            return Err("VM package download is cancelled".to_string());
        }
        Ok(state)
    }
}
