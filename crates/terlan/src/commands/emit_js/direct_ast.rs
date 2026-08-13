use crate::terlan_typeck::{CoreExpr, CoreModule};

use super::cast_semantics::cast_can_lower_as_js_identity;
use super::direct_helpers::{
    core_binary_operator_to_oxc, core_float_literal_to_oxc_number, core_unary_operator_to_oxc,
    is_direct_oxc_js_identifier, is_js_safe_integer, oxc_ident_name, oxc_string_value,
};
use super::direct_reachability::reachable_direct_function_names;
use super::template_runtime::template_renderer_identifier;

mod case;
mod comprehension;

use case::core_case_clauses_to_oxc_expression;
use comprehension::core_list_comprehension_to_oxc_expression;

/// Builds and prints a minimal JavaScript module directly through Oxc AST APIs.
///
/// Inputs:
/// - None. The fixture shape is fixed to a small exported arithmetic function.
///
/// Output:
/// - JavaScript source printed by Oxc codegen.
///
/// Transformation:
/// - Constructs an Oxc `Program` with `AstBuilder`, then prints it through
///   `oxc_codegen`. This proves the direct AST construction path compiles
///   before production CoreIR lowering switches to Oxc AST nodes.
#[cfg(test)]
pub(crate) fn emit_minimal_direct_oxc_ast_module() -> String {
    use oxc_ast::{
        ast::{FormalParameterKind, FunctionType, ImportOrExportKind, Statement},
        AstBuilder, NONE,
    };
    use oxc_span::{SourceType, SPAN};
    use oxc_syntax::operator::BinaryOperator;

    let allocator = oxc_allocator::Allocator::default();
    let ast = AstBuilder::new(&allocator);

    let param_a = ast.formal_parameter(
        SPAN,
        ast.vec(),
        ast.binding_pattern_binding_identifier(SPAN, "A"),
        NONE,
        NONE,
        false,
        None,
        false,
        false,
    );
    let param_b = ast.formal_parameter(
        SPAN,
        ast.vec(),
        ast.binding_pattern_binding_identifier(SPAN, "B"),
        NONE,
        NONE,
        false,
        None,
        false,
        false,
    );
    let params = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec_from_array([param_a, param_b]),
        NONE,
    );
    let return_expr = ast.expression_binary(
        SPAN,
        ast.expression_identifier(SPAN, "A"),
        BinaryOperator::Addition,
        ast.expression_identifier(SPAN, "B"),
    );
    let return_stmt = ast.statement_return(SPAN, Some(return_expr));
    let body = ast.alloc_function_body(SPAN, ast.vec(), ast.vec1(return_stmt));
    let declaration = ast.declaration_function(
        SPAN,
        FunctionType::FunctionDeclaration,
        Some(ast.binding_identifier(SPAN, "add")),
        false,
        false,
        false,
        NONE,
        NONE,
        params,
        NONE,
        Some(body),
    );
    let export = ast.module_declaration_export_named_declaration(
        SPAN,
        Some(declaration),
        ast.vec(),
        None,
        ImportOrExportKind::Value,
        NONE,
    );
    let program = ast.program(
        SPAN,
        SourceType::mjs(),
        "",
        ast.vec(),
        None,
        ast.vec(),
        ast.vec1(Statement::from(export)),
    );
    oxc_codegen::Codegen::new().build(&program).code
}

