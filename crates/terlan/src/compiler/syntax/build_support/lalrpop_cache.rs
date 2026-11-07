//! Content-addressed sharing for feature-independent generated parser output.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const CONTRACT: &[u8] = b"terlan-lalrpop-cache-v1\0";

/// Identifies output by every input that can change generated parser code.
pub(crate) fn identity(grammar: &[u8], lockfile: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(CONTRACT);
    digest.update(Sha256::digest(grammar));
    digest.update(Sha256::digest(lockfile));
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Restores one cached parser into Cargo's profile-specific output directory.
pub(crate) fn restore(root: &Path, identity: &str, output: &Path) -> io::Result<bool> {
    let cached = cache_file(root, identity);
    if !cached.is_file() {
        return Ok(false);
    }
    let parent = output.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "parser output has no parent")
    })?;
    fs::create_dir_all(parent)?;
    fs::copy(cached, output)?;
    Ok(true)
}

/// Publishes generated output atomically after LALRPOP completes successfully.
pub(crate) fn record(root: &Path, identity: &str, output: &Path) -> io::Result<()> {
    fs::create_dir_all(root)?;
    let cached = cache_file(root, identity);
    let temporary = root.join(format!(".{identity}.{}.tmp", std::process::id()));
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    fs::copy(output, &temporary)?;
    fs::rename(temporary, cached)
}

fn cache_file(root: &Path, identity: &str) -> PathBuf {
    root.join(format!("{identity}.rs"))
}

#[cfg(test)]
#[path = "lalrpop_cache_test.rs"]
mod tests;
