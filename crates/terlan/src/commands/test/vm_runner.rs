use std::path::Path;

use super::manifest::{TestRunReport, TestRunResult, TestRunStatus};
use super::DiscoveredTest;
use crate::runtime::vm::package_native_helper::execute_call;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;

/// Executes selected tests exclusively through native exports.
pub(super) fn run_discovered_terlan_vm_tests(
    module_name: &str,
    tests: &[DiscoveredTest],
    native_image: Option<&Path>,
) -> Result<TestRunReport, String> {
    let native_image = native_image.ok_or_else(|| {
        format!(
            "error[test.aot_required]: test module `{module_name}` did not produce a native image; runtime CoreIR interpretation has been removed"
        )
    })?;
    let mut native = PureNativeExecutionShard::load_image(native_image)?;
    for test in tests {
        let qualified_name = format!("{module_name}.{}", test.name);
        if !native.has_export(&qualified_name, 0) {
            return Err(format!(
                "error[test.aot_export_missing]: native image does not contain `{qualified_name}/0`; runtime CoreIR interpretation has been removed"
            ));
        }
    }
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut results = Vec::new();
    let mut package_helper = None;

    for test in tests {
        let qualified_name = format!("{module_name}.{}", test.name);
        let result = execute_call(&mut native, &mut package_helper, &qualified_name, &[]);
        match result {
            Ok(ReplValue::Bool(true)) => {
                passed += 1;
                results.push(TestRunResult {
                    name: test.name.clone(),
                    status: TestRunStatus::Passed,
                    message: None,
                    span_start: test.span_start,
                    span_end: test.span_end,
                });
            }
            Ok(ReplValue::Bool(false)) => {
                failed += 1;
                results.push(TestRunResult {
                    name: test.name.clone(),
                    status: TestRunStatus::Failed,
                    message: Some("assertion returned false".to_string()),
                    span_start: test.span_start,
                    span_end: test.span_end,
                });
            }
            Ok(value) => {
                failed += 1;
                results.push(TestRunResult {
                    name: test.name.clone(),
                    status: TestRunStatus::Failed,
                    message: Some(format!("unexpected test result: {}", value.render())),
                    span_start: test.span_start,
                    span_end: test.span_end,
                });
            }
            Err(message) => {
                failed += 1;
                results.push(TestRunResult {
                    name: test.name.clone(),
                    status: TestRunStatus::Failed,
                    message: Some(message),
                    span_start: test.span_start,
                    span_end: test.span_end,
                });
            }
        }
    }

    native.shutdown()?;
    Ok(TestRunReport {
        passed,
        failed,
        results,
    })
}
