use std::fs;

use super::support::{make_quality_temp_dir, read_make_graph};

#[test]
fn make_graph_reads_root_and_literal_includes() {
    let root = make_quality_temp_dir("make_graph_includes");
    fs::create_dir(root.join("mk")).unwrap();
    fs::write(
        root.join("Makefile"),
        "root:\ninclude mk/one.mk mk/two.mk\n-include mk/optional.mk $(DYNAMIC_MAKE)\n",
    )
    .unwrap();
    fs::write(root.join("mk/one.mk"), "one:\ninclude mk/nested.mk\n").unwrap();
    fs::write(root.join("mk/two.mk"), "two:\n").unwrap();
    fs::write(root.join("mk/nested.mk"), "nested:\n").unwrap();

    let graph = read_make_graph(&root).unwrap();
    assert!(graph.contains("root:"));
    assert!(graph.contains("one:"));
    assert!(graph.contains("two:"));
    assert!(graph.contains("nested:"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn make_graph_reports_a_missing_root() {
    let root = make_quality_temp_dir("make_graph_missing_root");
    let error = read_make_graph(&root).unwrap_err();
    assert!(error.contains("Makefile: failed to read Make source"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn make_graph_reports_a_missing_include() {
    let root = make_quality_temp_dir("make_graph_missing_include");
    fs::write(root.join("Makefile"), "include mk/missing.mk\n").unwrap();
    let error = read_make_graph(&root).unwrap_err();
    assert!(error.contains("mk/missing.mk: failed to read Make source"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn make_graph_terminates_include_cycles() {
    let root = make_quality_temp_dir("make_graph_cycle");
    fs::create_dir(root.join("mk")).unwrap();
    fs::write(root.join("Makefile"), "include mk/cycle.mk\nroot:\n").unwrap();
    fs::write(root.join("mk/cycle.mk"), "include Makefile\ncycle:\n").unwrap();

    let graph = read_make_graph(&root).unwrap();
    assert_eq!(graph.matches("root:").count(), 1);
    assert_eq!(graph.matches("cycle:").count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn make_graph_rejects_repository_escape() {
    let root = make_quality_temp_dir("make_graph_escape");
    fs::write(root.join("Makefile"), "include ../outside.mk\n").unwrap();

    let error = read_make_graph(&root).unwrap_err();
    assert!(error.contains("must stay inside the repository"));
    fs::remove_dir_all(root).unwrap();
}
