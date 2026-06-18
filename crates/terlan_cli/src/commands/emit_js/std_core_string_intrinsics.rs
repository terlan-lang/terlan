use terlan_typeck::{CoreExpr, CorePrimitiveIntrinsic};

use super::direct_ast::{core_expr_to_oxc_expression, oxc_ident_name, oxc_string_value};

/// Lowers a supported `std.core.String` intrinsic call into an Oxc expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `intrinsic`: backend-neutral primitive intrinsic identity.
/// - `args`: CoreIR intrinsic arguments in receiver-first order.
///
/// Output:
/// - `Some(Expression)` for the supported string intrinsic subset.
/// - `None` for non-string intrinsics or unsupported argument shapes.
///
/// Transformation:
/// - Maps compiler-owned `std.core.String` intrinsic ids to JavaScript string
///   and array operations without exposing JavaScript method names to CoreIR.
pub(super) fn core_string_intrinsic_call_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    intrinsic: &CorePrimitiveIntrinsic,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    match intrinsic {
        CorePrimitiveIntrinsic::StringContains => {
            core_string_method_intrinsic_to_oxc_expression(ast, args, "includes")
        }
        CorePrimitiveIntrinsic::StringStartsWith => {
            core_string_method_intrinsic_to_oxc_expression(ast, args, "startsWith")
        }
        CorePrimitiveIntrinsic::StringEndsWith => {
            core_string_method_intrinsic_to_oxc_expression(ast, args, "endsWith")
        }
        CorePrimitiveIntrinsic::StringLength => {
            core_string_length_intrinsic_to_oxc_expression(ast, args)
        }
        CorePrimitiveIntrinsic::StringIsEmpty => {
            core_string_is_empty_intrinsic_to_oxc_expression(ast, args)
        }
        CorePrimitiveIntrinsic::StringAppend => {
            core_string_append_intrinsic_to_oxc_expression(ast, args)
        }
        CorePrimitiveIntrinsic::StringConcat => {
            core_string_concat_intrinsic_to_oxc_expression(ast, args)
        }
        CorePrimitiveIntrinsic::StringLowercase => {
            core_string_method_intrinsic_to_oxc_expression(ast, args, "toLowerCase")
        }
        CorePrimitiveIntrinsic::StringUppercase => {
            core_string_method_intrinsic_to_oxc_expression(ast, args, "toUpperCase")
        }
        CorePrimitiveIntrinsic::StringTrim => {
            core_string_method_intrinsic_to_oxc_expression(ast, args, "trim")
        }
        CorePrimitiveIntrinsic::StringTrimStart => {
            core_string_method_intrinsic_to_oxc_expression(ast, args, "trimStart")
        }
        CorePrimitiveIntrinsic::StringTrimEnd => {
            core_string_method_intrinsic_to_oxc_expression(ast, args, "trimEnd")
        }
        _ => None,
    }
}

/// Lowers a one-argument or two-argument string intrinsic into a JavaScript
/// string method call.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `args`: CoreIR intrinsic arguments with the receiver string first.
/// - `method`: JavaScript method name selected by the backend contract.
///
/// Output:
/// - `Some(Expression)` for `value.method()` or `value.method(arg)`.
/// - `None` when arity or child expressions are unsupported.
///
/// Transformation:
/// - Converts backend-neutral string method intrinsics into JavaScript string
///   method calls while keeping JavaScript method names local to the JS
///   backend.
fn core_string_method_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
    method: &str,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let [value, tail @ ..] = args else {
        return None;
    };
    if tail.len() > 1 {
        return None;
    }
    let callee = ast
        .member_expression_static(
            SPAN,
            core_expr_to_oxc_expression(ast, value)?,
            ast.identifier_name(SPAN, oxc_ident_name(ast, method)),
            false,
        )
        .into();
    let mut arguments = ast.vec();
    for arg in tail {
        arguments.push(Argument::from(core_expr_to_oxc_expression(ast, arg)?));
    }
    Some(ast.expression_call(SPAN, callee, oxc_ast::NONE, arguments, false))
}

/// Lowers `core.string.is_empty` into a JavaScript strict empty-string check.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `args`: CoreIR intrinsic arguments in `(value)` order.
///
/// Output:
/// - `Some(Expression)` for `value === ""`.
/// - `None` when the intrinsic has the wrong arity or unsupported value.
///
/// Transformation:
/// - Converts the backend-neutral empty-string predicate into direct
///   JavaScript strict equality without consulting target runtime helpers.
fn core_string_is_empty_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;
    use oxc_syntax::operator::BinaryOperator;

    let [value] = args else {
        return None;
    };
    Some(ast.expression_binary(
        SPAN,
        core_expr_to_oxc_expression(ast, value)?,
        BinaryOperator::StrictEquality,
        ast.expression_string_literal(SPAN, oxc_string_value(ast, ""), None),
    ))
}

/// Lowers `core.string.append` into JavaScript string concatenation.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `args`: CoreIR intrinsic arguments in `(left, right)` order.
///
/// Output:
/// - `Some(Expression)` for `left + right`.
/// - `None` when the intrinsic has the wrong arity or unsupported operands.
///
/// Transformation:
/// - Converts the backend-neutral append operation into JavaScript `+` because
///   the std contract guarantees both operands are typed as `String`.
fn core_string_append_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;
    use oxc_syntax::operator::BinaryOperator;

    let [left, right] = args else {
        return None;
    };
    Some(ast.expression_binary(
        SPAN,
        core_expr_to_oxc_expression(ast, left)?,
        BinaryOperator::Addition,
        core_expr_to_oxc_expression(ast, right)?,
    ))
}

/// Lowers `core.string.concat` into a JavaScript array `.join("")` call.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `args`: CoreIR intrinsic arguments in `(parts)` order.
///
/// Output:
/// - `Some(Expression)` for `parts.join("")`.
/// - `None` when the intrinsic has the wrong arity or unsupported input.
///
/// Transformation:
/// - Converts the backend-neutral concat operation into JavaScript array join,
///   relying on the typechecker to ensure the receiver is a list of strings.
fn core_string_concat_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let [parts] = args else {
        return None;
    };
    let callee = ast
        .member_expression_static(
            SPAN,
            core_expr_to_oxc_expression(ast, parts)?,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "join")),
            false,
        )
        .into();
    Some(ast.expression_call(
        SPAN,
        callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(ast.expression_string_literal(
            SPAN,
            oxc_string_value(ast, ""),
            None,
        ))),
        false,
    ))
}

/// Lowers `core.string.length` into `Array.from(value).length`.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `args`: CoreIR intrinsic arguments in `(value)` order.
///
/// Output:
/// - `Some(Expression)` for JavaScript text-length calculation.
/// - `None` when the intrinsic has the wrong arity or unsupported value.
///
/// Transformation:
/// - Converts the backend-neutral text-length intrinsic into `Array.from` over
///   the JavaScript string value so the probe avoids UTF-16 code-unit `.length`
///   semantics.
fn core_string_length_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let [value] = args else {
        return None;
    };
    let array_from_callee = ast
        .member_expression_static(
            SPAN,
            ast.expression_identifier(SPAN, oxc_ident_name(ast, "Array")),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "from")),
            false,
        )
        .into();
    let array_from = ast.expression_call(
        SPAN,
        array_from_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(core_expr_to_oxc_expression(ast, value)?)),
        false,
    );
    Some(
        ast.member_expression_static(
            SPAN,
            array_from,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "length")),
            false,
        )
        .into(),
    )
}
