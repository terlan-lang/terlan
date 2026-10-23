use super::{core_expr_to_oxc_expression, core_lam_param_to_oxc_formal_parameter};
use crate::terlan_typeck::{CoreExpr, CoreLetBinding};

/// Preserves sequential binding scopes and evaluates each initializer once.
pub(super) fn core_let_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    bindings: &[CoreLetBinding],
    body: &CoreExpr,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::{Argument, FormalParameterKind};
    use oxc_span::SPAN;

    let mut expression = core_expr_to_oxc_expression(ast, body)?;
    for binding in bindings.iter().rev() {
        let parameter = core_lam_param_to_oxc_formal_parameter(ast, &binding.pattern)?;
        let parameters = ast.alloc_formal_parameters(
            SPAN,
            FormalParameterKind::FormalParameter,
            ast.vec1(parameter),
            oxc_ast::NONE,
        );
        let body = ast.alloc_function_body(
            SPAN,
            ast.vec(),
            ast.vec1(ast.statement_expression(SPAN, expression)),
        );
        let callee = ast.expression_arrow_function(
            SPAN,
            true,
            false,
            oxc_ast::NONE,
            parameters,
            oxc_ast::NONE,
            body,
        );
        expression = ast.expression_call(
            SPAN,
            callee,
            oxc_ast::NONE,
            ast.vec1(Argument::from(core_expr_to_oxc_expression(
                ast,
                &binding.value,
            )?)),
            false,
        );
    }
    Some(expression)
}
