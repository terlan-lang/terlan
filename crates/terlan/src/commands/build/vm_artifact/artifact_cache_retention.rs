//! Bounded compiler-private artifact storage with explicit consumer leases.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use super::super::BuildOneError;
use super::native_cache::{is_sha256, publish_file, CacheBuildLock};

const ACCESS: &str = "access.v1";
const MAX_SCAN_ENTRIES: usize = 65_536;
const MAX_ENTRY_FILES: usize = 128;

/// Closed payload policies keep retention from adopting unrelated files.
#[derive(Clone, Copy)]
pub(super) enum CacheFamily {
    /// Relocatable objects consumed by native linking.
    NativeObjects,
    /// Serialized checked syntax and implementation consumed by the frontend.
    CheckedImplementations,
}

impl CacheFamily {
    fn payload_names(self) -> &'static [&'static str] {
        match self {
            Self::NativeObjects => &["module.o", "module.obj"],
            Self::CheckedImplementations => &["checked.json"],
        }
    }

    fn temporary_prefixes(self) -> &'static [&'static str] {
        match self {
            Self::NativeObjects => &[".module.o.", ".module.obj.", ".manifest.v1."],
            Self::CheckedImplementations => &[".checked.json.", ".manifest.v1."],
        }
    }
}

/// Byte bounds include staging space; active consumer inputs may never be evicted.
#[derive(Clone, Copy)]
pub(super) struct Budget {
    /// Maximum logical bytes, including space reserved for atomic replacement.
    pub(super) bytes: u64,
    /// Maximum generations, including missing members of the active link set.
    pub(super) entries: usize,
    /// Maximum idle age of an unpinned generation.
    pub(super) age: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            bytes: 2 * 1024 * 1024 * 1024,
            entries: 4096,
            age: Duration::from_secs(7 * 24 * 3600),
        }
    }
}

#[derive(Clone)]
struct Entry {
    bytes: u64,
    used: SystemTime,
}

/// The stable root lock serializes object-set publication and retirement, not
/// frontend work or parallel Cranelift workers inside a single object set.
pub(super) struct RetainedCache {
    root: PathBuf,
    family: CacheFamily,
    pinned: BTreeSet<String>,
    budget: Budget,
    entries: Mutex<BTreeMap<String, Entry>>,
    _lease: CacheBuildLock,
}

fn failure(message: impl std::fmt::Display) -> BuildOneError {
    BuildOneError::Message(format!("error[build.cache.retention]: {message}"))
}

fn directory(path: &Path) -> Result<(), BuildOneError> {
    fs::create_dir_all(path).map_err(failure)?;
    let metadata = fs::symlink_metadata(path).map_err(failure)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(failure(format!(
            "not an owned directory: {}",
            path.display()
        )));
    }
    Ok(())
}

impl RetainedCache {
    /// Callers provide a private namespace not used by older, unleased writers.
    pub(super) fn open(
        cache_root: &Path,
        family: CacheFamily,
        pinned: BTreeSet<String>,
        budget: Budget,
    ) -> Result<Self, BuildOneError> {
        Self::open_at(cache_root, family, pinned, budget, SystemTime::now())
    }

