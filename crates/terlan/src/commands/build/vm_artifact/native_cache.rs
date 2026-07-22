use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::super::BuildOneError;

pub(super) const CACHE_MANIFEST_NAME: &str = "manifest.v1";
const CACHE_BUILD_LOCK_NAME: &str = "build.lock";
const CACHE_BUILD_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
static NEXT_TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

pub(super) struct TemporaryCacheFile {
    path: PathBuf,
}

impl TemporaryCacheFile {
    pub(super) fn beside(path: &Path) -> Result<Self, BuildOneError> {
        let parent = path.parent().ok_or_else(|| {
            BuildOneError::Message(format!(
                "error[tvm.cache.temporary_path]: cache file `{}` has no parent directory",
                path.display()
            ))
        })?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                BuildOneError::Message(format!(
                    "error[tvm.cache.temporary_path]: cache file `{}` has no UTF-8 filename",
                    path.display()
                ))
            })?;
        let id = NEXT_TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            path: parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), id)),
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryCacheFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Owns construction of one content-addressed native cache entry.
pub(super) struct CacheBuildLock {
    _file: File,
}

impl CacheBuildLock {
    pub(super) fn acquire(directory: &Path) -> Result<Self, BuildOneError> {
        let path = directory.join(CACHE_BUILD_LOCK_NAME);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| {
                BuildOneError::Message(format!(
                    "error[tvm.cache.lock_create]: failed to open native cache lock `{}`: {error}",
                    path.display()
                ))
            })?;
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(TryLockError::WouldBlock) => {
                    if started.elapsed() >= CACHE_BUILD_LOCK_TIMEOUT {
                        return Err(BuildOneError::Message(format!(
                            "error[tvm.cache.lock_timeout]: timed out waiting for native cache lock `{}`",
                            path.display()
                        )));
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(TryLockError::Error(error)) => {
                    return Err(BuildOneError::Message(format!(
                        "error[tvm.cache.lock_acquire]: failed to acquire native cache lock `{}`: {error}",
                        path.display()
                    )));
                }
            }
        }
    }
}

/// Publishes one complete cache file without exposing a partial write.
pub(super) fn publish_file(path: &Path, bytes: &[u8]) -> Result<(), BuildOneError> {
    let temporary = TemporaryCacheFile::beside(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary.path())
        .map_err(|error| {
            BuildOneError::Message(format!(
                "error[tvm.cache.temporary_create]: failed to create `{}`: {error}",
                temporary.path().display()
            ))
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            BuildOneError::Message(format!(
                "error[tvm.cache.temporary_write]: failed to write `{}`: {error}",
                temporary.path().display()
            ))
        })?;
    drop(file);
    match fs::rename(temporary.path(), path) {
        Ok(()) => Ok(()),
        Err(first_error) if path.exists() => {
            fs::remove_file(path).and_then(|()| fs::rename(temporary.path(), path)).map_err(
                |error| {
                    BuildOneError::Message(format!(
                        "error[tvm.cache.publish]: failed to replace cache file `{}` after {first_error}: {error}",
                        path.display()
                    ))
                },
            )
        }
        Err(error) => Err(BuildOneError::Message(format!(
            "error[tvm.cache.publish]: failed to publish cache file `{}`: {error}",
            path.display()
        ))),
    }
}

/// Computes lowercase SHA-256 without depending on a host checksum utility.
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Builds the exact manifest for one complete content-addressed native entry.
///
/// The manifest deliberately uses a small line protocol rather than JSON. Its
/// byte-for-byte comparison validates the expected input identity, target,
/// backend, filenames, lengths, and content digests without a permissive parser.
pub(super) fn cache_manifest_bytes(
    input_sha256: &str,
    target: &str,
    backend: &str,
    files: &[(&str, &[u8])],
) -> Vec<u8> {
    let mut manifest = format!(
        "terlan-native-cache-v1\ninput-sha256 {input_sha256}\ntarget {target}\nbackend {backend}\n"
    );
    for (name, bytes) in files {
        manifest.push_str(&format!(
            "file {name} {} {}\n",
            bytes.len(),
            sha256_hex(bytes)
        ));
    }
    manifest.into_bytes()
}

/// Loads one cache entry only when every required file matches its manifest.
pub(super) fn load_verified_entry(
    directory: &Path,
    input_sha256: &str,
    target: &str,
    backend: &str,
    file_names: &[&str],
    image_name: &str,
) -> Option<Vec<u8>> {
    if !is_sha256(input_sha256)
        || directory.file_name().and_then(|name| name.to_str()) != Some(input_sha256)
    {
        return None;
    }
    let files = file_names
        .iter()
        .map(|name| {
            fs::read(directory.join(name))
                .ok()
                .map(|bytes| (*name, bytes))
        })
        .collect::<Option<Vec<_>>>()?;
    let expected = cache_manifest_bytes(
        input_sha256,
        target,
        backend,
        &files
            .iter()
            .map(|(name, bytes)| (*name, bytes.as_slice()))
            .collect::<Vec<_>>(),
    );
    let manifest = fs::read(directory.join(CACHE_MANIFEST_NAME)).ok()?;
    if manifest != expected {
        return None;
    }
    files
        .into_iter()
        .find_map(|(name, bytes)| (name == image_name).then_some(bytes))
}

/// Returns whether a string is one canonical lowercase SHA-256 identity.
pub(super) fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Keeps deployable VM output to one native image for the current application.
pub(super) fn remove_stale_tvm_images(
    vm_dir: &Path,
    retained_name: Option<&str>,
) -> Result<(), BuildOneError> {
    let entries = fs::read_dir(vm_dir).map_err(|error| {
        BuildOneError::Message(format!(
            "failed to inspect VM artifact directory `{}`: {error}",
            vm_dir.display()
        ))
    })?;
    for entry in entries {
        let path = entry
            .map_err(|error| {
                BuildOneError::Message(format!(
                    "failed to inspect VM artifact directory `{}`: {error}",
                    vm_dir.display()
                ))
            })?
            .path();
        let file_name = path.file_name().and_then(|value| value.to_str());
        let is_tvm_image = path.extension().and_then(|value| value.to_str()) == Some("tvm");
        let is_legacy_sidecar = file_name
            .is_some_and(|name| name.ends_with(".tvm.json") || name.ends_with(".tvm.reuse"));
        let is_retained = retained_name.is_some_and(|name| file_name == Some(name));
        if (is_tvm_image && !is_retained) || is_legacy_sidecar {
            fs::remove_file(&path).map_err(|error| {
                BuildOneError::Message(format!(
                    "failed to remove stale TVM artifact `{}`: {error}",
                    path.display()
                ))
            })?;
        }
    }
    Ok(())
}
