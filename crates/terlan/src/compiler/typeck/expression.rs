mod calls;
mod casts;
mod construction;
mod control_flow;
mod function_inference;
mod indexing;
mod operators;
mod overloads;
mod sql;
mod values;

pub(crate) use calls::infer_syntax_expr_with_expected;
pub(super) use calls::syntax_callee_name;
use calls::{
    infer_syntax_call_expr, infer_syntax_function_value_call, infer_syntax_macro_call,
    infer_syntax_pipe_forward, trait_method_candidate_matches_call,
};
use casts::infer_syntax_cast_expr;
use construction::{
    infer_imported_module_member_function_value_with_expected, infer_syntax_constructor_chain,
    infer_syntax_field_access, infer_syntax_record_access, infer_syntax_record_construct,
    infer_syntax_record_update, infer_syntax_template_instantiation,
};
use control_flow::{
    infer_syntax_case_expr, infer_syntax_fun_expr, infer_syntax_if_expr, infer_syntax_let_expr,
    infer_syntax_list_comprehension, infer_syntax_try_expr,
};
use function_inference::*;
pub(crate) use function_inference::{
    check_function_bounds, collect_trait_bound_impl_type_args, infer_function_with_bounds,
    TraitLookupCache,
};
use indexing::{infer_syntax_index, infer_syntax_index_assign};
use operators::{infer_syntax_binary_op, infer_syntax_unary_op};
pub(crate) use overloads::{
    infer_function_scheme_overload, infer_function_scheme_overload_with_explicit_type_args,
};
use overloads::{
    infer_imported_function_candidate_matches,
    infer_interface_function_overload_with_explicit_type_args,
};
use sql::{infer_sql_form_result_type, validate_sql_form_row_type};
use values::infer_syntax_var;
pub(crate) use values::is_constructor_name;

use super::*;

/// Shared expression-inference context for one module.
///
/// Inputs:
/// - Resolver metadata, imported interfaces, aliases, constructors, templates,
///   receiver methods, trait signatures, and active function bounds.
///
/// Output:
/// - Borrowed lookup context consumed by expression inference helpers.
///
/// Transformation:
/// - Groups all immutable typechecking lookup tables plus a scoped trait cache
///   so expression inference functions do not need long parameter lists.
pub(super) struct ExprInferContext<'a> {
    pub(super) local_fns: &'a HashMap<(String, usize), FunctionSymbol>,
    pub(super) signatures: &'a HashMap<(String, usize), Vec<FunctionScheme>>,
    pub(super) interface_map: &'a HashMap<String, ModuleInterface>,
    pub(super) module_aliases: &'a HashMap<String, String>,
    pub(super) file_imports: &'a HashMap<String, String>,
    pub(super) markdown_imports: &'a HashMap<String, String>,
    pub(super) function_imports: &'a HashMap<String, ImportedFunctionTarget>,
    pub(super) imported_type_names: &'a HashMap<String, QualifiedTypeName>,
    pub(super) constructor_aliases: &'a HashMap<String, QualifiedTypeName>,
    pub(super) constructors: &'a HashMap<String, Vec<ConstructorScheme>>,
    pub(super) templates: &'a HashMap<String, TemplateScheme>,
    pub(super) aliases: &'a HashMap<String, TypeAlias>,
    pub(super) struct_fields: &'a HashMap<String, HashMap<String, Type>>,
    pub(super) struct_field_visibility: &'a HashMap<String, HashMap<String, StructFieldVisibility>>,
    pub(super) receiver_methods: &'a HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    pub(super) trait_method_calls: &'a HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    pub(super) trait_bound_impl_type_args: &'a HashMap<String, Vec<Vec<Type>>>,
    pub(super) trait_signatures: &'a HashMap<String, ParsedTraitSignature>,
    pub(super) alias_names: &'a HashSet<String>,
    pub(super) current_bounds: &'a [FunctionBound],
    pub(super) current_constructor_target: Option<&'a str>,
    pub(super) trait_lookup_cache: &'a RefCell<TraitLookupCache>,
}

