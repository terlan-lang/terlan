use std::collections::{BTreeMap, BTreeSet};

use terlan_typeck::{CoreExpr, CoreFunction, CoreModule};

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
            if !matches!(pattern, Some(terlan_typeck::CorePattern::Var(name)) if name == &param.name)
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

/// Computes the function set that direct JavaScript emission must include.
///
/// Inputs:
/// - `module`: CoreIR module whose public functions define the emitted module
///   surface.
///
/// Output:
/// - Set of function names reachable from public functions through local CoreIR
///   calls.
///
/// Transformation:
/// - Builds a name index, seeds traversal from public functions, recursively
///   follows `CoreExpr::Call` edges that target functions in the same module,
///   and ignores unused private functions so unsupported dead helpers do not
///   force the direct backend to fall back.
fn reachable_direct_function_names(module: &CoreModule) -> BTreeSet<String> {
    let functions_by_name = module
        .functions
        .iter()
        .map(|function| (function.name.as_str(), function))
        .collect::<BTreeMap<_, _>>();
    let mut reachable = BTreeSet::new();
    let mut pending = module
        .functions
        .iter()
        .filter(|function| function.public)
        .map(|function| function.name.clone())
        .collect::<Vec<_>>();

    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        let Some(function) = functions_by_name.get(name.as_str()) else {
            continue;
        };
        for clause in &function.clauses {
            if let Some(expr) = clause.body.core_expr.as_ref() {
                collect_core_expr_local_calls(expr, &functions_by_name, &mut pending);
            }
        }
    }

    reachable
}

