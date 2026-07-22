#![forbid(unsafe_code)]

use std::process::ExitCode;

#[path = "../database_schema.rs"]
pub(crate) mod database_schema;
#[path = "../mobile/mobile_bridge.rs"]
pub(crate) mod mobile_bridge;
#[path = "../mobile/mobile_debug_identity.rs"]
pub(crate) mod mobile_debug_identity;
#[path = "../compiler/hir/mod.rs"]
pub mod terlan_hir;
#[path = "../html/mod.rs"]
pub mod terlan_html;
#[path = "mod.rs"]
pub mod terlan_lsp;
#[path = "../compiler/purity.rs"]
pub(crate) mod terlan_purity;
#[path = "../compiler/syntax/mod.rs"]
pub mod terlan_syntax;
#[path = "../compiler/typeck/mod.rs"]
pub mod terlan_typeck;
#[path = "../compiler/value_lifecycle.rs"]
pub(crate) mod value_lifecycle;

/// Minimal mobile namespace required by standalone LSP typechecking.
pub mod mobile {
    pub(crate) use crate::mobile_bridge;
    #[cfg(test)]
    pub(crate) use crate::mobile_debug_identity;
}

/// Prints `terlan-lsp` command usage.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Usage text written to standard output.
///
/// Transformation:
/// - Keeps the standalone LSP binary help text separate from the main `terlc`
///   CLI help surface.
fn print_usage() {
    println!("terlan-lsp --stdio");
    println!("Starts the Terlan language server on standard I/O.");
}

/// Runs the Terlan LSP binary.
///
/// Inputs:
/// - Process arguments from `std::env::args`.
///
/// Output:
/// - `ExitCode::SUCCESS` for help or a started stdio server.
/// - `ExitCode::from(2)` for unexpected arguments.
/// - Server exit code from `crate::terlan_lsp::run_stdio_server`.
///
/// Transformation:
/// - Validates the tiny LSP-specific CLI surface and delegates stdio server
///   execution to the `terlan_lsp` crate.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        print_usage();
        return ExitCode::SUCCESS;
    }

    if args.len() > 1 && args[1] != "--stdio" {
        eprintln!("unexpected argument: {}", args[1]);
        print_usage();
        return ExitCode::from(2);
    }

    crate::terlan_lsp::run_stdio_server()
}
