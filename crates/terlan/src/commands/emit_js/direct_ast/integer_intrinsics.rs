//! Direct Oxc lowering for checked integer intrinsics.

use super::{
    core_expr_to_oxc_expression, core_lam_param_to_oxc_formal_parameter, oxc_ident_name,
    oxc_string_value,
};

/// Lowers widened checked multiply-then-divide into the JavaScript Option layout.
pub(super) fn checked_mul_div_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[crate::terlan_typeck::CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::{Argument, ArrayExpressionElement, FormalParameterKind, NumberBase};
    use oxc_span::SPAN;
    use oxc_syntax::operator::{BinaryOperator, LogicalOperator};

    let [left, multiplier, divisor] = args else {
        return None;
    };
    let names = [
        "$terlan_checked_mul_div_left",
        "$terlan_checked_mul_div_multiplier",
        "$terlan_checked_mul_div_divisor",
    ];
    let identifier = |name: &str| ast.expression_identifier(SPAN, oxc_ident_name(ast, name));
    let call_global = |name: &str, argument| {
        ast.expression_call(
            SPAN,
            identifier(name),
            oxc_ast::NONE,
            ast.vec1(Argument::from(argument)),
            false,
        )
    };
    let as_big_int = |name: &str| call_global("BigInt", identifier(name));
    let quotient = ast.expression_binary(
        SPAN,
        ast.expression_binary(
            SPAN,
            as_big_int(names[0]),
            BinaryOperator::Multiplication,
            as_big_int(names[1]),
        ),
        BinaryOperator::Division,
        as_big_int(names[2]),
    );

    let result_name = "$terlan_checked_mul_div_result";
    let result = || identifier(result_name);
    let big_int_text = |text: &str| {
        call_global(
            "BigInt",
            ast.expression_string_literal(SPAN, oxc_string_value(ast, text), None),
        )
    };
    let in_range = ast.expression_logical(
        SPAN,
        ast.expression_binary(
            SPAN,
            result(),
            BinaryOperator::GreaterEqualThan,
            big_int_text("-9223372036854775808"),
        ),
        LogicalOperator::And,
        ast.expression_binary(
            SPAN,
            result(),
            BinaryOperator::LessEqualThan,
            big_int_text("9223372036854775807"),
        ),
    );
    let some = ast.expression_array(
        SPAN,
        ast.vec_from_array([
            ArrayExpressionElement::from(ast.expression_string_literal(
                SPAN,
                oxc_string_value(ast, "some"),
                None,
            )),
            ArrayExpressionElement::from(call_global("Number", result())),
        ]),
    );
    let none = || ast.expression_string_literal(SPAN, oxc_string_value(ast, "none"), None);
    let result_expression = ast.expression_conditional(SPAN, in_range, some, none());
    let result_parameter = core_lam_param_to_oxc_formal_parameter(
        ast,
        &crate::terlan_typeck::CorePattern::Var(result_name.to_string()),
    )?;
    let result_parameters = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec1(result_parameter),
        oxc_ast::NONE,
    );
    let result_body = ast.alloc_function_body(
        SPAN,
        ast.vec(),
        ast.vec1(ast.statement_expression(SPAN, result_expression)),
    );
    let result_function = ast.expression_arrow_function(
        SPAN,
        true,
        false,
        oxc_ast::NONE,
        result_parameters,
        oxc_ast::NONE,
        result_body,
    );
    let divided = ast.expression_call(
        SPAN,
        result_function,
        oxc_ast::NONE,
        ast.vec1(Argument::from(quotient)),
        false,
    );
    let zero = ast.expression_binary(
        SPAN,
        identifier(names[2]),
        BinaryOperator::StrictEquality,
        ast.expression_numeric_literal(SPAN, 0.0, None, NumberBase::Decimal),
    );
    let checked = ast.expression_conditional(SPAN, zero, none(), divided);

    let mut parameters = ast.vec();
    for name in names {
        parameters.push(core_lam_param_to_oxc_formal_parameter(
            ast,
            &crate::terlan_typeck::CorePattern::Var(name.to_string()),
        )?);
    }
    let parameters = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        parameters,
        oxc_ast::NONE,
    );
    let body = ast.alloc_function_body(
        SPAN,
        ast.vec(),
        ast.vec1(ast.statement_expression(SPAN, checked)),
    );
    let function = ast.expression_arrow_function(
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
        function,
        oxc_ast::NONE,
        ast.vec_from_array([
            Argument::from(core_expr_to_oxc_expression(ast, left)?),
            Argument::from(core_expr_to_oxc_expression(ast, multiplier)?),
            Argument::from(core_expr_to_oxc_expression(ast, divisor)?),
        ]),
        false,
    ))
}
