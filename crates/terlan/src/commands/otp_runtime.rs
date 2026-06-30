use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::{CliCommand, CliState};

/// Runs the hidden experimental OTP compatibility runtime command group.
///
/// Inputs:
/// - Parsed `otp-runtime` command arguments.
/// - Global CLI state, including the hidden `--experimental` flag.
///
/// Output:
/// - Process exit code from usage validation, runtime discovery, or delegated
///   runtime process execution.
///
/// Transformation:
/// - Locates the local experimental OTP compatibility runtime and delegates to
///   its `erl` or `erlc` binaries without adding those binaries to the public
///   top-level command surface.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    if !state.experimental {
        eprintln!("terlc otp-runtime is experimental; rerun with --experimental to enable it.");
        return ExitCode::from(2);
    }

    match parse_otp_runtime_args(&cmd.args) {
        OtpRuntimeArgs::Help => {
            print_otp_runtime_usage();
            ExitCode::SUCCESS
        }
        OtpRuntimeArgs::Version => run_otp_runtime_version(),
        OtpRuntimeArgs::Exec { binary, args } => run_otp_runtime_binary(binary, args),
        OtpRuntimeArgs::Error(message) => {
            eprintln!("{message}");
            print_otp_runtime_usage();
            ExitCode::from(2)
        }
    }
}

/// Parsed hidden OTP runtime command arguments.
enum OtpRuntimeArgs {
    Help,
    Version,
    Exec {
        binary: &'static str,
        args: Vec<String>,
    },
    Error(String),
}

/// Parses hidden OTP runtime subcommand arguments.
fn parse_otp_runtime_args(args: &[String]) -> OtpRuntimeArgs {
    match args {
        [] => OtpRuntimeArgs::Error(
            "terlc otp-runtime requires a subcommand: version, erl, or erlc".to_string(),
        ),
        [flag] if matches!(flag.as_str(), "--help" | "-h") => OtpRuntimeArgs::Help,
        [subcommand] if subcommand == "version" => OtpRuntimeArgs::Version,
        [subcommand, rest @ ..] if subcommand == "erl" => OtpRuntimeArgs::Exec {
            binary: "erl",
            args: strip_separator(rest),
        },
        [subcommand, rest @ ..] if subcommand == "erlc" => OtpRuntimeArgs::Exec {
            binary: "erlc",
            args: strip_separator(rest),
        },
        [subcommand, ..] => OtpRuntimeArgs::Error(format!(
            "unknown terlc otp-runtime subcommand: {subcommand}"
        )),
    }
}

/// Removes an optional `--` separator before forwarded runtime arguments.
fn strip_separator(args: &[String]) -> Vec<String> {
    match args {
        [separator, rest @ ..] if separator == "--" => rest.to_vec(),
        _ => args.to_vec(),
    }
}

/// Prints the experimental runtime's Erlang system version.
fn run_otp_runtime_version() -> ExitCode {
    run_otp_runtime_binary(
        "erl",
        vec![
            "-noshell".to_string(),
            "-eval".to_string(),
            "io:format(\"~s~n\", [erlang:system_info(system_version)]), halt().".to_string(),
        ],
    )
}

/// Runs one binary from the experimental OTP runtime payload.
fn run_otp_runtime_binary(binary: &str, args: Vec<String>) -> ExitCode {
    let runtime = match runtime_dir() {
        Ok(runtime) => runtime,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let executable = runtime.join("bin").join(binary);
    if !executable.is_file() {
        eprintln!(
            "terlc otp-runtime could not find `{}` in experimental OTP runtime payload {}",
            binary,
            runtime.display()
        );
        return ExitCode::from(2);
    }

    let status = Command::new(&executable).args(args).status();
    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(err) => {
            eprintln!(
                "terlc otp-runtime failed to run {}: {err}",
                executable.display()
            );
            ExitCode::from(1)
        }
    }
}

/// Returns the configured experimental OTP runtime directory.
fn runtime_dir() -> Result<PathBuf, String> {
    if let Some(runtime) = std::env::var_os("TERLAN_OTP_RUNTIME_DIR").map(PathBuf::from) {
        return Ok(runtime);
    }
    let exe = std::env::current_exe()
        .map_err(|err| format!("terlc otp-runtime could not locate current executable: {err}"))?;
    bundled_runtime_dir_from_exe(&exe).ok_or_else(|| {
        "terlc otp-runtime requires TERLAN_OTP_RUNTIME_DIR, a bundled experimental/terlan-vm payload next to terlc, or an installed ../lib/terlan/experimental/terlan-vm payload".to_string()
    })
}

/// Returns the bundled runtime path next to a compiler executable.
fn bundled_runtime_dir_from_exe(exe: &Path) -> Option<PathBuf> {
    let bin_dir = exe.parent()?;
    let candidates = [
        bin_dir.join("experimental").join("terlan-vm"),
        bin_dir
            .parent()?
            .join("lib")
            .join("terlan")
            .join("experimental")
            .join("terlan-vm"),
    ];
    for candidate in candidates {
        if runtime_payload_exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Returns whether a path has the minimum OTP runtime executable surface.
fn runtime_payload_exists(candidate: &Path) -> bool {
    candidate.join("bin").join("erl").is_file() && candidate.join("bin").join("erlc").is_file()
}

/// Prints hidden OTP runtime command usage.
fn print_otp_runtime_usage() {
    println!("terlc --experimental otp-runtime version");
    println!("terlc --experimental otp-runtime erl -- <erl-args>");
    println!("terlc --experimental otp-runtime erlc -- <erlc-args>");
}

#[cfg(test)]
#[path = "otp_runtime_test.rs"]
mod otp_runtime_test;