/// Creates an expression-inference context for one callable body.
///
/// Inputs:
/// - `ctx`: module-wide expression inference context.
/// - `current_bounds`: instantiated generic trait bounds declared by the
///   callable currently being checked.
///
/// Output:
/// - A shallow context view that shares all module-wide lookup tables while
///   exposing the active callable bounds to trait dispatch and bound checking.
///
/// Transformation:
/// - Copies immutable references from `ctx`, replaces only `current_bounds`,
///   and reuses the same trait-lookup cache so repeated lookups stay
///   deterministic within a typecheck pass.
pub(super) fn expr_ctx_with_current_bounds<'a, 'b>(
    ctx: &'a ExprInferContext<'a>,
    current_bounds: &'b [FunctionBound],
) -> ExprInferContext<'b>
where
    'a: 'b,
{
    ExprInferContext {
        local_fns: ctx.local_fns,
        signatures: ctx.signatures,
        interface_map: ctx.interface_map,
        module_aliases: ctx.module_aliases,
        file_imports: ctx.file_imports,
        markdown_imports: ctx.markdown_imports,
        function_imports: ctx.function_imports,
        imported_type_names: ctx.imported_type_names,
        constructor_aliases: ctx.constructor_aliases,
        constructors: ctx.constructors,
        templates: ctx.templates,
        aliases: ctx.aliases,
        struct_fields: ctx.struct_fields,
        struct_field_visibility: ctx.struct_field_visibility,
        receiver_methods: ctx.receiver_methods,
        trait_method_calls: ctx.trait_method_calls,
        trait_bound_impl_type_args: ctx.trait_bound_impl_type_args,
        trait_signatures: ctx.trait_signatures,
        alias_names: ctx.alias_names,
        current_bounds,
        current_constructor_target: ctx.current_constructor_target,
        trait_lookup_cache: ctx.trait_lookup_cache,
    }
}

/// Creates an expression-inference context for one constructor body.
///
/// Inputs:
/// - `ctx`: module-wide expression inference context.
/// - `constructor_target`: struct/type name whose constructor body is active.
///
/// Output:
/// - A shallow context view that marks the active constructor target.
///
/// Transformation:
/// - Preserves all lookup tables and callable bounds while setting the
///   constructor target used by default struct-initializer call validation.
pub(super) fn expr_ctx_with_current_constructor<'a, 'b>(
    ctx: &'a ExprInferContext<'a>,
    constructor_target: &'b str,
) -> ExprInferContext<'b>
where
    'a: 'b,
{
    ExprInferContext {
        local_fns: ctx.local_fns,
        signatures: ctx.signatures,
        interface_map: ctx.interface_map,
        module_aliases: ctx.module_aliases,
        file_imports: ctx.file_imports,
        markdown_imports: ctx.markdown_imports,
        function_imports: ctx.function_imports,
        imported_type_names: ctx.imported_type_names,
        constructor_aliases: ctx.constructor_aliases,
        constructors: ctx.constructors,
        templates: ctx.templates,
        aliases: ctx.aliases,
        struct_fields: ctx.struct_fields,
        struct_field_visibility: ctx.struct_field_visibility,
        receiver_methods: ctx.receiver_methods,
        trait_method_calls: ctx.trait_method_calls,
        trait_bound_impl_type_args: ctx.trait_bound_impl_type_args,
        trait_signatures: ctx.trait_signatures,
        alias_names: ctx.alias_names,
        current_bounds: ctx.current_bounds,
        current_constructor_target: Some(constructor_target),
        trait_lookup_cache: ctx.trait_lookup_cache,
    }
}

const SPANNED_EXPR_ERROR_PREFIX: &str = "\u{1f}terlan-span:";

/// Encodes an expression diagnostic with a precise source span override.
///
/// Inputs:
/// - `span`: source byte range that should be highlighted.
/// - `message`: diagnostic message.
///
/// Output:
/// - Internal expression-error string carrying span metadata.
///
/// Transformation:
/// - Prefixes the message with a private marker consumed only when expression
///   errors are converted back into public diagnostics.
fn spanned_expression_error(span: Span, message: impl Into<String>) -> String {
    format!(
        "{}{}:{}:{}",
        SPANNED_EXPR_ERROR_PREFIX,
        span.start,
        span.end,
        message.into()
    )
}

