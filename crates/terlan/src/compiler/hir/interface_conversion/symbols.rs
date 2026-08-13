use super::*;

/// Adds a function or method declaration to the local symbol table.
///
/// Inputs: declaration, mutable symbol table, and diagnostics sink. Output:
/// symbol table or diagnostics are updated. Transformation: extracts callable
/// shape, detects duplicate shapes, and records exported/public metadata.
pub(in crate::compiler::hir) fn add_syntax_function_symbol(
    declaration: &SyntaxDeclarationOutput,
    function_symbols: &mut HashMap<(String, usize), FunctionSymbol>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (
        name,
        generic_params,
        params,
        return_type,
        generic_bounds,
        receiver_method,
        receiver_mutable,
        is_public,
    ) = match &declaration.payload {
        SyntaxDeclarationPayload::Function {
            name,
            generic_params,
            params,
            return_type,
            generic_bounds,
            is_public,
            ..
        } => (
            name,
            generic_params.clone(),
            syntax_param_signatures(params),
            return_type.text.as_str(),
            generic_bounds.clone(),
            false,
            false,
            *is_public,
        ),
        SyntaxDeclarationPayload::Method {
            receiver,
            name,
            generic_params,
            params,
            return_type,
            generic_bounds,
            is_public,
            ..
        } => (
            name,
            generic_params.clone(),
            syntax_method_param_signatures(receiver, params),
            return_type.text.as_str(),
            generic_bounds.clone(),
            true,
            receiver.is_mutable,
            *is_public,
        ),
        _ => return,
    };

    let key = (name.clone(), params.len());
    if let Some(existing) = function_symbols.get(&key) {
        if function_symbol_shape_matches(
            existing,
            &params,
            return_type,
            receiver_method,
            receiver_mutable,
        ) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!("duplicate function definition: {} / {}", name, params.len()),
            });
        }
        return;
    }

    let symbol = FunctionSymbol {
        name: name.clone(),
        arity: params.len(),
        generic_params,
        params,
        return_type: return_type.to_string(),
        generic_bounds,
        receiver_method,
        receiver_mutable,
        public: is_public,
        exported: is_public,
        pure: declaration_has_marker_annotation(declaration, &["pure"]),
        docs: declaration.docs.clone(),
        span: declaration.span.into(),
    };
    function_symbols.insert(key, symbol);
}

/// Returns whether a declaration carries one exact marker annotation path.
///
/// Inputs:
/// - `declaration`: syntax-output declaration carrying parsed annotations.
/// - `path`: marker annotation path to test.
///
/// Output:
/// - `true` when one declaration annotation has exactly the requested path.
///
/// Transformation:
/// - Compares structured annotation path segments so HIR metadata extraction
///   does not depend on source text formatting.
pub(in crate::compiler::hir) fn declaration_has_marker_annotation(
    declaration: &SyntaxDeclarationOutput,
    path: &[&str],
) -> bool {
    declaration.annotations.iter().any(|annotation| {
        annotation.path.len() == path.len()
            && annotation
                .path
                .iter()
                .map(String::as_str)
                .zip(path.iter().copied())
                .all(|(actual, expected)| actual == expected)
    })
}

/// Checks whether two same-name same-arity function declarations have the same shape.
///
/// Inputs:
/// - `existing`: first HIR function symbol already recorded for the name and
///   arity.
/// - `params`, `return_type`, `receiver_method`, and `receiver_mutable`: shape
///   of the later declaration being considered.
///
/// Output:
/// - `true` when the later declaration is a duplicate of the existing symbol.
/// - `false` when it is a distinct overload candidate.
///
/// Transformation:
/// - Compares callable kind, receiver mutability, return annotation, and
///   parameter annotations. Parameter names are intentionally ignored so
///   overload identity is based on callable type shape, not local binding names.
fn function_symbol_shape_matches(
    existing: &FunctionSymbol,
    params: &[ParamSignature],
    return_type: &str,
    receiver_method: bool,
    receiver_mutable: bool,
) -> bool {
    existing.receiver_method == receiver_method
        && existing.receiver_mutable == receiver_mutable
        && existing.return_type == return_type
        && existing.params.len() == params.len()
        && existing
            .params
            .iter()
            .zip(params.iter())
            .all(|(left, right)| {
                left.annotation == right.annotation && left.is_mutable == right.is_mutable
            })
}

/// Converts method syntax parameters into callable HIR parameters.
///
/// Inputs:
/// - `receiver`: receiver parameter from a syntax-output method declaration.
/// - `params`: ordinary method parameters.
///
/// Output:
/// - Parameter signatures with the receiver first.
///
/// Transformation:
/// - Rewrites source-level receiver syntax into the backend/interface calling
///   convention `method(receiver, params...)`.
pub(in crate::compiler::hir) fn syntax_method_param_signatures(
    receiver: &SyntaxParamOutput,
    params: &[SyntaxParamOutput],
) -> Vec<ParamSignature> {
    std::iter::once(receiver)
        .chain(params.iter())
        .map(|param| ParamSignature {
            name: param.name.clone(),
            annotation: normalize_type_text(&param.annotation.text),
            is_mutable: param.is_mutable,
            default: canonical_param_default(param),
            default_text: param.default_text.clone(),
        })
        .collect()
}
