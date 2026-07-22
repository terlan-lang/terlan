use std::path::{Path, PathBuf};

/// Finds the installed `share/terlan` directory beside a shipped executable.
pub(crate) fn installed_share_root() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|executable| share_root_for_executable(&executable))
}

/// Resolves an installed share root from one executable path.
fn share_root_for_executable(executable: &Path) -> Option<PathBuf> {
    let parent = executable.parent()?;
    [
        parent.join("share/terlan"),
        parent.parent()?.join("share/terlan"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_dir())
}

#[cfg(test)]
#[path = "release_layout_test.rs"]
mod release_layout_test;
