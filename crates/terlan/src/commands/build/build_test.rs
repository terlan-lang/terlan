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

    #[cfg(test)]
    mod annotation_isolation_artifact_test;
    #[cfg(test)]
    mod args_test;
    #[cfg(test)]
    mod artifact_test;
    #[cfg(test)]
    mod asm_labels_artifact_test;
    #[cfg(test)]
    mod debug_info_artifact_test;
    #[cfg(test)]
    mod dependency_test;
    #[cfg(test)]
    mod deterministic_artifact_test;
    #[cfg(test)]
    mod embedded_line_coverage_artifact_test;
    #[cfg(test)]
    mod executable_vm_artifact_test;
    #[cfg(test)]
    mod import_constructor_test;
    #[cfg(test)]
    mod js_target_diagnostics_test;
    #[cfg(test)]
    mod key_compatibility_test;
    #[cfg(test)]
    mod latin1_source_policy_test;
    #[cfg(test)]
    mod parallel_compilation_test;
    #[cfg(test)]
    mod project_layout_test;
    #[cfg(test)]
    mod shape_js_test;
    #[cfg(test)]
    mod std_runtime_test;
    #[cfg(test)]
    mod std_source_application_closure_test;
    #[cfg(test)]
    mod wasm_artifact_metadata_test;
    #[cfg(test)]
    mod wasm_build_target_test;
}
