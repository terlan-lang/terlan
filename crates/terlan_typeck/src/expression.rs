mod calls;
mod construction;
mod control_flow;
mod indexing;
mod overloads;

pub(super) use calls::syntax_callee_name;
use calls::{
    infer_syntax_call_expr, infer_syntax_function_value_call, infer_syntax_macro_call,
    infer_syntax_pipe_forward, trait_method_candidate_matches_call,
};
use construction::{
    infer_syntax_constructor_chain, infer_syntax_field_access, infer_syntax_record_access,
    infer_syntax_record_construct, infer_syntax_record_update, infer_syntax_template_instantiation,
};
use control_flow::{
    infer_syntax_case_expr, infer_syntax_fun_expr, infer_syntax_if_expr, infer_syntax_let_expr,
    infer_syntax_list_comprehension, infer_syntax_try_expr,
};
use indexing::{infer_syntax_index, infer_syntax_index_assign};
pub(crate) use overloads::infer_function_scheme_overload;
use overloads::{infer_imported_function_candidate_matches, infer_interface_function_overload};

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
    pub(super) receiver_methods: &'a HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    pub(super) trait_method_calls: &'a HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    pub(super) trait_bound_impl_type_args: &'a HashMap<String, Vec<Vec<Type>>>,
    pub(super) trait_signatures: &'a HashMap<String, ParsedTraitSignature>,
    pub(super) alias_names: &'a HashSet<String>,
    pub(super) current_bounds: &'a [FunctionBound],
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
        receiver_methods: ctx.receiver_methods,
        trait_method_calls: ctx.trait_method_calls,
        trait_bound_impl_type_args: ctx.trait_bound_impl_type_args,
        trait_signatures: ctx.trait_signatures,
        alias_names: ctx.alias_names,
        current_bounds,
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
            let name = expr.text.as_deref().unwrap_or("<unknown>");
            errors.push(format!(
                "raw macro expression `{}` requires macro resolution before type checking",
                name
            ));
            Type::Dynamic
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