/// Emits a tiny CoreIR subset through direct Oxc AST construction.
///
/// Inputs:
/// - `module`: CoreIR module produced by the formal pipeline.
///
/// Output:
/// - `Some(String)` containing Oxc-printed JavaScript when every reachable
///   module function fits the direct-AST subset.
/// - `None` when a reachable function uses unsupported clauses, patterns, or
///   expressions.
///
/// Transformation:
/// - Builds JavaScript functions directly with Oxc `AstBuilder` for reachable
///   single-clause, unguarded CoreIR functions. Public functions are emitted as
///   named exports; reachable private functions are emitted as local
///   declarations so direct local calls can resolve inside the generated module.
pub(crate) fn emit_core_module_with_direct_oxc_ast(module: &CoreModule) -> Option<String> {
    use oxc_ast::{
        ast::{FormalParameterKind, FunctionType, ImportOrExportKind, Statement},
        AstBuilder, NONE,
    };
    use oxc_span::{SourceType, SPAN};
    let allocator = oxc_allocator::Allocator::default();
    let ast = AstBuilder::new(&allocator);
    let mut statements = ast.vec();
    let reachable_functions = reachable_direct_function_names(module);
    for function in module
        .functions
        .iter()
        .filter(|function| reachable_functions.contains(&function.name))
    {
        if !is_direct_oxc_js_identifier(&function.name) {
            return None;
        }
        let [clause] = function.clauses.as_slice() else {
            return None;
        };
        if clause.guard.is_some() || function.params.len() != clause.core_patterns.len() {
            return None;
        }
        for (param, pattern) in function.params.iter().zip(clause.core_patterns.iter()) {
            if !matches!(pattern, Some(crate::terlan_typeck::CorePattern::Var(name)) if name == &param.name)
                || !is_direct_oxc_js_identifier(&param.name)
            {
                return None;
            }
        }
        let mut params = ast.vec();
        for param in &function.params {
            let param_name = oxc_ident_name(ast, param.name.as_str());
            params.push(ast.formal_parameter(
                SPAN,
                ast.vec(),
                ast.binding_pattern_binding_identifier(SPAN, param_name),
                NONE,
                NONE,
                false,
                None,
                false,
                false,
            ));
        }
        let params =
            ast.alloc_formal_parameters(SPAN, FormalParameterKind::FormalParameter, params, NONE);
        let return_expr = core_expr_to_oxc_expression(ast, clause.body.core_expr.as_ref()?)?;
        let body = ast.alloc_function_body(
            SPAN,
            ast.vec(),
            ast.vec1(ast.statement_return(SPAN, Some(return_expr))),
        );
        let declaration = ast.declaration_function(
            SPAN,
            FunctionType::FunctionDeclaration,
            Some(ast.binding_identifier(SPAN, oxc_ident_name(ast, function.name.as_str()))),
            false,
            false,
            false,
            NONE,
            NONE,
            params,
            NONE,
            Some(body),
        );
        if function.public {
            let export = ast.module_declaration_export_named_declaration(
                SPAN,
                Some(declaration),
                ast.vec(),
                None,
                ImportOrExportKind::Value,
                NONE,
            );
            statements.push(Statement::from(export));
        } else {
            statements.push(Statement::from(declaration));
        }
    }
    let program = ast.program(
        SPAN,
        SourceType::mjs(),
        "",
        ast.vec(),
        None,
        ast.vec(),
        statements,
    );
    Some(oxc_codegen::Codegen::new().build(&program).code)
}

