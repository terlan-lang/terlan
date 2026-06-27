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
    Erlang,
    Js(TargetProfile),
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
    pub(super) declarations: bool,
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
///   defaulting the source path to the current directory and the target to
///   Erlang when they are not specified.
pub(super) fn parse_build_args(args: &[String]) -> Result<BuildArgs, String> {
    let mut path = None;
    let mut target = BuildTarget::Erlang;
    let mut declarations = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --target".to_string())?;
                target = parse_build_target(value)?;
                i += 2;
            }
            "--declarations" => {
                declarations = true;
                i += 1;
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
        declarations,
    })
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
        "erlang" => Ok(BuildTarget::Erlang),
        js_target => crate::commands::emit_js::target_contract::parse_js_build_target_profile(
            js_target,
        )
        .map(BuildTarget::Js)
        .ok_or_else(|| {
            if let Some(family) = TargetFamily::reserved_target(js_target) {
                format!(
                    "build target `{js_target}` is reserved for the {} target family but is not implemented yet; supported targets: erlang, js, js.shared, js.browser, js.worker",
                    family.as_str()
                )
            } else {
                format!("unsupported build target `{js_target}`; supported targets: erlang, js, js.shared, js.browser, js.worker")
            }
        }),
    }
}
