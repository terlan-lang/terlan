use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::terlan_syntax::{
    format_script_source, format_source_module_migrating_repeated_lets,
    format_validated_interface_module, format_validated_source_module, migrate_repeated_let_source,
    REPEATED_LET_BINDING_DIAGNOSTIC,
};

/// Executes the `fmt` CLI command.
///
/// Inputs:
/// - `args`: command-local arguments after the `fmt` verb.
///
/// Output:
/// - `ExitCode::SUCCESS` when the input file or directory can be formatted.
/// - `ExitCode::from(2)` when the command arguments are malformed.
/// - `ExitCode::from(1)` when reading/parsing fails.
///
/// Transformation:
/// - Reads a file path or walks a directory for `.terl`/`.terli` sources,
///   parses each once as a module/interface depending on extension, validates
///   the tree through formal syntax output, then prints single-file output,
///   rewrites an explicitly requested single file, or rewrites directory-mode
///   files in place.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let (mode, path) = match args {
        [path] => (FormatMode::WriteOrPrint, path),
        [flag, path] if flag == "--write" => (FormatMode::Write, path),
        [flag, path] if flag == "--check" => (FormatMode::Check, path),
        [flag, path] if flag == "--migrate-repeated-lets" => {
            (FormatMode::MigrateRepeatedLets, path)
        }
        _ => {
            eprintln!("missing or extra path argument");
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    if mode == FormatMode::MigrateRepeatedLets {
        return migrate_repeated_lets_path(Path::new(path));
    }

    if path.is_empty() {
        eprintln!("missing or extra path argument");
        crate::print_usage();
        return ExitCode::from(2);
    }

    let path_ref = Path::new(path);
    if path_ref.is_dir() {
        return format_directory(path_ref, mode);
    }

    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    match parse_source(path, &source) {
        Ok(formatted) => match mode {
            FormatMode::Check => check_formatted_source(path_ref, &source, &formatted),
            FormatMode::Write => match fs::write(path_ref, formatted) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("failed to write {}: {error}", path_ref.display());
                    ExitCode::from(1)
                }
            },
            FormatMode::WriteOrPrint => {
                print!("{formatted}");
                ExitCode::SUCCESS
            }
            FormatMode::MigrateRepeatedLets => unreachable!("handled before formatting"),
        },
        Err(err) => {
            eprintln!("parse_error: {err}");
            ExitCode::from(1)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FormatMode {
    WriteOrPrint,
    Write,
    Check,
    MigrateRepeatedLets,
}

fn check_formatted_source(path: &Path, source: &str, formatted: &str) -> ExitCode {
    if formatted == source {
        ExitCode::SUCCESS
    } else {
        eprintln!("would reformat: {}", path.display());
        ExitCode::from(1)
    }
}

fn migrate_repeated_lets_path(path: &Path) -> ExitCode {
    if path.is_dir() {
        let paths = match collect_format_paths(path) {
            Ok(paths) => paths,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
        let mut skipped = 0usize;
        for source_path in paths {
            if source_path
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("terl")
            {
                continue;
            }
            let source = match fs::read_to_string(&source_path) {
                Ok(source) => source,
                Err(error) => {
                    eprintln!("failed to read {}: {error}", source_path.display());
                    return ExitCode::from(1);
                }
            };
            let migrated = match migrate_repeated_let_source(&source) {
                Ok(migrated) => migrated,
                Err(error) => {
                    skipped += 1;
                    eprintln!(
                        "migration_skip: {}: {}",
                        source_path.display(),
                        error.message
                    );
                    continue;
                }
            };
            if migrated != source {
                if let Err(error) = fs::write(&source_path, migrated) {
                    eprintln!("failed to write {}: {error}", source_path.display());
                    return ExitCode::from(1);
                }
            }
        }
        if skipped > 0 {
            eprintln!("migration skipped {skipped} non-parseable source file(s)");
        }
        return ExitCode::SUCCESS;
    }

    let source = match crate::support::read_file(&path.to_string_lossy()) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    match migrate_repeated_let_source(&source) {
        Ok(migrated) => {
            print!("{migrated}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("parse_error: {}", error.message);
            ExitCode::from(1)
        }
    }
}

/// Formats every Terlan source/interface file under a directory in place.
fn format_directory(root: &Path, mode: FormatMode) -> ExitCode {
    let paths = match collect_format_paths(root) {
        Ok(paths) => paths,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };

    let mut noncanonical = Vec::new();
    for path in paths {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("failed to read {}: {err}", path.display());
                return ExitCode::from(1);
            }
        };
        if is_generated_do_not_edit(&source) {
            continue;
        }
        let path_text = path.to_string_lossy();
        let formatted = match parse_source(&path_text, &source) {
            Ok(formatted) => formatted,
            Err(err) => {
                eprintln!("parse_error: {}: {err}", path.display());
                return ExitCode::from(1);
            }
        };
        if formatted == source {
            continue;
        }
        if mode == FormatMode::Check {
            noncanonical.push(path);
        } else if let Err(err) = fs::write(&path, formatted) {
            eprintln!("failed to write {}: {err}", path.display());
            return ExitCode::from(1);
        }
    }

    if noncanonical.is_empty() {
        ExitCode::SUCCESS
    } else {
        for path in noncanonical {
            eprintln!("would reformat: {}", path.display());
        }
        ExitCode::from(1)
    }
}

/// Returns whether a generated artifact explicitly forbids direct edits.
fn is_generated_do_not_edit(source: &str) -> bool {
    let header = source.lines().take(24).collect::<Vec<_>>().join("\n");
    header.contains("@generated true") && header.contains("@do-not-edit true")
}

/// Collects `.terl` and `.terli` files under a directory in deterministic order.
fn collect_format_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    collect_format_paths_into(root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

/// Recursively collects formatter inputs.
fn collect_format_paths_into(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read directory {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| {
            format!(
                "failed to read directory entry under {}: {err}",
                dir.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_format_paths_into(&path, paths)?;
        } else if is_format_source_path(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

/// Returns whether a path is a formatter-owned Terlan file.
fn is_format_source_path(path: &Path) -> bool {
    crate::terlan_html::is_terlan_artifact_template_path(path)
        || matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("terl" | "terli" | "terls")
        )
}

/// Parses either a source module or interface file by extension.
///
/// Inputs:
/// - `path`: command input path used to choose parser behavior.
/// - `source`: raw module text.
///
/// Output:
/// - Canonically formatted module text on success.
/// - `String` parse error message on malformed syntax.
///
/// Transformation:
/// - Parses each source once, validates the resulting tree through syntax
///   output, and formats that same tree; `.terli` selects interface mode.
fn parse_source(path: &str, source: &str) -> Result<String, String> {
    if crate::terlan_html::is_terlan_artifact_template_path(path) {
        crate::terlan_html::validate_artifact_template_structure(source, path).map_err(
            |diagnostics| {
                diagnostics
                    .into_iter()
                    .map(|diagnostic| diagnostic.message)
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        )?;
        crate::terlan_html::format_template_interpolations(source)
            .map_err(|error| format!("line {}: {}", error.line, error.message))
    } else if path.ends_with(".terli") {
        format_validated_interface_module(source).map_err(|error| match error {
            crate::terlan_syntax::EbnfCompileError::Parse(message, _) => message,
            crate::terlan_syntax::EbnfCompileError::Serialize(message) => message,
        })
    } else if path.ends_with(".terls") {
        let formatted = format_script_source(source).map_err(|error| error.message)?;
        crate::terlan_syntax::parse_script_as_syntax_output(
            &formatted,
            &crate::formal_pipeline::script_module_name(Path::new(path)),
        )
        .map_err(|error| match error {
            crate::terlan_syntax::EbnfCompileError::Parse(message, _) => message,
            crate::terlan_syntax::EbnfCompileError::Serialize(message) => message,
        })?;
        Ok(formatted)
    } else {
        match format_validated_source_module(source) {
            Ok(formatted) => Ok(formatted),
            Err(crate::terlan_syntax::EbnfCompileError::Parse(message, _))
                if message == REPEATED_LET_BINDING_DIAGNOSTIC =>
            {
                format_source_module_migrating_repeated_lets(source).map_err(|error| error.message)
            }
            Err(crate::terlan_syntax::EbnfCompileError::Parse(message, _)) => Err(message),
            Err(crate::terlan_syntax::EbnfCompileError::Serialize(message)) => Err(message),
        }
    }
}

#[cfg(test)]
#[path = "fmt_test.rs"]
#[cfg(test)]
mod fmt_test;