/// Lowers a tiny CoreIR expression subset into an Oxc expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `expr`: CoreIR expression to lower.
///
/// Output:
/// - `Some(Expression)` for integer/float, string-like literal, tuple/list,
///   fixed array, index, field/record access, identifier-key map, record
///   construction/update, template instantiation, anonymous function values,
///   total literal-pattern case expressions, total if expressions, local call,
///   variable, and supported unary/binary expressions.
/// - `None` for unsupported expression forms.
///
/// Transformation:
/// - Recursively maps selected CoreIR expressions into Oxc expression nodes
///   without going through JavaScript source text.
pub(super) fn core_expr_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    expr: &crate::terlan_typeck::CoreExpr,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::ArrayExpressionElement;
    use oxc_span::SPAN;
    use oxc_syntax::number::NumberBase;

    match expr {
        crate::terlan_typeck::CoreExpr::Int(value) if *value >= 0 && is_js_safe_integer(*value) => {
            Some(ast.expression_numeric_literal(SPAN, *value as f64, None, NumberBase::Decimal))
        }
        crate::terlan_typeck::CoreExpr::Float(value) => Some(ast.expression_numeric_literal(
            SPAN,
            core_float_literal_to_oxc_number(value)?,
            None,
            NumberBase::Decimal,
        )),
        crate::terlan_typeck::CoreExpr::Atom(value)
        | crate::terlan_typeck::CoreExpr::Var(value)
            if value == "true" || value == "false" =>
        {
            Some(ast.expression_boolean_literal(SPAN, value == "true"))
        }
        crate::terlan_typeck::CoreExpr::Binary(value)
        | crate::terlan_typeck::CoreExpr::Atom(value) => {
            Some(ast.expression_string_literal(SPAN, oxc_string_value(ast, value.as_str()), None))
        }
        crate::terlan_typeck::CoreExpr::Var(name) if is_direct_oxc_js_identifier(name) => {
            Some(ast.expression_identifier(SPAN, oxc_ident_name(ast, name.as_str())))
        }
        crate::terlan_typeck::CoreExpr::Tuple(items)
        | crate::terlan_typeck::CoreExpr::List(items)
        | crate::terlan_typeck::CoreExpr::FixedArray(items) => {
            let mut elements = ast.vec();
            for item in items {
                elements.push(ArrayExpressionElement::from(core_expr_to_oxc_expression(
                    ast, item,
                )?));
            }
            Some(ast.expression_array(SPAN, elements))
        }
        crate::terlan_typeck::CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            lift,
        } => lift
            .is_none()
            .then(|| core_list_comprehension_to_oxc_expression(ast, expr, generators, guards))
            .flatten(),
        crate::terlan_typeck::CoreExpr::ListCons { head, tail } => {
            let mut elements = ast.vec();
            elements.push(ArrayExpressionElement::from(core_expr_to_oxc_expression(
                ast, head,
            )?));
            elements.push(ast.array_expression_element_spread_element(
                SPAN,
                core_expr_to_oxc_expression(ast, tail)?,
            ));
            Some(ast.expression_array(SPAN, elements))
        }
        crate::terlan_typeck::CoreExpr::Index { base, index } => Some(
            ast.member_expression_computed(
                SPAN,
                core_expr_to_oxc_expression(ast, base)?,
                core_expr_to_oxc_expression(ast, index)?,
                false,
            )
            .into(),
        ),
        crate::terlan_typeck::CoreExpr::FieldAccess { base, field }
            if is_direct_oxc_js_identifier(field) =>
        {
            Some(
                ast.member_expression_static(
                    SPAN,
                    core_expr_to_oxc_expression(ast, base)?,
                    ast.identifier_name(SPAN, oxc_ident_name(ast, field.as_str())),
                    false,
                )
                .into(),
            )
        }
        crate::terlan_typeck::CoreExpr::RecordAccess { base, field, .. }
            if is_direct_oxc_js_identifier(field) =>
        {
            Some(
                ast.member_expression_static(
                    SPAN,
                    core_expr_to_oxc_expression(ast, base)?,
                    ast.identifier_name(SPAN, oxc_ident_name(ast, field.as_str())),
                    false,
                )
                .into(),
            )
        }
        crate::terlan_typeck::CoreExpr::Map(fields) => {
            let mut properties = ast.vec();
            for field in fields {
                properties.push(core_object_field_to_oxc_property(
                    ast,
                    field.key.as_str(),
                    &field.value,
                )?);
            }
            Some(ast.expression_object(SPAN, properties))
        }
        crate::terlan_typeck::CoreExpr::RecordConstruct { fields, .. } => {
            let mut properties = ast.vec();
            for field in fields {
                properties.push(core_object_field_to_oxc_property(
                    ast,
                    field.key.as_str(),
                    &field.value,
                )?);
            }
            Some(ast.expression_object(SPAN, properties))
        }
        crate::terlan_typeck::CoreExpr::TemplateInstantiate { name, fields } => {
            use oxc_ast::ast::Argument;

            let mut properties = ast.vec();
            for field in fields {
                properties.push(core_object_field_to_oxc_property(
                    ast,
                    field.key.as_str(),
                    &field.value,
                )?);
            }
            let mut arguments = ast.vec();
            arguments.push(Argument::from(ast.expression_object(SPAN, properties)));
            Some(ast.expression_call(
                SPAN,
                ast.expression_identifier(
                    SPAN,
                    oxc_ident_name(ast, template_renderer_identifier(name)?.as_str()),
                ),
                oxc_ast::NONE,
                arguments,
                false,
            ))
        }
        crate::terlan_typeck::CoreExpr::RecordUpdate { base, fields, .. } => {
            let mut properties = ast.vec();
            properties.push(ast.object_property_kind_spread_property(
                SPAN,
                core_expr_to_oxc_expression(ast, base)?,
            ));
            for field in fields {
                properties.push(core_object_field_to_oxc_property(
                    ast,
                    field.key.as_str(),
                    &field.value,
                )?);
            }
            Some(ast.expression_object(SPAN, properties))
        }
        crate::terlan_typeck::CoreExpr::Case { scrutinee, clauses } => {
            core_case_clauses_to_oxc_expression(ast, scrutinee, clauses)
        }
        crate::terlan_typeck::CoreExpr::If { clauses } => {
            core_if_clauses_to_oxc_expression(ast, clauses)
        }
        crate::terlan_typeck::CoreExpr::Lam { params, body } => {
            core_lam_expr_to_oxc_expression(ast, params, body)
        }
        crate::terlan_typeck::CoreExpr::UnaryOp { operator, operand } => {
            Some(ast.expression_unary(
                SPAN,
                core_unary_operator_to_oxc(operator)?,
                core_expr_to_oxc_expression(ast, operand)?,
            ))
        }
        crate::terlan_typeck::CoreExpr::Call { function, args }
            if is_direct_oxc_js_identifier(function) =>
        {
            core_call_expr_to_oxc_expression(ast, function, args)
        }
        crate::terlan_typeck::CoreExpr::FunctionCall { callee, args } => {
            core_function_call_expr_to_oxc_expression(ast, callee, args)
        }
        crate::terlan_typeck::CoreExpr::Cast { expr, target_type } => {
            cast_can_lower_as_js_identity(expr, target_type)
                .then(|| core_expr_to_oxc_expression(ast, expr))?
        }
        crate::terlan_typeck::CoreExpr::Intrinsic(call) => {
            core_intrinsic_call_expr_to_oxc_expression(ast, call)
        }
        crate::terlan_typeck::CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if operator == "|>" => core_pipe_forward_to_oxc_expression(ast, left, right),
        crate::terlan_typeck::CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if operator == "div" => core_integer_division_to_oxc_expression(ast, left, right),
        crate::terlan_typeck::CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if matches!(operator.as_str(), "and" | "or") => {
            core_logical_expr_to_oxc_expression(ast, operator, left, right)
        }
        crate::terlan_typeck::CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => Some(ast.expression_binary(
            SPAN,
            core_expr_to_oxc_expression(ast, left)?,
            core_binary_operator_to_oxc(operator)?,
            core_expr_to_oxc_expression(ast, right)?,
        )),
        _ => None,
    }
}

