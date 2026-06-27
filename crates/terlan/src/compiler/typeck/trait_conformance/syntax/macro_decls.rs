use super::*;

/// Validates macro function return signatures.
///
/// Inputs:
/// - `module`: syntax-output module containing function declarations.
///
/// Output:
/// - Diagnostics for macro declarations whose return type is not `Ast[T]`.
///
/// Transformation:
/// - Scans only functions marked as macros and validates their return
///   annotation with the macro return-type shape helper.
pub(crate) fn check_syntax_macro_decl_signatures(module: &SyntaxModuleOutput) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Function {
            name,
            return_type,
            is_macro,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if !is_macro {
            continue;
        }

        if !is_valid_macro_return_type(&return_type.text) {
            diagnostics.push(Diagnostic {
                span: return_type.span.into(),
                message: format!(
                    "macro `{}` must return Ast[T], found {}",
                    name, return_type.text
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    diagnostics
}

/// Checks whether a macro return annotation has the required `Ast[T]` shape.
///
/// Inputs:
/// - `annotation`: source return type annotation text.
///
/// Output:
/// - `true` when the annotation is an `Ast` application with exactly one
///   non-empty type argument.
///
/// Transformation:
/// - Compacts whitespace, splits a named type application, and validates only
///   the structural return-type shape required for macro declarations.
fn is_valid_macro_return_type(annotation: &str) -> bool {
    let src = compact_spaces(annotation);
    let Some((base, args)) = split_named_type(&src) else {
        return false;
    };
    if base != "Ast" {
        return false;
    }

    let args = split_top_level_csv(&args);
    args.len() == 1 && !args[0].trim().is_empty()
}
