//! Runtime-owned immutable copies of admitted TVM executable images.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::{host_tvm_target, inspect_tvm_image, TvmNativeImageInspection};

/// Monotonic process-local suffix for private admission directories.
static NEXT_SEAL: AtomicU64 = AtomicU64::new(1);

/// One runtime-owned image whose inspected bytes are the bytes given to the loader.
pub(crate) struct SealedTvmImage {
    root: PathBuf,
    path: PathBuf,
    bytes_digest: [u8; 32],
    inspection: TvmNativeImageInspection,
    guard: Option<File>,
}

impl std::fmt::Debug for SealedTvmImage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SealedTvmImage")
            .field("target", &self.inspection.descriptor.target.triple)
            .field("format", &self.inspection.format)
            .field("architecture", &self.inspection.architecture)
            .finish_non_exhaustive()
    }
}

impl SealedTvmImage {
    /// Copies, validates, and locks one source image before dynamic loading.
    pub(crate) fn admit(source: &Path) -> Result<Self, String> {
        reject_tvm_image_sidecars(source)?;
        let bytes = fs::read(source).map_err(|error| {
            format!(
                "error[tvm.image.read]: failed to read `{}`: {error}",
                source.display()
            )
        })?;
        let target = host_tvm_target()?;
        let inspection = inspect_tvm_image(&bytes, &target.triple)?;
        let bytes_digest = Sha256::digest(&bytes).into();
        let root = create_private_root()?;
        let path = root.join("admitted.tvm");
        let result = (|| {
            let mut destination = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|error| format!("error[tvm.image.seal_create]: {error}"))?;
            destination
                .write_all(&bytes)
                .and_then(|()| destination.sync_all())
                .map_err(|error| format!("error[tvm.image.seal_write]: {error}"))?;
            set_sealed_permissions(&path)?;
            let guard = open_sealed_guard(&path)?;
            verify_path_digest(&path, bytes_digest)?;
            Ok(Self {
                root: root.clone(),
                path,
                bytes_digest,
                inspection,
                guard: Some(guard),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(root);
        }
        result
    }

    /// Returns the private path passed to the platform dynamic loader.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the inspection derived from the exact sealed byte snapshot.
    pub(crate) const fn inspection(&self) -> &TvmNativeImageInspection {
        &self.inspection
    }

    /// Returns the digest of the complete admitted executable image.
    pub(crate) const fn bytes_digest(&self) -> [u8; 32] {
        self.bytes_digest
    }

    /// Proves the private image still contains the admitted bytes.
    pub(crate) fn verify_unchanged(&self) -> Result<(), String> {
        verify_path_digest(&self.path, self.bytes_digest)
    }
}

impl Drop for SealedTvmImage {
    fn drop(&mut self) {
        self.guard.take();
        #[cfg(windows)]
        if let Ok(mut permissions) = fs::metadata(&self.path).map(|value| value.permissions()) {
            permissions.set_readonly(false);
            let _ = fs::set_permissions(&self.path, permissions);
        }
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.root);
    }
}

/// Rejects mutable metadata files adjacent to a native image.
pub(crate) fn reject_tvm_image_sidecars(image_path: &Path) -> Result<(), String> {
    let mut candidates = vec![
        image_path.with_extension("json"),
        image_path.with_extension("tvm.json"),
    ];
    if let Some(name) = image_path.file_name().and_then(|name| name.to_str()) {
        candidates.push(image_path.with_file_name(format!("{name}.reuse")));
    }
    if let Some(sidecar) = candidates.into_iter().find(|candidate| candidate.exists()) {
        return Err(format!(
            "error[tvm.image.sidecar]: native image must not have sidecar `{}`",
            sidecar.display()
        ));
    }
    Ok(())
}

/// Allocates one private directory without reusing a caller-controlled pathname.
fn create_private_root() -> Result<PathBuf, String> {
    for _ in 0..128 {
        let sequence = NEXT_SEAL
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "error[tvm.image.seal_identity]: seal identity exhausted".to_string())?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("error[tvm.image.seal_clock]: {error}"))?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "terlan-tvm-seal-{}-{sequence}-{nonce}",
            std::process::id()
        ));
        match create_private_directory(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("error[tvm.image.seal_directory]: {error}")),
        }
    }
    Err("error[tvm.image.seal_directory]: unique private directory unavailable".to_string())
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir(path)
}

/// Makes the private copy immutable through ordinary filesystem APIs.
fn set_sealed_permissions(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("error[tvm.image.seal_metadata]: {error}"))?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("error[tvm.image.seal_permissions]: {error}"))
}

#[cfg(windows)]
fn open_sealed_guard(path: &Path) -> Result<File, String> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 1;
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(|error| format!("error[tvm.image.seal_guard]: {error}"))
}

#[cfg(not(windows))]
fn open_sealed_guard(path: &Path) -> Result<File, String> {
    File::open(path).map_err(|error| format!("error[tvm.image.seal_guard]: {error}"))
}

/// Compares the current private file to the admitted whole-image digest.
fn verify_path_digest(path: &Path, expected: [u8; 32]) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| format!("error[tvm.image.seal_read]: {error}"))?;
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    if actual != expected {
        return Err("error[tvm.image.seal_changed]: admitted image bytes changed".to_string());
    }
    Ok(())
}

#[cfg(test)]
#[path = "sealed_test.rs"]
mod sealed_test;