/// Lowers Terlan boolean operators into Oxc logical expressions.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `operator`: CoreIR logical operator spelling.
/// - `left`: left-hand boolean expression.
/// - `right`: right-hand boolean expression.
///
/// Output:
/// - `Some(Expression)` for supported short-circuit logical operators.
/// - `None` when either child expression remains unsupported.
///
/// Transformation:
/// - Maps Terlan `and`/`or` and their symbolic aliases to JavaScript logical
///   expressions so direct Oxc lowering preserves short-circuit semantics.
fn core_logical_expr_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    operator: &str,
    left: &crate::terlan_typeck::CoreExpr,
    right: &crate::terlan_typeck::CoreExpr,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;
    use oxc_syntax::operator::LogicalOperator;

    let operator = match operator {
        "and" => LogicalOperator::And,
        "or" => LogicalOperator::Or,
        _ => return None,
    };
    Some(ast.expression_logical(
        SPAN,
        core_expr_to_oxc_expression(ast, left)?,
        operator,
        core_expr_to_oxc_expression(ast, right)?,
    ))
}

/// Lowers a supported CoreIR intrinsic call into an Oxc expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `call`: CoreIR intrinsic call with a closed backend-neutral intrinsic id.
///
/// Output:
/// - `Some(Expression)` for the supported intrinsic subset.
/// - `None` for intrinsic operations that are not yet selected for direct Oxc
///   emission.
///
/// Transformation:
/// - Maps compiler-owned primitive intrinsic ids to JavaScript standard
///   operations through Oxc AST nodes, without leaking Oxc or JavaScript names
///   into CoreIR.
fn core_intrinsic_call_expr_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    call: &crate::terlan_typeck::CoreIntrinsicCall,
) -> Option<oxc_ast::ast::Expression<'a>> {
    match &call.id {
        crate::terlan_typeck::CoreIntrinsicId::Primitive(intrinsic) => {
            super::std_core_string_intrinsics::core_string_intrinsic_call_to_oxc_expression(
                ast,
                intrinsic,
                call.args.as_slice(),
            )
        }
        _ => None,
    }
}

