//! Formal syntax-output to Erlang lowering.
//!
//! This module owns the direct `SyntaxModuleOutput` bridge emitter used
//! by the CoreIR-gated Erlang backend while CoreIR executable payload coverage
//! is still being expanded. It lowers compiler-facing syntax output into the
//! internal Erlang render model without routing through the source AST adapter.

use super::*;
use terlan_typeck::{sql_query_core_expr_from_syntax, CoreExpr, CorePrimitiveIntrinsic};

mod html;
use html::*;

mod indexing;
use indexing::*;

mod lets;
use lets::*;

mod imports;
use imports::*;

mod intrinsics;
use intrinsics::*;

mod collections;
use collections::*;

mod comprehensions;
use comprehensions::*;

mod construction;
use construction::*;

mod constructors;
use constructors::*;

mod declarations;
pub(super) use declarations::*;

mod patterns;
use patterns::*;

mod receiver_types;
use receiver_types::*;

mod native_vector;
use native_vector::*;

mod sequences;
use sequences::*;

mod type_values;
use type_values::*;

mod generic_dispatch;
use generic_dispatch::*;

mod calls;
use calls::*;

mod metadata;
use metadata::*;

mod context;
pub(super) use context::*;

mod context_build;

/// Lowers a ready typed SQL form to an internal runtime wrapper call.
///
/// Inputs:
/// - `expr`: syntax-output raw macro expression.
/// - `ctx`: module lowering context used to lower interpolation children.
/// - `env`: local lowering environment used to lower interpolation children.
///
/// Output:
/// - `Some(ErlExpr)` for ready `sql[Row] { ... }` forms.
/// - `None` for non-SQL raw macros or SQL forms without a wrapper plan.
///
/// Transformation:
/// - Reuses CoreIR SQL plan extraction, lowers interpolation children as
///   runtime parameters, and emits a stable internal BEAM runtime call. The
///   runtime adapter is intentionally separate; this fixes the backend wrapper
///   contract.
fn lower_syntax_sql_query_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let CoreExpr::SqlQuery {
        row_type,
        bound_sql,
        parameter_count,
        cardinality,
        result_type,
        projection_fields,
    } = sql_query_core_expr_from_syntax(expr)?
    else {
        return None;
    };

    if parameter_count != expr.children.len() {
        return None;
    }

    let function = match cardinality.as_str() {
        "optional_one" => "query_one",
        "many_rows" => "query",
        "affected_rows" => "execute",
        _ => return None,
    };
    let params = expr
        .children
        .iter()
        .map(|child| lower_syntax_expr_with_env(child, ctx, env))
        .collect::<Option<Vec<_>>>()?;
    let projection = projection_fields
        .iter()
        .map(|field| html_binary(field))
        .collect::<Vec<_>>();

    Some(ErlExpr::Call {
        module: Some("terlan_sql_runtime".to_string()),
        function: function.to_string(),
        args: vec![
            html_binary(&bound_sql),
            ErlExpr::List(params),
            html_binary(&row_type),
            ErlExpr::List(projection),
            html_binary(&result_type),
        ],
    })
}

