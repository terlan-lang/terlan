//! Shared Makefile fixture support for build-cache integration tests.

use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Copies the root Makefile, its includes, and auxiliary Make fragments.
pub fn copy_make_sources(destination: &Path) {
    let root = repository();
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    fs::write(destination.join("Makefile"), &makefile).unwrap();
    for relative in makefile
        .lines()
        .filter_map(|line| line.strip_prefix("include "))
        .flat_map(str::split_whitespace)
    {
        let source = root.join(relative);
        let target = destination.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source, target).unwrap();
    }
    for entry in fs::read_dir(root.join("mk")).unwrap() {
        let source = entry.unwrap().path();
        if source.extension().and_then(|extension| extension.to_str()) != Some("mk") {
            continue;
        }
        let target = destination.join(source.strip_prefix(&root).unwrap());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source, target).unwrap();
    }
}
