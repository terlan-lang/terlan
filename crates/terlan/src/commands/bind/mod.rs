use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::CliCommand;

/// Parsed `terlc bind rust` command options.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BindRustArgs {
    crate_name: String,
    out_dir: PathBuf,
}

/// Parsed `terlc bind js-dom` command options.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BindJsDomArgs {
    manifest_path: PathBuf,
    out_dir: PathBuf,
}

mod polars_probe;
mod polars_probe_files;
mod ts_dom_module_mapping;
mod ts_input_manifest;
mod ts_parser_adapter;
mod ts_type_mapping;

use polars_probe::{GeneratedFile, POLARS_FILES};
use ts_dom_generator::generate_js_dom_bindings;

mod ts_dom_generator;

/// Executes the `bind` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing the binding target and command-local
///   options.
///
/// Output:
/// - `ExitCode::SUCCESS` when the selected generator writes package files.
/// - `ExitCode::from(1)` for unsupported crates or filesystem failures.
/// - `ExitCode::from(2)` for malformed arguments or unsupported targets.
///
/// Transformation:
/// - Validates the reserved public `terlc bind rust --crate <name> --out <dir>`
///   shape and delegates to the selected target generator without reading
///   remote crate metadata or fetching dependencies.
pub(crate) fn run(cmd: CliCommand) -> ExitCode {
    if cmd.args.is_empty() {
        eprintln!("terlc bind requires a target");
        print_usage();
        return ExitCode::from(2);
    }

    match cmd.args[0].as_str() {
        "js-dom" => run_js_dom(&cmd.args[1..]),
        "rust" => run_rust(&cmd.args[1..]),
        other => {
            eprintln!("unsupported bind target `{other}`; supported targets: js-dom, rust");
            print_usage();
            ExitCode::from(2)
        }
    }
}

