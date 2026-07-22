use super::*;

/// Collects user-declared annotation schemas from syntax declarations.
///
/// Inputs:
/// - `declarations`: module declarations after syntax-output payload routing.
///
/// Output:
/// - Map from dotted annotation path to schema payload.
///
/// Transformation:
/// - Indexes user schemas while rejecting duplicate paths and overlap with
///   compiler-owned annotation namespaces.
pub(super) fn collect_user_annotation_schemas<'a>(
    declarations: &'a [SyntaxDeclarationOutput],
) -> EbnfCompileResult<std::collections::BTreeMap<String, &'a SyntaxDeclarationPayload>> {
    let mut schemas = std::collections::BTreeMap::new();
    for declaration in declarations {
        if let SyntaxDeclarationPayload::AnnotationSchema { path, .. } = &declaration.payload {
            let path_text = path.join(".");
            if annotation_schema_path_is_reserved(path) {
                return annotation_schema_error(
                    declaration,
                    format!(
                        "annotation schema `@{path_text}` uses reserved compiler metadata namespace `{}`",
                        path[0]
                    ),
                );
            }
            if schemas
                .insert(path_text.clone(), &declaration.payload)
                .is_some()
            {
                return annotation_schema_error(
                    declaration,
                    format!("duplicate annotation schema `@{path_text}`"),
                );
            }
        }
    }
    Ok(schemas)
}

/// Returns whether a user schema path overlaps compiler-owned metadata.
fn annotation_schema_path_is_reserved(path: &[String]) -> bool {
    matches!(
        path.first().map(String::as_str),
        Some("compiler" | "target" | "native")
    ) || matches!(path, [name] if name == "test" || name == "pure")
}

/// Builds a parse diagnostic at an annotation schema declaration.
fn annotation_schema_error<T>(
    declaration: &SyntaxDeclarationOutput,
    message: impl Into<String>,
) -> EbnfCompileResult<T> {
    Err(EbnfCompileError::Parse(
        message.into(),
        declaration.span.into(),
    ))
}
