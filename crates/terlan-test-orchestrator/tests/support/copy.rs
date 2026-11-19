//! Copies the production Make source graph into an isolated fixture.

use std::fs;
use std::path::Path;

#[path = "common.rs"]
mod common;

pub fn copy_make_sources(destination: &Path) {
    let root = common::repository();
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    fs::write(destination.join("Makefile"), &makefile).unwrap();
    for source in common::included_sources(&root, &makefile) {
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
