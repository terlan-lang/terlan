use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::terlan_syntax::{
    format_interface_source_module, format_source_module,
    format_source_module_migrating_repeated_lets, migrate_repeated_let_source,
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
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
///   parses each as a module/interface depending on extension using formal
///   syntax-output parsing, then prints single-file output or rewrites
///   directory-mode files in place.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let (migrate_repeated_lets, path) = match args {
        [path] => (false, path),
        [flag, path] if flag == "--migrate-repeated-lets" => (true, path),
        _ => {
            eprintln!("missing or extra path argument");
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    if migrate_repeated_lets {
        return migrate_repeated_lets_path(Path::new(path));
    }

    if path.is_empty() {
        eprintln!("missing or extra path argument");
        crate::print_usage();
        return ExitCode::from(2);
    }

    let path_ref = Path::new(path);
    if path_ref.is_dir() {
        return format_directory(path_ref);
    }

    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    match parse_source(path, &source) {
        Ok(formatted) => {
            print!("{formatted}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("parse_error: {err}");
            ExitCode::from(1)
        }
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
fn format_directory(root: &Path) -> ExitCode {
    let paths = match collect_format_paths(root) {
        Ok(paths) => paths,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };

    for path in paths {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("failed to read {}: {err}", path.display());
                return ExitCode::from(1);
            }
        };
        let path_text = path.to_string_lossy();
        let formatted = match parse_source(&path_text, &source) {
            Ok(formatted) => formatted,
            Err(err) => {
                eprintln!("parse_error: {}: {err}", path.display());
                return ExitCode::from(1);
            }
        };
        if let Err(err) = fs::write(&path, formatted) {
            eprintln!("failed to write {}: {err}", path.display());
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
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
            Some("terl" | "terli")
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
/// - Parses `.terli` sources with `parse_interface_module_as_syntax_output`, and all
///   others with `parse_module_as_syntax_output`.
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
        parse_interface_module_as_syntax_output(source).map_err(|error| match error {
            crate::terlan_syntax::EbnfCompileError::Parse(message, _) => message,
            crate::terlan_syntax::EbnfCompileError::Serialize(message) => message,
        })?;
        format_interface_source_module(source).map_err(|error| error.message)
    } else {
        match parse_module_as_syntax_output(source) {
            Ok(_) => format_source_module(source).map_err(|error| error.message),
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
mod fmt_test;
