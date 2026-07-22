use super::{core_expr_to_oxc_expression, core_lam_expr_to_oxc_expression};
use crate::commands::emit_js::direct_helpers::{
    core_float_literal_to_oxc_number, is_direct_oxc_js_identifier, is_js_safe_integer,
    oxc_ident_name, oxc_string_value,
};

#[derive(Clone)]
enum CasePathSegment {
    Index(usize),
    Field(String),
}

#[derive(Clone, Copy)]
struct CaseObjectField<'a> {
    key: &'a str,
    required: bool,
    value: &'a crate::terlan_typeck::CorePattern,
}

/// Lowers total supported-pattern CoreIR case clauses into an Oxc conditional expression.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `scrutinee`: CoreIR expression being matched.
/// - `clauses`: CoreIR case clauses in source order.
///
/// Output:
/// - `Some(Expression)` when the scrutinee is a direct variable, every
///   non-final clause is an unguarded supported literal or exact tuple/list
///   destructuring pattern, the final clause is an unguarded wildcard
///   fallback, and all branch bodies are directly lowerable.
/// - `None` for guarded, partial, or otherwise unsupported case expressions.
///
/// Transformation:
/// - Uses the final wildcard branch as the alternate expression and folds
///   preceding clauses from right to left into nested Oxc conditional
///   expressions. Tuple/list branches first prove array shape and exact length,
///   then invoke a destructuring arrow closure for branch-local bindings. The
///   scrutinee is restricted to a variable so this direct AST path does not
///   introduce repeated evaluation semantics.
pub(super) fn core_case_clauses_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee: &crate::terlan_typeck::CoreExpr,
    clauses: &[crate::terlan_typeck::CoreCaseClause],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;

    let crate::terlan_typeck::CoreExpr::Var(scrutinee_name) = scrutinee else {
        return None;
    };
    if !is_direct_oxc_js_identifier(scrutinee_name) {
        return None;
    }

    let (fallback, clauses) = clauses.split_last()?;
    if fallback.guard.is_some()
        || !matches!(
            fallback.pattern,
            crate::terlan_typeck::CorePattern::Wildcard
        )
    {
        return None;
    }

    let mut expr = core_expr_to_oxc_expression(ast, &fallback.body)?;
    for clause in clauses.iter().rev() {
        let consequent = if core_case_pattern_needs_destructuring(&clause.pattern) {
            core_case_destructuring_branch_to_oxc_expression(
                ast,
                scrutinee_name,
                &clause.pattern,
                &clause.body,
            )?
        } else {
            core_expr_to_oxc_expression(ast, &clause.body)?
        };
        let mut test =
            core_case_pattern_test_to_oxc_expression(ast, scrutinee_name, &clause.pattern)?;
        if let Some(guard) = &clause.guard {
            use oxc_span::SPAN;
            use oxc_syntax::operator::LogicalOperator;

            let guard = if core_case_pattern_needs_destructuring(&clause.pattern) {
                core_case_destructuring_branch_to_oxc_expression(
                    ast,
                    scrutinee_name,
                    &clause.pattern,
                    guard,
                )?
            } else {
                core_expr_to_oxc_expression(ast, guard)?
            };
            test = ast.expression_logical(SPAN, test, LogicalOperator::And, guard);
        }
        expr = ast.expression_conditional(SPAN, test, consequent, expr);
    }
    Some(expr)
}

/// Reports whether a structural case pattern needs a binding closure.
fn core_case_pattern_needs_destructuring(pattern: &crate::terlan_typeck::CorePattern) -> bool {
    match pattern {
        crate::terlan_typeck::CorePattern::Tuple(_)
        | crate::terlan_typeck::CorePattern::List(_)
        | crate::terlan_typeck::CorePattern::Map(_)
        | crate::terlan_typeck::CorePattern::Record { .. } => true,
        crate::terlan_typeck::CorePattern::Constructor { args, .. } => !args.is_empty(),
        _ => false,
    }
}

