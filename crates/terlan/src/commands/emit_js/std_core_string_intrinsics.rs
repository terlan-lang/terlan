use crate::terlan_typeck::{CoreExpr, CorePrimitiveIntrinsic};

use super::direct_ast::core_expr_to_oxc_expression;
use super::direct_helpers::{oxc_ident_name, oxc_string_value};

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
        CorePrimitiveIntrinsic::StringReverse => {
            core_string_reverse_intrinsic_to_oxc_expression(ast, args)
        }
        CorePrimitiveIntrinsic::StringCharacters => {
            core_string_characters_intrinsic_to_oxc_expression(ast, args)
        }
        CorePrimitiveIntrinsic::StringCodepoints => {
            core_string_codepoints_intrinsic_to_oxc_expression(ast, args)
        }
        CorePrimitiveIntrinsic::StringUtf8ByteAt => {
            core_string_utf8_byte_at_intrinsic_to_oxc_expression(ast, args)
        }
        CorePrimitiveIntrinsic::StringUtf8FindAnyByte => {
            core_string_utf8_find_any_byte_intrinsic_to_oxc_expression(ast, args)
        }
        CorePrimitiveIntrinsic::StringUtf8Slice => {
            core_string_utf8_slice_intrinsic_to_oxc_expression(ast, args)
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

/// Lowers `core.string.reverse` into `Array.from(value).reverse().join("")`.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `args`: CoreIR intrinsic arguments in `(value)` order.
///
/// Output:
/// - `Some(Expression)` for JavaScript text-unit reversal.
/// - `None` when the intrinsic has the wrong arity or unsupported value.
///
/// Transformation:
/// - Converts the backend-neutral reverse operation into `Array.from` over the
///   JavaScript string, then reverses and joins the resulting code-point array
///   so behavior matches the portable `String.length` text-unit contract.
fn core_string_reverse_intrinsic_to_oxc_expression<'a>(
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
    let reverse_callee = ast
        .member_expression_static(
            SPAN,
            array_from,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "reverse")),
            false,
        )
        .into();
    let reversed = ast.expression_call(SPAN, reverse_callee, oxc_ast::NONE, ast.vec(), false);
    let join_callee = ast
        .member_expression_static(
            SPAN,
            reversed,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "join")),
            false,
        )
        .into();
    Some(ast.expression_call(
        SPAN,
        join_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(ast.expression_string_literal(
            SPAN,
            oxc_string_value(ast, ""),
            None,
        ))),
        false,
    ))
}

/// Lowers `core.string.characters` into `Array.from(value)`.
fn core_string_characters_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let [value] = args else {
        return None;
    };
    let callee = ast
        .member_expression_static(
            SPAN,
            ast.expression_identifier(SPAN, oxc_ident_name(ast, "Array")),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "from")),
            false,
        )
        .into();
    Some(ast.expression_call(
        SPAN,
        callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(core_expr_to_oxc_expression(ast, value)?)),
        false,
    ))
}

/// Lowers `core.string.codepoints` into one JavaScript scalar-array map.
fn core_string_codepoints_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::{Argument, FormalParameterKind};
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
    let characters = ast.expression_call(
        SPAN,
        array_from_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(core_expr_to_oxc_expression(ast, value)?)),
        false,
    );
    let map_callee = ast
        .member_expression_static(
            SPAN,
            characters,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "map")),
            false,
        )
        .into();
    let scalar_name = "__terlan_scalar";
    let codepoint_callee = ast
        .member_expression_static(
            SPAN,
            ast.expression_identifier(SPAN, oxc_ident_name(ast, scalar_name)),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "codePointAt")),
            false,
        )
        .into();
    let zero = ast.expression_numeric_literal(SPAN, 0.0, None, oxc_ast::ast::NumberBase::Decimal);
    let codepoint = ast.expression_call(
        SPAN,
        codepoint_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(zero)),
        false,
    );
    let parameter = super::direct_ast::core_lam_param_to_oxc_formal_parameter(
        ast,
        &crate::terlan_typeck::CorePattern::Var(scalar_name.to_string()),
    )?;
    let parameters = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec1(parameter),
        oxc_ast::NONE,
    );
    let body = ast.alloc_function_body(
        SPAN,
        ast.vec(),
        ast.vec1(ast.statement_expression(SPAN, codepoint)),
    );
    let mapper = ast.expression_arrow_function(
        SPAN,
        true,
        false,
        oxc_ast::NONE,
        parameters,
        oxc_ast::NONE,
        body,
    );
    Some(ast.expression_call(
        SPAN,
        map_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(mapper)),
        false,
    ))
}

