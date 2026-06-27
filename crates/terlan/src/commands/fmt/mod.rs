use std::process::ExitCode;

use crate::terlan_syntax::{
    format_interface_source_module, format_source_module, parse_interface_module_as_syntax_output,
    parse_module_as_syntax_output,
};

/// Executes the `fmt` CLI command.
///
/// Inputs:
/// - `args`: command-local arguments after the `fmt` verb.
///
/// Output:
/// - `ExitCode::SUCCESS` when one file can be parsed and formatted.
/// - `ExitCode::from(2)` when the command arguments are malformed.
/// - `ExitCode::from(1)` when reading/parsing fails.
///
/// Transformation:
/// - Reads a single file path, parses it as a module/interface depending on
///   extension using formal syntax-output parsing, and prints the canonical
///   formatter output.
pub(crate) fn run(args: &[String]) -> ExitCode {
    if args.len() != 1 {
        eprintln!("missing or extra path argument");
        crate::print_usage();
        return ExitCode::from(2);
    }

    let path = &args[0];
    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    match parse_source(path, &source) {
        Ok(formatted) => {
            println!("{formatted}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("parse_error: {err}");
            ExitCode::from(1)
        }
    }
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
    if path.ends_with(".terli") {
        parse_interface_module_as_syntax_output(source).map_err(|error| match error {
            crate::terlan_syntax::EbnfCompileError::Parse(message, _) => message,
            crate::terlan_syntax::EbnfCompileError::Serialize(message) => message,
        })?;
        format_interface_source_module(source).map_err(|error| error.message)
    } else {
        parse_module_as_syntax_output(source).map_err(|error| match error {
            crate::terlan_syntax::EbnfCompileError::Parse(message, _) => message,
            crate::terlan_syntax::EbnfCompileError::Serialize(message) => message,
        })?;
        format_source_module(source).map_err(|error| error.message)
    }
}

#[cfg(test)]
#[path = "fmt_test.rs"]
mod fmt_test;