    /// Opens storage with an explicit retention clock for deterministic testing.
    pub(super) fn open_at(
        cache_root: &Path,
        family: CacheFamily,
        pinned: BTreeSet<String>,
        budget: Budget,
        now: SystemTime,
    ) -> Result<Self, BuildOneError> {
        if budget.bytes == 0
            || budget.entries == 0
            || budget.entries > MAX_SCAN_ENTRIES
            || budget.age.is_zero()
            || pinned.len() > budget.entries
            || pinned.iter().any(|key| !is_sha256(key))
        {
            return Err(failure("invalid budget or object-set identity inventory"));
        }
        let root = cache_root.to_path_buf();
        directory(&root)?;
        let lease = CacheBuildLock::acquire(&root)?;
        directory(&root.join("entries"))?;
        directory(&root.join("retired"))?;
        // Interrupted retirement is completed before any paths are handed out.
        for (key, _) in inventory(&root.join("retired"), family)? {
            remove_flat_entry(&root.join("retired").join(key), family)?;
        }
        let mut entries = inventory(&root.join("entries"), family)?;
        for (key, entry) in &mut entries {
            let path = root.join("entries").join(key);
            let mut changed = false;
            for value in fs::read_dir(&path).map_err(failure)? {
                let value = value.map_err(failure)?;
                if value
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with('.') && name.ends_with(".tmp"))
                {
                    fs::remove_file(value.path()).map_err(failure)?;
                    changed = true;
                }
            }
            if changed {
                *entry = measure(&path, family)?;
            }
        }
        let cache = Self {
            root,
            family,
            pinned,
            budget,
            entries: Mutex::new(BTreeMap::new()),
            _lease: lease,
        };
        cache.make_room(&mut entries, 0, now)?;
        *cache
            .entries
            .lock()
            .map_err(|_| failure("poisoned inventory"))? = entries;
        Ok(cache)
    }

    /// Resolves only registered inputs protected by this cache's link lease.
    pub(super) fn path(&self, key: &str) -> Result<PathBuf, BuildOneError> {
        if !self.pinned.contains(key) {
            return Err(failure(
                "artifact is not registered in the active consumer set",
            ));
        }
        Ok(self.root.join("entries").join(key))
    }

    /// A verified hit refreshes only access metadata, never object bytes.
    pub(super) fn touch(&self, key: &str) -> Result<(), BuildOneError> {
        let path = self.path(key)?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| failure("poisoned inventory"))?;
        touch(&path)?;
        entries.insert(key.to_owned(), measure(&path, self.family)?);
        Ok(())
    }

    /// Reserves staging space before writing. Publication and inventory updates
    /// are serialized while object emission remains parallel outside this call.
    pub(super) fn publish(
        &self,
        key: &str,
        object_name: &str,
        object: &[u8],
        manifest: &[u8],
    ) -> Result<(), BuildOneError> {
        if !self.family.payload_names().contains(&object_name) {
            return Err(failure("unexpected cache payload filename"));
        }
        let path = self.path(key)?;
        let additional = u64::try_from(object.len())
            .ok()
            .and_then(|bytes| {
                u64::try_from(manifest.len())
                    .ok()
                    .and_then(|manifest| bytes.checked_add(manifest))
            })
            .ok_or_else(|| failure("object size overflow"))?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| failure("poisoned inventory"))?;
        self.make_room(&mut entries, additional, SystemTime::now())?;
        directory(&path)?;
        let result = publish_file(&path.join(object_name), object)
            .and_then(|()| publish_file(&path.join("manifest.v1"), manifest))
            .and_then(|()| touch(&path));
        // Failed publication can still leave a complete object without a seal.
        // Account for those bytes before another worker reserves capacity.
        entries.insert(key.to_owned(), measure(&path, self.family)?);
        result
    }

    fn make_room(
        &self,
        entries: &mut BTreeMap<String, Entry>,
        additional: u64,
        now: SystemTime,
    ) -> Result<(), BuildOneError> {
        let mut bytes = entries
            .values()
            .try_fold(additional, |total, entry| total.checked_add(entry.bytes))
            .ok_or_else(|| failure("cache byte count overflow"))?;
        let mut count = entries.len()
            + self
                .pinned
                .iter()
                .filter(|key| !entries.contains_key(*key))
                .count();
        let mut eligible = entries
            .iter()
            .filter(|(key, _)| !self.pinned.contains(*key))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect::<Vec<_>>();
        eligible.sort_by_key(|(key, entry)| (entry.used, key.clone()));
        let mut retire = Vec::new();
        for (key, entry) in eligible {
            let expired = now
                .duration_since(entry.used)
                .is_ok_and(|age| age > self.budget.age);
            if expired || bytes > self.budget.bytes || count > self.budget.entries {
                bytes -= entry.bytes;
                count -= 1;
                retire.push(key);
            }
        }
        if bytes > self.budget.bytes || count > self.budget.entries {
            return Err(failure("active consumer inputs and staging exceed the artifact-cache budget; nothing was evicted"));
        }
        for key in retire {
            let source = self.root.join("entries").join(&key);
            let retired = self.root.join("retired").join(&key);
            fs::rename(&source, &retired).map_err(failure)?;
            entries.remove(&key);
            remove_flat_entry(&retired, self.family)?;
        }
        Ok(())
    }
}

