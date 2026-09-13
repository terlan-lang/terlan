//! Owns an AOT test's disposable source, image, and diagnostic files.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct FixtureDirectory(Option<PathBuf>);

impl FixtureDirectory {
    pub(super) fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "terlan-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        // A fresh leaf is required; never adopt an existing directory as ours.
        fs::create_dir(&path).expect("create owned AOT fixture directory");
        Self(Some(path))
    }

    pub(super) fn path(&self) -> &Path {
        self.0.as_deref().expect("open AOT fixture directory")
    }

    pub(super) fn close(mut self) {
        fs::remove_dir_all(self.path()).expect("remove owned AOT fixture directory");
        self.0 = None;
    }
}

impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            // Best effort during assertion unwinding, without masking its cause.
            if let Err(error) = fs::remove_dir_all(path) {
                eprintln!("AOT fixture cleanup failed for {}: {error}", path.display());
            }
        }
    }
}
