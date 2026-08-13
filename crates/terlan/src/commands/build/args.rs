use crate::compiler::native_ir::NativeCodegenPolicy;
use crate::validation::target_profile::{TargetFamily, TargetProfile};

/// Build target accepted by `terlc build`.
///
/// Inputs:
/// - Parsed from command-local `--target` arguments.
///
/// Output:
/// - Backend target selected for artifact generation.
///
/// Transformation:
/// - Narrows free-form CLI strings to the release-supported backend set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BuildTarget {
    Js(TargetProfile),
    TerlanVm,
    WasmCore,
}

/// Parsed command-local arguments for `terlc build`.
///
/// Inputs:
/// - Produced from the raw command-local argument vector.
///
/// Output:
/// - One source path, one backend target, and declaration-output intent.
///
/// Transformation:
/// - Separates source selection from target selection before the build runner
///   touches the filesystem or compiler pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BuildArgs {
    pub(super) path: String,
    pub(super) target: BuildTarget,
    pub(super) target_explicit: bool,
    pub(super) declarations: bool,
    pub(super) native_codegen_policy: NativeCodegenPolicy,
}

/// Parses command-local arguments for `terlc build`.
///
/// Inputs:
/// - `args`: raw command-local arguments after global CLI parsing.
///
/// Output:
/// - `Ok(BuildArgs)` with a source path and a supported target.
/// - `Err(message)` for extra paths, unknown options, missing option values,
///   or unsupported backend targets.
///
/// Transformation:
/// - Accepts zero or one positional path and optional backend `--target`,
///   defaulting the source path to the current directory and the target to the
///   compiler-owned Terlan VM when they are not specified.
pub(super) fn parse_build_args(args: &[String]) -> Result<BuildArgs, String> {
    let mut path = None;
    let mut target = BuildTarget::TerlanVm;
    let mut target_explicit = false;
    let mut declarations = false;
    let mut native_codegen_policy = NativeCodegenPolicy::Development;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --target".to_string())?;
                target = parse_build_target(value)?;
                target_explicit = true;
                i += 2;
            }
            "--declarations" => {
                declarations = true;
                i += 1;
            }
            "--release" => {
                native_codegen_policy = NativeCodegenPolicy::Release;
                i += 1;
            }
            option if is_removed_debug_info_key_option(option) => {
                return Err(
                    "debug-info key options were removed; Terlan VM artifacts use checksummed compiler metadata and never read ambient encryption keys"
                        .to_string(),
                );
            }
            option if is_removed_compiler_transform_option(option) => {
                return Err(
                    "compiler transform options were removed; Terlan compiles checked source directly through CoreIR and VM IR"
                        .to_string(),
                );
            }
            option if option.starts_with("--") => {
                return Err(format!("unknown build option: {option}"));
            }
            candidate => {
                if path.is_some() {
                    return Err("terlc build accepts at most one source path".to_string());
                }
                path = Some(candidate.to_string());
                i += 1;
            }
        }
    }

    let path = path.unwrap_or_else(|| ".".to_string());
    Ok(BuildArgs {
        path,
        target,
        target_explicit,
        declarations,
        native_codegen_policy,
    })
}

/// Detects historical Erlang debug-info encryption key arguments.
///
/// Inputs:
/// - `option`: one raw command-local build argument.
///
/// Output:
/// - `true` for supported legacy spellings, including Erlang term syntax.
///
/// Transformation:
/// - Classifies the option without retaining or rendering its secret value.
fn is_removed_debug_info_key_option(option: &str) -> bool {
    matches!(option, "--debug-info-key" | "--debug_info_key")
        || option.starts_with("--debug-info-key=")
        || option.starts_with("--debug_info_key=")
        || option.starts_with("+{debug_info_key,")
}

/// Detects historical Erlang parse and Core transform arguments.
///
/// Inputs:
/// - `option`: one raw command-local build argument.
///
/// Output:
/// - `true` for option-shaped legacy transform spellings.
///
/// Transformation:
/// - Classifies the option without retaining or rendering the transform module
///   or its payload.
fn is_removed_compiler_transform_option(option: &str) -> bool {
    matches!(
        option,
        "--parse-transform" | "--parse_transform" | "--core-transform" | "--core_transform"
    ) || option.starts_with("--parse-transform=")
        || option.starts_with("--parse_transform=")
        || option.starts_with("--core-transform=")
        || option.starts_with("--core_transform=")
        || option.starts_with("+{parse_transform,")
        || option.starts_with("+{core_transform,")
}

/// Parses a backend target string.
///
/// Inputs:
/// - `value`: command-local target name.
///
/// Output:
/// - `Ok(BuildTarget)` for release-supported targets.
/// - `Err(message)` for unsupported targets.
///
/// Transformation:
/// - Converts the CLI spelling into the internal target enum.
fn parse_build_target(value: &str) -> Result<BuildTarget, String> {
    match value {
        "erlang" => Err(
            "build target `erlang` was removed from the public CLI; use `terlan-vm`".to_string(),
        ),
        "terlan-vm" => Ok(BuildTarget::TerlanVm),
        "wasm.core" => Ok(BuildTarget::WasmCore),
        js_target => crate::commands::emit_js::target_contract::parse_js_build_target_profile(
            js_target,
        )
        .map(BuildTarget::Js)
        .ok_or_else(|| {
            if let Some(family) = TargetFamily::reserved_target(js_target) {
                format!(
                    "build target `{js_target}` is reserved for the {} target family but is not implemented yet; supported targets: terlan-vm, wasm.core, js, js.shared, js.browser, js.worker",
                    family.as_str()
                )
            } else {
                format!("unsupported build target `{js_target}`; supported targets: terlan-vm, wasm.core, js, js.shared, js.browser, js.worker")
            }
        }),
    }
}
