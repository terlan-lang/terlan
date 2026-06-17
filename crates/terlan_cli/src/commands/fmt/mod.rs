use std::process::ExitCode;

use terlan_syntax::{
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
            terlan_syntax::EbnfCompileError::Parse(message, _) => message,
            terlan_syntax::EbnfCompileError::Serialize(message) => message,
        })?;
        format_interface_source_module(source).map_err(|error| error.message)
    } else {
        parse_module_as_syntax_output(source).map_err(|error| match error {
            terlan_syntax::EbnfCompileError::Parse(message, _) => message,
            terlan_syntax::EbnfCompileError::Serialize(message) => message,
        })?;
        format_source_module(source).map_err(|error| error.message)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_source;

    /// Verifies that `terlc fmt` keeps canonical source modules on `pub`
    /// visibility instead of normalizing removed export-list syntax.
    ///
    /// Inputs:
    /// - A `.terl` path and source containing a source-mode `export` declaration.
    ///
    /// Output:
    /// - A parse error containing the canonical source-export diagnostic.
    ///
    /// Transformation:
    /// - Routes the source through the same formal syntax-output parser and parse tree
    ///   formatter preparation used by the CLI command.
    #[test]
    fn fmt_rejects_source_export_declarations() {
        let error = parse_source(
            "sample.terl",
            r#"
module sample.
export add/1.
add(x: Int): Int -> x.
"#,
        )
        .expect_err("source export declarations must be rejected before formatting");

        assert!(error.contains("source export declarations are not part of canonical Terlan"));
    }

    /// Verifies that `terlc fmt` still treats `.terli` export summaries as
    /// interface metadata rather than source module visibility.
    ///
    /// Inputs:
    /// - A `.terli` path and interface source containing an export summary.
    ///
    /// Output:
    /// - Formatted interface text preserving the export summary.
    ///
    /// Transformation:
    /// - Selects interface parsing by extension, validates the formal
    ///   syntax-output path, then formats the parse tree interface module.
    #[test]
    fn fmt_preserves_interface_export_summaries() {
        let formatted = parse_source(
            "sample.terli",
            r#"
module sample.
export add/1.
"#,
        )
        .expect("interface export summaries remain valid formatter input");

        assert!(formatted.contains("export add/1."));
    }

    /// Verifies `terlc fmt` canonicalizes noisy default-export type imports.
    ///
    /// Inputs:
    /// - A source module importing `std.core.Error.Error`, where the final path
    ///   segment repeats the imported type name.
    ///
    /// Output:
    /// - Formatted source using `import type std.core.Error.`.
    ///
    /// Transformation:
    /// - Parses through the formal syntax-output path, formats through the
    ///   source formatter, and applies the default-export import shorthand only
    ///   when the selected type has no alias.
    #[test]
    fn fmt_collapses_redundant_default_type_import() {
        let formatted = parse_source(
            "sample.terl",
            r#"
module sample.

import type std.core.Error.Error.

pub value(error: Error): Error -> error.
"#,
        )
        .expect("redundant default type import should format");

        assert!(formatted.contains("import type std.core.Error."));
        assert!(!formatted.contains("import type std.core.Error.Error."));
    }

    /// Verifies `terlc fmt` keeps aliased default-export type imports explicit.
    ///
    /// Inputs:
    /// - A source module importing `std.core.Error.Error as CoreError`.
    ///
    /// Output:
    /// - Formatted source preserving the selected import and alias.
    ///
    /// Transformation:
    /// - Guards against collapsing aliased imports because the shorthand cannot
    ///   represent a caller-selected local name.
    #[test]
    fn fmt_preserves_aliased_default_type_import() {
        let formatted = parse_source(
            "sample.terl",
            r#"
module sample.

import type std.core.Error.Error as CoreError.

pub value(error: CoreError): CoreError -> error.
"#,
        )
        .expect("aliased default type import should format");

        assert!(formatted.contains("import type std.core.Error. Error as CoreError."));
    }
}
