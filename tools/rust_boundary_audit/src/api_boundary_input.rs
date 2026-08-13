use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

use super::{AuditError, FunctionDeclaration};

pub(super) fn write_api_boundary_input(
    path: &Path,
    functions: &[FunctionDeclaration],
    parse_failures: usize,
    lint_attributes: usize,
) -> Result<(), AuditError> {
    let file = fs::File::create(path).map_err(|error| {
        AuditError::Message(format!(
            "cannot create API-boundary input `{}`: {error}",
            path.display()
        ))
    })?;
    let mut output = BufWriter::new(file);
    writeln!(output, "schema\tterlan.rust-api-boundary-input.v1")
        .and_then(|_| writeln!(output, "parse_failures\t{parse_failures}"))
        .and_then(|_| writeln!(output, "lint_attributes\t{lint_attributes}"))
        .and_then(|_| {
            writeln!(
                output,
                "name\tpath\tvisibility\targument_count\treturns_string_error\texplicit_ffi_parameters\timplementation"
            )
        })
        .map_err(|error| {
            AuditError::Message(format!(
                "cannot write API-boundary input `{}`: {error}",
                path.display()
            ))
        })?;
    for function in functions {
        if function.name.contains('\t')
            || function.name.contains('\n')
            || function.path.contains('\t')
            || function.path.contains('\n')
        {
            return Err(AuditError::Message(
                "API-boundary input contains a non-TSV-safe function identity".to_owned(),
            ));
        }
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            function.name,
            function.path,
            function.visibility,
            function.argument_count,
            function.returns_string_error,
            function.explicit_ffi_parameters,
            function.implementation
        )
        .map_err(|error| {
            AuditError::Message(format!(
                "cannot write API-boundary input `{}`: {error}",
                path.display()
            ))
        })?;
    }
    output.flush().map_err(|error| {
        AuditError::Message(format!(
            "cannot flush API-boundary input `{}`: {error}",
            path.display()
        ))
    })
}