/// Infers the type for a syntax-output cast expression.
///
/// Inputs:
/// - `expr`: syntax-output cast node with one child and target type text.
/// - `locals`, `ctx`, and `subst`: the active expression inference context.
/// - `errors`: mutable diagnostic text sink for unsupported conversion claims.
///
/// Output:
/// - Parsed target type when available, otherwise `Dynamic`.
///
/// Transformation:
/// - Type-checks the cast source child, parses the preserved target type text,
///   accepts casts that are already compatible after substitutions and aliases,
///   and rejects conversions that still require explicit conversion-trait
///   resolution.
fn infer_syntax_cast_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let source_type = expr
        .children
        .first()
        .map(|child| infer_syntax_expr(child, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let target_text = expr.text.as_deref().unwrap_or("Dynamic");
    let mut vars = HashMap::new();
    let mut next_var = 0;
    let alias_names = ctx.aliases.keys().cloned().collect::<HashSet<_>>();
    let target_type = parse_type_expr(target_text, &alias_names, &mut vars, &mut next_var)
        .unwrap_or_else(|| {
            errors.push(format!("invalid cast target type `{}`", target_text));
            Type::Dynamic
        });

    if !cast_source_is_assignable_to_target(&source_type, &target_type, ctx, subst)
        && !cast_source_has_conversion_to_target(&source_type, &target_type, ctx, subst)
    {
        errors.push(format!(
            "cast from {} to {} requires trait-backed conversion resolution before backend emission",
            pretty_type(&apply_subst(&source_type, subst)),
            pretty_type(&target_type)
        ));
    }
    target_type
}

/// Returns whether a cast source can already be viewed as the target type.
///
/// Inputs:
/// - `source_type`: inferred source expression type.
/// - `target_type`: parsed cast target type.
/// - `ctx` and `subst`: active expression inference context and substitutions.
///
/// Output:
/// - `true` when no runtime or trait-backed conversion is required.
///
/// Transformation:
/// - Applies current substitutions, expands visible type aliases on both sides,
///   then delegates to the existing subtype relation so literal widening,
///   `Number`, `Term`, and Unit equivalence stay consistent with the rest of
///   typechecking.
fn cast_source_is_assignable_to_target(
    source_type: &Type,
    target_type: &Type,
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let source = apply_subst(source_type, subst);
    let target = apply_subst(target_type, subst);
    let source = expand_type_aliases(&source, ctx.aliases);
    let target = expand_type_aliases(&target, ctx.aliases);
    is_subtype(&source, &target)
}

/// Returns whether a cast has an explicit conversion trait conformance.
///
/// Inputs:
/// - `source_type`: inferred source expression type.
/// - `target_type`: parsed cast target type.
/// - `ctx` and `subst`: active expression inference context and substitutions.
///
/// Output:
/// - `true` when a visible `Convertable[Source, Target]` implementation or
///   active generic bound proves the conversion is explicit.
///
/// Transformation:
/// - Applies substitutions, expands local aliases, canonicalizes the two trait
///   arguments, and reuses the normal trait-bound lookup cache so cast
///   conversion proof follows the same conformance visibility rules as generic
///   function bounds.
fn cast_source_has_conversion_to_target(
    source_type: &Type,
    target_type: &Type,
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let source = expand_type_aliases(&apply_subst(source_type, subst), ctx.aliases);
    let target = expand_type_aliases(&apply_subst(target_type, subst), ctx.aliases);
    let bound_args = canonicalize_trait_lookup_types(&[source, target]);
    trait_has_bound_implementation("Convertable", &bound_args, ctx)
}

/// Infers a variable-like expression.
///
/// Inputs:
/// - `name`: source identifier.
/// - `locals`: local binding type environment.
/// - `ctx`: module inference context with implicit values and imports.
///
/// Output:
/// - The resolved local, alias, intrinsic value, function-value, import, or
///   `Dynamic` type.
///
/// Transformation:
/// - Tries local bindings first, then singleton aliases, built-ins, unique
///   local functions, and imported file/markdown bindings.
fn infer_syntax_var(name: &str, locals: &HashMap<String, Type>, ctx: &ExprInferContext) -> Type {
    locals
        .get(name)
        .cloned()
        .or_else(|| infer_singleton_alias_value(name, ctx))
        .or_else(|| infer_implicit_unit_value(name))
        .or_else(|| infer_implicit_type_value(name))
        .or_else(|| infer_unique_local_function_value(name, ctx))
        .or_else(|| ctx.file_imports.get(name).map(|_| Type::Binary))
        .or_else(|| {
            ctx.markdown_imports.get(name).map(|_| Type::Named {
                module: None,
                name: "Markdown".to_string(),
                args: Vec::new(),
            })
        })
        .unwrap_or(Type::Dynamic)
}

/// Infers a bare singleton type alias used as a value expression.
///
/// Inputs:
/// - `name`: source identifier from a variable expression.
/// - `ctx`: expression inference context containing local aliases, selected
///   imported aliases, and provider interfaces.
///
/// Output:
/// - The alias representation type for zero-payload aliases such as
///   `None = Atom["none"]` or `Unit = Atom["unit"]`.
/// - `None` for aliases that carry associated values, non-alias names, opaque
///   aliases, or unresolved imports.
///
/// Transformation:
/// - Resolves local aliases directly from the merged alias map.
/// - Resolves selected imported aliases through their provider interface, then
///   qualifies any provider-local type references before returning the expanded
///   singleton representation.
fn infer_singleton_alias_value(name: &str, ctx: &ExprInferContext<'_>) -> Option<Type> {
    if let Some(alias) = ctx.aliases.get(name) {
        return singleton_alias_value_type(alias, ctx.aliases);
    }

    let imported = ctx.constructor_aliases.get(name)?;
    let interface = ctx.interface_map.get(&imported.module)?;
    let interface_aliases = interface_type_aliases(interface);
    let alias = interface_aliases.get(&imported.name)?;
    let qualified_names = interface_qualified_type_names(interface);
    singleton_alias_value_type(alias, &interface_aliases)
        .map(|ty| qualify_type_names(&ty, &qualified_names))
}

/// Returns the value type represented by a zero-payload transparent alias.
///
/// Inputs:
/// - `alias`: transparent type alias candidate.
/// - `aliases`: alias environment used to expand the candidate body.
///
/// Output:
/// - `Some(Type)` for aliases whose runtime representation is a single literal
///   atom and carries no associated values.
/// - `None` for aliases with type parameters, opaque aliases, tuple payloads,
///   unions, or any non-singleton representation.
///
/// Transformation:
/// - Expands aliases before checking singleton shape so source spelling does
///   not affect whether the value can be used bare.
fn singleton_alias_value_type(
    alias: &TypeAlias,
    aliases: &HashMap<String, TypeAlias>,
) -> Option<Type> {
    if alias.is_opaque || !alias.params.is_empty() {
        return None;
    }

    match expand_type_aliases(&alias.body, aliases) {
        Type::LiteralAtom(atom) => Some(Type::LiteralAtom(atom)),
        _ => None,
    }
}

/// Infers a bare local function name used as a first-class value.
///
/// Inputs:
/// - `name`: source identifier from a variable expression.
/// - `ctx`: expression inference context containing local function schemes.
///
/// Output:
/// - `Some(Type::Function)` when exactly one local function with `name` is in
///   scope; otherwise `None`.
///
/// Transformation:
/// - Converts a unique local function signature into a function-value type so
///   higher-order calls can constrain callback parameters without treating the
///   identifier as an arbitrary dynamic value.
fn infer_unique_local_function_value(name: &str, ctx: &ExprInferContext<'_>) -> Option<Type> {
    let mut matches = ctx
        .signatures
        .iter()
        .filter(|((candidate, _arity), _schemes)| candidate == name)
        .flat_map(|(_key, schemes)| schemes.iter())
        .map(instantiate_function_scheme);

    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }

    Some(Type::Function {
        params: first.params,
        ret: Box::new(first.ret),
    })
}

