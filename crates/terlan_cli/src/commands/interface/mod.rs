use std::fs;
use std::process::ExitCode;

use terlan_hir::syntax_module_output_to_interface;
use terlan_syntax::parse_interface_module_as_syntax_output;

use crate::{support::write_if_changed_or_forced, CliState};

/// Executes the `interface` CLI command.
///
/// Inputs:
/// - `args`: command-local arguments after the `interface` verb.
/// - `state`: parsed global CLI state, including output directory,
///   incremental-write mode, and diagnostic format.
///
/// Output:
/// - `ExitCode::SUCCESS` when interface output is written successfully.
/// - `ExitCode::from(2)` when command-local arguments are malformed.
/// - `ExitCode::from(1)` on read, parse, serialization, directory, or write
///   failures.
///
/// Transformation:
/// - Reads one `.terli` source, parses it through the formal syntax-output
///   interface path, converts that output to Terlan interface text, and writes
///   `<module>.typi` into the configured output directory.
pub(crate) fn run(args: &[String], state: &CliState) -> ExitCode {
    if args.len() != 1 {
        eprintln!("missing or extra path argument");
        crate::print_usage();
        return ExitCode::from(2);
    }

    let path = &args[0];
    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let syntax_output = match parse_interface_module_as_syntax_output(&source) {
        Ok(output) => output,
        Err(terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
            crate::support::emit_diagnostic(
                "parse_error",
                &message,
                path,
                span.start,
                span.end,
                state.diagnostic_format,
            );
            return ExitCode::from(1);
        }
        Err(terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    if let Err(err) = fs::create_dir_all(&state.out_dir) {
        eprintln!("cannot create output directory: {}", err);
        return ExitCode::from(1);
    }

    let interface = syntax_module_output_to_interface(&syntax_output).to_terlan_interface_text();
    let target = state
        .out_dir
        .join(format!("{}.typi", syntax_output.module_name));
    if let Err(err) = write_if_changed_or_forced(&target, interface.as_bytes(), state.incremental) {
        eprintln!("failed to write interface output: {}", err);
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
