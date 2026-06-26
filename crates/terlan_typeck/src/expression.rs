mod calls;
mod casts;
mod construction;
mod control_flow;
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

/// Cache for repeated trait lookup work during expression inference.
///
/// Inputs:
/// - Trait bound checks and method lookup requests encountered while checking
///   one module.
///
/// Output:
/// - Memoized lookup results reused by expression inference.
///
/// Transformation:
/// - Avoids recomputing trait conformance and method dispatch searches while
///   keeping cache scope local to one typecheck pass.
#[derive(Debug, Default)]
pub(super) struct TraitLookupCache {
    bound_checks: HashMap<TraitBoundLookupKey, bool>,
    method_calls: HashMap<TraitMethodLookupKey, TraitMethodLookupResult>,
}

/// Cache key for a trait bound conformance lookup.
///
/// Inputs:
/// - Trait name and concrete bound type arguments.
///
/// Output:
/// - Hashable key for `TraitLookupCache`.
///
/// Transformation:
/// - Normalizes the lookup request into owned type data so repeated bound
///   checks can share the same memoized result.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitBoundLookupKey {
    trait_name: String,
    bound_args: Vec<Type>,
}

/// Cache key for a trait method dispatch lookup.
///
/// Inputs:
/// - Trait name, method name, and concrete call argument types.
///
/// Output:
/// - Hashable key for trait method call lookup results.
///
/// Transformation:
/// - Records the full method dispatch request so repeated calls can reuse the
///   previous ambiguity/single-candidate result.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitMethodLookupKey {
    trait_name: String,
    method_name: String,
    arg_types: Vec<Type>,
}