/// Converts an internal expression error into a public diagnostic.
///
/// Inputs:
/// - `error`: expression-inference error string, optionally span-prefixed.
/// - `fallback_span`: source range used for ordinary expression diagnostics.
///
/// Output:
/// - Public diagnostic with severity `Error`.
///
/// Transformation:
/// - Decodes precise span overrides for selected expression errors and keeps
///   existing fallback-span behavior for all other expression diagnostics.
pub(super) fn expression_error_to_diagnostic(error: String, fallback_span: Span) -> Diagnostic {
    if let Some(rest) = error.strip_prefix(SPANNED_EXPR_ERROR_PREFIX) {
        if let Some((start_text, rest)) = rest.split_once(':') {
            if let Some((end_text, message)) = rest.split_once(':') {
                if let (Ok(start), Ok(end)) =
                    (start_text.parse::<usize>(), end_text.parse::<usize>())
                {
                    return Diagnostic {
                        span: Span::new(start, end),
                        message: message.to_string(),
                        severity: DiagSeverity::Error,
                    };
                }
            }
        }
    }

    Diagnostic {
        span: fallback_span,
        message: error,
        severity: DiagSeverity::Error,
    }
}

/// Infers the type of a syntax-output expression.
///
/// Inputs:
/// - `expr`: expression node to infer.
/// - `locals`, `ctx`, `subst`, and `errors`: local bindings, module inference
///   context, mutable type substitutions, and diagnostic text sink.
///
/// Output:
/// - Best-effort inferred type for the expression.
///
/// Transformation:
/// - Dispatches by expression kind, recursively infers children, records
///   recoverable type errors, and returns `Dynamic` for unsupported shapes.
pub(super) fn infer_syntax_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match expr.kind {
        SyntaxExprKind::Int => {
            Type::LiteralInt(expr.text.as_deref().unwrap_or("0").parse().unwrap_or(0))
        }
        SyntaxExprKind::Float => Type::Float,
        SyntaxExprKind::Binary => Type::Binary,
        SyntaxExprKind::Atom => {
            let name = expr.text.as_deref().unwrap_or_default();
            if is_reserved_lowercase_unit_spelling(name) && !is_explicit_atom_literal_expr(expr) {
                errors
                    .push("`unit` is not a built-in unit value; use uppercase `Unit`".to_string());
            }
            if is_literal_atom(name) {
                if name == "true" || name == "false" {
                    Type::Bool
                } else {
                    Type::LiteralAtom(name.to_string())
                }
            } else {
                Type::Atom
            }
        }
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref().unwrap_or_default();
            let inferred = infer_syntax_var(name, locals, ctx);
            if is_reserved_uppercase_bool_literal_spelling(name) && inferred == Type::Dynamic {
                errors.push(format!(
                    "`{name}` is not a built-in boolean literal; use lowercase `{}` or declare `{name}` explicitly",
                    name.to_ascii_lowercase()
                ));
            }
            if is_reserved_lowercase_unit_spelling(name) && inferred == Type::Dynamic {
                errors
                    .push("`unit` is not a built-in unit value; use uppercase `Unit`".to_string());
            }
            inferred
        }
        SyntaxExprKind::Tuple => Type::Tuple(
            expr.children
                .iter()
                .map(|item| infer_syntax_expr(item, locals, ctx, subst, errors))
                .collect(),
        ),
        SyntaxExprKind::List => {
            let inferred = expr
                .children
                .iter()
                .map(|value| {
                    widen_list_literal_element_type(infer_syntax_expr(
                        value, locals, ctx, subst, errors,
                    ))
                })
                .collect::<Vec<_>>();
            Type::List(Box::new(normalize_union(inferred)))
        }
        SyntaxExprKind::ListCons => {
            let head_type = expr
                .children
                .first()
                .map(|head| infer_syntax_expr(head, locals, ctx, subst, errors))
                .unwrap_or(Type::Dynamic);
            if let Some(tail) = expr.children.get(1) {
                let tail_type = infer_syntax_expr(tail, locals, ctx, subst, errors);
                if let Err(message) =
                    unify(&tail_type, &Type::List(Box::new(head_type.clone())), subst)
                {
                    errors.push(format!("list cons tail {}", message));
                }
            }
            Type::List(Box::new(apply_subst(&head_type, subst)))
        }
        SyntaxExprKind::FixedArray => {
            let elem_type = normalize_union(
                expr.children
                    .iter()
                    .map(
                        |elem| match infer_syntax_expr(elem, locals, ctx, subst, errors) {
                            Type::LiteralInt(_) => Type::Int,
                            Type::LiteralAtom(_) => Type::Atom,
                            other => other,
                        },
                    )
                    .collect(),
            );
            Type::FixedArray {
                size: expr.children.len(),
                elem: Box::new(elem_type),
            }
        }
        SyntaxExprKind::Index => infer_syntax_index(expr, locals, ctx, subst, errors),
        SyntaxExprKind::IndexAssign => infer_syntax_index_assign(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Map => Type::Map(
            expr.fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: infer_syntax_expr(&field.value, locals, ctx, subst, errors),
                    required: field.required,
                })
                .collect(),
        ),
        SyntaxExprKind::Case => infer_syntax_case_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Try => infer_syntax_try_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::If => infer_syntax_if_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::ListComprehension => {
            infer_syntax_list_comprehension(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::Let => infer_syntax_let_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Cast => infer_syntax_cast_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Call => infer_syntax_call_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::FunctionCall => {
            infer_syntax_function_value_call(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::Fun => infer_syntax_fun_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::RemoteFunRef => Type::Function {
            params: vec![Type::Dynamic; expr.arity],
            ret: Box::new(Type::Dynamic),
        },
        SyntaxExprKind::Macro => {
            let arg_types = expr
                .children
                .iter()
                .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
                .collect::<Vec<_>>();

            if let Some(macro_name) = expr.text.as_deref() {
                if let Some(return_type) =
                    infer_syntax_macro_call(macro_name, &arg_types, ctx, subst, errors)
                {
                    return return_type;
                }
            }
            Type::Dynamic
        }
        SyntaxExprKind::RawMacro => {
            for child in &expr.children {
                infer_syntax_expr(child, locals, ctx, subst, errors);
            }
            validate_sql_form_row_type(expr, ctx, errors);
            let sql_result_type = infer_sql_form_result_type(expr, ctx, errors);
            if crate::terlan_typeck::raw_macros::raw_macro_requires_resolution_diagnostic(expr) {
                errors.push(
                    crate::terlan_typeck::raw_macros::raw_macro_resolution_message_for_expr(expr),
                );
            }
            sql_result_type.unwrap_or(Type::Dynamic)
        }
        SyntaxExprKind::HtmlBlock => infer_syntax_html_block(expr, locals, ctx, subst, errors),
        SyntaxExprKind::RecordConstruct => {
            infer_syntax_record_construct(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::RecordAccess => {
            infer_syntax_record_access(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::FieldAccess => infer_syntax_field_access(expr, locals, ctx, subst, errors),
        SyntaxExprKind::RecordUpdate => {
            infer_syntax_record_update(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::TemplateInstantiate => {
            infer_syntax_template_instantiation(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::ConstructorChain => {
            infer_syntax_constructor_chain(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::UnaryOp => infer_syntax_unary_op(expr, locals, ctx, subst, errors),
        SyntaxExprKind::BinaryOp => infer_syntax_binary_op(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Quote => {
            let value_type = expr
                .children
                .first()
                .map(|inner| infer_syntax_expr(inner, locals, ctx, subst, errors))
                .unwrap_or(Type::Dynamic);
            Type::Named {
                module: None,
                name: "Ast".to_string(),
                args: vec![value_type],
            }
        }
        SyntaxExprKind::Unquote => expr
            .children
            .first()
            .map(|inner| infer_syntax_expr(inner, locals, ctx, subst, errors))
            .unwrap_or(Type::Dynamic),
        SyntaxExprKind::Sequence => expr
            .children
            .last()
            .map(|inner| infer_syntax_expr(inner, locals, ctx, subst, errors))
            .unwrap_or(Type::Dynamic),
    }
}