/// Lowers a syntax-output expression with local lowering state.
///
/// Inputs:
/// - `expr`: syntax-output expression tree.
/// - `ctx`: module lowering context.
/// - `env`: local value/type/replacement environment.
///
/// Output:
/// - Erlang render expression, or `None` for unsupported bridge shapes.
///
/// Transformation:
/// - Recursively lowers literals, collections, calls, control flow, templates,
///   records, aliases, receiver methods, and operators into the internal
///   Erlang render model.
fn lower_syntax_expr_with_env(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    match expr.kind {
        SyntaxExprKind::Int => Some(ErlExpr::Int(expr.text.as_deref()?.parse().ok()?)),
        SyntaxExprKind::Float => Some(ErlExpr::Float(expr.text.clone()?)),
        SyntaxExprKind::Atom => Some(ErlExpr::Atom(expr.text.clone()?)),
        SyntaxExprKind::Binary => Some(ErlExpr::Binary(expr.text.clone()?)),
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref()?;
            if name == "Unit" {
                return Some(ErlExpr::Atom("unit".to_string()));
            }
            if is_bool_literal_name(name) {
                return Some(ErlExpr::Atom(name.to_string()));
            }
            if let Some(target) = ctx.singleton_alias_value_target(name) {
                return lower_syntax_alias_constructor_expr(target, &[], &[], ctx, env);
            }
            if let Some(replacement) = env.value_replacements.get(name) {
                return Some(replacement.clone());
            }
            if env.value_locals.contains(name) {
                return Some(ErlExpr::Var(sanitize_erlang_var(name)));
            }
            if let Some(arity) = ctx.local_function_values.get(name) {
                return Some(ErlExpr::Raw(format!(
                    "fun {}/{}",
                    sanitize_erlang_fn_name(name),
                    arity
                )));
            }
            Some(
                ctx.file_imports
                    .get(name)
                    .map(|bytes| ErlExpr::Binary(erlang_binary_bytes(bytes)))
                    .unwrap_or_else(|| ErlExpr::Var(sanitize_erlang_var(name))),
            )
        }
        SyntaxExprKind::Tuple => Some(ErlExpr::Tuple(
            expr.children
                .iter()
                .map(|child| lower_syntax_expr_with_env(child, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::List => Some(ErlExpr::List(
            expr.children
                .iter()
                .map(|child| lower_syntax_expr_with_env(child, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::ListCons => Some(ErlExpr::ListCons(
            Box::new(lower_syntax_expr_with_env(
                expr.children.first()?,
                ctx,
                env,
            )?),
            Box::new(lower_syntax_expr_with_env(expr.children.get(1)?, ctx, env)?),
        )),
        SyntaxExprKind::FixedArray => Some(ErlExpr::FixedArray(
            expr.children
                .iter()
                .map(|child| lower_syntax_expr_with_env(child, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::Index => lower_syntax_index_expr(expr, ctx, env),
        SyntaxExprKind::IndexAssign => lower_syntax_index_assign_expr(expr, ctx, env),
        SyntaxExprKind::Map => Some(ErlExpr::Map(
            expr.fields
                .iter()
                .map(|field| lower_syntax_map_expr_field(field, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::ListComprehension => lower_syntax_list_comprehension_expr(expr, ctx, env),
        SyntaxExprKind::Let => lower_syntax_let_expr(expr, ctx, env),
        SyntaxExprKind::Cast => None,
        SyntaxExprKind::Call => lower_syntax_call_expr(expr, ctx, env),
        SyntaxExprKind::FunctionCall => lower_syntax_function_value_call_expr(expr, ctx, env),
        SyntaxExprKind::Case => {
            let scrutinee = expr.children.first()?;
            let scrutinee_type = infer_syntax_trait_dispatch_type(scrutinee, ctx, env);
            Some(ErlExpr::Case {
                scrutinee: Box::new(lower_syntax_expr_with_env(scrutinee, ctx, env)?),
                clauses: expr
                    .clauses
                    .iter()
                    .map(|clause| {
                        let pattern = clause.patterns.first()?;
                        let clause_env = syntax_clause_env(
                            env,
                            &clause.patterns,
                            scrutinee_type.as_deref(),
                            ctx,
                        );
                        Some(ErlCaseClause {
                            pattern: lower_syntax_pattern(pattern, ctx)?,
                            guard: match clause.guard.as_deref() {
                                Some(guard) => {
                                    Some(lower_syntax_expr_with_env(guard, ctx, &clause_env)?)
                                }
                                None => None,
                            },
                            body: lower_syntax_expr_with_env(&clause.body, ctx, &clause_env)?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
            })
        }
        SyntaxExprKind::Try => Some(ErlExpr::Try {
            body: Box::new(lower_syntax_expr_with_env(
                expr.children.first()?,
                ctx,
                env,
            )?),
            of_clauses: expr
                .clauses
                .iter()
                .map(|clause| {
                    let pattern = clause.patterns.first()?;
                    let clause_env = syntax_clause_env(env, &clause.patterns, None, ctx);
                    Some(ErlCaseClause {
                        pattern: lower_syntax_pattern(pattern, ctx)?,
                        guard: match clause.guard.as_deref() {
                            Some(guard) => {
                                Some(lower_syntax_expr_with_env(guard, ctx, &clause_env)?)
                            }
                            None => None,
                        },
                        body: lower_syntax_expr_with_env(&clause.body, ctx, &clause_env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            catch_clauses: expr
                .catch_clauses
                .iter()
                .map(|clause| {
                    let pattern = clause.patterns.first()?;
                    let clause_env = syntax_clause_env(env, &clause.patterns, None, ctx);
                    Some(ErlCaseClause {
                        pattern: lower_syntax_pattern(pattern, ctx)?,
                        guard: match clause.guard.as_deref() {
                            Some(guard) => {
                                Some(lower_syntax_expr_with_env(guard, ctx, &clause_env)?)
                            }
                            None => None,
                        },
                        body: lower_syntax_expr_with_env(&clause.body, ctx, &clause_env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            after_clause: expr.try_after.as_ref().and_then(|after| {
                let trigger = lower_syntax_expr_with_env(&after.trigger, ctx, env)?;
                let body = lower_syntax_expr_with_env(&after.body, ctx, env)?;
                Some(ErlTryAfterClause {
                    trigger: Box::new(trigger),
                    body: Box::new(body),
                })
            }),
        }),
        SyntaxExprKind::If => Some(ErlExpr::If(
            expr.clauses
                .iter()
                .map(|clause| {
                    Some(ErlIfClause {
                        condition: lower_syntax_expr_with_env(clause.guard.as_deref()?, ctx, env)?,
                        body: lower_syntax_expr_with_env(&clause.body, ctx, env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::Fun => Some(ErlExpr::Fun(
            expr.clauses
                .iter()
                .map(|clause| {
                    let clause_env = syntax_clause_env(env, &clause.patterns, None, ctx);
                    Some(ErlFunctionClause {
                        patterns: clause
                            .patterns
                            .iter()
                            .map(|pattern| lower_syntax_pattern(pattern, ctx))
                            .collect::<Option<Vec<_>>>()?,
                        guard: match clause.guard.as_ref() {
                            Some(guard) => {
                                Some(lower_syntax_expr_with_env(guard, ctx, &clause_env)?)
                            }
                            None => None,
                        },
                        body: lower_syntax_expr_with_env(&clause.body, ctx, &clause_env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::RemoteFunRef => Some(ErlExpr::RemoteFunRef {
            module: expr.remote.clone()?,
            function: expr.text.clone()?,
            arity: expr.arity,
        }),
        SyntaxExprKind::Macro => Some(ErlExpr::MacroCall {
            name: expr.text.clone()?,
            args: expr
                .children
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        }),
        SyntaxExprKind::RawMacro => lower_syntax_sql_query_expr(expr, ctx, env),
        SyntaxExprKind::HtmlBlock => lower_syntax_html_block_with_env(expr, ctx, env),
        SyntaxExprKind::RecordAccess => {
            let (name, field) = expr.text.as_deref()?.split_once('.')?;
            Some(ErlExpr::RecordAccess {
                value: Box::new(lower_syntax_expr_with_env(
                    expr.children.first()?,
                    ctx,
                    env,
                )?),
                name: name.to_string(),
                field: field.to_string(),
            })
        }
        SyntaxExprKind::FieldAccess => {
            let field = expr.text.clone()?;
            let value = expr.children.first()?;
            if let Some(name) = syntax_expr_name(value) {
                if let Some(target) = ctx.imported_module_member_function_target(name, &field) {
                    return Some(ErlExpr::RemoteFunRef {
                        module: target.module.clone(),
                        function: target.function.clone(),
                        arity: target.fixed_arity,
                    });
                }
                if let Some(markdown) = ctx.markdown_imports.get(name) {
                    return match field.as_str() {
                        "raw" => Some(ErlExpr::Binary(erlang_binary_bytes(
                            markdown.raw_source.as_bytes(),
                        ))),
                        "html" => Some(ErlExpr::Binary(erlang_binary_bytes(
                            markdown.rendered_html.as_bytes(),
                        ))),
                        _ => None,
                    };
                }
            }
            let record_name = resolve_syntax_field_access_struct(value, ctx, env)
                .unwrap_or_else(|| field.clone());
            Some(ErlExpr::RecordAccess {
                value: Box::new(lower_syntax_expr_with_env(value, ctx, env)?),
                name: record_name,
                field,
            })
        }
        SyntaxExprKind::RecordUpdate => Some(ErlExpr::RecordUpdate {
            value: Box::new(lower_syntax_expr_with_env(
                expr.children.first()?,
                ctx,
                env,
            )?),
            name: expr.text.clone()?,
            fields: expr
                .fields
                .iter()
                .map(|field| lower_syntax_expr_field(field, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        }),
        SyntaxExprKind::RecordConstruct => Some(ErlExpr::RecordConstruct {
            name: expr.text.clone()?,
            fields: expr
                .fields
                .iter()
                .map(|field| lower_syntax_expr_field(field, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        }),
        SyntaxExprKind::TemplateInstantiate => lower_syntax_template_instantiation(expr, ctx, env),
        SyntaxExprKind::ConstructorChain => lower_syntax_constructor_chain(expr, ctx, env),
        SyntaxExprKind::BinaryOp => {
            let left = expr.children.first()?;
            let right = expr.children.get(1)?;
            if expr.operator.as_deref() == Some("|>") {
                return lower_syntax_pipe_forward(left, right, ctx, env);
            }
            if expr.operator.as_deref() == Some("+")
                && (is_syntax_string_expr(left, ctx, env) || is_syntax_string_expr(right, ctx, env))
            {
                return Some(lower_syntax_string_concat(
                    lower_syntax_expr_with_env(left, ctx, env)?,
                    lower_syntax_expr_with_env(right, ctx, env)?,
                ));
            }
            Some(ErlExpr::BinaryOp {
                op: lower_syntax_binary_op_for_operands(expr, left, right, ctx, env),
                left: Box::new(lower_syntax_expr_with_env(left, ctx, env)?),
                right: Box::new(lower_syntax_expr_with_env(right, ctx, env)?),
            })
        }
        SyntaxExprKind::UnaryOp => Some(ErlExpr::UnaryOp {
            op: lower_syntax_unary_op(expr.operator.as_deref()),
            expr: Box::new(lower_syntax_expr_with_env(
                expr.children.first()?,
                ctx,
                env,
            )?),
        }),
        SyntaxExprKind::Quote => Some(ErlExpr::Raw(format!(
            "quote {}",
            lower_syntax_expr_with_env(expr.children.first()?, ctx, env)?.render()
        ))),
        SyntaxExprKind::Unquote => Some(ErlExpr::Raw(format!(
            "unquote({})",
            lower_syntax_expr_with_env(expr.children.first()?, ctx, env)?.render()
        ))),
        SyntaxExprKind::Sequence => lower_syntax_sequence_expr(expr, ctx, env),
    }
}

/// Selects the BEAM binary operator for a syntax-output binary expression.
///
/// Inputs:
/// - `expr`: source binary operator expression.
/// - `left`, `right`: operand expressions.
/// - `ctx`, `env`: lowering context and local type metadata.
///
/// Output:
/// - Erlang operator model to render for the expression.
///
/// Transformation:
/// - Uses the ordinary source operator mapping by default, but lowers `/` over
///   integer-shaped operands to BEAM `div`. This preserves Terlan's practical
///   `Int / Int -> Int` behavior for index arithmetic while leaving non-integer
///   division on Erlang `/`.
fn lower_syntax_binary_op_for_operands(
    expr: &SyntaxExprOutput,
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> ErlBinaryOp {
    if expr.operator.as_deref() == Some("/")
        && syntax_expr_is_int_shaped(left, ctx, env)
        && syntax_expr_is_int_shaped(right, ctx, env)
    {
        return ErlBinaryOp::DivRem;
    }

    lower_syntax_binary_op(expr.operator.as_deref())
}

/// Returns whether an expression is known to produce an integer-shaped value.
///
/// Inputs:
/// - `expr`: expression being lowered.
/// - `ctx`, `env`: lowering context and local type metadata.
///
/// Output:
/// - `true` for integer literals, integer locals, and integer arithmetic whose
///   type can be inferred by syntax metadata.
///
/// Transformation:
/// - Delegates to syntax trait-dispatch inference and normalizes known core
///   type spellings before backend operator selection.
fn syntax_expr_is_int_shaped(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> bool {
    infer_syntax_trait_dispatch_type(expr, ctx, env)
        .is_some_and(|type_text| is_syntax_int_type_text(&type_text))
}

/// Builds a lexical environment for a clause body.
///
/// Inputs:
/// - `env`: parent expression-lowering environment.
/// - `patterns`: clause patterns that introduce local value bindings.
///
/// Output:
/// - Parent environment extended with variables bound by nested patterns.
///
/// Transformation:
/// - Keeps module-level lookup data untouched and only adds pattern-bound
///   value locals so method-shaped calls on clause bindings are routed as
///   receiver methods instead of remote module calls.
fn syntax_clause_env(
    env: &SyntaxLowerEnv,
    patterns: &[SyntaxPatternOutput],
    matched_type: Option<&str>,
    ctx: &SyntaxLowerCtx,
) -> SyntaxLowerEnv {
    let mut clause_env = env.clone();
    for pattern in patterns {
        collect_syntax_pattern_value_locals(pattern, &mut clause_env.value_locals);
    }
    if let (Some(pattern), Some(type_text)) = (patterns.first(), matched_type) {
        collect_syntax_pattern_value_types(pattern, type_text, ctx, &mut clause_env.value_types);
    }
    clause_env
}

/// Returns whether an expression is known to produce a string.
///
/// Inputs:
/// - `expr`: source expression being lowered.
/// - `env`: local lowering environment containing typed value bindings.
///
/// Output:
/// - `true` when the syntax bridge can see a string literal or string-typed
///   local value.
///
/// Transformation:
/// - Uses closed syntax shapes and already-collected local type metadata to
///   route source `+` through string concatenation only for string-shaped
///   operands.
fn is_syntax_string_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> bool {
    match expr.kind {
        SyntaxExprKind::Binary => true,
        SyntaxExprKind::Var => expr
            .text
            .as_deref()
            .and_then(|name| env.value_types.get(name))
            .is_some_and(|type_text| {
                normalize_syntax_trait_dispatch_type_key(type_text) == "String"
            }),
        SyntaxExprKind::Call => infer_syntax_trait_dispatch_type(expr, ctx, env)
            .is_some_and(|type_text| type_text == "String"),
        _ => false,
    }
}

/// Lowers Terlan string concatenation to a binary-safe Erlang expression.
///
/// Inputs:
/// - `left` and `right`: already lowered Erlang operands.
///
/// Output:
/// - Erlang expression that renders both operands as UTF-8 character data and
///   joins them into one binary.
///
/// Transformation:
/// - Wraps each operand in a backend-owned scalar-to-text case expression so
///   source code such as `"index: " + value` works for printable scalar
///   values without exposing BEAM formatting details to Terlan users.
fn lower_syntax_string_concat(left: ErlExpr, right: ErlExpr) -> ErlExpr {
    ErlExpr::Raw(format!(
        "unicode:characters_to_binary([{}, {}])",
        lower_syntax_string_concat_part(left).render(),
        lower_syntax_string_concat_part(right).render()
    ))
}

/// Converts one Erlang operand to a string-concat part.
///
/// Inputs:
/// - `expr`: lowered Erlang expression.
///
/// Output:
/// - Erlang case expression producing UTF-8 character data for strings,
///   integers, floats, booleans, and atoms.
///
/// Transformation:
/// - Evaluates the operand once through `case` and maps primitive BEAM values
///   onto text-compatible fragments consumed by `unicode:characters_to_binary`.
fn lower_syntax_string_concat_part(expr: ErlExpr) -> ErlExpr {
    ErlExpr::Raw(format!(
        "case {} of V when is_binary(V) -> V; V when is_list(V) -> V; V when is_integer(V) -> integer_to_binary(V); V when is_float(V) -> float_to_binary(V); true -> \"true\"; false -> \"false\"; V when is_atom(V) -> atom_to_binary(V, utf8) end",
        expr.render()
    ))
}
