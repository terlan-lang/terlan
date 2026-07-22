use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn temp_db_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "terlan_db_command_test_{}_{}_{}",
        std::process::id(),
        nanos,
        label
    ));
    fs::create_dir_all(&directory).expect("create temp db command directory");
    directory
}

pub(super) fn remove_dir(directory: &Path) {
    fs::remove_dir_all(directory).expect("remove temp db command directory");
}
