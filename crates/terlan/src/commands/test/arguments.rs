use std::path::PathBuf;

/// Supported backend runner for `terlc test`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TestTarget {
    TerlanVm,
    Js,
    Wasm,
}

pub(super) const TEST_SOURCE_PATTERN_DESCRIPTION: &str = "*Test.terl or *_test.terl";

/// Parsed command-local arguments for `terlc test`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestArgs {
    pub(super) path: String,
    pub(super) additional_paths: Vec<String>,
    pub(super) target: TestTarget,
    pub(super) test_names: Vec<String>,
    pub(super) benchmark: bool,
    pub(super) benchmark_warmup: usize,
    pub(super) benchmark_samples: usize,
    pub(super) emit_test_manifest: Option<PathBuf>,
    pub(super) emit_test_result_manifest: Option<PathBuf>,
}

/// Parses command-local arguments for `terlc test`.
///
/// Inputs:
/// - `args`: command arguments after the CLI dispatcher has removed global
///   options and the command verb.
///
/// Output:
/// - `Ok(TestArgs)` for zero or more source paths, an optional target,
///   repeated exact-name selectors, benchmark controls, and single-path
///   manifest destinations.
/// - `Err(message)` for malformed flags, unsupported targets, or ambiguous
///   multi-path manifest output.
///
/// Transformation:
/// - Separates ordered source paths from shared test-run policy without
///   touching the filesystem or starting a compiler session.
pub(super) fn parse_test_args(args: &[String]) -> Result<TestArgs, String> {
    let mut paths = Vec::new();
    let mut target = TestTarget::TerlanVm;
    let mut test_names = Vec::new();
    let mut benchmark = false;
    let mut benchmark_warmup = 1usize;
    let mut benchmark_samples = 10usize;
    let mut benchmark_tuning_seen = false;
    let mut emit_test_manifest = None;
    let mut emit_test_result_manifest = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--target requires a value".to_string());
                };
                target = match value.as_str() {
                    "erlang" => {
                        return Err(
                            "test target `erlang` was removed from the public CLI; use `terlan-vm`"
                                .to_string(),
                        );
                    }
                    "terlan-vm" => TestTarget::TerlanVm,
                    "js" => TestTarget::Js,
                    "wasm" | "wasm.core" => TestTarget::Wasm,
                    other => return Err(format!("unsupported test target: {other}")),
                };
                i += 2;
            }
            "--name" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--name requires a test function name".to_string());
                };
                if test_names.contains(value) {
                    return Err(format!("duplicate --name selector: {value}"));
                }
                test_names.push(value.clone());
                i += 2;
            }
            "--bench" => {
                if benchmark {
                    return Err("duplicate --bench".to_string());
                }
                benchmark = true;
                i += 1;
            }
            "--warmup" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--warmup requires a non-negative integer".to_string());
                };
                benchmark_warmup = value
                    .parse::<usize>()
                    .map_err(|_| "--warmup requires a non-negative integer".to_string())?;
                benchmark_tuning_seen = true;
                i += 2;
            }
            "--samples" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--samples requires a positive integer".to_string());
                };
                benchmark_samples = value
                    .parse::<usize>()
                    .ok()
                    .filter(|samples| *samples > 0)
                    .ok_or_else(|| "--samples requires a positive integer".to_string())?;
                benchmark_tuning_seen = true;
                i += 2;
            }
            "--emit-test-manifest" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--emit-test-manifest requires a path".to_string());
                };
                if emit_test_manifest.replace(PathBuf::from(value)).is_some() {
                    return Err("duplicate --emit-test-manifest".to_string());
                }
                i += 2;
            }
            "--emit-test-result-manifest" => {
                let Some(value) = args.get(i + 1) else {
                    return Err("--emit-test-result-manifest requires a path".to_string());
                };
                if emit_test_result_manifest
                    .replace(PathBuf::from(value))
                    .is_some()
                {
                    return Err("duplicate --emit-test-result-manifest".to_string());
                }
                i += 2;
            }
            arg if arg.starts_with("--") => {
                return Err(format!("unsupported test option: {arg}"));
            }
            arg => {
                paths.push(arg.to_string());
                i += 1;
            }
        }
    }

    if benchmark_tuning_seen && !benchmark {
        return Err("--warmup and --samples require --bench".to_string());
    }
    if benchmark && target != TestTarget::TerlanVm {
        return Err("@benchmark execution currently requires --target terlan-vm".to_string());
    }
    if paths.len() > 1 && (emit_test_manifest.is_some() || emit_test_result_manifest.is_some()) {
        return Err("test manifest output requires exactly one source path".to_string());
    }

    let path = if paths.is_empty() {
        "tests".to_string()
    } else {
        paths.remove(0)
    };

    Ok(TestArgs {
        path,
        additional_paths: paths,
        target,
        test_names,
        benchmark,
        benchmark_warmup,
        benchmark_samples,
        emit_test_manifest,
        emit_test_result_manifest,
    })
}
