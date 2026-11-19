//! Common paths used by the focused Make test helpers.

use std::path::{Path, PathBuf};

pub(super) fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(super) fn included_sources(root: &Path, makefile: &str) -> Vec<PathBuf> {
    makefile
        .lines()
        .filter_map(|line| line.strip_prefix("include "))
        .flat_map(str::split_whitespace)
        .map(|path| root.join(path))
        .collect()
}