/// Lowers a local named CoreIR call into an Oxc call expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `function`: local CoreIR function name.
/// - `args`: CoreIR argument expressions.
///
/// Output:
/// - `Some(Expression)` for supported names and argument expressions.
/// - `None` when the function name is not safe for JavaScript identifier
///   emission or any argument remains unsupported.
///
/// Transformation:
/// - Builds a JavaScript `function(arg1, arg2, ...)` call without introducing
///   module, constructor, or general callable-value semantics.
fn core_call_expr_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    function: &str,
    args: &[crate::terlan_typeck::CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    if !is_direct_oxc_js_identifier(function) {
        return None;
    }
    let mut arguments = ast.vec();
    for arg in args {
        arguments.push(Argument::from(core_expr_to_oxc_expression(ast, arg)?));
    }
    Some(ast.expression_call(
        SPAN,
        ast.expression_identifier(SPAN, oxc_ident_name(ast, function)),
        oxc_ast::NONE,
        arguments,
        false,
    ))
}

/// Lowers a CoreIR function-value invocation into an Oxc call expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `callee`: CoreIR expression that evaluates to a callable value.
/// - `args`: CoreIR argument expressions.
///
/// Output:
/// - `Some(Expression)` when the callee and all arguments are supported by the
///   direct Oxc backend subset.
/// - `None` when any child expression is unsupported.
///
/// Transformation:
/// - Builds a JavaScript `(callee)(arg1, arg2, ...)` call and keeps Terlan's
///   function-value invocation distinct from local named calls.
fn core_function_call_expr_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    callee: &crate::terlan_typeck::CoreExpr,
    args: &[crate::terlan_typeck::CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let mut arguments = ast.vec();
    for arg in args {
        arguments.push(Argument::from(core_expr_to_oxc_expression(ast, arg)?));
    }
    Some(ast.expression_call(
        SPAN,
        core_expr_to_oxc_expression(ast, callee)?,
        oxc_ast::NONE,
        arguments,
        false,
    ))
}

/// Lowers a focused pipe-forward CoreIR expression into an Oxc call expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `left`: CoreIR expression supplying the piped first argument.
/// - `right`: CoreIR expression expected to be a local named call or a
///   function-value invocation.
///
/// Output:
/// - `Some(Expression)` for `left |> f(args...)` or `left |> f(args...)` in
///   the supported subset.
/// - `None` when the right side is not a supported call shape or any child
///   expression remains unsupported.
///
/// Transformation:
/// - Prepends the piped value to either named-call arguments or dedicated
///   function-value invocation arguments.
fn core_pipe_forward_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    left: &crate::terlan_typeck::CoreExpr,
    right: &crate::terlan_typeck::CoreExpr,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    match right {
        crate::terlan_typeck::CoreExpr::Call { function, args } => {
            if !is_direct_oxc_js_identifier(function) {
                return None;
            }
            let mut arguments = ast.vec();
            arguments.push(Argument::from(core_expr_to_oxc_expression(ast, left)?));
            for arg in args {
                arguments.push(Argument::from(core_expr_to_oxc_expression(ast, arg)?));
            }
            Some(ast.expression_call(
                SPAN,
                ast.expression_identifier(SPAN, oxc_ident_name(ast, function)),
                oxc_ast::NONE,
                arguments,
                false,
            ))
        }
        crate::terlan_typeck::CoreExpr::FunctionCall { callee, args } => {
            let mut arguments = ast.vec();
            arguments.push(Argument::from(core_expr_to_oxc_expression(ast, left)?));
            for arg in args {
                arguments.push(Argument::from(core_expr_to_oxc_expression(ast, arg)?));
            }
            Some(ast.expression_call(
                SPAN,
                core_expr_to_oxc_expression(ast, callee)?,
                oxc_ast::NONE,
                arguments,
                false,
            ))
        }
        _ => None,
    }
}