/// Cached trait method lookup result.
///
/// Inputs:
/// - Candidate trait methods and call argument types.
///
/// Output:
/// - No match, ambiguous match, or a single selected candidate index.
///
/// Transformation:
/// - Stores dispatch outcome without keeping borrowed candidate data in the
///   cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TraitMethodLookupResult {
    NoMatch,
    Ambiguous,
    Single(usize),
}

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
            if crate::raw_macros::raw_macro_requires_resolution_diagnostic(expr) {
                errors.push(crate::raw_macros::raw_macro_resolution_message_for_expr(
                    expr,
                ));
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

/// Finds a transparent alias name for a concrete type.
///
/// Inputs:
/// - `ty`: type to match.
/// - `aliases`: visible transparent aliases.
///
/// Output:
/// - Alias name whose expanded representation equals `ty`.
///
/// Transformation:
/// - Expands zero-parameter aliases and compares their pretty-printed
///   representation to the target type.
fn alias_name_for_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> Option<String> {
    let rendered = pretty_type(ty);
    aliases.iter().find_map(|(name, alias)| {
        if !alias.params.is_empty() {
            return None;
        }
        if pretty_type(&expand_type_aliases(&alias.body, aliases)) == rendered {
            Some(name.clone())
        } else {
            None
        }
    })
}

/// Infers a function call while checking generic trait bounds.
///
/// Inputs:
/// - `scheme`: function type scheme.
/// - `function_name`: optional diagnostic context.
/// - `args`: inferred argument types.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - Instantiated return type or diagnostic string.
///
/// Transformation:
/// - Instantiates generic variables, unifies parameters with arguments,
///   validates generic bounds, and returns the substituted return type.
pub(super) fn infer_function_with_bounds(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    args: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    infer_function_with_explicit_type_args(scheme, function_name, args, &[], ctx, subst)
}

/// Infers a function call with optional explicit generic arguments.
///
/// Inputs:
/// - `scheme`: function type scheme.
/// - `function_name`: optional diagnostic context.
/// - `args`: inferred argument types.
/// - `type_args`: explicit source type arguments from `name[Type](...)`.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - Instantiated return type or diagnostic string.
///
/// Transformation:
/// - Instantiates generic variables, binds explicit call type arguments to the
///   scheme's deterministic type-variable order, unifies parameters with
///   value arguments, validates bounds, and returns the substituted result.
pub(super) fn infer_function_with_explicit_type_args(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    args: &[Type],
    type_args: &[SyntaxTypeOutput],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    let instantiated =
        instantiate_function_scheme_from(scheme, next_function_type_var(args, subst));
    bind_explicit_call_type_args(&instantiated, function_name, type_args, ctx, subst)?;
    infer_instantiated_function_with_bounds(&instantiated, function_name, args, ctx, subst)
}

/// Infers a function call from an already-instantiated function scheme.
///
/// Inputs:
/// - `scheme`: function scheme whose generic variables have already been
///   freshened for this call site.
/// - `function_name`: optional diagnostic context.
/// - `args`: inferred argument types.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - Instantiated return type or diagnostic string.
///
/// Transformation:
/// - Checks arity, unifies instantiated parameters with value arguments,
///   validates trait bounds, and applies the final substitution to the return
///   type. This is used when a caller must freshen a larger synthetic callable,
///   such as receiver-method dispatch where the receiver type and method
///   parameters must share one type-variable mapping.
pub(super) fn infer_instantiated_function_with_bounds(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    args: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    if scheme.params.len() != args.len() {
        return Err(format!(
            "wrong arity for function call: expected {} args, found {}",
            scheme.params.len(),
            args.len()
        ));
    }

    for (expected, actual) in scheme.params.iter().zip(args.iter()) {
        let expected_substituted = apply_subst(expected, subst);
        let actual_substituted = apply_subst(actual, subst);
        if is_subtype_with_aliases(&actual_substituted, &expected_substituted, ctx.aliases) {
            continue;
        }
        if let Err(original_message) = unify(expected, actual, subst) {
            let expected_expanded = expand_type_aliases(&expected_substituted, ctx.aliases);
            let actual_expanded = expand_type_aliases(&actual_substituted, ctx.aliases);
            if is_subtype_with_aliases(&actual_expanded, &expected_expanded, ctx.aliases) {
                continue;
            }
            if unify(&expected_expanded, &actual_expanded, subst).is_err() {
                return Err(original_message);
            }
        }
    }

    if let Err(message) = check_function_bounds(scheme, function_name, ctx, subst) {
        return Err(message);
    }

    Ok(instantiate_type(&scheme.ret, subst))
}

/// Binds explicit call type arguments to instantiated function type variables.
///
/// Inputs:
/// - `scheme`: already instantiated function scheme.
/// - `function_name`: optional diagnostic context.
/// - `type_args`: explicit call-site type arguments.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - `Ok(())` when explicit arguments match generic arity and parse.
/// - `Err(message)` when a call supplies the wrong number of type args or an
///   unparseable type argument.
///
/// Transformation:
/// - Collects type variables from parameters, return type, and bounds in first
///   occurrence order, parses explicit source type arguments with the current
///   module type context, and unifies each variable with its supplied type.
fn bind_explicit_call_type_args(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    type_args: &[SyntaxTypeOutput],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if type_args.is_empty() {
        return Ok(());
    }

    let generic_vars = ordered_function_scheme_type_vars(scheme);
    if generic_vars.len() != type_args.len() {
        let name = function_name.unwrap_or("function");
        return Err(format!(
            "wrong type-argument arity for {}: expected {} type args, found {}",
            name,
            generic_vars.len(),
            type_args.len()
        ));
    }

    for (index, (var, type_arg)) in generic_vars.into_iter().zip(type_args.iter()).enumerate() {
        let supplied = parse_explicit_call_type_arg(type_arg, ctx)?;
        if let Some(generic_param) = scheme.generic_params.get(index) {
            validate_explicit_hkt_type_arg_variance(generic_param, &supplied, type_arg, ctx)?;
        }
        if let Err(message) = unify(&Type::Var(var), &supplied, subst) {
            return Err(message);
        }
    }

    Ok(())
}

/// Validates explicit HKT constructor arguments against source slot variance.
///
/// Inputs:
/// - `generic_param`: source generic parameter text such as `F[+_]`.
/// - `supplied`: parsed explicit type argument.
/// - `type_arg`: original syntax-output type argument for diagnostics.
/// - `ctx`: expression context containing visible type aliases.
///
/// Output:
/// - `Ok(())` when no variance requirement exists or the supplied constructor
///   satisfies every required slot.
/// - `Err(message)` when an explicit constructor argument violates an HKT slot
///   variance requirement.
///
/// Transformation:
/// - Reads `+_` and `-_` markers from the generic parameter, resolves the
///   supplied bare constructor's declared variance, and rejects invariant or
///   opposite-variance constructors before ordinary unification can hide the
///   mismatch.
fn validate_explicit_hkt_type_arg_variance(
    generic_param: &str,
    supplied: &Type,
    type_arg: &SyntaxTypeOutput,
    ctx: &ExprInferContext<'_>,
) -> Result<(), String> {
    let requirements = hkt_slot_variance_requirements(generic_param);
    if requirements.iter().all(Option::is_none) {
        return Ok(());
    }

    let Some((module, name)) = bare_constructor_type_arg(supplied) else {
        return Err(format!(
            "explicit type argument `{}` must be a bare type constructor for `{}`",
            type_arg.text, generic_param
        ));
    };
    let actual = explicit_constructor_variance(module, name, ctx.aliases, requirements.len());
    for (slot_index, requirement) in requirements.iter().enumerate() {
        let Some(required) = requirement else {
            continue;
        };
        let actual = actual
            .get(slot_index)
            .copied()
            .unwrap_or(Variance::Invariant);
        if actual != *required {
            return Err(format!(
                "explicit type argument `{}` for `{}` requires slot {} to be {}, found {} constructor",
                type_arg.text,
                generic_param,
                slot_index + 1,
                variance_display(*required),
                variance_display(actual)
            ));
        }
    }

    Ok(())
}

/// Extracts HKT slot variance requirements from a generic parameter.
///
/// Inputs:
/// - `generic_param`: source generic parameter text.
///
/// Output:
/// - One optional variance requirement per HKT slot.
///
/// Transformation:
/// - Treats `_` as unconstrained, `+_` as covariant, and `-_` as
///   contravariant while ignoring outer type-parameter variance.
fn hkt_slot_variance_requirements(generic_param: &str) -> Vec<Option<Variance>> {
    let Some((_, slots)) = generic_param.split_once('[') else {
        return Vec::new();
    };
    let Some((slots, _)) = slots.rsplit_once(']') else {
        return Vec::new();
    };
    slots
        .split(',')
        .map(|slot| match compact_spaces(slot).as_str() {
            "+_" => Some(Variance::Covariant),
            "-_" => Some(Variance::Contravariant),
            _ => None,
        })
        .collect()
}

/// Extracts a bare constructor from an explicit type argument.
///
/// Inputs:
/// - `supplied`: parsed explicit type argument.
///
/// Output:
/// - Module/name pair for bare named constructors, otherwise `None`.
///
/// Transformation:
/// - Keeps HKT constructor arguments distinct from applied concrete types such
///   as `Option[Int]`.
fn bare_constructor_type_arg(supplied: &Type) -> Option<(Option<&str>, &str)> {
    match supplied {
        Type::Named { module, name, args } if args.is_empty() => {
            Some((module.as_deref(), name.as_str()))
        }
        _ => None,
    }
}

/// Resolves declared variance for an explicit constructor argument.
///
/// Inputs:
/// - `module` and `name`: bare constructor identity.
/// - `aliases`: visible alias metadata.
/// - `fallback_len`: number of slots that need a conservative fallback.
///
/// Output:
/// - Constructor parameter variances, or invariant fallbacks when the
///   constructor has no visible metadata.
///
/// Transformation:
/// - Uses the same conservative rule as named-type subtyping: unknown generic
///   constructors are invariant, while selected built-in collection
///   constructors expose known covariance.
fn explicit_constructor_variance(
    module: Option<&str>,
    name: &str,
    aliases: &HashMap<String, TypeAlias>,
    fallback_len: usize,
) -> Vec<Variance> {
    if let Some(module) = module {
        let qualified = format!("{}.{}", module, name);
        if let Some(alias) = aliases.get(&qualified) {
            return alias.param_variance.clone();
        }
    }
    if let Some(alias) = aliases.get(name) {
        return alias.param_variance.clone();
    }
    match name {
        "List" => vec![Variance::Covariant],
        "Map" => vec![Variance::Covariant, Variance::Covariant],
        "FixedArray" => vec![Variance::Covariant],
        _ => vec![Variance::Invariant; fallback_len],
    }
}

/// Renders variance for call-site diagnostics.
///
/// Inputs:
/// - `variance`: declared or inferred variance direction.
///
/// Output:
/// - Stable lower-case diagnostic text.
///
/// Transformation:
/// - Keeps user-facing diagnostics independent from Rust enum debug output.
fn variance_display(variance: Variance) -> &'static str {
    match variance {
        Variance::Invariant => "invariant",
        Variance::Covariant => "covariant",
        Variance::Contravariant => "contravariant",
    }
}

