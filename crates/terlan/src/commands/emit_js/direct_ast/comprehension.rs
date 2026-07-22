use crate::terlan_typeck::{
    CoreExpr, CoreListComprehensionGenerator, CorePattern, COMPLETED_GUARD_RESULT_TAG,
};

use super::{
    case::{core_case_binding_pattern, core_case_pattern_test_to_oxc_expression},
    core_expr_to_oxc_expression, core_lam_expr_to_oxc_expression,
    core_lam_param_to_oxc_formal_parameter,
};

const COMPREHENSION_GUARD_RESULT: &str = "__terlan_comprehension_guard_result";
const COMPREHENSION_CANDIDATE: &str = "__terlan_comprehension_candidate";

/// Lowers ordered CoreIR list-comprehension generators into Oxc collection calls.
pub(super) fn core_list_comprehension_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    expr: &CoreExpr,
    generators: &[CoreListComprehensionGenerator],
    guards: &[CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let (generator, remaining) = generators.split_first()?;
    let mut receiver = core_expr_to_oxc_expression(ast, &generator.source)?;
    receiver =
        core_comprehension_pattern_filter_to_oxc_expression(ast, receiver, &generator.pattern)?;
    let binding_pattern = core_comprehension_binding_pattern(&generator.pattern)?;
    if !remaining.is_empty() {
        let nested = core_list_comprehension_to_oxc_expression(ast, expr, remaining, guards)?;
        let callee = ast
            .member_expression_static(SPAN, receiver, ast.identifier_name(SPAN, "flatMap"), false)
            .into();
        let mut args = ast.vec();
        args.push(Argument::from(core_pattern_arrow_to_oxc_expression(
            ast,
            &binding_pattern,
            nested,
        )?));
        return Some(ast.expression_call(SPAN, callee, oxc_ast::NONE, args, false));
    }
    for guard in guards {
        let callee = ast
            .member_expression_static(SPAN, receiver, ast.identifier_name(SPAN, "filter"), false)
            .into();
        let guard = core_expr_to_oxc_expression(ast, guard)?;
        let guard = core_comprehension_guard_to_oxc_expression(ast, guard)?;
        let mut args = ast.vec();
        args.push(Argument::from(core_pattern_arrow_to_oxc_expression(
            ast,
            &binding_pattern,
            guard,
        )?));
        receiver = ast.expression_call(SPAN, callee, oxc_ast::NONE, args, false);
    }
    let callee = ast
        .member_expression_static(SPAN, receiver, ast.identifier_name(SPAN, "map"), false)
        .into();
    let mut args = ast.vec();
    args.push(Argument::from(core_lam_expr_to_oxc_expression(
        ast,
        std::slice::from_ref(&binding_pattern),
        expr,
    )?));
    Some(ast.expression_call(SPAN, callee, oxc_ast::NONE, args, false))
}

/// Filters generator candidates through the same exact matcher used by `case`.
fn core_comprehension_pattern_filter_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    receiver: oxc_ast::ast::Expression<'a>,
    pattern: &CorePattern,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    if matches!(pattern, CorePattern::Var(_) | CorePattern::Wildcard) {
        return Some(receiver);
    }

    let test = core_case_pattern_test_to_oxc_expression(ast, COMPREHENSION_CANDIDATE, pattern)?;
    let predicate = core_pattern_arrow_to_oxc_expression(
        ast,
        &CorePattern::Var(COMPREHENSION_CANDIDATE.to_string()),
        test,
    )?;
    let callee = ast
        .member_expression_static(SPAN, receiver, ast.identifier_name(SPAN, "filter"), false)
        .into();
    Some(ast.expression_call(
        SPAN,
        callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(predicate)),
        false,
    ))
}

/// Projects a matching pattern into a JavaScript binding-only callback pattern.
fn core_comprehension_binding_pattern(pattern: &CorePattern) -> Option<CorePattern> {
    match pattern {
        CorePattern::Var(name) => Some(CorePattern::Var(name.clone())),
        CorePattern::Wildcard => Some(CorePattern::Var(COMPREHENSION_CANDIDATE.to_string())),
        _ => match core_case_binding_pattern(pattern) {
            Some(CorePattern::Wildcard) | None => {
                Some(CorePattern::Var(COMPREHENSION_CANDIDATE.to_string()))
            }
            binding => binding,
        },
    }
}

/// Normalizes a statically validated Boolean or completed GuardResult filter.
fn core_comprehension_guard_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    guard: oxc_ast::ast::Expression<'a>,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;
    use oxc_syntax::number::NumberBase;
    use oxc_syntax::operator::{BinaryOperator, LogicalOperator};

    let value = || ast.expression_identifier(SPAN, COMPREHENSION_GUARD_RESULT);
    let boolean_true = ast.expression_binary(
        SPAN,
        value(),
        BinaryOperator::StrictEquality,
        ast.expression_boolean_literal(SPAN, true),
    );
    let is_array = ast.expression_call(
        SPAN,
        ast.member_expression_static(
            SPAN,
            ast.expression_identifier(SPAN, "Array"),
            ast.identifier_name(SPAN, "isArray"),
            false,
        )
        .into(),
        oxc_ast::NONE,
        ast.vec1(Argument::from(value())),
        false,
    );
    let indexed = |index| {
        ast.member_expression_computed(
            SPAN,
            value(),
            ast.expression_numeric_literal(SPAN, index, None, NumberBase::Decimal),
            false,
        )
        .into()
    };
    let tag_matches = ast.expression_binary(
        SPAN,
        indexed(0.0),
        BinaryOperator::StrictEquality,
        ast.expression_string_literal(SPAN, COMPLETED_GUARD_RESULT_TAG, None),
    );
    let decision = ast.expression_binary(
        SPAN,
        indexed(1.0),
        BinaryOperator::StrictEquality,
        ast.expression_boolean_literal(SPAN, true),
    );
    let completed = ast.expression_logical(
        SPAN,
        ast.expression_logical(SPAN, is_array, LogicalOperator::And, tag_matches),
        LogicalOperator::And,
        decision,
    );
    let body = ast.expression_logical(SPAN, boolean_true, LogicalOperator::Or, completed);
    let decoder = core_pattern_arrow_to_oxc_expression(
        ast,
        &CorePattern::Var(COMPREHENSION_GUARD_RESULT.to_string()),
        body,
    )?;
    Some(ast.expression_call(
        SPAN,
        decoder,
        oxc_ast::NONE,
        ast.vec1(Argument::from(guard)),
        false,
    ))
}

/// Builds the arrow callback that binds one comprehension generator pattern.
fn core_pattern_arrow_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    pattern: &CorePattern,
    body_expr: oxc_ast::ast::Expression<'a>,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::FormalParameterKind;
    use oxc_span::SPAN;

    let params = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec1(core_lam_param_to_oxc_formal_parameter(ast, pattern)?),
        oxc_ast::NONE,
    );
    let body = ast.alloc_function_body(
        SPAN,
        ast.vec(),
        ast.vec1(ast.statement_expression(SPAN, body_expr)),
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