fn core_string_utf8_byte_at_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;

    let [value, index] = args else {
        return None;
    };
    let encoded = text_encoder_call(ast, core_expr_to_oxc_expression(ast, value)?);
    Some(
        ast.member_expression_computed(
            SPAN,
            encoded,
            core_expr_to_oxc_expression(ast, index)?,
            false,
        )
        .into(),
    )
}

/// Lowers byte-set search without converting UTF-8 payloads into scalar lists.
fn core_string_utf8_find_any_byte_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::{Argument, BinaryOperator, FormalParameterKind};
    use oxc_span::SPAN;
    use oxc_syntax::operator::LogicalOperator;

    let [value, start, candidates] = args else {
        return None;
    };
    let value_name = "__terlan_utf8_value";
    let start_name = "__terlan_utf8_start";
    let candidates_name = "__terlan_utf8_candidates";
    let byte_name = "__terlan_utf8_byte";
    let index_name = "__terlan_utf8_index";

    let parameter = |name: &str| {
        super::direct_ast::core_lam_param_to_oxc_formal_parameter(
            ast,
            &crate::terlan_typeck::CorePattern::Var(name.to_string()),
        )
    };
    let parameters = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec_from_array([
            parameter(value_name)?,
            parameter(start_name)?,
            parameter(candidates_name)?,
        ]),
        oxc_ast::NONE,
    );

    let encoded_value = || {
        text_encoder_call(
            ast,
            ast.expression_identifier(SPAN, oxc_ident_name(ast, value_name)),
        )
    };
    let encoded_candidates = || {
        text_encoder_call(
            ast,
            ast.expression_identifier(SPAN, oxc_ident_name(ast, candidates_name)),
        )
    };
    let start_identifier = || ast.expression_identifier(SPAN, oxc_ident_name(ast, start_name));

    let nonnegative = ast.expression_binary(
        SPAN,
        start_identifier(),
        BinaryOperator::GreaterEqualThan,
        ast.expression_numeric_literal(SPAN, 0.0, None, oxc_ast::ast::NumberBase::Decimal),
    );
    let value_length: oxc_ast::ast::Expression<'a> = ast
        .member_expression_static(
            SPAN,
            encoded_value(),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "length")),
            false,
        )
        .into();
    let within_value = ast.expression_binary(
        SPAN,
        start_identifier(),
        BinaryOperator::LessEqualThan,
        value_length,
    );

    let ascii_parameter = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec1(parameter(byte_name)?),
        oxc_ast::NONE,
    );
    let ascii_test = ast.expression_binary(
        SPAN,
        ast.expression_identifier(SPAN, oxc_ident_name(ast, byte_name)),
        BinaryOperator::LessThan,
        ast.expression_numeric_literal(SPAN, 128.0, None, oxc_ast::ast::NumberBase::Decimal),
    );
    let ascii_body = ast.alloc_function_body(
        SPAN,
        ast.vec(),
        ast.vec1(ast.statement_expression(SPAN, ascii_test)),
    );
    let ascii_predicate = ast.expression_arrow_function(
        SPAN,
        true,
        false,
        oxc_ast::NONE,
        ascii_parameter,
        oxc_ast::NONE,
        ascii_body,
    );
    let every_callee = ast
        .member_expression_static(
            SPAN,
            encoded_candidates(),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "every")),
            false,
        )
        .into();
    let ascii_candidates = ast.expression_call(
        SPAN,
        every_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(ascii_predicate)),
        false,
    );
    let valid = ast.expression_logical(
        SPAN,
        ast.expression_logical(SPAN, nonnegative, LogicalOperator::And, within_value),
        LogicalOperator::And,
        ascii_candidates,
    );

    let match_parameters = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec_from_array([parameter(byte_name)?, parameter(index_name)?]),
        oxc_ast::NONE,
    );
    let after_start = ast.expression_binary(
        SPAN,
        ast.expression_identifier(SPAN, oxc_ident_name(ast, index_name)),
        BinaryOperator::GreaterEqualThan,
        start_identifier(),
    );
    let includes_callee = ast
        .member_expression_static(
            SPAN,
            encoded_candidates(),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "includes")),
            false,
        )
        .into();
    let candidate_matches = ast.expression_call(
        SPAN,
        includes_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(
            ast.expression_identifier(SPAN, oxc_ident_name(ast, byte_name)),
        )),
        false,
    );
    let match_test =
        ast.expression_logical(SPAN, after_start, LogicalOperator::And, candidate_matches);
    let match_body = ast.alloc_function_body(
        SPAN,
        ast.vec(),
        ast.vec1(ast.statement_expression(SPAN, match_test)),
    );
    let match_predicate = ast.expression_arrow_function(
        SPAN,
        true,
        false,
        oxc_ast::NONE,
        match_parameters,
        oxc_ast::NONE,
        match_body,
    );
    let find_callee = ast
        .member_expression_static(
            SPAN,
            encoded_value(),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "findIndex")),
            false,
        )
        .into();
    let found = ast.expression_call(
        SPAN,
        find_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(match_predicate)),
        false,
    );

    let error = ast.expression_new(
        SPAN,
        ast.expression_identifier(SPAN, oxc_ident_name(ast, "RangeError")),
        oxc_ast::NONE,
        ast.vec1(Argument::from(ast.expression_string_literal(
            SPAN,
            oxc_string_value(ast, "invalid UTF-8 byte search"),
            None,
        ))),
    );
    let invalid_body =
        ast.alloc_function_body(SPAN, ast.vec(), ast.vec1(ast.statement_throw(SPAN, error)));
    let invalid_parameters = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec(),
        oxc_ast::NONE,
    );
    let invalid_arrow = ast.expression_arrow_function(
        SPAN,
        false,
        false,
        oxc_ast::NONE,
        invalid_parameters,
        oxc_ast::NONE,
        invalid_body,
    );
    let invalid = ast.expression_call(SPAN, invalid_arrow, oxc_ast::NONE, ast.vec(), false);
    let result = ast.expression_conditional(SPAN, valid, found, invalid);
    let body = ast.alloc_function_body(
        SPAN,
        ast.vec(),
        ast.vec1(ast.statement_expression(SPAN, result)),
    );
    let search = ast.expression_arrow_function(
        SPAN,
        true,
        false,
        oxc_ast::NONE,
        parameters,
        oxc_ast::NONE,
        body,
    );
    Some(ast.expression_call(
        SPAN,
        search,
        oxc_ast::NONE,
        ast.vec_from_array([
            Argument::from(core_expr_to_oxc_expression(ast, value)?),
            Argument::from(core_expr_to_oxc_expression(ast, start)?),
            Argument::from(core_expr_to_oxc_expression(ast, candidates)?),
        ]),
        false,
    ))
}