/// Builds the JavaScript match test for one supported CoreIR case pattern.
///
/// Inputs:
/// - `ast`: Oxc AST builder tied to the destination allocator.
/// - `scrutinee_name`: validated JavaScript identifier holding the case value.
/// - `pattern`: non-final CoreIR case pattern.
///
/// Output:
/// - Literal equality, recursive tuple/list array tests, or safe object tests.
/// - `None` for unsupported patterns.
///
/// Transformation:
/// - Reuses literal equality lowering and adds structural tests for tuple/list
///   patterns containing variables, wildcards, or nested tuple/list patterns.
pub(super) fn core_case_pattern_test_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    pattern: &crate::terlan_typeck::CorePattern,
) -> Option<oxc_ast::ast::Expression<'a>> {
    core_case_pattern_test_at_path(ast, scrutinee_name, &[], pattern)
}

/// Builds one supported pattern test at a path within the case scrutinee.
fn core_case_pattern_test_at_path<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    path: &[CasePathSegment],
    pattern: &crate::terlan_typeck::CorePattern,
) -> Option<oxc_ast::ast::Expression<'a>> {
    match pattern {
        crate::terlan_typeck::CorePattern::Tuple(_)
        | crate::terlan_typeck::CorePattern::List(_) => {
            core_case_sequence_pattern_test_to_oxc_expression(ast, scrutinee_name, path, pattern)
        }
        crate::terlan_typeck::CorePattern::Map(fields) => {
            core_case_map_pattern_test_to_oxc_expression(ast, scrutinee_name, path, fields)
        }
        crate::terlan_typeck::CorePattern::Record { fields, .. } => {
            core_case_record_pattern_test_to_oxc_expression(ast, scrutinee_name, path, fields)
        }
        crate::terlan_typeck::CorePattern::Constructor { name, args, .. } => {
            core_case_constructor_pattern_test_to_oxc_expression(
                ast,
                scrutinee_name,
                path,
                name,
                args,
            )
        }
        _ => core_case_literal_pattern_test_to_oxc_expression(ast, scrutinee_name, path, pattern),
    }
}

/// Builds an exact recursive array-shape test for a tuple/list case pattern.
fn core_case_sequence_pattern_test_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    path: &[CasePathSegment],
    pattern: &crate::terlan_typeck::CorePattern,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;
    use oxc_syntax::number::NumberBase;
    use oxc_syntax::operator::{BinaryOperator, LogicalOperator};

    let items = match pattern {
        crate::terlan_typeck::CorePattern::Tuple(items)
        | crate::terlan_typeck::CorePattern::List(items) => items,
        _ => return None,
    };
    let is_array = core_case_array_is_array_to_oxc_expression(
        ast,
        core_case_scrutinee_path_to_oxc_expression(ast, scrutinee_name, path),
    );
    let length = ast
        .member_expression_static(
            SPAN,
            core_case_scrutinee_path_to_oxc_expression(ast, scrutinee_name, path),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "length")),
            false,
        )
        .into();
    let expected_length =
        ast.expression_numeric_literal(SPAN, items.len() as f64, None, NumberBase::Decimal);
    let mut test = ast.expression_logical(
        SPAN,
        is_array,
        LogicalOperator::And,
        ast.expression_binary(
            SPAN,
            length,
            BinaryOperator::StrictEquality,
            expected_length,
        ),
    );

    for (index, item) in items.iter().enumerate() {
        if matches!(
            item,
            crate::terlan_typeck::CorePattern::Var(_) | crate::terlan_typeck::CorePattern::Wildcard
        ) {
            continue;
        }
        let mut child_path = path.to_vec();
        child_path.push(CasePathSegment::Index(index));
        let child_test = core_case_pattern_test_at_path(ast, scrutinee_name, &child_path, item)?;
        test = ast.expression_logical(SPAN, test, LogicalOperator::And, child_test);
    }
    Some(test)
}