/// Lowers Terlan integer division into an Oxc `Math.trunc(left / right)` call.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `left`: CoreIR dividend expression.
/// - `right`: CoreIR divisor expression.
///
/// Output:
/// - `Some(Expression)` when both child expressions fit the direct Oxc subset.
/// - `None` when either child expression remains unsupported.
///
/// Transformation:
/// - Builds a JavaScript `Math.trunc(left / right)` call so Terlan `div`
///   preserves integer quotient semantics without lowering to floating-point
///   `/` directly.
fn core_integer_division_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    left: &crate::terlan_typeck::CoreExpr,
    right: &crate::terlan_typeck::CoreExpr,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;
    use oxc_syntax::operator::BinaryOperator;

    let callee = ast
        .member_expression_static(
            SPAN,
            ast.expression_identifier(SPAN, oxc_ident_name(ast, "Math")),
            ast.identifier_name(SPAN, "trunc"),
            false,
        )
        .into();
    let quotient = ast.expression_binary(
        SPAN,
        core_expr_to_oxc_expression(ast, left)?,
        BinaryOperator::Division,
        core_expr_to_oxc_expression(ast, right)?,
    );
    let mut args = ast.vec();
    args.push(Argument::from(quotient));
    Some(ast.expression_call(SPAN, callee, oxc_ast::NONE, args, false))
}

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
fn core_lam_expr_to_oxc_expression<'a>(
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
pub(super) fn core_lam_param_to_oxc_formal_parameter<'a>(
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
            Some(ast.binding_pattern_binding_identifier(SPAN, oxc_ident_name(ast, name.as_str())))
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

/// Lowers total CoreIR if clauses into an Oxc conditional expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `clauses`: CoreIR if clauses in source order.
///
/// Output:
/// - `Some(Expression)` when the final clause is literal `true` and every
///   condition/body expression is directly lowerable.
/// - `None` when the if expression lacks an explicit fallback or uses
///   unsupported child expressions.
///
/// Transformation:
/// - Uses the final `true -> body` clause as the alternate expression and folds
///   preceding clauses from right to left into nested Oxc conditional
///   expressions, preserving Terlan branch order without modeling no-match
///   runtime failure for partial if expressions.
fn core_if_clauses_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    clauses: &[crate::terlan_typeck::CoreIfClause],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;

    let (fallback, clauses) = clauses.split_last()?;
    if !core_expr_is_true_literal(&fallback.condition) {
        return None;
    }

    let mut expr = core_expr_to_oxc_expression(ast, &fallback.body)?;
    for clause in clauses.iter().rev() {
        expr = ast.expression_conditional(
            SPAN,
            core_expr_to_oxc_expression(ast, &clause.condition)?,
            core_expr_to_oxc_expression(ast, &clause.body)?,
            expr,
        );
    }
    Some(expr)
}

/// Checks whether a CoreIR expression is the boolean `true` literal.
///
/// Inputs:
/// - `expr`: CoreIR expression to classify.
///
/// Output:
/// - `true` when the expression is `CoreExpr::Atom("true")` or
///   `CoreExpr::Var("true")`.
///
/// Transformation:
/// - Recognizes the CoreIR representation currently produced for Terlan
///   boolean `true`, without treating arbitrary atom values as booleans.
fn core_expr_is_true_literal(expr: &CoreExpr) -> bool {
    matches!(expr, CoreExpr::Atom(value) | CoreExpr::Var(value) if value == "true")
}

/// Lowers a CoreIR object-like field into an Oxc object property.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `key`: CoreIR object-like field key, such as a map or record field name.
/// - `value`: CoreIR expression stored under the field key.
///
/// Output:
/// - `Some(ObjectPropertyKind)` when the key fits the direct JavaScript
///   identifier subset and the value expression is directly lowerable.
/// - `None` when either the key or value requires a backend policy outside the
///   current direct-AST subset.
///
/// Transformation:
/// - Preserves the CoreIR field as a JavaScript static object property and
///   recursively lowers the value through the direct Oxc expression path.
fn core_object_field_to_oxc_property<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    key: &str,
    value: &CoreExpr,
) -> Option<oxc_ast::ast::ObjectPropertyKind<'a>> {
    use oxc_ast::ast::PropertyKind;
    use oxc_span::SPAN;

    if !is_direct_oxc_js_identifier(key) {
        return None;
    }

    Some(ast.object_property_kind_object_property(
        SPAN,
        PropertyKind::Init,
        ast.property_key_static_identifier(SPAN, oxc_ident_name(ast, key)),
        core_expr_to_oxc_expression(ast, value)?,
        false,
        false,
        false,
    ))
}
