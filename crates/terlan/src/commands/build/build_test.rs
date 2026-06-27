use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::test_fs;
    use crate::validation::target_profile::TargetProfile;
    use std::path::PathBuf;
    use std::process::Command;

    /// Builds a command argument vector from string slices.
    ///
    /// Inputs:
    /// - `items`: borrowed argument strings.
    ///
    /// Output:
    /// - Owned `String` vector accepted by parser helpers.
    ///
    /// Transformation:
    /// - Clones each slice into owned CLI-like arguments.
    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    /// Creates a clean temporary directory for build command tests.
    ///
    /// Inputs:
    /// - `name`: stable test-specific name segment.
    ///
    /// Output:
    /// - Path to an empty directory under the process temp directory.
    ///
    /// Transformation:
    /// - Delegates to the shared test filesystem helper with the build-command
    ///   namespace.
    fn make_temp_dir(name: &str) -> PathBuf {
        test_fs::temp_dir("build_command", name)
    }

    /// Asserts that a generated file has at least one executable bit when the
    /// platform exposes Unix mode bits.
    ///
    /// Inputs:
    /// - `path`: generated file path.
    ///
    /// Output:
    /// - Test assertion success or panic.
    ///
    /// Transformation:
    /// - Reads Unix mode bits and verifies user/group/other execute permission
    ///   exists; non-Unix platforms use a no-op fallback.
    #[cfg(unix)]
    fn assert_executable_bit(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(path)
            .expect("read executable metadata")
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "launcher should be executable");
    }

    /// Asserts that a generated file has at least one executable bit when the
    /// platform exposes Unix mode bits.
    ///
    /// Inputs:
    /// - `path`: generated file path.
    ///
    /// Output:
    /// - Always succeeds on non-Unix platforms.
    ///
    /// Transformation:
    /// - Keeps launcher tests portable while execution permissions remain a
    ///   Unix-specific build artifact detail.
    #[cfg(not(unix))]
    fn assert_executable_bit(_path: &Path) {}

    mod args_test;
    mod artifact_test;
    mod data_closure_test;
    mod dependency_test;
    mod diagnostics_test;
    mod executable_language_test;
    mod import_constructor_test;
    mod project_layout_test;
    mod sql_runtime_test;
    mod std_collection_mutator_test;
    mod std_collection_test;
    mod std_import_test;
    mod std_runtime_test;
    mod std_trait_test;
    mod wasm_artifact_metadata_test;
    mod wasm_build_target_test;
}