/// Builds an exact tagged-tuple test for a CoreIR constructor pattern.
fn core_case_constructor_pattern_test_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    path: &[CasePathSegment],
    name: &str,
    args: &[crate::terlan_typeck::CorePattern],
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;
    use oxc_syntax::number::NumberBase;
    use oxc_syntax::operator::{BinaryOperator, LogicalOperator};

    let tag = core_constructor_pattern_tag(name);
    if args.is_empty() {
        return Some(ast.expression_binary(
            SPAN,
            core_case_scrutinee_path_to_oxc_expression(ast, scrutinee_name, path),
            BinaryOperator::StrictEquality,
            ast.expression_string_literal(SPAN, oxc_string_value(ast, &tag), None),
        ));
    }

    let value = || core_case_scrutinee_path_to_oxc_expression(ast, scrutinee_name, path);
    let is_array = core_case_array_is_array_to_oxc_expression(ast, value());
    let length = ast
        .member_expression_static(
            SPAN,
            value(),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "length")),
            false,
        )
        .into();
    let expected_length =
        ast.expression_numeric_literal(SPAN, (args.len() + 1) as f64, None, NumberBase::Decimal);
    let mut test = ast.expression_logical(
        SPAN,
        is_array,
        LogicalOperator::And,
        ast.expression_binary(
            SPAN,
            length,
            BinaryOperator::StrictEquality,
            expected_length,
        ),
    );

    let mut tag_path = path.to_vec();
    tag_path.push(CasePathSegment::Index(0));
    let tag_test = ast.expression_binary(
        SPAN,
        core_case_scrutinee_path_to_oxc_expression(ast, scrutinee_name, &tag_path),
        BinaryOperator::StrictEquality,
        ast.expression_string_literal(SPAN, oxc_string_value(ast, &tag), None),
    );
    test = ast.expression_logical(SPAN, test, LogicalOperator::And, tag_test);

    for (index, arg) in args.iter().enumerate() {
        if matches!(
            arg,
            crate::terlan_typeck::CorePattern::Var(_) | crate::terlan_typeck::CorePattern::Wildcard
        ) {
            continue;
        }
        let mut child_path = path.to_vec();
        child_path.push(CasePathSegment::Index(index + 1));
        let child_test = core_case_pattern_test_at_path(ast, scrutinee_name, &child_path, arg)?;
        test = ast.expression_logical(SPAN, test, LogicalOperator::And, child_test);
    }
    Some(test)
}

/// Returns the VM tagged-tuple atom used by a constructor pattern.
fn core_constructor_pattern_tag(name: &str) -> String {
    let name = name.rsplit('.').next().unwrap_or(name);
    if name == "Err" {
        "error".to_string()
    } else {
        crate::terlan_syntax::type_name_to_atom_payload(name)
    }
}

/// Builds a required-own-field structural test for a CoreIR map pattern.
fn core_case_map_pattern_test_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    path: &[CasePathSegment],
    fields: &[crate::terlan_typeck::CoreMapPatternField],
) -> Option<oxc_ast::ast::Expression<'a>> {
    core_case_object_pattern_test_to_oxc_expression(
        ast,
        scrutinee_name,
        path,
        fields.iter().map(|field| CaseObjectField {
            key: field.key.as_str(),
            required: field.required,
            value: &field.value,
        }),
    )
}

/// Builds a required-own-field structural test for a CoreIR record pattern.
fn core_case_record_pattern_test_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    path: &[CasePathSegment],
    fields: &[crate::terlan_typeck::CoreRecordPatternField],
) -> Option<oxc_ast::ast::Expression<'a>> {
    core_case_object_pattern_test_to_oxc_expression(
        ast,
        scrutinee_name,
        path,
        fields.iter().map(|field| CaseObjectField {
            key: field.key.as_str(),
            required: field.required,
            value: &field.value,
        }),
    )
}

fn core_case_object_pattern_test_to_oxc_expression<'a, 'field>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    path: &[CasePathSegment],
    fields: impl IntoIterator<Item = CaseObjectField<'field>>,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;
    use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator};

    let fields = fields.into_iter().collect::<Vec<_>>();
    if fields.iter().any(|field| !field.required) {
        return None;
    }

    let value = || core_case_scrutinee_path_to_oxc_expression(ast, scrutinee_name, path);
    let is_object = ast.expression_binary(
        SPAN,
        ast.expression_unary(SPAN, UnaryOperator::Typeof, value()),
        BinaryOperator::StrictEquality,
        ast.expression_string_literal(SPAN, oxc_string_value(ast, "object"), None),
    );
    let is_not_null = ast.expression_binary(
        SPAN,
        value(),
        BinaryOperator::StrictInequality,
        ast.expression_null_literal(SPAN),
    );
    let is_not_array = ast.expression_unary(
        SPAN,
        UnaryOperator::LogicalNot,
        core_case_array_is_array_to_oxc_expression(ast, value()),
    );
    let mut test = ast.expression_logical(
        SPAN,
        ast.expression_logical(SPAN, is_object, LogicalOperator::And, is_not_null),
        LogicalOperator::And,
        is_not_array,
    );

    for field in fields {
        let has_field = core_case_has_own_field_to_oxc_expression(ast, value(), field.key);
        test = ast.expression_logical(SPAN, test, LogicalOperator::And, has_field);
        if matches!(
            field.value,
            crate::terlan_typeck::CorePattern::Var(_) | crate::terlan_typeck::CorePattern::Wildcard
        ) {
            continue;
        }
        let mut child_path = path.to_vec();
        child_path.push(CasePathSegment::Field(field.key.to_string()));
        let child_test =
            core_case_pattern_test_at_path(ast, scrutinee_name, &child_path, field.value)?;
        test = ast.expression_logical(SPAN, test, LogicalOperator::And, child_test);
    }
    Some(test)
}