/// Collects local function calls contained in a CoreIR expression.
///
/// Inputs:
/// - `expr`: CoreIR expression to inspect.
/// - `functions_by_name`: module-local function name index.
/// - `pending`: traversal stack receiving newly discovered local callees.
///
/// Output:
/// - Mutates `pending` with called local function names.
///
/// Transformation:
/// - Recursively walks expression children and adds local `CoreExpr::Call`
///   targets to the pending reachability stack. Remote calls and constructor
///   calls still have their argument expressions traversed but do not add local
///   function dependencies.
fn collect_core_expr_local_calls<'a>(
    expr: &CoreExpr,
    functions_by_name: &BTreeMap<&'a str, &'a CoreFunction>,
    pending: &mut Vec<String>,
) {
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                collect_core_expr_local_calls(item, functions_by_name, pending);
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            collect_core_expr_local_calls(head, functions_by_name, pending);
            collect_core_expr_local_calls(tail, functions_by_name, pending);
        }
        CoreExpr::ListComprehension {
            expr,
            source,
            guard,
            ..
        } => {
            collect_core_expr_local_calls(expr, functions_by_name, pending);
            collect_core_expr_local_calls(source, functions_by_name, pending);
            if let Some(guard) = guard.as_ref() {
                collect_core_expr_local_calls(guard, functions_by_name, pending);
            }
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                collect_core_expr_local_calls(&field.value, functions_by_name, pending);
            }
        }
        CoreExpr::RecordConstruct { fields, .. }
        | CoreExpr::RecordUpdate { fields, .. }
        | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                collect_core_expr_local_calls(&field.value, functions_by_name, pending);
            }
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            collect_core_expr_local_calls(base, functions_by_name, pending);
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
            collect_core_expr_local_calls(record, functions_by_name, pending);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
        }
        CoreExpr::Call { function, args } => {
            if functions_by_name.contains_key(function.as_str()) {
                pending.push(function.clone());
            }
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            collect_core_expr_local_calls(receiver, functions_by_name, pending);
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            collect_core_expr_local_calls(callee, functions_by_name, pending);
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            collect_core_expr_local_calls(scrutinee, functions_by_name, pending);
            for clause in clauses {
                if let Some(guard) = clause.guard.as_ref() {
                    collect_core_expr_local_calls(guard, functions_by_name, pending);
                }
                collect_core_expr_local_calls(&clause.body, functions_by_name, pending);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            collect_core_expr_local_calls(body, functions_by_name, pending);
            for clause in of_clauses.iter().chain(catch_clauses.iter()) {
                if let Some(guard) = clause.guard.as_ref() {
                    collect_core_expr_local_calls(guard, functions_by_name, pending);
                }
                collect_core_expr_local_calls(&clause.body, functions_by_name, pending);
            }
            if let Some(after_clause) = after_clause.as_ref() {
                collect_core_expr_local_calls(&after_clause.trigger, functions_by_name, pending);
                collect_core_expr_local_calls(&after_clause.body, functions_by_name, pending);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                collect_core_expr_local_calls(&clause.condition, functions_by_name, pending);
                collect_core_expr_local_calls(&clause.body, functions_by_name, pending);
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                collect_core_expr_local_calls(&binding.value, functions_by_name, pending);
            }
            collect_core_expr_local_calls(body, functions_by_name, pending);
        }
        CoreExpr::Lam { body, .. } | CoreExpr::UnaryOp { operand: body, .. } => {
            collect_core_expr_local_calls(body, functions_by_name, pending);
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            collect_core_expr_local_calls(left, functions_by_name, pending);
            collect_core_expr_local_calls(right, functions_by_name, pending);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
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
fn core_expr_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    expr: &terlan_typeck::CoreExpr,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::ArrayExpressionElement;
    use oxc_span::SPAN;
    use oxc_syntax::number::NumberBase;

    match expr {
        terlan_typeck::CoreExpr::Int(value) if *value >= 0 && is_js_safe_integer(*value) => {
            Some(ast.expression_numeric_literal(SPAN, *value as f64, None, NumberBase::Decimal))
        }
        terlan_typeck::CoreExpr::Float(value) => Some(ast.expression_numeric_literal(
            SPAN,
            core_float_literal_to_oxc_number(value)?,
            None,
            NumberBase::Decimal,
        )),
        terlan_typeck::CoreExpr::Atom(value) | terlan_typeck::CoreExpr::Var(value)
            if value == "true" || value == "false" =>
        {
            Some(ast.expression_boolean_literal(SPAN, value == "true"))
        }
        terlan_typeck::CoreExpr::Binary(value) | terlan_typeck::CoreExpr::Atom(value) => {
            Some(ast.expression_string_literal(SPAN, oxc_string_value(ast, value.as_str()), None))
        }
        terlan_typeck::CoreExpr::Var(name) if is_direct_oxc_js_identifier(name) => {
            Some(ast.expression_identifier(SPAN, oxc_ident_name(ast, name.as_str())))
        }
        terlan_typeck::CoreExpr::Tuple(items)
        | terlan_typeck::CoreExpr::List(items)
        | terlan_typeck::CoreExpr::FixedArray(items) => {
            let mut elements = ast.vec();
            for item in items {
                elements.push(ArrayExpressionElement::from(core_expr_to_oxc_expression(
                    ast, item,
                )?));
            }
            Some(ast.expression_array(SPAN, elements))
        }
        terlan_typeck::CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => {
            core_list_comprehension_to_oxc_expression(ast, expr, pattern, source, guard.as_deref())
        }
        terlan_typeck::CoreExpr::ListCons { head, tail } => {
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
        terlan_typeck::CoreExpr::Index { base, index } => Some(
            ast.member_expression_computed(
                SPAN,
                core_expr_to_oxc_expression(ast, base)?,
                core_expr_to_oxc_expression(ast, index)?,
                false,
            )
            .into(),
        ),
        terlan_typeck::CoreExpr::FieldAccess { base, field }
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
        terlan_typeck::CoreExpr::RecordAccess { base, field, .. }
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
        terlan_typeck::CoreExpr::Map(fields) => {
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
        terlan_typeck::CoreExpr::RecordConstruct { fields, .. }
        | terlan_typeck::CoreExpr::TemplateInstantiate { fields, .. } => {
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
        terlan_typeck::CoreExpr::RecordUpdate { base, fields, .. } => {
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
        terlan_typeck::CoreExpr::Case { scrutinee, clauses } => {
            core_case_clauses_to_oxc_expression(ast, scrutinee, clauses)
        }
        terlan_typeck::CoreExpr::If { clauses } => core_if_clauses_to_oxc_expression(ast, clauses),
        terlan_typeck::CoreExpr::Lam { params, body } => {
            core_lam_expr_to_oxc_expression(ast, params, body)
        }
        terlan_typeck::CoreExpr::UnaryOp { operator, operand } => Some(ast.expression_unary(
            SPAN,
            core_unary_operator_to_oxc(operator)?,
            core_expr_to_oxc_expression(ast, operand)?,
        )),
        terlan_typeck::CoreExpr::Call { function, args }
            if is_direct_oxc_js_identifier(function) =>
        {
            core_call_expr_to_oxc_expression(ast, function, args)
        }
        terlan_typeck::CoreExpr::FunctionCall { callee, args } => {
            core_function_call_expr_to_oxc_expression(ast, callee, args)
        }
        terlan_typeck::CoreExpr::Intrinsic(call) => {
            core_intrinsic_call_expr_to_oxc_expression(ast, call)
        }
        terlan_typeck::CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if operator == "|>" => core_pipe_forward_to_oxc_expression(ast, left, right),
        terlan_typeck::CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if operator == "div" => core_integer_division_to_oxc_expression(ast, left, right),
        terlan_typeck::CoreExpr::BinaryOp {
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
    call: &terlan_typeck::CoreIntrinsicCall,
) -> Option<oxc_ast::ast::Expression<'a>> {
    match &call.id {
        terlan_typeck::CoreIntrinsicId::Primitive(
            terlan_typeck::CorePrimitiveIntrinsic::StringContains,
        ) => core_string_contains_intrinsic_to_oxc_expression(ast, call.args.as_slice()),
        terlan_typeck::CoreIntrinsicId::Primitive(
            terlan_typeck::CorePrimitiveIntrinsic::StringStartsWith,
        ) => core_string_starts_with_intrinsic_to_oxc_expression(ast, call.args.as_slice()),
        terlan_typeck::CoreIntrinsicId::Primitive(
            terlan_typeck::CorePrimitiveIntrinsic::StringLength,
        ) => core_string_length_intrinsic_to_oxc_expression(ast, call.args.as_slice()),
        _ => None,
    }
}

/// Lowers `core.string.contains` into a JavaScript `.includes(...)` call.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `args`: CoreIR intrinsic arguments in `(value, pattern)` order.
///
/// Output:
/// - `Some(Expression)` for a JavaScript `value.includes(pattern)` call.
/// - `None` when the intrinsic has the wrong arity or unsupported arguments.
///
/// Transformation:
/// - Converts the backend-neutral string containment intrinsic into the
///   JavaScript string API selected for the JS/Oxc neutrality probe.
fn core_string_contains_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[terlan_typeck::CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let [value, pattern] = args else {
        return None;
    };
    let callee = ast
        .member_expression_static(
            SPAN,
            core_expr_to_oxc_expression(ast, value)?,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "includes")),
            false,
        )
        .into();
    let arguments = ast.vec1(Argument::from(core_expr_to_oxc_expression(ast, pattern)?));
    Some(ast.expression_call(SPAN, callee, oxc_ast::NONE, arguments, false))
}

/// Lowers `core.string.starts_with` into a JavaScript `.startsWith(...)` call.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `args`: CoreIR intrinsic arguments in `(value, prefix)` order.
///
/// Output:
/// - `Some(Expression)` for a JavaScript `value.startsWith(prefix)` call.
/// - `None` when the intrinsic has the wrong arity or unsupported arguments.
///
/// Transformation:
/// - Converts the backend-neutral string-prefix intrinsic into the JavaScript
///   string API selected for the JS/Oxc neutrality probe.
fn core_string_starts_with_intrinsic_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    args: &[terlan_typeck::CoreExpr],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let [value, prefix] = args else {
        return None;
    };
    let callee = ast
        .member_expression_static(
            SPAN,
            core_expr_to_oxc_expression(ast, value)?,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "startsWith")),
            false,
        )
        .into();
    let arguments = ast.vec1(Argument::from(core_expr_to_oxc_expression(ast, prefix)?));
    Some(ast.expression_call(SPAN, callee, oxc_ast::NONE, arguments, false))
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
    args: &[terlan_typeck::CoreExpr],
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
    args: &[terlan_typeck::CoreExpr],
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
    callee: &terlan_typeck::CoreExpr,
    args: &[terlan_typeck::CoreExpr],
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
/// - `Some(Expression)` for `left |> f(args...)` or `left |> f.(args...)` in
///   the supported subset.
/// - `None` when the right side is not a supported call shape or any child
///   expression remains unsupported.
///
/// Transformation:
/// - Prepends the piped value to either named-call arguments or dedicated
///   function-value invocation arguments.
fn core_pipe_forward_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    left: &terlan_typeck::CoreExpr,
    right: &terlan_typeck::CoreExpr,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    match right {
        terlan_typeck::CoreExpr::Call { function, args } => {
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
        terlan_typeck::CoreExpr::FunctionCall { callee, args } => {
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
    left: &terlan_typeck::CoreExpr,
    right: &terlan_typeck::CoreExpr,
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

/// Lowers a simple CoreIR list comprehension into an Oxc `.map(...)` call.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `expr`: yielded CoreIR expression.
/// - `pattern`: generator pattern bound for each source element.
/// - `source`: CoreIR source-list expression.
/// - `guard`: optional CoreIR guard expression.
///
/// Output:
/// - `Some(Expression)` for a single-generator, variable-pattern, unguarded
///   list comprehension whose yield/source expressions are directly lowerable.
/// - `None` for guarded comprehensions, destructuring patterns, unsupported
///   parameter names, or unsupported yield/source expressions.
///
/// Transformation:
/// - Converts `[yield | value <- source]` into `source.map((value) => yield)`.
///   This preserves the current list-valued artifact shape without introducing
///   filter semantics or pattern dispatch.
fn core_list_comprehension_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    expr: &terlan_typeck::CoreExpr,
    pattern: &terlan_typeck::CorePattern,
    source: &terlan_typeck::CoreExpr,
    guard: Option<&terlan_typeck::CoreExpr>,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    if guard.is_some() {
        return None;
    }

    let callee = ast
        .member_expression_static(
            SPAN,
            core_expr_to_oxc_expression(ast, source)?,
            ast.identifier_name(SPAN, "map"),
            false,
        )
        .into();
    let mut args = ast.vec();
    args.push(Argument::from(core_lam_expr_to_oxc_expression(
        ast,
        std::slice::from_ref(pattern),
        expr,
    )?));
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
///   callable-value invocation is handled by the dedicated `f.(args)` syntax.
fn core_lam_expr_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    params: &[terlan_typeck::CorePattern],
    body: &terlan_typeck::CoreExpr,
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
/// - `Some(FormalParameter)` for direct variable patterns.
/// - `None` for every non-variable pattern or unsupported identifier.
///
/// Transformation:
/// - Keeps this JS slice limited to non-destructuring anonymous functions and
///   reuses the direct Oxc backend's conservative JavaScript identifier policy.
fn core_lam_param_to_oxc_formal_parameter<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    param: &terlan_typeck::CorePattern,
) -> Option<oxc_ast::ast::FormalParameter<'a>> {
    use oxc_span::SPAN;

    let terlan_typeck::CorePattern::Var(name) = param else {
        return None;
    };
    if !is_direct_oxc_js_identifier(name) {
        return None;
    }
    Some(ast.formal_parameter(
        SPAN,
        ast.vec(),
        ast.binding_pattern_binding_identifier(SPAN, oxc_ident_name(ast, name.as_str())),
        oxc_ast::NONE,
        oxc_ast::NONE,
        false,
        None,
        false,
        false,
    ))
}

/// Lowers total literal-pattern CoreIR case clauses into an Oxc conditional expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `scrutinee`: CoreIR expression being matched.
/// - `clauses`: CoreIR case clauses in source order.
///
/// Output:
/// - `Some(Expression)` when the scrutinee is a direct variable, every
///   non-final clause is an unguarded atom, integer, or finite-float literal
///   pattern, the final clause is an unguarded wildcard fallback, and all
///   branch bodies are directly lowerable.
/// - `None` for guarded, partial, binding, destructuring, or otherwise
///   unsupported case expressions.
///
/// Transformation:
/// - Uses the final wildcard branch as the alternate expression and folds
///   preceding literal clauses from right to left into nested Oxc conditional
///   expressions. The scrutinee is restricted to a variable so this
///   direct AST path does not introduce repeated evaluation semantics.
fn core_case_clauses_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee: &terlan_typeck::CoreExpr,
    clauses: &[terlan_typeck::CoreCaseClause],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;

    let terlan_typeck::CoreExpr::Var(scrutinee_name) = scrutinee else {
        return None;
    };
    if !is_direct_oxc_js_identifier(scrutinee_name) {
        return None;
    }

    let (fallback, clauses) = clauses.split_last()?;
    if fallback.guard.is_some() || !matches!(fallback.pattern, terlan_typeck::CorePattern::Wildcard)
    {
        return None;
    }

    let mut expr = core_expr_to_oxc_expression(ast, &fallback.body)?;
    for clause in clauses.iter().rev() {
        if clause.guard.is_some() {
            return None;
        }
        expr = ast.expression_conditional(
            SPAN,
            core_case_literal_pattern_test_to_oxc_expression(ast, scrutinee_name, &clause.pattern)?,
            core_expr_to_oxc_expression(ast, &clause.body)?,
            expr,
        );
    }
    Some(expr)
}

/// Builds an Oxc strict-equality test for one supported CoreIR case pattern.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `scrutinee_name`: already-validated JavaScript identifier holding the case
///   scrutinee value.
/// - `pattern`: CoreIR pattern from a non-final case clause.
///
/// Output:
/// - `Some(Expression)` for atom, integer, and finite-float literal patterns.
/// - `None` for every other pattern shape.
///
/// Transformation:
/// - Reconstructs the scrutinee identifier for each comparison and compares it
///   with the same atom artifact value used by expression lowering or a
///   JavaScript numeric literal for numeric patterns.
fn core_case_literal_pattern_test_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    pattern: &terlan_typeck::CorePattern,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;
    use oxc_syntax::number::NumberBase;
    use oxc_syntax::operator::BinaryOperator;

    let literal = match pattern {
        terlan_typeck::CorePattern::Atom(value) => {
            core_atom_artifact_to_oxc_expression(ast, value)?
        }
        terlan_typeck::CorePattern::Int(value) if *value >= 0 && is_js_safe_integer(*value) => {
            ast.expression_numeric_literal(SPAN, *value as f64, None, NumberBase::Decimal)
        }
        terlan_typeck::CorePattern::Float(value) => ast.expression_numeric_literal(
            SPAN,
            core_float_literal_to_oxc_number(value)?,
            None,
            NumberBase::Decimal,
        ),
        _ => return None,
    };
    Some(ast.expression_binary(
        SPAN,
        ast.expression_identifier(SPAN, oxc_ident_name(ast, scrutinee_name)),
        BinaryOperator::StrictEquality,
        literal,
    ))
}

/// Lowers a CoreIR atom artifact into an Oxc expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `value`: CoreIR atom payload without Terlan's source-level `:` prefix.
///
/// Output:
/// - Oxc boolean literal for `true` and `false`.
/// - Oxc string literal for every other atom artifact.
///
/// Transformation:
/// - Mirrors `core_expr_to_oxc_expression` atom handling so atom patterns and
///   atom expressions compare against the same JavaScript artifact values.
fn core_atom_artifact_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    value: &str,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;

    if value == "true" || value == "false" {
        Some(ast.expression_boolean_literal(SPAN, value == "true"))
    } else {
        Some(ast.expression_string_literal(SPAN, oxc_string_value(ast, value), None))
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
    clauses: &[terlan_typeck::CoreIfClause],
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

/// Checks whether an integer can be represented exactly as a JavaScript number.
///
/// Inputs:
/// - `value`: CoreIR integer value under consideration for direct JS numeric
///   literal emission.
///
/// Output:
/// - `true` when the integer is within JavaScript's exact safe-integer range.
///
/// Transformation:
/// - Compares the integer to the ECMAScript `Number.MAX_SAFE_INTEGER` bound so
///   wider integer handling can fall back until the backend has a deliberate
///   bigint or runtime-number policy.
fn is_js_safe_integer(value: i64) -> bool {
    value <= 9_007_199_254_740_991
}

/// Converts a CoreIR float payload into an Oxc numeric literal value.
///
/// Inputs:
/// - `value`: float payload text captured in CoreIR.
///
/// Output:
/// - Finite `f64` value for Oxc numeric-literal construction.
/// - `None` when the payload cannot be represented as a finite JavaScript
///   number.
///
/// Transformation:
/// - Parses the canonical CoreIR float payload and rejects infinities/NaN so
///   the direct backend does not invent target-specific runtime-number policy.
fn core_float_literal_to_oxc_number(value: &str) -> Option<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

/// Copies a source identifier name into Oxc's AST arena.
///
/// Inputs:
/// - `ast`: Oxc AST builder that owns the destination allocator.
/// - `name`: CoreIR identifier text borrowed from compiler data structures.
///
/// Output:
/// - Identifier text with the same arena lifetime as the Oxc AST being built.
///
/// Transformation:
/// - Allocates the identifier bytes in Oxc's arena so generated AST nodes do
///   not borrow from the shorter-lived CoreIR module.
fn oxc_ident_name<'a>(ast: oxc_ast::AstBuilder<'a>, name: &str) -> &'a str {
    ast.allocator.alloc_str(name)
}

