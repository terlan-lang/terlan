//! Shared access to the complete production Make source graph.

use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn included_sources(root: &Path, makefile: &str) -> Vec<PathBuf> {
    makefile
        .lines()
        .filter_map(|line| line.strip_prefix("include "))
        .flat_map(str::split_whitespace)
        .map(|path| root.join(path))
        .collect()
}

/// Reads the root Makefile and every literal source it includes, in include order.
#[allow(dead_code)]
pub fn read_make_sources() -> String {
    let root = repository();
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    let includes = included_sources(&root, &makefile);
    let mut source = makefile
        .lines()
        .filter(|line| !line.starts_with("include "))
        .collect::<Vec<_>>()
        .join("\n");
    source.push('\n');
    for include in includes {
        source.push('\n');
        source.push_str(&fs::read_to_string(include).unwrap());
    }
    source
}

/// Copies the complete production Make source graph and auxiliary Make fragments.
#[allow(dead_code)]
pub fn copy_make_sources(destination: &Path) {
    let root = repository();
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    fs::write(destination.join("Makefile"), &makefile).unwrap();
    for source in included_sources(&root, &makefile) {
        let relative = source.strip_prefix(&root).unwrap();
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
