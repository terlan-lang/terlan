use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Canonicalizes a path when possible.
pub(super) fn canonicalize_optional(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

/// Computes a coarse fingerprint for a directory tree.
pub(crate) fn directory_fingerprint(root: &Path, exclude: Option<&Path>) -> u64 {
    let mut files = Vec::new();
    collect_directory_files(root, exclude, &mut files);
    files.sort();

    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    for path in files {
        path.hash(&mut hasher);
        if let Ok(metadata) = fs::metadata(&path) {
            metadata.len().hash(&mut hasher);
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                    duration.as_nanos().hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}

fn collect_directory_files(root: &Path, exclude: Option<&Path>, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if exclude.is_some_and(|exclude| {
            fs::canonicalize(&path)
                .map(|canonical| canonical.starts_with(exclude))
                .unwrap_or(false)
        }) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            collect_directory_files(&path, exclude, files);
        } else if metadata.is_file() {
            files.push(path);
        }
    }
}