/// Copies a runtime string literal value into Oxc's AST arena.
///
/// Inputs:
/// - `ast`: Oxc AST builder that owns the destination allocator.
/// - `value`: CoreIR string-like literal payload, which may still include
///   source-level surrounding quotes.
///
/// Output:
/// - String value with the same arena lifetime as the Oxc AST being built.
///
/// Transformation:
/// - Normalizes quoted CoreIR binary payloads to runtime string content, then
///   allocates the literal payload in Oxc's arena before creating a
///   `StringLiteral` node.
fn oxc_string_value<'a>(ast: oxc_ast::AstBuilder<'a>, value: &str) -> &'a str {
    let value = core_string_runtime_value(value);
    ast.allocator.alloc_str(value.as_str())
}

/// Normalizes a CoreIR string-like payload into a runtime string value.
///
/// Inputs:
/// - `value`: CoreIR string-like payload from `CoreExpr::Binary` or
///   `CoreExpr::Atom`.
///
/// Output:
/// - Runtime string content for quoted source literals, or the original payload
///   for atom/unquoted values.
///
/// Transformation:
/// - Parses JSON-compatible quoted source string payloads so Oxc string
///   literals do not preserve Terlan source delimiters as runtime characters.
fn core_string_runtime_value(value: &str) -> String {
    if value.starts_with('"') && value.ends_with('"') {
        serde_json::from_str::<String>(value)
            .unwrap_or_else(|_| value.trim_matches('"').to_string())
    } else {
        value.to_string()
    }
}