/// Parses one explicit call type argument into the typechecker model.
///
/// Inputs:
/// - `type_arg`: syntax-output type argument text.
/// - `ctx`: active expression context containing aliases and imported type
///   names.
///
/// Output:
/// - Parsed and qualified type.
///
/// Transformation:
/// - Reuses normal type-expression parsing, preserves bare generic alias
///   constructors for HKT arguments, expands local value-level aliases, and
///   qualifies selected imported type names so call-site generics obey the same
///   naming rules as annotations.
fn parse_explicit_call_type_arg(
    type_arg: &SyntaxTypeOutput,
    ctx: &ExprInferContext<'_>,
) -> Result<Type, String> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let parsed = parse_type_expr(&type_arg.text, ctx.alias_names, &mut vars, &mut next_var)
        .ok_or_else(|| format!("cannot parse call type argument `{}`", type_arg.text))?;
    if is_bare_generic_alias_constructor(&parsed, ctx.aliases) {
        return Ok(qualify_type_names(&parsed, ctx.imported_type_names));
    }
    let parsed = expand_type_aliases(&parsed, ctx.aliases);
    Ok(qualify_type_names(&parsed, ctx.imported_type_names))
}

/// Returns whether an explicit type argument names a generic alias constructor.
///
/// Inputs:
/// - `ty`: parsed explicit call type argument.
/// - `aliases`: visible local/imported type aliases.
///
/// Output:
/// - `true` when the argument is a bare `TypeName` whose alias has parameters.
///
/// Transformation:
/// - Keeps higher-kinded explicit call arguments such as `identity[Option, Int]`
///   as constructors instead of expanding `Option[T]` into its union body.
fn is_bare_generic_alias_constructor(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } if args.is_empty() => aliases
            .get(name)
            .is_some_and(|alias| !alias.params.is_empty()),
        Type::Named {
            module: Some(module),
            name,
            args,
        } if args.is_empty() => {
            let qualified = format!("{}.{}", module, name);
            aliases
                .get(&qualified)
                .is_some_and(|alias| !alias.params.is_empty())
        }
        _ => false,
    }
}