/// Executes the TypeScript DOM binding generator surface.
///
/// Inputs:
/// - `args`: command-local arguments after `terlc bind js-dom`.
///
/// Output:
/// - `ExitCode::SUCCESS` when generated DOM binding files are written.
/// - `ExitCode::from(1)` for manifest, parser, mapping, or filesystem errors.
/// - `ExitCode::from(2)` when required arguments are missing or malformed.
///
/// Transformation:
/// - Parses the deterministic manifest/output command shape and delegates to
///   the Oxc-backed TypeScript DOM generator without using npm resolution or
///   network access.
fn run_js_dom(args: &[String]) -> ExitCode {
    let options = match parse_bind_js_dom_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    let repo_root = match std::env::current_dir() {
        Ok(repo_root) => repo_root,
        Err(err) => {
            eprintln!("failed to read current directory: {err}");
            return ExitCode::from(1);
        }
    };

    match generate_js_dom_bindings(&repo_root, &options.manifest_path, &options.out_dir) {
        Ok(()) => {
            println!(
                "generated JS DOM bindings from `{}` at {}",
                options.manifest_path.display(),
                options.out_dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Executes the reserved Rust binding generator surface.
///
/// Inputs:
/// - `args`: command-local arguments after `terlc bind rust`.
///
/// Output:
/// - `ExitCode::SUCCESS` when a supported crate skeleton is written.
/// - `ExitCode::from(1)` for unsupported crates or filesystem failures.
/// - `ExitCode::from(2)` when required arguments are missing or malformed.
///
/// Transformation:
/// - Parses Rust binding options and runs the current deterministic P0.3
///   generator probe. No Cargo metadata, network, or Rust source inspection
///   occurs here.
fn run_rust(args: &[String]) -> ExitCode {
    let options = match parse_bind_rust_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    match options.crate_name.as_str() {
        "polars" => match generate_package(&options.out_dir, POLARS_FILES) {
            Ok(()) => {
                println!(
                    "generated Rust binding skeleton for crate `polars` at {}",
                    options.out_dir.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        other => {
            eprintln!("unsupported rust binding crate `{other}`; supported crates: polars");
            ExitCode::from(1)
        }
    }
}

/// Writes a generated package skeleton.
///
/// Inputs:
/// - `out_dir`: destination package directory.
/// - `files`: relative paths and file contents to materialize.
///
/// Output:
/// - `Ok(())` when all files are written.
/// - `Err(String)` when the destination would overwrite existing content or a
///   filesystem operation fails.
///
/// Transformation:
/// - Refuses non-empty destinations, creates parent directories, and writes
///   deterministic template files without consulting package registries.
fn generate_package(out_dir: &Path, files: &[GeneratedFile]) -> Result<(), String> {
    if out_dir.exists() {
        let mut entries = fs::read_dir(out_dir).map_err(|err| {
            format!(
                "failed to read output directory `{}`: {err}",
                out_dir.display()
            )
        })?;
        if entries
            .next()
            .transpose()
            .map_err(|err| {
                format!(
                    "failed to inspect output directory `{}`: {err}",
                    out_dir.display()
                )
            })?
            .is_some()
        {
            return Err(format!(
                "refusing to generate into non-empty output directory `{}`",
                out_dir.display()
            ));
        }
    } else {
        fs::create_dir_all(out_dir).map_err(|err| {
            format!(
                "failed to create output directory `{}`: {err}",
                out_dir.display()
            )
        })?;
    }

    for file in files {
        let path = out_dir.join(file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!("failed to create directory `{}`: {err}", parent.display())
            })?;
        }
        fs::write(&path, file.contents)
            .map_err(|err| format!("failed to write generated file `{}`: {err}", path.display()))?;
    }

    Ok(())
}

/// Parses `terlc bind rust` command-local arguments.
///
/// Inputs:
/// - `args`: command-local arguments after the `rust` target.
///
/// Output:
/// - `Ok(BindRustArgs)` when `--crate <name>` and `--out <dir>` are present
///   exactly once.
/// - `Err(String)` with a user-facing diagnostic for malformed input.
///
/// Transformation:
/// - Walks the flat argument list, extracts required option values, rejects
///   duplicate or unknown options, and leaves paths as user-supplied relative
///   or absolute values.
fn parse_bind_rust_args(args: &[String]) -> Result<BindRustArgs, String> {
    let mut crate_name = None;
    let mut out_dir = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--crate" => {
                if crate_name.is_some() {
                    return Err("--crate can be supplied only once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--crate requires a value".to_string())?;
                if value.trim().is_empty() {
                    return Err("--crate requires a non-empty value".to_string());
                }
                crate_name = Some(value.clone());
                index += 2;
            }
            "--out" => {
                if out_dir.is_some() {
                    return Err("--out can be supplied only once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--out requires a value".to_string())?;
                if value.trim().is_empty() {
                    return Err("--out requires a non-empty value".to_string());
                }
                out_dir = Some(PathBuf::from(value));
                index += 2;
            }
            other => {
                return Err(format!("unexpected terlc bind rust argument `{other}`"));
            }
        }
    }

    Ok(BindRustArgs {
        crate_name: crate_name
            .ok_or_else(|| "terlc bind rust requires --crate <name>".to_string())?,
        out_dir: out_dir.ok_or_else(|| "terlc bind rust requires --out <dir>".to_string())?,
    })
}

/// Parses `terlc bind js-dom` command-local arguments.
///
/// Inputs:
/// - `args`: command-local arguments after the `js-dom` target.
///
/// Output:
/// - `Ok(BindJsDomArgs)` when `--manifest <path>` and `--out <dir>` are
///   present exactly once.
/// - `Err(String)` with a user-facing diagnostic for malformed input.
///
/// Transformation:
/// - Walks the flat argument list, extracts required paths, rejects duplicate
///   or unknown options, and leaves paths as user-supplied relative or absolute
///   values.
fn parse_bind_js_dom_args(args: &[String]) -> Result<BindJsDomArgs, String> {
    let mut manifest_path = None;
    let mut out_dir = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--manifest" => {
                if manifest_path.is_some() {
                    return Err("--manifest can be supplied only once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--manifest requires a value".to_string())?;
                if value.trim().is_empty() {
                    return Err("--manifest requires a non-empty value".to_string());
                }
                manifest_path = Some(PathBuf::from(value));
                index += 2;
            }
            "--out" => {
                if out_dir.is_some() {
                    return Err("--out can be supplied only once".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--out requires a value".to_string())?;
                if value.trim().is_empty() {
                    return Err("--out requires a non-empty value".to_string());
                }
                out_dir = Some(PathBuf::from(value));
                index += 2;
            }
            other => {
                return Err(format!("unexpected terlc bind js-dom argument `{other}`"));
            }
        }
    }

    Ok(BindJsDomArgs {
        manifest_path: manifest_path
            .ok_or_else(|| "terlc bind js-dom requires --manifest <path>".to_string())?,
        out_dir: out_dir.ok_or_else(|| "terlc bind js-dom requires --out <dir>".to_string())?,
    })
}

/// Prints bind command usage.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Writes concise command usage to standard error.
///
/// Transformation:
/// - Emits static help text without inspecting command state or filesystem
///   paths.
fn print_usage() {
    eprintln!("terlc bind rust --crate <crate-name> --out <dir>");
    eprintln!("terlc bind js-dom --manifest <path> --out <dir>");
}

#[cfg(test)]
#[path = "bind_test.rs"]
mod bind_test;

#[cfg(test)]
#[path = "ts_type_mapping_test.rs"]
mod ts_type_mapping_test;

#[cfg(test)]
#[path = "ts_input_manifest_test.rs"]
mod ts_input_manifest_test;

#[cfg(test)]
#[path = "ts_parser_adapter_test.rs"]
mod ts_parser_adapter_test;

#[cfg(test)]
#[path = "ts_dom_module_mapping_test.rs"]
mod ts_dom_module_mapping_test;