/// Infers a binary operator expression.
///
/// Inputs:
/// - `expr`: syntax-output binary operator expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Operator result type, or `Dynamic` for malformed operators.
///
/// Transformation:
/// - Infers both operands, parses the operator token, handles pipe forwarding,
///   and delegates ordinary operators to binary type rules.
fn infer_syntax_binary_op(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let op = syntax_binary_op(expr.operator.as_deref());
    if matches!(op, SyntaxBinaryOp::PipeForward) {
        return infer_syntax_pipe_forward(expr, locals, ctx, subst, errors);
    }
    let left_type = expr
        .children
        .first()
        .map(|left| infer_syntax_expr(left, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let right_type = expr
        .children
        .get(1)
        .map(|right| infer_syntax_expr(right, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    infer_syntax_binary_types(&op, &left_type, &right_type, ctx.aliases, subst, errors)
}

/// Infers a unary operator expression.
///
/// Inputs:
/// - `expr`: syntax-output unary operator expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Unary operator result type.
///
/// Transformation:
/// - Infers the operand, then applies numeric or boolean unary constraints.
fn infer_syntax_unary_op(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let inner_type = expr
        .children
        .first()
        .map(|inner| infer_syntax_expr(inner, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    infer_unary_operator(
        expr.operator.as_deref().unwrap_or(""),
        &inner_type,
        subst,
        errors,
    )
}

/// Applies unary operator type rules.
///
/// Inputs:
/// - `op`: unary operator token.
/// - `value`: inferred operand type.
/// - `subst` and `errors`: active substitution and diagnostic state.
///
/// Output:
/// - Operator result type.
///
/// Transformation:
/// - Constrains numeric negation to numbers and logical negation to booleans.
fn infer_unary_operator(
    op: &str,
    value: &Type,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match op {
        "-" => {
            if let Err(message) = unify(value, &Type::Number, subst) {
                errors.push(message);
            }
            let normalized = apply_subst(value, subst);
            if is_int_like(&normalized) {
                Type::Int
            } else if matches!(normalized, Type::Float) {
                Type::Float
            } else {
                Type::Number
            }
        }
        "not" | "!" => {
            if let Err(message) = unify(value, &Type::Bool, subst) {
                errors.push(message);
            }
            Type::Bool
        }
        _ => Type::Dynamic,
    }
}

/// Applies binary operator type rules.
///
/// Inputs:
/// - `op`: parsed binary operator.
/// - `left` and `right`: inferred operand types.
/// - `aliases`, `subst`, and `errors`: alias, substitution, and diagnostic
///   state.
///
/// Output:
/// - Operator result type.
///
/// Transformation:
/// - Enforces arithmetic, comparison, boolean, and division constraints and
///   returns the source-level operator result type.
fn infer_syntax_binary_types(
    op: &SyntaxBinaryOp,
    left: &Type,
    right: &Type,
    aliases: &HashMap<String, TypeAlias>,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match op {
        SyntaxBinaryOp::Add | SyntaxBinaryOp::Sub | SyntaxBinaryOp::Mul => {
            if let Err(message) = unify(left, &Type::Number, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(right, &Type::Number, subst) {
                errors.push(format!("right side {}", message));
            }

            let normalized_left = apply_subst(left, subst);
            let normalized_right = apply_subst(right, subst);
            if is_int_like(&normalized_left) && is_int_like(&normalized_right) {
                Type::Int
            } else {
                Type::Number
            }
        }
        SyntaxBinaryOp::Div => {
            if let Err(message) = unify(left, &Type::Number, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(right, &Type::Number, subst) {
                errors.push(format!("right side {}", message));
            }

            Type::Number
        }
        SyntaxBinaryOp::DivRem => {
            if let Err(message) = unify(left, &Type::Int, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(right, &Type::Int, subst) {
                errors.push(format!("right side {}", message));
            }
            Type::Int
        }
        SyntaxBinaryOp::Eq
        | SyntaxBinaryOp::EqEq
        | SyntaxBinaryOp::EqEqEq
        | SyntaxBinaryOp::NotEq
        | SyntaxBinaryOp::NotEqEq
        | SyntaxBinaryOp::Lt
        | SyntaxBinaryOp::Gt
        | SyntaxBinaryOp::LtEq
        | SyntaxBinaryOp::GtEq => {
            if let Err(message) = unify_comparable_types(left, right, aliases, subst) {
                errors.push(message);
            }
            Type::Bool
        }
        SyntaxBinaryOp::And | SyntaxBinaryOp::Or => {
            if let Err(message) = unify(&Type::Bool, left, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(&Type::Bool, right, subst) {
                errors.push(format!("right side {}", message));
            }
            Type::Bool
        }
        SyntaxBinaryOp::PipeForward => Type::Dynamic,
    }
}

/// Unifies binary comparison operands with transparent alias expansion.
///
/// Inputs:
/// - `left` and `right`: inferred operand types from a comparison expression.
/// - `aliases`: the visible type aliases for the current inference context.
/// - `subst`: the mutable type-variable substitution table.
///
/// Output:
/// - `Ok(())` when the operands are directly compatible, or compatible after
///   transparent alias expansion.
/// - The original direct-unification diagnostic when expansion still fails.
///
/// Transformation:
/// - First attempts normal unification so existing substitutions and
///   diagnostics remain unchanged for ordinary comparisons.
/// - If that fails, expands non-opaque aliases on both sides and retries using
///   the same substitution table.
fn unify_comparable_types(
    left: &Type,
    right: &Type,
    aliases: &HashMap<String, TypeAlias>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if let Err(original_message) = unify(left, right, subst) {
        let left_expanded = expand_type_aliases(left, aliases);
        let right_expanded = expand_type_aliases(right, aliases);
        if unify(&left_expanded, &right_expanded, subst).is_err() {
            return Err(original_message);
        }
    }

    Ok(())
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

/// Checks whether a type can act as an integer.
///
/// Inputs:
/// - `ty`: inferred type.
///
/// Output:
/// - `true` for `Int` and integer literals.
///
/// Transformation:
/// - Performs a small structural match without alias expansion.
fn is_int_like(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::LiteralInt(_))
}

/// Checks whether a name has constructor spelling.
///
/// Inputs:
/// - `name`: source identifier.
///
/// Output:
/// - `true` when the identifier starts with an uppercase ASCII character.
///
/// Transformation:
/// - Uses spelling only; semantic constructor validation happens elsewhere.
pub(crate) fn is_constructor_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

/// Infers an ordinary function call against a scheme.
///
/// Inputs:
/// - `scheme`: function type scheme.
/// - `args`: inferred argument types.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - Instantiated return type or call diagnostic.
///
/// Transformation:
/// - Delegates to bound-aware function inference without a named call context.
fn infer_function_call(
    scheme: &FunctionScheme,
    args: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    infer_function_with_bounds(scheme, None, args, ctx, subst)
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
    let instantiated =
        instantiate_function_scheme_from(scheme, next_function_type_var(args, subst));
    if instantiated.params.len() != args.len() {
        return Err(format!(
            "wrong arity for function call: expected {} args, found {}",
            instantiated.params.len(),
            args.len()
        ));
    }

    for (expected, actual) in instantiated.params.iter().zip(args.iter()) {
        if let Err(original_message) = unify(expected, actual, subst) {
            let expected_expanded = expand_type_aliases(expected, ctx.aliases);
            let actual_expanded = expand_type_aliases(actual, ctx.aliases);
            if unify(&expected_expanded, &actual_expanded, subst).is_err() {
                return Err(original_message);
            }
        }
    }

    if let Err(message) = check_function_bounds(&instantiated, function_name, ctx, subst) {
        return Err(message);
    }

    Ok(instantiate_type(&instantiated.ret, subst))
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
            method_vars.insert(name.clone(), next_method_var);
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
            let var_id = *method_vars.get(param_name)?;
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

/// Binary operators recognized by expression type inference.
///
/// Inputs:
/// - Constructed from syntax-output operator text by `syntax_binary_op`.
///
/// Output:
/// - Internal operator category used to select type inference rules.
///
/// Transformation:
/// - Groups spelling variants such as `!=` and `/=` into one semantic branch
///   while preserving compatibility branches that still need diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntaxBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    EqEq,
    EqEqEq,
    NotEq,
    NotEqEq,
    GtEq,
    Lt,
    Gt,
    LtEq,
    DivRem,
    And,
    Or,
    PipeForward,
}

/// Converts syntax-output operator text into an internal operator category.
///
/// Inputs:
/// - `operator`: optional operator spelling from a syntax-output binary
///   expression.
///
/// Output:
/// - Internal binary operator category.
///
/// Transformation:
/// - Maps supported arithmetic, comparison, boolean, and pipe spellings to the
///   inference enum, defaulting to assignment-style equality for missing or
///   unknown spellings to preserve existing recovery behavior.
fn syntax_binary_op(operator: Option<&str>) -> SyntaxBinaryOp {
    match operator.unwrap_or("=") {
        "+" => SyntaxBinaryOp::Add,
        "-" => SyntaxBinaryOp::Sub,
        "*" => SyntaxBinaryOp::Mul,
        "/" => SyntaxBinaryOp::Div,
        "=" => SyntaxBinaryOp::Eq,
        "==" => SyntaxBinaryOp::EqEq,
        "=:=" => SyntaxBinaryOp::EqEqEq,
        "!=" | "/=" => SyntaxBinaryOp::NotEq,
        "=/=" => SyntaxBinaryOp::NotEqEq,
        ">=" => SyntaxBinaryOp::GtEq,
        "<" => SyntaxBinaryOp::Lt,
        ">" => SyntaxBinaryOp::Gt,
        "<=" => SyntaxBinaryOp::LtEq,
        "div" | "rem" => SyntaxBinaryOp::DivRem,
        "and" | "&&" => SyntaxBinaryOp::And,
        "or" | "||" => SyntaxBinaryOp::Or,
        "|>" => SyntaxBinaryOp::PipeForward,
        _ => SyntaxBinaryOp::Eq,
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
