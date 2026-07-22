use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::test_fs;
    use crate::validation::target_profile::TargetProfile;
    use std::path::PathBuf;

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

    mod annotation_isolation_artifact_test;
    mod args_test;
    mod artifact_test;
    mod asm_labels_artifact_test;
    mod debug_info_artifact_test;
    mod dependency_test;
    mod deterministic_artifact_test;
    mod embedded_line_coverage_artifact_test;
    mod executable_vm_artifact_test;
    mod import_constructor_test;
    mod js_target_diagnostics_test;
    mod key_compatibility_test;
    mod latin1_source_policy_test;
    mod mobile_build_test;
    mod parallel_compilation_test;
    mod project_layout_test;
    mod shape_js_test;
    mod std_runtime_test;
    mod wasm_artifact_metadata_test;
    mod wasm_build_target_test;
}
