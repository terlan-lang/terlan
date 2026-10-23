use super::core_expr_to_oxc_expression;
use crate::commands::emit_js::direct_helpers::{
    is_direct_oxc_js_identifier, oxc_binding_name, oxc_ident_name,
};

/// Lowers a CoreIR anonymous function value into an Oxc arrow function expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `params`: CoreIR lambda parameter patterns.
/// - `body`: CoreIR lambda body expression.
///
/// Output:
/// - `Some(Expression)` when every parameter is a direct variable binding and
///   the body expression is directly lowerable.
/// - `None` for destructuring parameters, wildcard parameters, unsupported
///   parameter names, or unsupported body expressions.
///
/// Transformation:
/// - Converts Terlan `(patterns) -> Expr` lambda values into JavaScript
///   expression-body arrow functions. This only lowers the function value;
///   callable-value invocation is handled by the dedicated `f(args)` syntax.
pub(super) fn core_lam_expr_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    params: &[crate::terlan_typeck::CorePattern],
    body: &crate::terlan_typeck::CoreExpr,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::FormalParameterKind;
    use oxc_span::SPAN;

    let mut formal_params = ast.vec();
    for param in params {
        formal_params.push(core_lam_param_to_oxc_formal_parameter(ast, param)?);
    }
    let params = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        formal_params,
        oxc_ast::NONE,
    );
    let body_expr = core_expr_to_oxc_expression(ast, body)?;
    let body_span = SPAN;
    let body = ast.alloc_function_body(
        body_span,
        ast.vec(),
        ast.vec1(ast.statement_expression(body_span, body_expr)),
    );
    Some(ast.expression_arrow_function(
        SPAN,
        true,
        false,
        oxc_ast::NONE,
        params,
        oxc_ast::NONE,
        body,
    ))
}

/// Converts one CoreIR lambda parameter pattern into an Oxc formal parameter.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `param`: CoreIR lambda parameter pattern.
///
/// Output:
/// - `Some(FormalParameter)` for direct variable and tuple/list destructuring
///   patterns.
/// - `None` for unsupported identifiers or patterns.
///
/// Transformation:
/// - Reuses the direct Oxc backend's conservative JavaScript identifier policy
///   and lowers Terlan tuple/list patterns to JavaScript array destructuring
///   because this backend represents tuple and list values as arrays.
pub(in crate::commands::emit_js) fn core_lam_param_to_oxc_formal_parameter<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    param: &crate::terlan_typeck::CorePattern,
) -> Option<oxc_ast::ast::FormalParameter<'a>> {
    use oxc_span::SPAN;

    Some(ast.formal_parameter(
        SPAN,
        ast.vec(),
        core_pattern_to_oxc_binding_pattern(ast, param)?,
        oxc_ast::NONE,
        oxc_ast::NONE,
        false,
        None,
        false,
        false,
    ))
}

/// Converts a CoreIR pattern into an Oxc binding pattern for direct JS lambdas.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `param`: CoreIR lambda parameter pattern.
///
/// Output:
/// - `Some(BindingPattern)` for variable, sequence, map, and record patterns.
/// - `None` for wildcards at the top level or unsupported nested patterns.
///
/// Transformation:
/// - Maps Terlan tuple/list destructuring onto arrays and map/record patterns
///   onto objects. Wildcards are handled only as nested array holes by
///   `core_pattern_to_oxc_array_binding_element`.
fn core_pattern_to_oxc_binding_pattern<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    param: &crate::terlan_typeck::CorePattern,
) -> Option<oxc_ast::ast::BindingPattern<'a>> {
    use oxc_span::SPAN;

    match param {
        crate::terlan_typeck::CorePattern::Var(name) if is_direct_oxc_js_identifier(name) => {
            Some(ast.binding_pattern_binding_identifier(SPAN, oxc_binding_name(ast, name.as_str())))
        }
        crate::terlan_typeck::CorePattern::Tuple(items)
        | crate::terlan_typeck::CorePattern::List(items) => {
            let mut elements = ast.vec();
            for item in items {
                elements.push(core_pattern_to_oxc_array_binding_element(ast, item)?);
            }
            Some(ast.binding_pattern_array_pattern(SPAN, elements, oxc_ast::NONE))
        }
        crate::terlan_typeck::CorePattern::Map(fields) => {
            let mut properties = ast.vec();
            for field in fields {
                if !is_direct_oxc_js_identifier(&field.key) {
                    return None;
                }
                properties.push(ast.binding_property(
                    SPAN,
                    ast.property_key_static_identifier(
                        SPAN,
                        oxc_ident_name(ast, field.key.as_str()),
                    ),
                    core_pattern_to_oxc_binding_pattern(ast, &field.value)?,
                    false,
                    false,
                ));
            }
            Some(ast.binding_pattern_object_pattern(SPAN, properties, oxc_ast::NONE))
        }
        crate::terlan_typeck::CorePattern::Record { fields, .. } => {
            let mut properties = ast.vec();
            for field in fields {
                if !is_direct_oxc_js_identifier(&field.key) {
                    return None;
                }
                properties.push(ast.binding_property(
                    SPAN,
                    ast.property_key_static_identifier(
                        SPAN,
                        oxc_ident_name(ast, field.key.as_str()),
                    ),
                    core_pattern_to_oxc_binding_pattern(ast, &field.value)?,
                    false,
                    false,
                ));
            }
            Some(ast.binding_pattern_object_pattern(SPAN, properties, oxc_ast::NONE))
        }
        _ => None,
    }
}

/// Converts a nested CoreIR pattern into an Oxc array-binding element.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `param`: CoreIR pattern inside tuple/list destructuring.
///
/// Output:
/// - `Some(Some(BindingPattern))` for bindable child patterns.
/// - `Some(None)` for wildcard children, represented as JavaScript array holes.
/// - `None` for unsupported child patterns.
///
/// Transformation:
/// - Preserves wildcard non-binding semantics without inventing placeholder
///   identifiers in generated JavaScript.
fn core_pattern_to_oxc_array_binding_element<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    param: &crate::terlan_typeck::CorePattern,
) -> Option<Option<oxc_ast::ast::BindingPattern<'a>>> {
    match param {
        crate::terlan_typeck::CorePattern::Wildcard => Some(None),
        _ => Some(Some(core_pattern_to_oxc_binding_pattern(ast, param)?)),
    }
}
