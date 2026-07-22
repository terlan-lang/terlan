use std::process::ExitCode;

#[cfg(feature = "editor-lsp")]
use crate::terlan_lsp::run_stdio_server;

/// Executes the `lsp` CLI command.
///
/// Inputs:
/// - `args`: command-local arguments after the `lsp` verb.
///
/// Output:
/// - `ExitCode::SUCCESS` when help is printed or the language server exits
///   successfully.
/// - Non-zero `ExitCode` when arguments are invalid or the language server
///   returns an error status.
///
/// Transformation:
/// - Validates `--help` and `--stdio` command-local arguments, prints help or
///   usage errors when needed, and delegates stdio transport to `terlan_lsp`.
pub(crate) fn run(args: &[String]) -> ExitCode {
    if args.len() == 1 && args[0] == "--help" {
        println!("terlc lsp --stdio");
        println!("Starts the Terlan language server over stdio transport.");
        println!(
            "Current implementation is a lightweight placeholder; protocol handlers are coming next."
        );
        return ExitCode::SUCCESS;
    }

    if args.len() > 1 {
        eprintln!("lsp accepts at most --help");
        crate::print_usage();
        return ExitCode::from(2);
    }

    if args.first().is_some_and(|arg| arg == "--stdio") || args.is_empty() {
        run_editor_lsp_stdio_server()
    } else {
        eprintln!("lsp received unexpected argument: {}", args[0]);
        crate::print_usage();
        ExitCode::from(2)
    }
}

/// Starts the editor LSP stdio server when editor tooling is enabled.
#[cfg(feature = "editor-lsp")]
fn run_editor_lsp_stdio_server() -> ExitCode {
    run_stdio_server()
}

/// Reports unavailable LSP tooling in default runtime builds.
#[cfg(not(feature = "editor-lsp"))]
fn run_editor_lsp_stdio_server() -> ExitCode {
    eprintln!("terlc lsp requires the `editor-lsp` build feature");
    ExitCode::from(2)
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
