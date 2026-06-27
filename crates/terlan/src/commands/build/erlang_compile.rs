use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// Compiles one Erlang source file into the build `ebin` directory.
///
/// Inputs:
/// - `source_dir`: build source directory used as the Erlang include path.
/// - `ebin_dir`: destination directory for generated `.beam` files.
/// - `erl_path`: Erlang source file to compile.
/// - `incremental`: whether an already-current `.beam` may be reused.
///
/// Output:
/// - `Ok(())` when `erlc` exits successfully.
/// - `Err(message)` for process spawn failures or non-zero compiler exits.
///
/// Transformation:
/// - In incremental mode, skips `erlc` when the destination `.beam` is newer
///   than the `.erl` and generated headers. Otherwise runs
///   `erlc -I <source_dir> -o <ebin_dir> <erl_path>` with crash dumps
///   redirected outside the build source tree.
pub(super) fn compile_erlang_source(
    source_dir: &Path,
    ebin_dir: &Path,
    erl_path: &Path,
    incremental: bool,
) -> Result<(), String> {
    if incremental && erlang_source_compile_is_current(source_dir, ebin_dir, erl_path)? {
        return Ok(());
    }

    let crash_dump = ebin_dir.join("erl_crash.dump");
    let mut command = Command::new("erlc");
    command
        .arg("-I")
        .arg(source_dir)
        .arg("-o")
        .arg(ebin_dir)
        .arg(erl_path);
    let output = run_command_with_no_erl_crash_dump(&mut command, "erlc", Some(&crash_dump))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.trim().is_empty() {
        Err(format!(
            "erlc failed for {} with status {}",
            erl_path.display(),
            output.status
        ))
    } else {
        Err(format!(
            "erlc failed for {}: {}",
            erl_path.display(),
            stderr
        ))
    }
}

/// Returns whether an Erlang source already has a current BEAM artifact.
///
/// Inputs:
/// - `source_dir`: generated Erlang source directory containing `.erl` and
///   optional `.hrl` files.
/// - `ebin_dir`: generated BEAM output directory.
/// - `erl_path`: generated Erlang source being considered for compilation.
///
/// Output:
/// - `Ok(true)` when the expected `.beam` exists and is newer than the source
///   and every generated header.
/// - `Ok(false)` when the source should be compiled.
/// - `Err(message)` when filesystem metadata cannot be read.
///
/// Transformation:
/// - Maps `foo.erl` to `foo.beam`, compares filesystem modification times,
///   and treats any newer generated header as invalidating the BEAM. Header
///   invalidation is intentionally conservative because the current bridge may
///   include generated records from any source module.
pub(super) fn erlang_source_compile_is_current(
    source_dir: &Path,
    ebin_dir: &Path,
    erl_path: &Path,
) -> Result<bool, String> {
    let Some(stem) = erl_path.file_stem().and_then(|stem| stem.to_str()) else {
        return Ok(false);
    };
    let beam_path = ebin_dir.join(format!("{stem}.beam"));
    if !beam_path.exists() {
        return Ok(false);
    }

    let beam_modified = file_modified_at(&beam_path)?;
    if file_modified_at(erl_path)? > beam_modified {
        return Ok(false);
    }

    let entries = fs::read_dir(source_dir)
        .map_err(|err| format!("failed to read build source directory: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read build source entry: {err}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("hrl")
            && file_modified_at(&path)? > beam_modified
        {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Reads the modification time for a generated artifact.
///
/// Inputs:
/// - `path`: generated source, header, or BEAM artifact path.
///
/// Output:
/// - Filesystem modification timestamp.
/// - `Err(message)` when metadata or modification time cannot be read.
///
/// Transformation:
/// - Wraps `std::fs::metadata(...).modified()` with build-oriented error
///   context so incremental compiler-cache decisions fail visibly.
fn file_modified_at(path: &Path) -> Result<std::time::SystemTime, String> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|err| {
            format!(
                "failed to read modification time for {}: {err}",
                path.display()
            )
        })
}

/// Runs a process while preventing local Erlang crash dumps in source output.
///
/// Inputs:
/// - `command`: process builder to execute.
/// - `label`: human-readable tool name used in spawn failures.
/// - `erl_crash_dump`: optional path assigned to `ERL_CRASH_DUMP`.
///
/// Output:
/// - `Ok(Output)` when the process starts and exits.
/// - `Err(message)` when the process cannot be spawned.
///
/// Transformation:
/// - Adds the Erlang crash-dump environment override and delegates to
///   `Command::output`.
fn run_command_with_no_erl_crash_dump(
    command: &mut Command,
    label: &str,
    erl_crash_dump: Option<&Path>,
) -> Result<Output, String> {
    if let Some(path) = erl_crash_dump {
        command.env("ERL_CRASH_DUMP", path);
    }
    command
        .output()
        .map_err(|err| format!("failed to run {label}: {err}"))
}