/// Collects function-scheme type variables in deterministic first-use order.
///
/// Inputs:
/// - `scheme`: instantiated function scheme.
///
/// Output:
/// - Type variable identifiers in the order explicit call type arguments bind
///   to them.
///
/// Transformation:
/// - Traverses parameters, return type, and bounds recursively while preserving
///   first occurrence order and removing duplicates.
fn ordered_function_scheme_type_vars(scheme: &FunctionScheme) -> Vec<TypeVarId> {
    let mut vars = Vec::new();
    for param in &scheme.params {
        collect_type_vars_in_order(param, &mut vars);
    }
    collect_type_vars_in_order(&scheme.ret, &mut vars);
    for bound in &scheme.bounds {
        for arg in &bound.trait_args {
            collect_type_vars_in_order(arg, &mut vars);
        }
    }
    vars
}

/// Collects type variables from one type in first-use order.
///
/// Inputs:
/// - `ty`: type to inspect.
/// - `vars`: accumulator preserving existing order.
///
/// Output:
/// - No direct return value; `vars` is extended in place.
///
/// Transformation:
/// - Recursively walks structural type forms and appends each unseen
///   `Type::Var` identifier exactly once.
fn collect_type_vars_in_order(ty: &Type, vars: &mut Vec<TypeVarId>) {
    match ty {
        Type::Var(id) => {
            if !vars.contains(id) {
                vars.push(*id);
            }
        }
        Type::Apply { constructor, args } => {
            if !vars.contains(constructor) {
                vars.push(*constructor);
            }
            for arg in args {
                collect_type_vars_in_order(arg, vars);
            }
        }
        Type::Existential { params, body } => {
            collect_type_vars_in_order_excluding(body, vars, params);
        }
        Type::List(inner) => collect_type_vars_in_order(inner, vars),
        Type::Tuple(items) | Type::Union(items) => {
            for item in items {
                collect_type_vars_in_order(item, vars);
            }
        }
        Type::Map(fields) => {
            for field in fields {
                collect_type_vars_in_order(&field.value, vars);
            }
        }
        Type::FixedArray { elem, .. } => collect_type_vars_in_order(elem, vars),
        Type::Named { args, .. } => {
            for arg in args {
                collect_type_vars_in_order(arg, vars);
            }
        }
        Type::Function { params, ret } => {
            for param in params {
                collect_type_vars_in_order(param, vars);
            }
            collect_type_vars_in_order(ret, vars);
        }
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::Placeholder
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_) => {}
    }
}

