//! Exclusive, bounded publication of the suite's latest observation snapshot.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_REPORT_BYTES: u64 = 1024 * 1024;
const MAX_DOCUMENT_BYTES: u64 = 32 * 1024 * 1024;
static NEXT_RUN: AtomicU64 = AtomicU64::new(0);

/// Keeps a publication reader or writer alive until its operation completes.
pub(super) struct ReportLease(File);

impl Drop for ReportLease {
    fn drop(&mut self) {
        // Closing only our descriptor can retain the lock in a concurrent
        // fork-before-exec child. End ownership explicitly before closing it.
        let _ = self.0.unlock();
    }
}

/// Joins an existing publication without creating a lease for ordinary build outputs.
pub(super) fn read_lease(path: &Path) -> Result<Option<ReportLease>, String> {
    let lock = sibling(path, ".lock");
    if !regular_or_absent(&lock, MAX_REPORT_BYTES)? {
        return Ok(None);
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock)
        .map_err(|error| error.to_string())?;
    file.try_lock_shared()
        .map_err(|error| format!("publication is being replaced or cannot be locked: {error}"))?;
    let lease = ReportLease(file);
    verify_lock(&lock, &lease)?;
    Ok(Some(lease))
}

fn verify_lock(lock: &Path, lease: &ReportLease) -> Result<(), String> {
    let visible = fs::symlink_metadata(lock).map_err(|error| error.to_string())?;
    let opened = lease.0.metadata().map_err(|error| error.to_string())?;
    if !visible.is_file() || !crate::file_identity::same_file(&visible, &opened) {
        return Err("publication lock was replaced or is not a regular file".into());
    }
    Ok(())
}

/// Holds the exclusive writer lease and bounded staging namespace for one report.
pub(super) struct ReportFile {
    path: PathBuf,
    pending: PathBuf,
    run_id: String,
    limit: u64,
    _lease: ReportLease,
}

impl ReportFile {
    /// Acquires ownership before retaining any abandoned staging as diagnostics.
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        Self::open_bounded(path, MAX_REPORT_BYTES)
    }

    /// Uses an explicit document budget without changing the suite-report ceiling.
    pub(super) fn open_bounded(path: &Path, limit: u64) -> Result<Self, String> {
        if limit == 0 || limit > MAX_DOCUMENT_BYTES {
            return Err("invalid publication document byte budget".into());
        }
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let name = path.file_name().ok_or("report has no filename")?;
        let path = fs::canonicalize(parent)
            .map_err(|error| error.to_string())?
            .join(name);
        regular_or_absent(&path, limit)?;
        let lock = sibling(&path, ".lock");
        regular_or_absent(&lock, MAX_REPORT_BYTES)?;
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)
            .map_err(|error| error.to_string())?;
        lease.try_lock().map_err(|error| {
            format!("suite report is already owned or cannot be locked: {error}")
        })?;
        let lease = ReportLease(lease);
        verify_lock(&lock, &lease)?;
        let pending = sibling(&path, ".pending");
        if regular_or_absent(&pending, limit)? {
            let interrupted = sibling(&path, ".interrupted");
            regular_or_absent(&interrupted, limit)?;
            fs::rename(&pending, interrupted).map_err(|error| error.to_string())?;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        Ok(Self {
            path,
            pending,
            limit,
            run_id: format!(
                "{}-{timestamp}-{}",
                std::process::id(),
                NEXT_RUN.fetch_add(1, Ordering::Relaxed)
            ),
            _lease: lease,
        })
    }

    /// Identifies this observation run without claiming a reusable input identity.
    pub(super) fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Fixed companion path protected by this suite's exclusive writer lease.
    pub(super) fn selections_path(&self) -> PathBuf {
        sibling(&self.path, ".selections.json")
    }

    /// Replaces the public snapshot only after its new bytes have been synced.
    pub(super) fn publish(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.publish_contents(bytes, None)
    }

    /// Publishes executable bytes and permissions together under the writer lease.
    pub(super) fn publish_executable(
        &mut self,
        bytes: &[u8],
        permissions: fs::Permissions,
    ) -> Result<(), String> {
        self.publish_contents(bytes, Some(permissions))
    }

    fn publish_contents(
        &mut self,
        bytes: &[u8],
        permissions: Option<fs::Permissions>,
    ) -> Result<(), String> {
        if bytes.len() as u64 > self.limit {
            return Err(format!(
                "publication document exceeds its {} byte budget",
                self.limit
            ));
        }
        regular_or_absent(&self.path, self.limit)?;
        let mut pending = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.pending)
            .map_err(|error| format!("cannot reserve suite report staging: {error}"))?;
        let staged = pending.write_all(bytes).and_then(|()| {
            if let Some(permissions) = permissions {
                pending.set_permissions(permissions)?;
            }
            pending.sync_all()
        });
        if let Err(error) = staged {
            drop(pending);
            let _ = fs::remove_file(&self.pending);
            return Err(format!("cannot stage suite report: {error}"));
        }
        drop(pending);
        fs::rename(&self.pending, &self.path)
            .map_err(|error| format!("cannot publish suite report: {error}"))?;
        #[cfg(unix)]
        File::open(self.path.parent().ok_or("report has no parent")?)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn regular_or_absent(path: &Path, limit: u64) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= limit => Ok(true),
        Ok(_) => Err(format!(
            "{} is not a bounded regular report file",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
#[path = "report_file_test.rs"]
mod tests;
