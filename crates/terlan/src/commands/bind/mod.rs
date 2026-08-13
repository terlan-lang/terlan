use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::CliCommand;

#[cfg(test)]
fn native_helper_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// Parsed `terlc bind native` command options for Rust native packages.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BindNativeArgs {
    crate_name: String,
    out_dir: PathBuf,
}

/// Parsed `terlc bind js-dom` command options.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BindJsDomArgs {
    manifest_path: PathBuf,
    out_dir: PathBuf,
}

/// Parsed `terlc bind cpp` command options.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BindCppArgs {
    manifest_path: PathBuf,
    out_dir: PathBuf,
}

/// Parsed `terlc bind c` command options.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BindCArgs {
    manifest_path: PathBuf,
    out_dir: PathBuf,
}

mod c_abi_binding_generator;
mod cpp_binding_generator;
mod polars_probe;
mod polars_probe_files;
mod ts_angular_facade;
mod ts_dom_manifest;
mod ts_dom_module_mapping;
mod ts_generated_artifact;
mod ts_input_manifest;
mod ts_parser_adapter;
mod ts_type_mapping;

use polars_probe::{GeneratedFile, POLARS_FILES};
use ts_dom_generator::generate_js_dom_bindings;

mod ts_dom_generator;
use c_abi_binding_generator::generate_c_abi_bindings;
use cpp_binding_generator::generate_cpp_bindings;

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
/// - Routes Rust native packages, C ABI manifests, C++ manifests, and pinned
///   TypeScript inputs to separate generators without fetching dependencies.
pub(crate) fn run(cmd: CliCommand) -> ExitCode {
    if cmd.args.is_empty() {
        eprintln!("terlc bind requires a target");
        print_usage();
        return ExitCode::from(2);
    }

    match cmd.args[0].as_str() {
        "c" => run_c(&cmd.args[1..]),
        "cpp" => run_cpp(&cmd.args[1..]),
        "js-dom" => run_js_dom(&cmd.args[1..]),
        "native" => run_native(&cmd.args[1..]),
        other => {
            eprintln!(
                "unsupported bind target `{other}`; supported targets: c, cpp, js-dom, native"
            );
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

/// Executes the manifest-backed C++ binding generator surface.
///
/// Inputs:
/// - `args`: command-local arguments after `terlc bind cpp`.
///
/// Output:
/// - `ExitCode::SUCCESS` when generated native binding files are written.
/// - `ExitCode::from(1)` for manifest validation or filesystem errors.
/// - `ExitCode::from(2)` when required arguments are missing or malformed.
///
/// Transformation:
/// - Parses normalized metadata from maintained C++ tooling and delegates to
///   the deterministic generator. The generator copies declared C++ inputs
///   into a real `cxx` crate without parsing headers itself.
fn run_cpp(args: &[String]) -> ExitCode {
    let options = match parse_bind_cpp_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    match generate_cpp_bindings(&options.manifest_path, &options.out_dir) {
        Ok(summary) => {
            println!(
                "generated {} native binding modules with {} functions at {}",
                summary.module_count,
                summary.function_count,
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

/// Executes the manifest-backed C ABI binding generator surface.
///
/// Inputs:
/// - `args`: command-local arguments after `terlc bind c`.
///
/// Output:
/// - `ExitCode::SUCCESS` when generated C ABI binding files are written.
/// - `ExitCode::from(1)` for metadata, validation, or filesystem errors.
/// - `ExitCode::from(2)` when required arguments are missing or malformed.
///
/// Transformation:
/// - Consumes normalized C declaration metadata and generates raw Rust FFI,
///   a safe ownership adapter, C compilation inputs, NativeBoundary metadata,
///   and an executable Terlan package consumer.
fn run_c(args: &[String]) -> ExitCode {
    let options = match parse_bind_c_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    match generate_c_abi_bindings(&options.manifest_path, &options.out_dir) {
        Ok(summary) => {
            println!(
                "generated {} C ABI binding modules with {} functions at {}",
                summary.module_count,
                summary.function_count,
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

/// Executes the reserved Rust native-package generator surface.
///
/// Inputs:
/// - `args`: command-local arguments after `terlc bind native`.
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
fn run_native(args: &[String]) -> ExitCode {
    let options = match parse_bind_native_args(args) {
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
            eprintln!("unsupported native binding crate `{other}`; supported crates: polars");
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

/// Parses `terlc bind native` command-local arguments.
///
/// Inputs:
/// - `args`: command-local arguments after the `native` target.
///
/// Output:
/// - `Ok(BindNativeArgs)` when `--crate <name>` and `--out <dir>` are present
///   exactly once.
/// - `Err(String)` with a user-facing diagnostic for malformed input.
///
/// Transformation:
/// - Walks the flat argument list, extracts required option values, rejects
///   duplicate or unknown options, and leaves paths as user-supplied relative
///   or absolute values.
fn parse_bind_native_args(args: &[String]) -> Result<BindNativeArgs, String> {
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
                return Err(format!("unexpected terlc bind native argument `{other}`"));
            }
        }
    }

    Ok(BindNativeArgs {
        crate_name: crate_name
            .ok_or_else(|| "terlc bind native requires --crate <name>".to_string())?,
        out_dir: out_dir.ok_or_else(|| "terlc bind native requires --out <dir>".to_string())?,
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

/// Parses `terlc bind cpp` command-local arguments.
///
/// Inputs:
/// - `args`: command-local arguments after the `cpp` target.
///
/// Output:
/// - `Ok(BindCppArgs)` when `--manifest <path>` and `--out <dir>` are
///   present exactly once.
/// - `Err(String)` with a user-facing diagnostic for malformed input.
///
/// Transformation:
/// - Walks the flat argument list, extracts required paths, rejects duplicate
///   or unknown options, and leaves paths as user-supplied relative or
///   absolute values.
fn parse_bind_cpp_args(args: &[String]) -> Result<BindCppArgs, String> {
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
                return Err(format!("unexpected terlc bind cpp argument `{other}`"));
            }
        }
    }

    Ok(BindCppArgs {
        manifest_path: manifest_path
            .ok_or_else(|| "terlc bind cpp requires --manifest <path>".to_string())?,
        out_dir: out_dir.ok_or_else(|| "terlc bind cpp requires --out <dir>".to_string())?,
    })
}

/// Parses `terlc bind c` command-local arguments.
fn parse_bind_c_args(args: &[String]) -> Result<BindCArgs, String> {
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
            other => return Err(format!("unexpected terlc bind c argument `{other}`")),
        }
    }

    Ok(BindCArgs {
        manifest_path: manifest_path
            .ok_or_else(|| "terlc bind c requires --manifest <path>".to_string())?,
        out_dir: out_dir.ok_or_else(|| "terlc bind c requires --out <dir>".to_string())?,
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
    eprintln!("terlc bind native --crate <crate-name> --out <dir>");
    eprintln!("terlc bind js-dom --manifest <path> --out <dir>");
    eprintln!("terlc bind cpp --manifest <path> --out <dir>");
    eprintln!("terlc bind c --manifest <path> --out <dir>");
}

#[cfg(test)]
#[path = "bind_test.rs"]
#[cfg(test)]
mod bind_test;

#[cfg(test)]
#[path = "cpp_package_consumer_test.rs"]
#[cfg(test)]
mod cpp_package_consumer_test;

#[cfg(test)]
#[path = "ts_type_mapping_test.rs"]
#[cfg(test)]
mod ts_type_mapping_test;

#[cfg(test)]
#[path = "ts_input_manifest_test.rs"]
#[cfg(test)]
mod ts_input_manifest_test;

#[cfg(test)]
#[path = "ts_parser_adapter_test.rs"]
#[cfg(test)]
mod ts_parser_adapter_test;

#[cfg(test)]
#[path = "ts_dom_module_mapping_test.rs"]
#[cfg(test)]
mod ts_dom_module_mapping_test;
