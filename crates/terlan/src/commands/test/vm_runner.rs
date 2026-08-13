use std::path::Path;
use std::time::Instant;

use super::manifest::{TestRunReport, TestRunResult, TestRunStatus};
use super::DiscoveredTest;
use crate::runtime::vm::package_native_helper::{execute_call, VmPackageNativeHelpers};
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;

/// Executes selected tests exclusively through native exports.
pub(super) fn run_discovered_terlan_vm_tests(
    module_name: &str,
    tests: &[DiscoveredTest],
    native_image: Option<&Path>,
    native_helper_environment: &[(String, std::path::PathBuf)],
    benchmark: Option<(usize, usize)>,
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
    let mut package_helpers =
        VmPackageNativeHelpers::from_helper_environment(native_helper_environment)?;

    for test in tests {
        let qualified_name = format!("{module_name}.{}", test.name);
        if let Some((warmup, samples)) = benchmark {
            let result = run_benchmark_case(
                &mut native,
                &mut package_helpers,
                &qualified_name,
                warmup,
                samples,
            );
            match result {
                Ok(measurement) => {
                    passed += 1;
                    results.push(TestRunResult {
                        name: test.name.clone(),
                        kind: test.kind,
                        status: TestRunStatus::Passed,
                        message: None,
                        execution_nanoseconds: measurement.median_nanoseconds,
                        benchmark_samples: Some(samples),
                        benchmark_min_nanoseconds: Some(measurement.min_nanoseconds),
                        benchmark_p95_nanoseconds: Some(measurement.p95_nanoseconds),
                        span_start: test.span_start,
                        span_end: test.span_end,
                    });
                }
                Err(message) => {
                    failed += 1;
                    results.push(TestRunResult {
                        name: test.name.clone(),
                        kind: test.kind,
                        status: TestRunStatus::Failed,
                        message: Some(message),
                        execution_nanoseconds: 0,
                        benchmark_samples: Some(samples),
                        benchmark_min_nanoseconds: None,
                        benchmark_p95_nanoseconds: None,
                        span_start: test.span_start,
                        span_end: test.span_end,
                    });
                }
            }
            continue;
        }

        let started = Instant::now();
        let result = execute_boolean_case(&mut native, &mut package_helpers, &qualified_name);
        let execution_nanoseconds = started.elapsed().as_nanos().max(1);
        match result {
            Ok(()) => {
                passed += 1;
                results.push(TestRunResult {
                    name: test.name.clone(),
                    kind: test.kind,
                    status: TestRunStatus::Passed,
                    message: None,
                    execution_nanoseconds,
                    benchmark_samples: None,
                    benchmark_min_nanoseconds: None,
                    benchmark_p95_nanoseconds: None,
                    span_start: test.span_start,
                    span_end: test.span_end,
                });
            }
            Err(message) => {
                failed += 1;
                results.push(TestRunResult {
                    name: test.name.clone(),
                    kind: test.kind,
                    status: TestRunStatus::Failed,
                    message: Some(message),
                    execution_nanoseconds,
                    benchmark_samples: None,
                    benchmark_min_nanoseconds: None,
                    benchmark_p95_nanoseconds: None,
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

struct BenchmarkMeasurement {
    min_nanoseconds: u128,
    median_nanoseconds: u128,
    p95_nanoseconds: u128,
}

fn execute_boolean_case(
    native: &mut PureNativeExecutionShard,
    package_helpers: &mut VmPackageNativeHelpers,
    qualified_name: &str,
) -> Result<(), String> {
    match execute_call(native, package_helpers, qualified_name, &[]) {
        Ok(ReplValue::Bool(true)) => Ok(()),
        Ok(ReplValue::Bool(false)) => Err("assertion returned false".to_string()),
        Ok(value) => Err(format!("unexpected test result: {}", value.render())),
        Err(message) => Err(message.to_string()),
    }
}

fn run_benchmark_case(
    native: &mut PureNativeExecutionShard,
    package_helpers: &mut VmPackageNativeHelpers,
    qualified_name: &str,
    warmup: usize,
    samples: usize,
) -> Result<BenchmarkMeasurement, String> {
    for _ in 0..warmup {
        execute_boolean_case(native, package_helpers, qualified_name)?;
    }
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        execute_boolean_case(native, package_helpers, qualified_name)?;
        durations.push(started.elapsed().as_nanos().max(1));
    }
    durations.sort_unstable();
    let median_nanoseconds = durations[durations.len() / 2];
    let p95_index = durations
        .len()
        .saturating_mul(95)
        .div_ceil(100)
        .saturating_sub(1);
    Ok(BenchmarkMeasurement {
        min_nanoseconds: durations[0],
        median_nanoseconds,
        p95_nanoseconds: durations[p95_index],
    })
}