/// Maps a CoreIR unary operator to an Oxc unary operator.
///
/// Inputs:
/// - `operator`: CoreIR operator spelling.
///
/// Output:
/// - Oxc unary operator for the supported direct backend subset, or `None`
///   otherwise.
///
/// Transformation:
/// - Preserves Terlan unary minus as JavaScript unary negation and rejects all
///   other unary spellings until their semantics are selected explicitly.
fn core_unary_operator_to_oxc(operator: &str) -> Option<oxc_syntax::operator::UnaryOperator> {
    use oxc_syntax::operator::UnaryOperator;

    match operator {
        "-" => Some(UnaryOperator::UnaryNegation),
        _ => None,
    }
}

/// Checks whether a CoreIR name is safe for direct Oxc identifier emission.
///
/// Inputs:
/// - `name`: function, parameter, or variable name from CoreIR.
///
/// Output:
/// - `true` when the name can be represented as a JavaScript identifier in the
///   current direct-AST subset.
///
/// Transformation:
/// - Applies the same conservative ASCII identifier subset used by bootstrap
///   string lowering so unsupported names can fall back instead of producing a
///   malformed JavaScript AST.
fn is_direct_oxc_js_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

/// Maps a CoreIR binary operator to an Oxc binary operator.
///
/// Inputs:
/// - `operator`: CoreIR operator spelling.
///
/// Output:
/// - Oxc binary operator for the supported smoke subset, or `None` otherwise.
///
/// Transformation:
/// - Converts Terlan equality to JavaScript strict equality and preserves
///   arithmetic/comparison operators with equivalent JavaScript semantics.
fn core_binary_operator_to_oxc(operator: &str) -> Option<oxc_syntax::operator::BinaryOperator> {
    use oxc_syntax::operator::BinaryOperator;

    match operator {
        "+" => Some(BinaryOperator::Addition),
        "-" => Some(BinaryOperator::Subtraction),
        "*" => Some(BinaryOperator::Multiplication),
        "/" => Some(BinaryOperator::Division),
        "rem" => Some(BinaryOperator::Remainder),
        "==" => Some(BinaryOperator::StrictEquality),
        "=:=" => Some(BinaryOperator::StrictEquality),
        "!=" | "/=" => Some(BinaryOperator::StrictInequality),
        "=/=" => Some(BinaryOperator::StrictInequality),
        "<" => Some(BinaryOperator::LessThan),
        "<=" => Some(BinaryOperator::LessEqualThan),
        ">" => Some(BinaryOperator::GreaterThan),
        ">=" => Some(BinaryOperator::GreaterEqualThan),
        _ => None,
    }
}
