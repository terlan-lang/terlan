use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::share_root_for_executable;

fn temp_release_layout(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-release-layout-{name}-{}-{unique}",
        std::process::id()
    ))
}

#[test]
fn share_root_for_executable_accepts_archive_and_prefix_layouts() {
    let root = temp_release_layout("accepted");
    let archive_bin = root.join("archive");
    fs::create_dir_all(archive_bin.join("share/terlan")).expect("archive share");
    assert_eq!(
        share_root_for_executable(&archive_bin.join("terlc")),
        Some(archive_bin.join("share/terlan"))
    );

    let prefix = root.join("prefix");
    fs::create_dir_all(prefix.join("bin")).expect("prefix bin");
    fs::create_dir_all(prefix.join("share/terlan")).expect("prefix share");
    assert_eq!(
        share_root_for_executable(&prefix.join("bin/terlc")),
        Some(prefix.join("share/terlan"))
    );
    fs::remove_dir_all(root).expect("remove temp release layout");
}

#[test]
fn share_root_for_executable_rejects_partial_layout() {
    let root = temp_release_layout("partial");
    let binary = root.join("bin/terlc");
    fs::create_dir_all(binary.parent().expect("bin parent")).expect("bin");
    assert_eq!(share_root_for_executable(&binary), None);
    fs::remove_dir_all(root).expect("remove temp release layout");
}
