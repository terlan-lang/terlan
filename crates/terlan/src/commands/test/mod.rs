mod discovery;
mod execution;
mod manifest;
mod project_context;
mod style;
#[cfg(test)]
mod test_shape_import_test;
mod vm_runner;
mod wasm;

#[cfg(test)]
#[path = "test_command_test.rs"]
#[cfg(test)]
mod test_command_test;

pub(crate) use execution::run;

use discovery::{discover_tests, select_tests, DiscoveredTest};
use execution::{print_runtime_test_report, TestArgs, TEST_SOURCE_PATTERN_DESCRIPTION};
use manifest::{
    write_test_manifest, write_test_result_manifest, TestRunReport, TestRunResult, TestRunStatus,
};
use project_context::{
    collect_test_files, is_test_source_path, prepare_test_project_context,
    test_target_profile_options,
};
use style::TestOutputStyle;