fn touch(path: &Path) -> Result<(), BuildOneError> {
    let marker = path.join(ACCESS);
    if let Ok(metadata) = fs::symlink_metadata(&marker) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(failure("access marker is not an owned regular file"));
        }
    }
    File::create(marker)
        .and_then(|file| file.set_modified(SystemTime::now()))
        .map_err(failure)
}

fn inventory(root: &Path, family: CacheFamily) -> Result<BTreeMap<String, Entry>, BuildOneError> {
    let mut entries = BTreeMap::new();
    for value in fs::read_dir(root).map_err(failure)? {
        if entries.len() == MAX_SCAN_ENTRIES {
            return Err(failure("cache inventory exceeds its scan budget"));
        }
        let value = value.map_err(failure)?;
        let key = value
            .file_name()
            .into_string()
            .map_err(|_| failure("non-UTF-8 cache entry"))?;
        if !is_sha256(&key) || !value.file_type().map_err(failure)?.is_dir() {
            return Err(failure("unowned entry in artifact-cache namespace"));
        }
        entries.insert(key, measure(&value.path(), family)?);
    }
    Ok(entries)
}

fn allowed_file(name: &str, family: CacheFamily) -> bool {
    if family.payload_names().contains(&name)
        || matches!(name, "manifest.v1" | "build.lock" | ACCESS)
    {
        return true;
    }
    family.temporary_prefixes().iter().any(|prefix| {
        name.strip_prefix(prefix)
            .and_then(|suffix| suffix.strip_suffix(".tmp"))
            .and_then(|suffix| suffix.split_once('.'))
            .is_some_and(|(pid, ordinal)| {
                !pid.is_empty()
                    && !ordinal.is_empty()
                    && pid.bytes().all(|byte| byte.is_ascii_digit())
                    && ordinal.bytes().all(|byte| byte.is_ascii_digit())
            })
    })
}

fn measure(path: &Path, family: CacheFamily) -> Result<Entry, BuildOneError> {
    let metadata = fs::symlink_metadata(path).map_err(failure)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(failure("object generation is not an owned directory"));
    }
    let mut bytes = 0_u64;
    let mut used = metadata.modified().map_err(failure)?;
    for (count, value) in fs::read_dir(path).map_err(failure)?.enumerate() {
        if count == MAX_ENTRY_FILES {
            return Err(failure("object generation exceeds its file budget"));
        }
        let value = value.map_err(failure)?;
        let name = value.file_name();
        if !name.to_str().is_some_and(|name| allowed_file(name, family))
            || !value.file_type().map_err(failure)?.is_file()
        {
            return Err(failure("unowned file in object generation"));
        }
        let metadata = value.metadata().map_err(failure)?;
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or_else(|| failure("object byte count overflow"))?;
        used = used.max(metadata.modified().map_err(failure)?);
    }
    Ok(Entry { bytes, used })
}

/// Flat, allowlisted unlinking never recursively follows a supplied directory.
fn remove_flat_entry(path: &Path, family: CacheFamily) -> Result<(), BuildOneError> {
    measure(path, family)?;
    for value in fs::read_dir(path).map_err(failure)? {
        fs::remove_file(value.map_err(failure)?.path()).map_err(failure)?;
    }
    fs::remove_dir(path).map_err(failure)
}