/// Builds `Array.isArray(value)` for a structural case test.
fn core_case_array_is_array_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    value: oxc_ast::ast::Expression<'a>,
) -> oxc_ast::ast::Expression<'a> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let callee = ast
        .member_expression_static(
            SPAN,
            ast.expression_identifier(SPAN, oxc_ident_name(ast, "Array")),
            ast.identifier_name(SPAN, oxc_ident_name(ast, "isArray")),
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
}

/// Builds an own-property test without trusting a value's prototype.
fn core_case_has_own_field_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    value: oxc_ast::ast::Expression<'a>,
    field: &str,
) -> oxc_ast::ast::Expression<'a> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let object = ast.expression_identifier(SPAN, oxc_ident_name(ast, "Object"));
    let prototype = ast
        .member_expression_static(
            SPAN,
            object,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "prototype")),
            false,
        )
        .into();
    let has_own_property = ast
        .member_expression_static(
            SPAN,
            prototype,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "hasOwnProperty")),
            false,
        )
        .into();
    let call = ast
        .member_expression_static(
            SPAN,
            has_own_property,
            ast.identifier_name(SPAN, oxc_ident_name(ast, "call")),
            false,
        )
        .into();
    ast.expression_call(
        SPAN,
        call,
        oxc_ast::NONE,
        ast.vec_from_array([
            Argument::from(value),
            Argument::from(ast.expression_string_literal(SPAN, oxc_string_value(ast, field), None)),
        ]),
        false,
    )
}

/// Rebuilds a scrutinee member path for structural case tests.
fn core_case_scrutinee_path_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    path: &[CasePathSegment],
) -> oxc_ast::ast::Expression<'a> {
    use oxc_span::SPAN;
    use oxc_syntax::number::NumberBase;

    let mut expression = ast.expression_identifier(SPAN, oxc_ident_name(ast, scrutinee_name));
    for segment in path {
        let property = match segment {
            CasePathSegment::Index(index) => {
                ast.expression_numeric_literal(SPAN, *index as f64, None, NumberBase::Decimal)
            }
            CasePathSegment::Field(field) => {
                ast.expression_string_literal(SPAN, oxc_string_value(ast, field), None)
            }
        };
        expression = ast
            .member_expression_computed(SPAN, expression, property, false)
            .into();
    }
    expression
}

/// Builds a structural branch closure that scopes destructured bindings.
fn core_case_destructuring_branch_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    pattern: &crate::terlan_typeck::CorePattern,
    body: &crate::terlan_typeck::CoreExpr,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_ast::ast::Argument;
    use oxc_span::SPAN;

    let binding_pattern = core_case_binding_pattern(pattern)?;
    let callee =
        core_lam_expr_to_oxc_expression(ast, std::slice::from_ref(&binding_pattern), body)?;
    Some(ast.expression_call(
        SPAN,
        callee,
        oxc_ast::NONE,
        ast.vec1(Argument::from(ast.expression_identifier(
            SPAN,
            oxc_ident_name(ast, scrutinee_name),
        ))),
        false,
    ))
}