fn core_string_utf8_slice_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::{Argument, BinaryOperator};
    use oxc_span::SPAN;

    let [value, start, length] = args else {
        return None;
    };
    let encoded = text_encoder_call(ast, core_expr_to_oxc_expression(ast, value)?);
    let slice_callee = ast
        .member_expression_static(
            SPAN,
            encoded,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "slice")),
            false,
        )
        .into();
    let end = ast.expression_binary(
        SPAN,
        core_expr_to_oxc_expression(ast, start)?,
        BinaryOperator::Addition,
        core_expr_to_oxc_expression(ast, length)?,
    );
    let sliced = ast.expression_call(
        SPAN,
        slice_callee,
        oxc_ast::NONE,
        ast.vec_from_array([
            Argument::from(core_expr_to_oxc_expression(ast, start)?),
            Argument::from(end),
        ]),
        false,
    );
    let decoder = ast.expression_new(
        SPAN,
        ast.expression_identifier(SPAN, oxc_ident_name(ast, "TextDecoder")),
        oxc_ast::NONE,
        ast.vec(),
    );
    let decode_callee = ast
        .member_expression_static(
            SPAN,
            decoder,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "decode")),
            false,
        )
        .into();
    Some(ast.expression_call(
        SPAN,
        decode_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(sliced)),
        false,
    ))
}

fn text_encoder_call<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    value: oxc_ast::ast::Expression<'a>,
) -> oxc_ast::ast::Expression<'a> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let encoder = ast.expression_new(
        SPAN,
        ast.expression_identifier(SPAN, oxc_ident_name(ast, "TextEncoder")),
        oxc_ast::NONE,
        ast.vec(),
    );
    let encode_callee = ast
        .member_expression_static(
            SPAN,
            encoder,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "encode")),
            false,
        )
        .into();
    ast.expression_call(
        SPAN,
        encode_callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(value)),
        false,
    )
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
