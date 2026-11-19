//! Shared access to the complete production Make source graph.

use std::fs;

mod common;

/// Reads the root Makefile and every literal source it includes, in include order.
pub fn read_make_sources() -> String {
    let root = common::repository();
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    let includes = common::included_sources(&root, &makefile);
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