/// Collects free type variables while excluding existential binders.
///
/// Inputs:
/// - `ty`: type tree to inspect.
/// - `vars`: accumulator preserving first-use order.
/// - `excluded`: locally bound type-variable ids to ignore.
///
/// Output:
/// - No direct return value; free variables are appended to `vars`.
///
/// Transformation:
/// - Walks the same structures as `collect_type_vars_in_order`, extending the
///   exclusion set through nested existential scopes.
fn collect_type_vars_in_order_excluding(
    ty: &Type,
    vars: &mut Vec<TypeVarId>,
    excluded: &[TypeVarId],
) {
    match ty {
        Type::Var(id) => {
            if !excluded.contains(id) && !vars.contains(id) {
                vars.push(*id);
            }
        }
        Type::Apply { constructor, args } => {
            if !excluded.contains(constructor) && !vars.contains(constructor) {
                vars.push(*constructor);
            }
            for arg in args {
                collect_type_vars_in_order_excluding(arg, vars, excluded);
            }
        }
        Type::Existential { params, body } => {
            let mut nested_excluded = excluded.to_vec();
            nested_excluded.extend(params);
            collect_type_vars_in_order_excluding(body, vars, &nested_excluded);
        }
        Type::List(inner) => collect_type_vars_in_order_excluding(inner, vars, excluded),
        Type::Tuple(items) | Type::Union(items) => {
            for item in items {
                collect_type_vars_in_order_excluding(item, vars, excluded);
            }
        }
        Type::Map(fields) => {
            for field in fields {
                collect_type_vars_in_order_excluding(&field.value, vars, excluded);
            }
        }
        Type::FixedArray { elem, .. } => {
            collect_type_vars_in_order_excluding(elem, vars, excluded);
        }
        Type::Named { args, .. } => {
            for arg in args {
                collect_type_vars_in_order_excluding(arg, vars, excluded);
            }
        }
        Type::Function { params, ret } => {
            for param in params {
                collect_type_vars_in_order_excluding(param, vars, excluded);
            }
            collect_type_vars_in_order_excluding(ret, vars, excluded);
        }
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::Placeholder
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_) => {}
    }
}

