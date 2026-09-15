use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use std::time::Duration;

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(5))
}

fn tree(root: &Path) {
    fs::create_dir(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("lib/rustlib")).unwrap();
    fs::write(root.join("bin/rustc"), "compiler").unwrap();
    fs::write(root.join("lib/rustlib/library"), "runtime").unwrap();
}

#[test]
fn tool_tree_matches_unchanged_bytes_and_rejects_timestamp_restored_mutation() {
    let fixture = temporary_fixture("tool-tree-content");
    tree(&fixture.0);
    let before = ToolTree::capture(&fixture.0, control()).unwrap();
    assert!(before == ToolTree::capture(&fixture.0, control()).unwrap());
    let path = fixture.0.join("lib/rustlib/library");
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "changed").unwrap();
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert!(before != ToolTree::capture(&fixture.0, control()).unwrap());
    assert_eq!(before.json()["bytes"], 15);
}

#[test]
fn tool_tree_binds_membership_and_not_unselected_documentation() {
    let fixture = temporary_fixture("tool-tree-membership");
    tree(&fixture.0);
    let before = ToolTree::capture(&fixture.0, control()).unwrap();
    fs::create_dir(fixture.0.join("share")).unwrap();
    fs::write(fixture.0.join("share/manual"), "documentation").unwrap();
    assert!(before == ToolTree::capture(&fixture.0, control()).unwrap());
    let extra = fixture.0.join("lib/component");
    fs::write(&extra, "additional").unwrap();
    assert!(before != ToolTree::capture(&fixture.0, control()).unwrap());
    fs::remove_file(extra).unwrap();
    assert!(before == ToolTree::capture(&fixture.0, control()).unwrap());
}

#[test]
fn tool_tree_enforces_byte_entry_and_cancellation_budgets() {
    let fixture = temporary_fixture("tool-tree-budget");
    tree(&fixture.0);
    assert!(capture(&fixture.0, control(), 1, MAX_BYTES).is_err());
    assert!(capture(&fixture.0, control(), MAX_ENTRIES, 1).is_err());
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    assert_eq!(
        ToolTree::capture(&fixture.0, control().with_cancellation(&cancelled))
            .err()
            .unwrap()
            .outcome,
        "cancelled"
    );
    let missing = temporary_fixture("tool-tree-missing");
    assert!(ToolTree::capture(&missing.0, control()).is_err());
}

#[cfg(unix)]
#[test]
fn tool_tree_checks_file_links_rejects_escapes_cycles_and_special_files() {
    use std::os::unix::fs::symlink;
    let fixture = temporary_fixture("tool-tree-links");
    tree(&fixture.0);
    let alias = fixture.0.join("lib/alias");
    symlink("rustlib/library", &alias).unwrap();
    let before = ToolTree::capture(&fixture.0, control()).unwrap();
    fs::remove_file(&alias).unwrap();
    symlink("../bin/rustc", &alias).unwrap();
    assert!(before != ToolTree::capture(&fixture.0, control()).unwrap());
    fs::remove_file(&alias).unwrap();
    symlink("/bin/sh", &alias).unwrap();
    assert!(ToolTree::capture(&fixture.0, control()).is_err());
    fs::remove_file(&alias).unwrap();
    symlink(".", &alias).unwrap();
    assert!(ToolTree::capture(&fixture.0, control()).is_err());
    fs::remove_file(alias).unwrap();
    let mut command = std::process::Command::new("mkfifo");
    command.arg(fixture.0.join("lib/pipe"));
    control().run(&mut command, |_| Ok(())).unwrap();
    assert!(ToolTree::capture(&fixture.0, control()).is_err());
}
