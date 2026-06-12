use std::process::ExitCode;

use terlan_syntax::{
    format_module, parse_interface_module, parse_interface_module_as_syntax_output, parse_module,
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
///   formatter output built from the AST module AST.
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
/// - Parses `.tli` sources with `parse_interface_module_as_syntax_output`, and all
///   others with `parse_module_as_syntax_output`.
/// - Uses the AST module parser only to obtain formatter input for now.
fn parse_source(path: &str, source: &str) -> Result<String, String> {
    if path.ends_with(".tli") {
        parse_interface_module_as_syntax_output(source).map_err(|error| match error {
            terlan_syntax::EbnfCompileError::Parse(message, _) => message,
            terlan_syntax::EbnfCompileError::Serialize(message) => message,
        })?;
        parse_interface_module(source)
            .map_err(|error| error.message)
            .map(|module| format_module(&module))
    } else {
        parse_module_as_syntax_output(source).map_err(|error| match error {
            terlan_syntax::EbnfCompileError::Parse(message, _) => message,
            terlan_syntax::EbnfCompileError::Serialize(message) => message,
        })?;
        parse_module(source)
            .map_err(|error| error.message)
            .map(|module| format_module(&module))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_source;

    /// Verifies that `terlc fmt` keeps canonical source modules on `pub`
    /// visibility instead of normalizing removed export-list syntax.
    ///
    /// Inputs:
    /// - A `.tl` path and source containing a source-mode `export` declaration.
    ///
    /// Output:
    /// - A parse error containing the canonical source-export diagnostic.
    ///
    /// Transformation:
    /// - Routes the source through the same formal syntax-output parser and AST
    ///   formatter preparation used by the CLI command.
    #[test]
    fn fmt_rejects_source_export_declarations() {
        let error = parse_source(
            "sample.tl",
            r#"
module sample.
export add/1.
add(x: Int): Int -> x.
"#,
        )
        .expect_err("source export declarations must be rejected before formatting");

        assert!(error.contains("source export declarations are not part of canonical Terlan"));
    }

    /// Verifies that `terlc fmt` still treats `.tli` export summaries as
    /// interface metadata rather than source module visibility.
    ///
    /// Inputs:
    /// - A `.tli` path and interface source containing an export summary.
    ///
    /// Output:
    /// - Formatted interface text preserving the export summary.
    ///
    /// Transformation:
    /// - Selects interface parsing by extension, validates the formal
    ///   syntax-output path, then formats the AST interface module.
    #[test]
    fn fmt_preserves_interface_export_summaries() {
        let formatted = parse_source(
            "sample.tli",
            r#"
module sample.
export add/1.
"#,
        )
        .expect("interface export summaries remain valid formatter input");

        assert!(formatted.contains("export add/1."));
    }
}
