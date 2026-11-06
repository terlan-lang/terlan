//! Direct Oxc lowering for calculator-grade Float intrinsics.

use super::{core_expr_to_oxc_expression, core_lam_param_to_oxc_formal_parameter, oxc_ident_name};

pub(super) fn float_math_method(
    intrinsic: &crate::terlan_typeck::CorePrimitiveIntrinsic,
) -> Option<&'static str> {
    use crate::terlan_typeck::CorePrimitiveIntrinsic as P;

    Some(match intrinsic {
        P::FloatFloor => "floor",
        P::FloatCeil => "ceil",
        P::FloatLog => "log",
        P::FloatAtan => "atan",
        P::FloatSqrt => "sqrt",
        P::FloatRound => "round",
        P::FloatTrunc => "trunc",
        P::FloatExp => "exp",
        P::FloatLog2 => "log2",
        P::FloatLog10 => "log10",
        P::FloatCbrt => "cbrt",
        P::FloatSin => "sin",
        P::FloatCos => "cos",
        P::FloatTan => "tan",
        P::FloatAsin => "asin",
        P::FloatAcos => "acos",
        P::FloatSinh => "sinh",
        P::FloatCosh => "cosh",
        P::FloatTanh => "tanh",
        P::FloatAsinh => "asinh",
        P::FloatAcosh => "acosh",
        P::FloatAtanh => "atanh",
        P::FloatExp2 => "pow",
        P::FloatExpm1 => "expm1",
        P::FloatLog1p => "log1p",
        P::FloatPow => "pow",
        P::FloatAtan2 => "atan2",
        P::FloatHypot => "hypot",
        _ => return None,
    })
}

pub(super) fn float_intrinsic_call_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    intrinsic: &crate::terlan_typeck::CorePrimitiveIntrinsic,
    args: &[crate::terlan_typeck::CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::{Argument, FormalParameterKind, NumberBase};
    use oxc_span::SPAN;
    use oxc_syntax::operator::{BinaryOperator, UnaryOperator};

    let expected = if matches!(
        intrinsic,
        crate::terlan_typeck::CorePrimitiveIntrinsic::FloatPow
            | crate::terlan_typeck::CorePrimitiveIntrinsic::FloatAtan2
            | crate::terlan_typeck::CorePrimitiveIntrinsic::FloatHypot
    ) {
        2
    } else {
        1
    };
    if args.len() != expected {
        return None;
    }

    if matches!(
        intrinsic,
        crate::terlan_typeck::CorePrimitiveIntrinsic::FloatRound
    ) {
        let name = "$terlan_float_round_value";
        let identifier = || ast.expression_identifier(SPAN, oxc_ident_name(ast, name));
        let round_call = |value| {
            let callee = ast
                .member_expression_static(
                    SPAN,
                    ast.expression_identifier(SPAN, oxc_ident_name(ast, "Math")),
                    ast.identifier_name(SPAN, oxc_ident_name(ast, "round")),
                    false,
                )
                .into();
            ast.expression_call(
                SPAN,
                callee,
                oxc_ast::NONE,
                ast.vec1(Argument::from(value)),
                false,
            )
        };
        let negative = ast.expression_unary(
            SPAN,
            UnaryOperator::UnaryNegation,
            round_call(ast.expression_unary(SPAN, UnaryOperator::UnaryNegation, identifier())),
        );
        let positive = round_call(identifier());
        let result = ast.expression_conditional(
            SPAN,
            ast.expression_binary(
                SPAN,
                identifier(),
                BinaryOperator::LessThan,
                ast.expression_numeric_literal(SPAN, 0.0, None, NumberBase::Decimal),
            ),
            negative,
            positive,
        );
        let parameter = core_lam_param_to_oxc_formal_parameter(
            ast,
            &crate::terlan_typeck::CorePattern::Var(name.to_string()),
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
            ast.vec1(ast.statement_expression(SPAN, result)),
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
        return Some(ast.expression_call(
            SPAN,
            function,
            oxc_ast::NONE,
            ast.vec1(Argument::from(core_expr_to_oxc_expression(ast, &args[0])?)),
            false,
        ));
    }

    if matches!(
        intrinsic,
        crate::terlan_typeck::CorePrimitiveIntrinsic::FloatExp2
    ) {
        let callee = ast
            .member_expression_static(
                SPAN,
                ast.expression_identifier(SPAN, oxc_ident_name(ast, "Math")),
                ast.identifier_name(SPAN, oxc_ident_name(ast, "pow")),
                false,
            )
            .into();
        return Some(ast.expression_call(
            SPAN,
            callee,
            oxc_ast::NONE,
            ast.vec_from_array([
                Argument::from(ast.expression_numeric_literal(
                    SPAN,
                    2.0,
                    None,
                    NumberBase::Decimal,
                )),
                Argument::from(core_expr_to_oxc_expression(ast, &args[0])?),
            ]),
            false,
        ));
    }

    let callee = ast
        .member_expression_static(
            SPAN,
            ast.expression_identifier(SPAN, oxc_ident_name(ast, "Math")),
            ast.identifier_name(SPAN, oxc_ident_name(ast, float_math_method(intrinsic)?)),
            false,
        )
        .into();
    let mut arguments = ast.vec();
    for argument in args {
        arguments.push(Argument::from(core_expr_to_oxc_expression(ast, argument)?));
    }
    Some(ast.expression_call(SPAN, callee, oxc_ast::NONE, arguments, false))
}