/// Projects a validated case pattern into a JavaScript binding-only pattern.
///
/// Literal children become wildcard holes because the surrounding structural
/// test has already established their values. This projection is deliberately
/// case-local so ordinary lambda parameters cannot discard literal semantics.
pub(super) fn core_case_binding_pattern(
    pattern: &crate::terlan_typeck::CorePattern,
) -> Option<crate::terlan_typeck::CorePattern> {
    use crate::terlan_typeck::CorePattern;

    match pattern {
        CorePattern::Var(name) => Some(CorePattern::Var(name.clone())),
        CorePattern::Wildcard
        | CorePattern::Atom(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::String(_) => Some(CorePattern::Wildcard),
        CorePattern::Tuple(items) => Some(CorePattern::Tuple(
            items
                .iter()
                .map(core_case_binding_pattern)
                .collect::<Option<Vec<_>>>()?,
        )),
        CorePattern::List(items) => Some(CorePattern::List(
            items
                .iter()
                .map(core_case_binding_pattern)
                .collect::<Option<Vec<_>>>()?,
        )),
        CorePattern::Map(fields) => {
            let mut bindings = Vec::new();
            for field in fields {
                let value = core_case_binding_pattern(&field.value)?;
                if core_case_pattern_binds_value(&value) {
                    bindings.push(crate::terlan_typeck::CoreMapPatternField {
                        key: field.key.clone(),
                        required: field.required,
                        value,
                    });
                }
            }
            Some(CorePattern::Map(bindings))
        }
        CorePattern::Record { name, fields } => {
            let mut bindings = Vec::new();
            for field in fields {
                let value = core_case_binding_pattern(&field.value)?;
                if core_case_pattern_binds_value(&value) {
                    bindings.push(crate::terlan_typeck::CoreRecordPatternField {
                        key: field.key.clone(),
                        required: field.required,
                        value,
                    });
                }
            }
            Some(CorePattern::Record {
                name: name.clone(),
                fields: bindings,
            })
        }
        CorePattern::Constructor { args, .. } if !args.is_empty() => {
            let mut bindings = Vec::with_capacity(args.len() + 1);
            bindings.push(CorePattern::Wildcard);
            bindings.extend(
                args.iter()
                    .map(core_case_binding_pattern)
                    .collect::<Option<Vec<_>>>()?,
            );
            Some(CorePattern::Tuple(bindings))
        }
        _ => None,
    }
}

/// Reports whether a projected case pattern contains a runtime binding.
fn core_case_pattern_binds_value(pattern: &crate::terlan_typeck::CorePattern) -> bool {
    use crate::terlan_typeck::CorePattern;

    match pattern {
        CorePattern::Var(_) => true,
        CorePattern::Tuple(items) | CorePattern::List(items) => {
            items.iter().any(core_case_pattern_binds_value)
        }
        CorePattern::Map(fields) => fields
            .iter()
            .any(|field| core_case_pattern_binds_value(&field.value)),
        CorePattern::Record { fields, .. } => fields
            .iter()
            .any(|field| core_case_pattern_binds_value(&field.value)),
        _ => false,
    }
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
/// - `Some(Expression)` for atom, string, integer, and finite-float literal
///   patterns.
/// - `None` for every other pattern shape.
///
/// Transformation:
/// - Reconstructs the scrutinee identifier for each comparison and compares it
///   with the same atom artifact value used by expression lowering or a
///   JavaScript numeric literal for numeric patterns.
fn core_case_literal_pattern_test_to_oxc_expression<'a>(
    ast: oxc_ast::AstBuilder<'a>,
    scrutinee_name: &str,
    path: &[CasePathSegment],
    pattern: &crate::terlan_typeck::CorePattern,
) -> Option<oxc_ast::ast::Expression<'a>> {
    use oxc_span::SPAN;
    use oxc_syntax::number::NumberBase;
    use oxc_syntax::operator::BinaryOperator;

    let literal = match pattern {
        crate::terlan_typeck::CorePattern::Atom(value) => {
            core_atom_artifact_to_oxc_expression(ast, value)?
        }
        crate::terlan_typeck::CorePattern::Int(value)
            if *value >= 0 && is_js_safe_integer(*value) =>
        {
            ast.expression_numeric_literal(SPAN, *value as f64, None, NumberBase::Decimal)
        }
        crate::terlan_typeck::CorePattern::Float(value) => ast.expression_numeric_literal(
            SPAN,
            core_float_literal_to_oxc_number(value)?,
            None,
            NumberBase::Decimal,
        ),
        crate::terlan_typeck::CorePattern::String(value) => {
            ast.expression_string_literal(SPAN, oxc_string_value(ast, value), None)
        }
        _ => return None,
    };
    Some(ast.expression_binary(
        SPAN,
        core_case_scrutinee_path_to_oxc_expression(ast, scrutinee_name, path),
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