/// Checks instantiated function trait bounds.
///
/// Inputs:
/// - `scheme`: instantiated function scheme with bounds.
/// - `function_name`: optional call-site diagnostic context.
/// - `ctx`: expression context with trait impl visibility.
/// - `subst`: current type substitutions.
///
/// Output:
/// - `Ok(())` when all bounds are satisfied, otherwise a diagnostic string.
///
/// Transformation:
/// - Resolves bound arguments through substitutions and alias expansion, then
///   checks visible impls and active callable bounds.
pub(super) fn check_function_bounds(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if scheme.bounds.is_empty() {
        return Ok(());
    }

    for bound in &scheme.bounds {
        let resolved_args = bound
            .trait_args
            .iter()
            .map(|arg| {
                let arg = apply_subst(arg, subst);
                expand_type_aliases(&arg, ctx.aliases)
            })
            .collect::<Vec<_>>();
        let resolved_args = canonicalize_trait_lookup_types(&resolved_args);

        if !trait_has_bound_implementation(&bound.trait_name, &resolved_args, ctx) {
            let trait_description = if resolved_args.is_empty() {
                bound.trait_name.clone()
            } else {
                format!(
                    "{}[{}]",
                    bound.trait_name,
                    resolved_args
                        .iter()
                        .map(pretty_type)
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            };

            let context = function_name.unwrap_or("expression");
            return Err(format!(
                "at `{}` call site: expected trait bound `{}`",
                context, trait_description
            ));
        }
    }

    Ok(())
}

/// Infers a trait method call using the active callable's generic bounds.
///
/// Inputs:
/// - `trait_name`: scoped trait name used at the call site.
/// - `method_name`: trait method name used at the call site.
/// - `arg_types`: already-inferred argument types at the call site.
/// - `ctx`: expression inference context with visible trait signatures and
///   active callable bounds.
/// - `subst`: mutable type substitution accumulated by the enclosing
///   expression inference.
///
/// Output:
/// - `Some(return_type)` when an active bound such as `Eq[A]` satisfies
///   `Eq.equal(...)` and the trait method signature type-checks with the
///   provided arguments.
/// - `None` when no active bound applies or the signature does not match.
///
/// Transformation:
/// - Specializes the trait method signature through the active bound's trait
///   arguments, then runs ordinary function-call inference against that
///   specialized signature. This does not synthesize a global impl candidate,
///   so concrete calls without an impl still produce the normal missing-impl
///   diagnostic.
fn infer_trait_method_call_from_current_bounds(
    trait_name: &str,
    method_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Option<Type> {
    let trait_signature = ctx.trait_signatures.get(trait_name)?;
    let inherited_methods = collect_trait_methods_with_inheritance(
        ctx.trait_signatures,
        trait_name,
        &mut HashMap::new(),
        &mut HashSet::new(),
    )?;
    let method_sig = inherited_methods.get(method_name)?;

    for bound in ctx
        .current_bounds
        .iter()
        .filter(|bound| bound.trait_name == trait_name)
    {
        if bound.trait_args.len() != trait_signature.type_params.len() {
            continue;
        }

        let mut method_vars = HashMap::new();
        let mut next_method_var = 0usize;
        for name in &trait_signature.type_params {
            method_vars.insert(normalize_type_param_name(name), next_method_var);
            next_method_var += 1;
        }

        let parsed_params = method_sig
            .params
            .iter()
            .map(|param| {
                parse_type_expr(
                    &param.ty,
                    ctx.alias_names,
                    &mut method_vars,
                    &mut next_method_var,
                )
            })
            .collect::<Option<Vec<_>>>()?;
        let parsed_return = parse_type_expr(
            &method_sig.return_type,
            ctx.alias_names,
            &mut method_vars,
            &mut next_method_var,
        )?;

        let mut trait_subst = HashMap::new();
        for (param_name, arg_type) in trait_signature.type_params.iter().zip(&bound.trait_args) {
            let var_id = *method_vars.get(&normalize_type_param_name(param_name))?;
            trait_subst.insert(var_id, arg_type.clone());
        }

        let bounds =
            parse_generic_bounds(&method_sig.generic_bounds, &method_vars, ctx.alias_names)
                .into_iter()
                .map(|method_bound| FunctionBound {
                    trait_name: method_bound.trait_name,
                    trait_args: method_bound
                        .trait_args
                        .into_iter()
                        .map(|arg| substitute_type_vars(&arg, &trait_subst))
                        .collect(),
                })
                .collect();
        let scheme = FunctionScheme {
            params: parsed_params
                .into_iter()
                .map(|param| substitute_type_vars(&param, &trait_subst))
                .collect(),
            ret: substitute_type_vars(&parsed_return, &trait_subst),
            generic_params: Vec::new(),
            bounds,
        };

        let mut trial_subst = subst.clone();
        if let Ok(return_type) =
            infer_function_with_bounds(&scheme, Some(method_name), arg_types, ctx, &mut trial_subst)
        {
            *subst = trial_subst;
            return Some(return_type);
        }
    }

    None
}

/// Checks whether a trait bound has a visible implementation.
///
/// Inputs:
/// - `trait_name`: required trait name.
/// - `bound_args`: canonicalized trait arguments.
/// - `ctx`: expression context with impl candidates and active bounds.
///
/// Output:
/// - `true` when an impl or active bound satisfies the requirement.
///
/// Transformation:
/// - Uses a cache for top-level lookups, compares impl arguments with
///   renaming-tolerant unification, and falls back to current bounds.
fn trait_has_bound_implementation(
    trait_name: &str,
    bound_args: &[Type],
    ctx: &ExprInferContext<'_>,
) -> bool {
    let cache_key = TraitBoundLookupKey {
        trait_name: trait_name.to_string(),
        bound_args: bound_args.to_vec(),
    };
    if ctx.current_bounds.is_empty() {
        let cache = ctx.trait_lookup_cache.borrow();
        if let Some(cached) = cache.bound_checks.get(&cache_key) {
            return *cached;
        }
    }

    let Some(candidates) = ctx.trait_bound_impl_type_args.get(trait_name) else {
        let found = current_bounds_satisfy_trait_bound(trait_name, bound_args, ctx);
        if ctx.current_bounds.is_empty() {
            ctx.trait_lookup_cache
                .borrow_mut()
                .bound_checks
                .insert(cache_key, found);
        }
        return found;
    };

    let mut found = false;
    for impl_args in candidates {
        if impl_args.len() != bound_args.len() {
            continue;
        }

        let expanded_impl_args = impl_args
            .iter()
            .map(|arg| expand_type_aliases(arg, ctx.aliases))
            .collect::<Vec<_>>();

        if types_unify_with_renaming(bound_args, &expanded_impl_args).is_ok() {
            found = true;
            break;
        }
    }

    if !found {
        found = current_bounds_satisfy_trait_bound(trait_name, bound_args, ctx);
    }

    if ctx.current_bounds.is_empty() {
        ctx.trait_lookup_cache
            .borrow_mut()
            .bound_checks
            .insert(cache_key, found);
    }
    found
}

/// Checks whether active generic bounds satisfy a requested trait bound.
///
/// Inputs:
/// - `trait_name`: trait being required, such as `Eq`.
/// - `bound_args`: canonicalized required trait arguments.
/// - `ctx`: expression inference context carrying the current callable bounds.
///
/// Output:
/// - `true` when one active callable bound has the same trait name and
///   unifies with `bound_args`; otherwise `false`.
///
/// Transformation:
/// - Expands local aliases in the active bound arguments and performs a
///   renaming-tolerant unification check without mutating inference
///   substitution state.
fn current_bounds_satisfy_trait_bound(
    trait_name: &str,
    bound_args: &[Type],
    ctx: &ExprInferContext<'_>,
) -> bool {
    ctx.current_bounds.iter().any(|bound| {
        if bound.trait_name != trait_name || bound.trait_args.len() != bound_args.len() {
            return false;
        }

        let active_args = bound
            .trait_args
            .iter()
            .map(|arg| expand_type_aliases(arg, ctx.aliases))
            .collect::<Vec<_>>();
        types_unify_with_renaming(bound_args, &active_args).is_ok()
    })
}

/// Collects trait implementation argument shapes from resolved methods.
///
/// Inputs:
/// - `trait_method_calls`: resolved trait method dispatch table.
///
/// Output:
/// - Map from trait name to unique implemented type-argument vectors.
///
/// Transformation:
/// - Deduplicates implementation type arguments across methods so bound checks
///   can operate at trait level rather than method level.
pub(super) fn collect_trait_bound_impl_type_args(
    trait_method_calls: &HashMap<(String, String), Vec<ResolvedTraitMethod>>,
) -> HashMap<String, Vec<Vec<Type>>> {
    let mut impl_type_args = HashMap::new();
    for ((trait_name, _), methods) in trait_method_calls {
        let candidates: &mut Vec<Vec<Type>> = impl_type_args.entry(trait_name.clone()).or_default();
        for method in methods {
            if candidates
                .iter()
                .any(|existing| existing == &method.impl_type_args)
            {
                continue;
            }
            candidates.push(method.impl_type_args.clone());
        }
    }
    impl_type_args
}

/// Unifies type lists while allowing type-variable renaming.
///
/// Inputs:
/// - `expected`: expected type arguments.
/// - `actual`: candidate type arguments.
///
/// Output:
/// - `Ok(())` when the lists unify after renaming candidate variables.
///
/// Transformation:
/// - Remaps candidate type-variable IDs into a fresh range before ordinary
///   unification.
fn types_unify_with_renaming(expected: &[Type], actual: &[Type]) -> Result<(), String> {
    let mut next_var = max_type_var_id(expected);
    let mut remap = HashMap::new();
    let normalized_actual = actual
        .iter()
        .map(|arg| remap_type_var_id(arg, &mut next_var, &mut remap))
        .collect::<Vec<_>>();

    let mut local_subst = HashMap::new();
    for (expected_arg, actual_arg) in expected.iter().zip(normalized_actual.iter()) {
        unify(expected_arg, actual_arg, &mut local_subst)?;
    }
    Ok(())
}

/// Finds the next free type-variable id after a list of types.
///
/// Inputs:
/// - `types`: type list to scan.
///
/// Output:
/// - One greater than the maximum contained type-variable id, or `0`.
///
/// Transformation:
/// - Traverses all nested type variables and computes the fresh-id lower bound.
fn max_type_var_id(types: &[Type]) -> TypeVarId {
    types
        .iter()
        .filter_map(max_type_var)
        .max()
        .map(|id| id + 1)
        .unwrap_or(0)
}

/// Remaps type-variable IDs inside one type.
///
/// Inputs:
/// - `ty`: type to remap.
/// - `next_var`: next fresh variable id.
/// - `remap`: accumulated old-to-new id table.
///
/// Output:
/// - Type with remapped variable IDs.
///
/// Transformation:
/// - Rewrites type variables through `remap_type`, allocating fresh IDs for
///   previously unseen variables.
pub(super) fn remap_type_var_id(
    ty: &Type,
    next_var: &mut TypeVarId,
    remap: &mut HashMap<TypeVarId, TypeVarId>,
) -> Type {
    remap_type(ty, &mut |id| {
        if let Some(remapped) = remap.get(id) {
            *remapped
        } else {
            let remapped = *next_var;
            remap.insert(*id, remapped);
            *next_var += 1;
            remapped
        }
    })
}

/// Refines local types using a simple syntax guard.
///
/// Inputs:
/// - `guard`: syntax-output guard expression.
/// - `locals`: mutable local type environment.
/// - `aliases` and `subst`: visible aliases and current substitutions.
///
/// Output:
/// - No direct return value; `locals` may be narrowed.
///
/// Transformation:
/// - Recognizes supported type-test guard calls and narrows the target local
///   when the narrowed type unifies with the existing type.
fn refine_by_syntax_guard(
    guard: &SyntaxExprOutput,
    locals: &mut HashMap<String, Type>,
    aliases: &HashMap<String, TypeAlias>,
    subst: &mut HashMap<TypeVarId, Type>,
) {
    if guard.kind != SyntaxExprKind::Call || guard.remote.is_some() || guard.children.len() != 2 {
        return;
    }

    let Some(callee_name) = syntax_callee_name(guard) else {
        return;
    };
    let Some(guard_target) = guard.children.get(1).and_then(|arg| match arg.kind {
        SyntaxExprKind::Var => arg.text.as_deref(),
        _ => None,
    }) else {
        return;
    };
    let Some(narrowed) = guard_narrow_type(callee_name) else {
        return;
    };

    if let Some(existing) = locals.get(guard_target) {
        if unify(existing, &narrowed, subst).is_ok() {
            let narrowed = expand_type_aliases(&narrowed, aliases);
            if let Some(value) = locals.get_mut(guard_target) {
                *value = narrowed;
            }
        }
    }
}

/// Canonicalizes trait lookup types for cache keys.
///
/// Inputs:
/// - `types`: trait lookup argument types.
///
/// Output:
/// - Type list with deterministic type-variable IDs.
///
/// Transformation:
/// - Remaps every type variable through a fresh dense ID sequence so equivalent
///   generic lookups share cache keys even when they came from different
///   instantiation sites.
fn canonicalize_trait_lookup_types(types: &[Type]) -> Vec<Type> {
    let mut next_var = 0usize;
    let mut remap = HashMap::new();
    types
        .iter()
        .map(|ty| remap_type_var_id(ty, &mut next_var, &mut remap))
        .collect()
}
