use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::run_dormant_runtime_code;

#[test]
fn dormant_runtime_code_rejects_uninventoried_vm_module() {
    let root = make_quality_temp_dir("uninventoried");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "pub(crate) mod map_layout;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/map_layout.rs",
        "pub(crate) fn select() -> i32 { 1 }\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\n",
    );

    let error = run_dormant_runtime_code(&root).expect_err("dormant module should fail");
    assert!(
        error.contains("crates/terlan/src/runtime/vm/map_layout.rs"),
        "expected dormant module diagnostic: {error}"
    );
}

#[test]
fn dormant_runtime_code_accepts_inventoried_vm_module() {
    let root = make_quality_temp_dir("inventoried");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "pub(crate) mod map_layout;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/map_layout.rs",
        "pub(crate) fn select() -> i32 { 1 }\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\ncrates/terlan/src/runtime/vm/map_layout.rs\tdesign-only\tA-CHAMP selection is not wired yet\tWire storage or delete design code\n",
    );

    let summary = run_dormant_runtime_code(&root).expect("inventoried dormant module");
    assert_eq!(summary.dormant_module_count, 1);
    assert_eq!(summary.inventory_row_count, 1);
}

#[test]
fn dormant_runtime_code_rejects_unsupported_inventory_classification() {
    let root = make_quality_temp_dir("unsupported_classification");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "pub(crate) mod map_layout;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/map_layout.rs",
        "pub(crate) fn select() -> i32 { 1 }\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\ncrates/terlan/src/runtime/vm/map_layout.rs\tmaybe-later\tA-CHAMP selection is not wired yet\tWire storage or delete design code\n",
    );

    let error = run_dormant_runtime_code(&root).expect_err("unsupported classification fails");
    assert!(
        error.contains("unsupported dormant runtime classification"),
        "expected unsupported classification diagnostic: {error}"
    );
}

#[test]
fn dormant_runtime_code_rejects_placeholder_inventory_metadata() {
    let root = make_quality_temp_dir("placeholder_metadata");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "pub(crate) mod map_layout;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/map_layout.rs",
        "pub(crate) fn select() -> i32 { 1 }\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\ncrates/terlan/src/runtime/vm/map_layout.rs\tdesign-only\tTODO\tFixme before release\n",
    );

    let error = run_dormant_runtime_code(&root).expect_err("placeholder metadata fails");
    assert!(
        error.contains("must not use placeholder values"),
        "expected placeholder diagnostic: {error}"
    );
}

#[test]
fn dormant_runtime_code_rejects_unsorted_inventory_rows() {
    let root = make_quality_temp_dir("unsorted_inventory");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "pub(crate) mod zeta;\npub(crate) mod alpha;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/alpha.rs",
        "pub(crate) fn select() -> i32 { 1 }\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/zeta.rs",
        "pub(crate) fn select() -> i32 { 1 }\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\ncrates/terlan/src/runtime/vm/zeta.rs\tdesign-only\tZeta is not wired yet\tWire or delete\ncrates/terlan/src/runtime/vm/alpha.rs\tdesign-only\tAlpha is not wired yet\tWire or delete\n",
    );

    let error = run_dormant_runtime_code(&root).expect_err("unsorted rows fail");
    assert!(
        error.contains("must be sorted by path"),
        "expected sorted inventory diagnostic: {error}"
    );
}

#[test]
fn dormant_runtime_code_rejects_duplicate_inventory_rows() {
    let root = make_quality_temp_dir("duplicate_inventory");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "pub(crate) mod map_layout;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/map_layout.rs",
        "pub(crate) fn select() -> i32 { 1 }\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\ncrates/terlan/src/runtime/vm/map_layout.rs\tdesign-only\tA-CHAMP selection is not wired yet\tWire storage or delete design code\ncrates/terlan/src/runtime/vm/map_layout.rs\tdesign-only\tDuplicate evidence must fail\tRemove duplicate row\n",
    );

    let error = run_dormant_runtime_code(&root).expect_err("duplicate rows should fail");
    assert!(
        error.contains("duplicate row"),
        "expected duplicate inventory diagnostic: {error}"
    );
}

#[test]
fn dormant_runtime_code_rejects_stale_inventory_after_runtime_reference() {
    let root = make_quality_temp_dir("stale");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "pub(crate) mod map_layout;\nfn active() { let _ = map_layout::select(); }\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/map_layout.rs",
        "pub(crate) fn select() -> i32 { 1 }\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\ncrates/terlan/src/runtime/vm/map_layout.rs\tdesign-only\tA-CHAMP selection is not wired yet\tWire storage or delete design code\n",
    );

    let error = run_dormant_runtime_code(&root).expect_err("stale inventory should fail");
    assert!(
        error.contains("stale dormant runtime inventory row"),
        "expected stale inventory diagnostic: {error}"
    );
}

#[test]
fn dormant_runtime_code_accepts_textually_included_vm_source() {
    let root = make_quality_temp_dir("included");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "include!(\"vm/evaluation_helpers.rs\");\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/evaluation_helpers.rs",
        "pub(crate) fn select() -> i32 { 1 }\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\n",
    );

    let summary = run_dormant_runtime_code(&root).expect("included source is active");
    assert_eq!(summary.dormant_module_count, 0);
    assert_eq!(summary.inventory_row_count, 0);
}

#[test]
fn dormant_runtime_code_ignores_numbered_test_fragments() {
    let root = make_quality_temp_dir("test_fragment");
    write_file(&root, "crates/terlan/src/runtime/vm.rs", "");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/http_test_part_001.rs",
        "#[test]\nfn request_works() {}\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\n",
    );

    let summary = run_dormant_runtime_code(&root).expect("test fragments are not runtime modules");
    assert_eq!(summary.dormant_module_count, 0);
}

#[test]
fn dormant_runtime_code_accepts_path_split_vm_implementation() {
    let root = make_quality_temp_dir("path_split");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "pub(crate) mod actor;\nfn run(_: actor::VmActorRuntime) {}\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/actor.rs",
        "pub(crate) struct VmActorRuntime;\n#[path = \"actor_exit.rs\"]\nmod actor_exit;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/actor_exit.rs",
        "impl VmActorRuntime { pub(crate) fn exit_actor(&mut self) {} }\n",
    );
    write_file(
        &root,
        "docs/runtime/DORMANT_RUNTIME_CODE.tsv",
        "path\tclassification\treason\tnext_action\n",
    );

    let summary = run_dormant_runtime_code(&root).expect("path-split source is active");
    assert_eq!(summary.dormant_module_count, 0);
    assert_eq!(summary.inventory_row_count, 0);
}

fn write_file(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture directory");
    }
    fs::write(path, text).expect("write fixture file");
}

fn make_quality_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "terlan_dormant_runtime_code_{label}_{}_{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&path).expect("create quality temp dir");
    path
}
