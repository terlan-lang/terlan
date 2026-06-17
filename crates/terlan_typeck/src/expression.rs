use super::*;

#[derive(Debug, Default)]
pub(super) struct TraitLookupCache {
    bound_checks: HashMap<TraitBoundLookupKey, bool>,
    method_calls: HashMap<TraitMethodLookupKey, TraitMethodLookupResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitBoundLookupKey {
    trait_name: String,
    bound_args: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitMethodLookupKey {
    trait_name: String,
    method_name: String,
    arg_types: Vec<Type>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TraitMethodLookupResult {
    NoMatch,
    Ambiguous,
    Single(usize),
}

pub(super) struct ExprInferContext<'a> {
    pub(super) local_fns: &'a HashMap<(String, usize), FunctionSymbol>,
    pub(super) signatures: &'a HashMap<(String, usize), FunctionScheme>,
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

/// Infers the placeholder type for a syntax-output cast expression.
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
///   records that trait-backed conversion resolution is not implemented yet,
///   and returns the target type as the syntax-preserved expectation.
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

    errors.push(format!(
        "cast from {} to {} requires trait-backed conversion resolution before backend emission",
        pretty_type(&apply_subst(&source_type, subst)),
        pretty_type(&target_type)
    ));
    target_type
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
        .filter(|((candidate, _arity), _scheme)| candidate == name)
        .map(|(_key, scheme)| instantiate_function_scheme(scheme));

    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }

    Some(Type::Function {
        params: first.params,
        ret: Box::new(first.ret),
    })
}

/// Infers a bracket index read.
///
/// Inputs:
/// - `expr`: syntax-output index expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Element type for fixed arrays or trait-selected `IndexGet` return type.
///
/// Transformation:
/// - Checks fixed-array bounds when possible, otherwise delegates to
///   `IndexGet.get_at` trait dispatch.
fn infer_syntax_index(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let target_type = expr
        .children
        .first()
        .map(|value| infer_syntax_expr(value, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let index_type = expr
        .children
        .get(1)
        .map(|index| infer_syntax_expr(index, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);

    match target_type {
        Type::FixedArray { size, elem } => match index_type {
            Type::LiteralInt(index) => {
                if index < 0 || index as usize >= size {
                    errors.push(format!(
                        "index {} is out of bounds for {}\nvalid indices: 0..{}",
                        index,
                        pretty_type(&Type::FixedArray {
                            size,
                            elem: elem.clone(),
                        }),
                        size.saturating_sub(1)
                    ));
                }
                *elem
            }
            Type::Int => *elem,
            Type::Var(_) | Type::Dynamic | Type::Number => *elem,
            _ => {
                errors.push(format!("expected Int found {}", pretty_type(&index_type)));
                Type::Dynamic
            }
        },
        _ => infer_index_get_trait_call(&target_type, &index_type, ctx, subst, errors)
            .unwrap_or(Type::Dynamic),
    }
}

/// Infers bracket assignment through `IndexSet`.
///
/// Inputs:
/// - `expr`: syntax-output `IndexAssign` node with collection, index, and value
///   children.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Unit` when a visible `IndexSet.set_at(collection, index, value)`
///   implementation matches.
/// - `Dynamic` when the node shape is malformed or no matching implementation
///   can be found.
///
/// Transformation:
/// - Treats `collection[index] = value` as assignment syntax backed by the
///   same trait system as explicit calls. Successful assignment expressions
///   have `Unit` type; later lowering decides how mutable receiver rebinding is
///   represented for the selected target.
fn infer_syntax_index_assign(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if !matches!(expr.kind, SyntaxExprKind::IndexAssign) || expr.children.len() != 3 {
        errors.push("malformed indexed assignment expression".to_string());
        return Type::Dynamic;
    }

    let collection_type = infer_syntax_expr(&expr.children[0], locals, ctx, subst, errors);
    let index_type = infer_syntax_expr(&expr.children[1], locals, ctx, subst, errors);
    let value_type = infer_syntax_expr(&expr.children[2], locals, ctx, subst, errors);

    match infer_index_set_trait_call(
        &collection_type,
        &index_type,
        &value_type,
        ctx,
        subst,
        errors,
    ) {
        Some(return_type) => {
            if !is_unit_named_type(&return_type) && !is_unit_literal_type(&return_type) {
                errors.push(format!(
                    "IndexSet.set_at must return Unit, found {}",
                    pretty_type(&return_type)
                ));
            }
            Type::Named {
                module: None,
                name: "Unit".to_string(),
                args: Vec::new(),
            }
        }
        None => {
            errors.push(format!(
                "cannot find IndexSet.set_at implementation for [{}, {}, {}]",
                pretty_type(&collection_type),
                pretty_type(&index_type),
                pretty_type(&value_type)
            ));
            Type::Dynamic
        }
    }
}

/// Infers non-fixed-array bracket reads through `IndexGet`.
///
/// Inputs:
/// - `target_type`: inferred type of the expression before `[index]`.
/// - `index_type`: inferred type of the bracket index expression.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(return_type)` when exactly one visible `IndexGet.get_at` candidate
///   matches `(target_type, index_type)`.
/// - `None` when no visible `IndexGet` trait or impl applies.
///
/// Transformation:
/// - Treats `collection[index]` as a compiler-owned shorthand for an
///   `IndexGet.get_at(collection, index)` trait call. The parser remains
///   collection-agnostic while typechecking uses the same conformance metadata
///   as explicit trait method calls.
fn infer_index_get_trait_call(
    target_type: &Type,
    index_type: &Type,
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let arg_types = vec![
        apply_subst(target_type, subst),
        apply_subst(index_type, subst),
    ];
    let mut selected = None::<(Type, HashMap<TypeVarId, Type>)>;
    let mut matches = 0usize;

    for ((trait_name, method_name), impls) in ctx.trait_method_calls {
        if method_name != "get_at" || !is_index_get_trait_name(trait_name) {
            continue;
        }

        for impl_candidate in impls {
            if !trait_method_candidate_matches_call(impl_candidate, &arg_types, ctx, subst) {
                continue;
            }

            let mut trial_subst = subst.clone();
            if let Ok(return_type) =
                infer_function_call(&impl_candidate.scheme, &arg_types, ctx, &mut trial_subst)
            {
                matches += 1;
                if selected.is_none() {
                    selected = Some((return_type, trial_subst));
                } else {
                    break;
                }
            }
        }

        if matches > 1 {
            break;
        }

        if matches == 0 {
            if let Some(return_type) = infer_trait_method_call_from_current_bounds(
                trait_name,
                method_name,
                &arg_types,
                ctx,
                subst,
            ) {
                return Some(return_type);
            }
        }
    }

    match (matches, selected) {
        (0, _) => None,
        (1, Some((return_type, inferred_subst))) => {
            *subst = inferred_subst;
            Some(return_type)
        }
        _ => {
            errors.push(format!(
                "ambiguous IndexGet.get_at implementation for [{}]",
                arg_types
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            Some(Type::Dynamic)
        }
    }
}

/// Infers bracket assignments through `IndexSet`.
///
/// Inputs:
/// - `collection_type`: inferred type before `[index]`.
/// - `index_type`: inferred bracket index type.
/// - `value_type`: inferred assigned value type.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(return_type)` when exactly one visible `IndexSet.set_at` candidate
///   matches `(collection_type, index_type, value_type)`.
/// - `None` when no visible `IndexSet` trait or impl applies.
///
/// Transformation:
/// - Reuses the trait-method call resolver shape from `IndexGet`, but includes
///   the assigned value as the third call argument and filters on the canonical
///   `IndexSet` trait name.
fn infer_index_set_trait_call(
    collection_type: &Type,
    index_type: &Type,
    value_type: &Type,
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let arg_types = vec![
        apply_subst(collection_type, subst),
        apply_subst(index_type, subst),
        apply_subst(value_type, subst),
    ];
    let mut selected = None::<(Type, HashMap<TypeVarId, Type>)>;
    let mut matches = 0usize;

    for ((trait_name, method_name), impls) in ctx.trait_method_calls {
        if method_name != "set_at" || !is_index_set_trait_name(trait_name) {
            continue;
        }

        for impl_candidate in impls {
            if !trait_method_candidate_matches_call(impl_candidate, &arg_types, ctx, subst) {
                continue;
            }

            let mut trial_subst = subst.clone();
            if let Ok(return_type) =
                infer_function_call(&impl_candidate.scheme, &arg_types, ctx, &mut trial_subst)
            {
                matches += 1;
                if selected.is_none() {
                    selected = Some((return_type, trial_subst));
                } else {
                    break;
                }
            }
        }

        if matches > 1 {
            break;
        }

        if matches == 0 {
            if let Some(return_type) = infer_trait_method_call_from_current_bounds(
                trait_name,
                method_name,
                &arg_types,
                ctx,
                subst,
            ) {
                return Some(return_type);
            }
        }
    }

    match (matches, selected) {
        (0, _) => None,
        (1, Some((return_type, inferred_subst))) => {
            *subst = inferred_subst;
            Some(return_type)
        }
        _ => {
            errors.push(format!(
                "ambiguous IndexSet.set_at implementation for [{}]",
                arg_types
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            Some(Type::Dynamic)
        }
    }
}

/// Returns whether a trait name denotes the standard index-read contract.
///
/// Inputs:
/// - `trait_name`: local or qualified trait name from conformance metadata.
///
/// Output:
/// - `true` for `IndexGet` and qualified names ending in `.IndexGet`.
///
/// Transformation:
/// - Keeps bracket syntax independent of import spelling while still requiring
///   the resolved trait to use the canonical `IndexGet` name.
fn is_index_get_trait_name(trait_name: &str) -> bool {
    trait_name == "IndexGet" || trait_name.ends_with(".IndexGet")
}

/// Returns whether a trait name denotes the standard index-write contract.
///
/// Inputs:
/// - `trait_name`: local or qualified trait name from conformance metadata.
///
/// Output:
/// - `true` for `IndexSet` and qualified names ending in `.IndexSet`.
///
/// Transformation:
/// - Keeps assignment syntax independent of import spelling while still
///   requiring the resolved trait to use the canonical `IndexSet` name.
fn is_index_set_trait_name(trait_name: &str) -> bool {
    trait_name == "IndexSet" || trait_name.ends_with(".IndexSet")
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

/// Infers a named call expression.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Resolved call return type.
///
/// Transformation:
/// - Infers argument types first, then routes the call through constructor,
///   local, remote, receiver, trait, intrinsic, and import dispatch.
fn infer_syntax_call_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arg_types = expr
        .children
        .iter()
        .skip(1)
        .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
        .collect::<Vec<_>>();
    infer_syntax_call_with_arg_types(expr, &arg_types, locals, ctx, subst, errors)
}

/// Infers a dedicated function-value invocation expression.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression whose first child is the
///   callable expression and remaining children are call arguments.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - The invoked function's return type when the callee has a function type.
/// - `Dynamic` when the callee is malformed, non-callable, or has invalid
///   argument types.
///
/// Transformation:
/// - Infers all non-callee arguments, then delegates to the shared
///   function-value invocation checker so pipe-forward can prepend a synthetic
///   first argument without rebuilding syntax nodes.
fn infer_syntax_function_value_call(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arg_types = expr
        .children
        .iter()
        .skip(1)
        .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
        .collect::<Vec<_>>();
    infer_syntax_function_value_call_with_arg_types(expr, &arg_types, locals, ctx, subst, errors)
}

/// Checks a function-value invocation with already inferred argument types.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression with a callable child.
/// - `arg_types`: argument types to check against the callee's function type.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - The callee return type with substitutions applied.
/// - `Dynamic` when the callee is not a function or arguments do not match.
///
/// Transformation:
/// - Infers the callee expression, requires a `Type::Function`, unifies each
///   parameter with the provided argument type, and returns the substituted
///   result type.
fn infer_syntax_function_value_call_with_arg_types(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(callee) = expr.children.first() else {
        errors.push("function-value invocation is missing a callee".to_string());
        return Type::Dynamic;
    };

    let callee_type = apply_subst(
        &infer_syntax_expr(callee, locals, ctx, subst, errors),
        subst,
    );
    match callee_type {
        Type::Function { params, ret } => {
            if params.len() != arg_types.len() {
                errors.push(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params.len(),
                    arg_types.len()
                ));
                return Type::Dynamic;
            }

            for (expected, actual) in params.iter().zip(arg_types.iter()) {
                if let Err(message) = unify(expected, actual, subst) {
                    errors.push(message);
                }
            }

            apply_subst(ret.as_ref(), subst)
        }
        Type::Dynamic => Type::Dynamic,
        other => {
            errors.push(format!(
                "function-value invocation requires function value, found {}",
                pretty_type(&other)
            ));
            Type::Dynamic
        }
    }
}

/// Infers a resolved macro call.
///
/// Inputs:
/// - `macro_name`: source macro identifier.
/// - `arg_types`: inferred argument types.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Macro return type when a visible macro signature matches.
///
/// Transformation:
/// - Looks up macro-call signatures and checks arguments through ordinary
///   function inference, unwrapping macro-specific return wrappers.
fn infer_syntax_macro_call(
    name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let candidates: Vec<_> = ctx
        .signatures
        .iter()
        .filter_map(|((candidate_name, arity), scheme)| {
            if candidate_name == name {
                Some((*arity, scheme))
            } else {
                None
            }
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    for (arity, scheme) in candidates.iter() {
        if *arity == arg_types.len() {
            match infer_function_with_bounds(scheme, Some(name), arg_types, ctx, subst) {
                Ok(ty) => return Some(unwrap_macro_return_type(ty)),
                Err(message) => {
                    errors.push(message);
                    return Some(Type::Dynamic);
                }
            }
        }
    }

    let arities = candidates
        .iter()
        .map(|(arity, _)| arity.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "wrong arity for macro `{}`: expected one of [{}] args, found {}",
        name,
        arities,
        arg_types.len()
    ));
    Some(Type::Dynamic)
}

/// Infers a call expression after argument types are known.
///
/// Inputs:
/// - `expr`: call expression.
/// - `arg_types`: previously inferred argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Resolved call return type.
///
/// Transformation:
/// - Applies call resolution without re-inferring arguments, allowing pipe and
///   function-value callers to share dispatch.
fn infer_syntax_call_with_arg_types(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if expr.remote.is_none() {
        if let Some(ty) =
            infer_syntax_primitive_receiver_method_call(expr, arg_types, locals, ctx, subst, errors)
        {
            return ty;
        }
        if let Some(ty) =
            infer_syntax_receiver_method_call(expr, arg_types, locals, ctx, subst, errors)
        {
            return ty;
        }
    }

    let Some(function_name) = syntax_callee_name(expr) else {
        return Type::Dynamic;
    };

    if expr.remote.is_none() && syntax_callee_is_var(expr) {
        if let Some(constructed) =
            infer_constructor_call(function_name, &arg_types, ctx, subst, errors)
        {
            return constructed;
        }

        if let Some(imported) = ctx.constructor_aliases.get(function_name) {
            if let Some(interface) = ctx.interface_map.get(&imported.module) {
                if interface.opaque_types.contains(&imported.name) {
                    errors.push(format!(
                        "cannot construct opaque type {}.{} outside defining module",
                        imported.module, imported.name
                    ));
                    return Type::Dynamic;
                }
                if let Some(schemes) = parse_interface_constructor_schemes(
                    interface
                        .constructors
                        .get(&imported.name)
                        .map(Vec::as_slice),
                    interface,
                ) {
                    if let Some(constructed) = infer_constructor_schemes(
                        function_name,
                        &schemes,
                        &arg_types,
                        subst,
                        errors,
                    ) {
                        let interface_aliases = interface_type_aliases(interface);
                        return expand_type_aliases(&constructed, &interface_aliases);
                    }
                }
            }
        }

        if let Some(constructed) =
            infer_opaque_constructor(function_name, &arg_types, ctx.aliases, errors)
        {
            return constructed;
        }

        if let Some(Type::Function { params, ret }) =
            locals.get(function_name).map(|ty| apply_subst(ty, subst))
        {
            if params.len() != arg_types.len() {
                errors.push(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params.len(),
                    arg_types.len()
                ));
                return Type::Dynamic;
            }

            for (expected, actual) in params.iter().zip(arg_types.iter()) {
                if let Err(message) = unify(expected, actual, subst) {
                    errors.push(message);
                }
            }

            return apply_subst(ret.as_ref(), subst);
        }

        if is_constructor_name(function_name) {
            errors.push(format!(
                "unknown constructor {} / {}",
                function_name,
                arg_types.len()
            ));
            return Type::Dynamic;
        }
    }

    if let Some(module_name) = expr.remote.as_deref() {
        return infer_syntax_remote_call(module_name, function_name, arg_types, ctx, subst, errors);
    }

    infer_syntax_local_call(function_name, arg_types, ctx, subst, errors)
}

/// Returns whether a local function can also accept a pipe-inserted call.
///
/// Inputs:
/// - `function_name`: unqualified pipe target name.
/// - `arg_types`: pipe-inserted argument types, including the receiver/input as
///   the first argument.
/// - `ctx` and `subst`: active inference context and current substitutions.
///
/// Output:
/// - `true` when a local function signature or resolved local function symbol
///   can accept the same pipe-inserted call.
/// - `false` when no local function candidate matches.
///
/// Transformation:
/// - Tries explicit source function schemes with cloned substitutions so
///   ambiguity detection does not mutate the real inference state or emit
///   diagnostics. Resolved HIR symbols are intentionally ignored here because
///   receiver methods also appear in the backend receiver-first symbol table;
///   those are not separate source-level function declarations.
fn local_function_pipe_target_matches(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    if let Some(scheme) = ctx
        .signatures
        .get(&(function_name.to_string(), arg_types.len()))
    {
        let mut trial_subst = subst.clone();
        if infer_function_with_bounds(
            scheme,
            Some(function_name),
            arg_types,
            ctx,
            &mut trial_subst,
        )
        .is_ok()
        {
            return true;
        }
    }

    false
}

/// Returns whether a selected imported function can accept pipe insertion.
///
/// Inputs:
/// - `function_name`: local selected-import name.
/// - `arg_types`: pipe-inserted argument types, including the receiver/input as
///   the first argument.
/// - `ctx` and `subst`: active inference context and current substitutions.
///
/// Output:
/// - `true` when the selected import resolves to a provider signature that can
///   accept the pipe-inserted arguments.
/// - `false` when the name is not a selected import, the provider interface is
///   unavailable, the arity is missing, or the arguments do not match.
///
/// Transformation:
/// - Resolves the local selected-import target through loaded interfaces and
///   checks the provider function scheme with cloned substitutions so ambiguity
///   detection does not mutate inference state or emit import diagnostics.
fn imported_function_pipe_target_matches(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let Some(target) = ctx.function_imports.get(function_name) else {
        return false;
    };
    let resolved_module = ctx
        .module_aliases
        .get(&target.module)
        .map(String::as_str)
        .unwrap_or(target.module.as_str());
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        return false;
    };
    let Some(signature) = interface
        .functions
        .get(&(target.function.clone(), arg_types.len()))
    else {
        return false;
    };
    let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) else {
        return false;
    };
    let mut trial_subst = subst.clone();
    infer_function_with_bounds(
        &scheme,
        Some(function_name),
        arg_types,
        ctx,
        &mut trial_subst,
    )
    .is_ok()
}

/// Infers pipe-forward syntax that targets a receiver method.
///
/// Inputs:
/// - `left`: pipe input expression used as the receiver.
/// - `right`: call expression written as `method(args...)`.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type)` when the right side names a receiver method for that arity.
/// - `None` when no receiver-method candidate exists, allowing ordinary pipe
///   insertion to run.
///
/// Transformation:
/// - Resolves `value |> method(args)` as `value.method(args)` before ordinary
///   function insertion. Immutable receiver methods return their declared
///   return type. Mutable receiver methods return the updated receiver type for
///   pipe continuation, regardless of the command method's declared result.
fn infer_syntax_receiver_method_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    if right.remote.is_some() || !syntax_callee_is_var(right) {
        return None;
    }

    let method = syntax_callee_name(right)?;
    let arity = right.children.len().saturating_sub(1);
    let candidates = ctx.receiver_methods.get(&(method.to_string(), arity))?;
    let receiver_type = infer_syntax_expr(left, locals, ctx, subst, errors);
    let arg_types = right
        .children
        .iter()
        .skip(1)
        .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
        .collect::<Vec<_>>();
    let mut pipe_inserted_arg_types = Vec::with_capacity(arg_types.len() + 1);
    pipe_inserted_arg_types.push(receiver_type.clone());
    pipe_inserted_arg_types.extend(arg_types.iter().cloned());

    for candidate in candidates {
        let mut trial_subst = subst.clone();
        if unify(&candidate.receiver_type, &receiver_type, &mut trial_subst).is_err() {
            continue;
        }
        if local_function_pipe_target_matches(method, &pipe_inserted_arg_types, ctx, &trial_subst)
            || imported_function_pipe_target_matches(
                method,
                &pipe_inserted_arg_types,
                ctx,
                &trial_subst,
            )
        {
            errors.push(format!(
                "ambiguous pipe target `{}` / {}: receiver method and ordinary function both match; use explicit receiver or function call syntax",
                method,
                arity
            ));
            return Some(Type::Dynamic);
        }
        match infer_function_with_bounds(
            &candidate.scheme,
            Some(method),
            &arg_types,
            ctx,
            &mut trial_subst,
        ) {
            Ok(ty) => {
                let pipe_type = if candidate.receiver_mutable {
                    apply_subst(&receiver_type, &trial_subst)
                } else {
                    ty
                };
                *subst = trial_subst;
                return Some(pipe_type);
            }
            Err(message) => {
                errors.push(message);
                return Some(Type::Dynamic);
            }
        }
    }

    let candidate_types = candidates
        .iter()
        .map(|candidate| pretty_type(&candidate.receiver_type))
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "no receiver method `{}` / {} for {}; candidates: {}",
        method,
        arity,
        pretty_type(&receiver_type),
        candidate_types
    ));
    Some(Type::Dynamic)
}

/// Infers a pipe-forwarding expression.
///
/// Inputs:
/// - `expr`: syntax-output pipe expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Return type of the right-side call after inserting the left value.
///
/// Transformation:
/// - Validates pipe shape, handles mutable receiver pipe forwarding, and
///   rewrites ordinary pipes to call inference with the left value prepended.
fn infer_syntax_pipe_forward(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(left) = expr.children.first() else {
        return Type::Dynamic;
    };
    let Some(right) = expr.children.get(1) else {
        return Type::Dynamic;
    };
    if !matches!(
        right.kind,
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall
    ) {
        errors.push("right side of |> must be a function call".to_string());
        let _ = infer_syntax_expr(left, locals, ctx, subst, errors);
        let _ = infer_syntax_expr(right, locals, ctx, subst, errors);
        return Type::Dynamic;
    }

    if right.kind == SyntaxExprKind::Call {
        if let Some(ty) =
            infer_syntax_receiver_method_pipe_forward(left, right, locals, ctx, subst, errors)
        {
            return ty;
        }
    }

    let mut arg_types = Vec::with_capacity(right.children.len());
    arg_types.push(infer_syntax_expr(left, locals, ctx, subst, errors));
    arg_types.extend(
        right
            .children
            .iter()
            .skip(1)
            .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors)),
    );

    match right.kind {
        SyntaxExprKind::FunctionCall => infer_syntax_function_value_call_with_arg_types(
            right, &arg_types, locals, ctx, subst, errors,
        ),
        _ => infer_syntax_call_with_arg_types(right, &arg_types, locals, ctx, subst, errors),
    }
}

/// Infers a raw struct construction expression from syntax output.
///
/// Inputs:
/// - `expr`: syntax-output record construction node carrying the target type
///   name and field expressions.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Type::Named` for the constructed source type when inference can continue.
///
/// Transformation:
/// - Typechecks every field value, then enforces the Terlan visibility rule
///   that imported/public struct type identity does not grant raw construction
///   authority outside the defining module.
fn infer_syntax_record_construct(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    for field in &expr.fields {
        let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        let _ = &field.key;
    }

    let name = expr.text.clone().unwrap_or_default();
    if let Some(imported) = ctx.imported_type_names.get(&name) {
        errors.push(format!(
            "cannot raw-construct imported struct {}.{} outside defining module; use an exported constructor",
            imported.module, imported.name
        ));
    }

    Type::Named {
        module: None,
        name,
        args: Vec::new(),
    }
}

/// Infers a constructor-chain expression.
///
/// Inputs:
/// - `expr`: syntax-output constructor-chain expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Type of the extended record expression.
///
/// Transformation:
/// - Infers the base constructor expression and then validates the extension
///   record as the resulting chain value.
fn infer_syntax_constructor_chain(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(base) = expr.children.first() else {
        errors.push("constructor chain expression is missing base expression".to_string());
        return Type::Dynamic;
    };

    let Some(record) = expr.children.get(1) else {
        errors
            .push("constructor chain expression is missing constructor target record".to_string());
        let _ = infer_syntax_expr(base, locals, ctx, subst, errors);
        return Type::Dynamic;
    };

    let _ = infer_syntax_expr(base, locals, ctx, subst, errors);

    if record.kind != SyntaxExprKind::RecordConstruct {
        errors.push("constructor chain requires a record construct on the right side".to_string());
        let _ = infer_syntax_expr(record, locals, ctx, subst, errors);
        return Type::Dynamic;
    }

    infer_syntax_record_construct(record, locals, ctx, subst, errors)
}

/// Infers a record field access expression.
///
/// Inputs:
/// - `expr`: syntax-output record access expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Field type when the record and field are known; otherwise `Dynamic`.
///
/// Transformation:
/// - Infers the receiver, resolves the record schema, and extracts the selected
///   field type.
fn infer_syntax_record_access(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let _ = expr
        .children
        .first()
        .map(|value| infer_syntax_expr(value, locals, ctx, subst, errors));
    let (name, field) = expr
        .text
        .as_deref()
        .and_then(|text| text.split_once('.'))
        .unwrap_or_default();
    if let Some(fields) = ctx.struct_fields.get(name) {
        if let Some(field_type) = fields.get(field) {
            field_type.clone()
        } else {
            errors.push(format!("unknown field {} on struct {}", field, name));
            Type::Dynamic
        }
    } else {
        errors.push(format!("unknown struct {}", name));
        Type::Dynamic
    }
}

/// Infers a dot field access expression.
///
/// Inputs:
/// - `expr`: syntax-output field access expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Field type when the receiver shape is known; otherwise `Dynamic`.
///
/// Transformation:
/// - Infers the receiver and resolves field lookup against known struct and
///   map-like shapes.
fn infer_syntax_field_access(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let receiver = expr
        .children
        .first()
        .map(|value| apply_subst(&infer_syntax_expr(value, locals, ctx, subst, errors), subst))
        .unwrap_or(Type::Dynamic);
    let field = expr.text.as_deref().unwrap_or_default();
    match receiver {
        Type::Named { name, .. } if name == "Markdown" => match field {
            "raw" => Type::Binary,
            "html" => Type::Named {
                module: None,
                name: "Html".to_string(),
                args: vec![Type::Never],
            },
            _ => {
                errors.push(format!("unknown field {} on Markdown import", field));
                Type::Dynamic
            }
        },
        Type::Named { name, .. } => {
            if let Some(fields) = ctx.struct_fields.get(&name) {
                if let Some(field_type) = fields.get(field) {
                    field_type.clone()
                } else {
                    errors.push(format!("unknown field {} on struct {}", field, name));
                    Type::Dynamic
                }
            } else {
                errors.push(format!(
                    "field access requires struct receiver, found {}",
                    pretty_type(&Type::Named {
                        module: None,
                        name,
                        args: Vec::new(),
                    })
                ));
                Type::Dynamic
            }
        }
        other => {
            errors.push(format!(
                "field access requires struct receiver, found {}",
                pretty_type(&other)
            ));
            Type::Dynamic
        }
    }
}

/// Infers a record update expression.
///
/// Inputs:
/// - `expr`: syntax-output record update expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Updated record type when the base record is valid.
///
/// Transformation:
/// - Infers the base record, validates updated fields against the record
///   schema, and returns the original record type.
fn infer_syntax_record_update(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let base = expr
        .children
        .first()
        .map(|value| infer_syntax_expr(value, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    for field in &expr.fields {
        let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        let _ = &field.key;
    }
    let _ = &expr.text;
    base
}

/// Infers an HTML/template instantiation expression.
///
/// Inputs:
/// - `expr`: syntax-output template instantiation expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Template result type, or `Dynamic` when the template is unresolved.
///
/// Transformation:
/// - Checks supplied props against the visible template scheme and returns the
///   backend-neutral HTML/template value type.
fn infer_syntax_template_instantiation(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let name = expr.text.as_deref().unwrap_or_default();
    let mut provided = HashSet::new();
    let Some(template) = ctx.templates.get(name) else {
        for field in &expr.fields {
            let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        }
        errors.push(format!("unknown template `{}`", name));
        return Type::Dynamic;
    };

    for field in &expr.fields {
        if !provided.insert(field.key.clone()) {
            errors.push(format!(
                "duplicate prop `{}` in template `{}` instantiation",
                field.key, name
            ));
        }

        let actual = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        let Some(expected) = template.props.get(&field.key) else {
            errors.push(format!(
                "template `{}` instantiation has unknown prop `{}`",
                name, field.key
            ));
            continue;
        };

        let expected = expand_type_aliases(expected, ctx.aliases);
        let actual = expand_type_aliases(&actual, ctx.aliases);
        if let Err(message) = unify(&expected, &actual, subst) {
            errors.push(format!(
                "template `{}` prop `{}`: {}",
                name, field.key, message
            ));
        }
    }

    for prop_name in template.props.keys() {
        if !provided.contains(prop_name) {
            errors.push(format!(
                "template `{}` instantiation is missing required prop `{}`",
                name, prop_name
            ));
        }
    }

    Type::Named {
        module: None,
        name: "Html".to_string(),
        args: vec![Type::Dynamic],
    }
}

/// Extracts the source-visible callee name from a call expression.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
///
/// Output:
/// - Callee text when the call head is a variable or atom.
///
/// Transformation:
/// - Reads the first call child and returns its preserved text only for
///   name-like callee nodes.
pub(super) fn syntax_callee_name(expr: &SyntaxExprOutput) -> Option<&str> {
    expr.children.first().and_then(|callee| match callee.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => callee.text.as_deref(),
        _ => None,
    })
}

/// Checks whether a call expression's callee is a variable node.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
///
/// Output:
/// - `true` when the first child is a variable callee.
///
/// Transformation:
/// - Examines only the call head shape without resolving the identifier.
pub(super) fn syntax_callee_is_var(expr: &SyntaxExprOutput) -> bool {
    matches!(
        expr.children.first().map(|callee| callee.kind),
        Some(SyntaxExprKind::Var)
    )
}

/// Infers an explicit remote call.
///
/// Inputs:
/// - `expr`: call expression with a remote qualifier.
/// - `arg_types`: inferred argument types.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Resolved remote call return type.
///
/// Transformation:
/// - Resolves imported modules, trait calls, target intrinsics, and interface
///   functions before falling back to dynamic typing with diagnostics.
fn infer_syntax_remote_call(
    module_name: &str,
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let resolved_module_name = ctx
        .module_aliases
        .get(module_name)
        .map(String::as_str)
        .unwrap_or(module_name);

    if resolved_module_name == "Html" && function_name == "raw" {
        if arg_types.len() != 1 {
            errors.push(format!(
                "function arity mismatch: expected 1 args, found {}",
                arg_types.len()
            ));
            return Type::Dynamic;
        }
        if let Err(message) = unify(&Type::Binary, &arg_types[0], subst) {
            errors.push(message);
        }
        return Type::Named {
            module: None,
            name: "Html".to_string(),
            args: vec![Type::Dynamic],
        };
    }

    let trait_key = (resolved_module_name.to_string(), function_name.to_string());
    if let Some(impls) = ctx.trait_method_calls.get(&trait_key) {
        let lookup_arg_types = arg_types
            .iter()
            .map(|arg| apply_subst(arg, subst))
            .collect::<Vec<_>>();
        let cached_lookup_arg_types = canonicalize_trait_lookup_types(lookup_arg_types.as_slice());
        let lookup_key = TraitMethodLookupKey {
            trait_name: resolved_module_name.to_string(),
            method_name: function_name.to_string(),
            arg_types: cached_lookup_arg_types,
        };
        let lookup_result = {
            let cache = ctx.trait_lookup_cache.borrow();
            if let Some(cached) = cache.method_calls.get(&lookup_key).copied() {
                Some(cached)
            } else {
                drop(cache);
                let mut matching = None::<usize>;
                let mut matches = 0usize;
                for (index, impl_candidate) in impls.iter().enumerate() {
                    if !trait_method_candidate_matches_call(
                        impl_candidate,
                        &lookup_arg_types,
                        ctx,
                        subst,
                    ) {
                        continue;
                    }
                    let mut trial_subst = subst.clone();
                    if infer_function_call(
                        &impl_candidate.scheme,
                        &lookup_arg_types,
                        ctx,
                        &mut trial_subst,
                    )
                    .is_ok()
                    {
                        matches += 1;
                        if matching.is_none() {
                            matching = Some(index);
                        } else {
                            break;
                        }
                    }
                }

                let resolved = match matching {
                    None => TraitMethodLookupResult::NoMatch,
                    Some(index) if matches == 1 => TraitMethodLookupResult::Single(index),
                    Some(_) => TraitMethodLookupResult::Ambiguous,
                };
                ctx.trait_lookup_cache
                    .borrow_mut()
                    .method_calls
                    .insert(lookup_key, resolved);
                Some(resolved)
            }
        };

        let provided_args = arg_types
            .iter()
            .map(pretty_type)
            .collect::<Vec<_>>()
            .join(", ");
        match lookup_result {
            Some(TraitMethodLookupResult::Single(index)) => {
                let mut inferred_subst = subst.clone();
                let mut success = None::<(Type, HashMap<TypeVarId, Type>)>;
                if let Some(impl_candidate) = impls.get(index) {
                    if let Ok(ty) = infer_function_call(
                        &impl_candidate.scheme,
                        &lookup_arg_types,
                        ctx,
                        &mut inferred_subst,
                    ) {
                        success = Some((ty, inferred_subst));
                    }
                }
                if let Some((ty, inferred_subst)) = success {
                    *subst = inferred_subst;
                    return ty;
                }
                errors.push(format!(
                    "at `{}.{}` call site: no impl for trait method {}.{} with provided arguments [{}]",
                    resolved_module_name, function_name, resolved_module_name, function_name, provided_args
                ));
                return Type::Dynamic;
            }
            Some(TraitMethodLookupResult::Ambiguous) => {
                errors.push(format!(
                    "at `{}.{}` call site: ambiguous trait method {}.{}",
                    resolved_module_name, function_name, resolved_module_name, function_name
                ));
                return Type::Dynamic;
            }
            _ => {
                if let Some(ty) = infer_trait_method_call_from_current_bounds(
                    resolved_module_name,
                    function_name,
                    &lookup_arg_types,
                    ctx,
                    subst,
                ) {
                    return ty;
                }
                errors.push(format!(
                    "at `{}.{}` call site: no impl for trait method {}.{} with provided arguments [{}]",
                    resolved_module_name, function_name, resolved_module_name, function_name, provided_args
                ));
                return Type::Dynamic;
            }
        }
    }

    if let Some(interface) = ctx.interface_map.get(resolved_module_name) {
        if let Some(signature) = interface
            .functions
            .get(&(function_name.to_string(), arg_types.len()))
        {
            if let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) {
                match infer_function_with_bounds(
                    &scheme,
                    Some(function_name),
                    arg_types,
                    ctx,
                    subst,
                ) {
                    Ok(ty) => return ty,
                    Err(message) => {
                        errors.push(message);
                        return Type::Dynamic;
                    }
                }
            }
        }
        if let Some(schemes) = parse_interface_constructor_schemes(
            interface.constructors.get(function_name).map(Vec::as_slice),
            interface,
        ) {
            if let Some(constructed) =
                infer_constructor_schemes(function_name, &schemes, arg_types, subst, errors)
            {
                let interface_aliases = interface_type_aliases(interface);
                return expand_type_aliases(&constructed, &interface_aliases);
            }
        }
        let interface_aliases = interface_type_aliases(interface);
        let qualified_alias_name = format!("{}.{}", resolved_module_name, function_name);
        let mut qualified_aliases = interface_aliases.clone();
        if let Some(alias) = interface_aliases.get(function_name) {
            qualified_aliases.insert(qualified_alias_name.clone(), alias.clone());
        }
        if let Some(schemes) =
            alias_constructor_call_schemes(&qualified_alias_name, &qualified_aliases)
        {
            if let Some(constructed) =
                infer_constructor_schemes(function_name, &schemes, arg_types, subst, errors)
            {
                return expand_type_aliases(&constructed, &qualified_aliases);
            }
        }
        if function_name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
            && interface.opaque_types.contains(function_name)
        {
            errors.push(format!(
                "cannot construct opaque type {}.{} outside defining module",
                resolved_module_name, function_name
            ));
            return Type::Dynamic;
        }
    }

    if is_constructor_name(function_name) {
        errors.push(format!(
            "unknown constructor {}.{} / {}",
            resolved_module_name,
            function_name,
            arg_types.len()
        ));
        return Type::Dynamic;
    }

    if resolved_module_name == "Group" && function_name == "broadcast" && arg_types.len() == 2 {
        if let Type::Named {
            name,
            args: group_args,
            ..
        } = &arg_types[0]
        {
            if name == "Group" && group_args.len() == 1 {
                if let Err(message) = unify(&group_args[0], &arg_types[1], subst) {
                    let expected = alias_name_for_type(&group_args[0], ctx.aliases)
                        .unwrap_or_else(|| pretty_type(&group_args[0]));
                    errors.push(format!(
                        "expected {} found {}",
                        expected,
                        pretty_type(&arg_types[1])
                    ));
                    let _ = message;
                }
            }
        }
        return Type::LiteralAtom("ok".to_string());
    }

    if (resolved_module_name == "Route" || resolved_module_name.ends_with(".Route"))
        && function_name == "to_path"
        && arg_types.len() == 1
    {
        return Type::Binary;
    }

    Type::Dynamic
}

/// Checks whether a trait candidate can own the current call.
///
/// Inputs:
/// - `candidate`: resolved trait method candidate with concrete impl type args.
/// - `arg_types`: inferred source-visible call argument types.
/// - `ctx`: expression inference context containing alias expansion rules.
/// - `subst`: current type-variable substitution table.
///
/// Output:
/// - `true` when the candidate has no owner type information or when its first
///   impl type argument unifies with the call's first argument type.
/// - `false` when a different concrete conformance owns the method.
///
/// Transformation:
/// - Uses a cloned substitution table and transparent alias expansion to filter
///   trait method candidates before ambiguity counting. This keeps imported
///   multi-conformance traits such as `std.core.String.Show` from treating
///   `Show[Int]`, `Show[Bool]`, and `Show[String]` as simultaneous matches for
///   one receiver/value argument.
fn trait_method_candidate_matches_call(
    candidate: &ResolvedTraitMethod,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let Some(owner_type) = candidate.impl_type_args.first() else {
        return true;
    };
    let Some(first_arg_type) = arg_types.first() else {
        return true;
    };

    let mut trial_subst = subst.clone();
    if unify(owner_type, first_arg_type, &mut trial_subst).is_ok() {
        return true;
    }

    let owner_expanded = expand_type_aliases(owner_type, ctx.aliases);
    let arg_expanded = expand_type_aliases(first_arg_type, ctx.aliases);
    unify(&owner_expanded, &arg_expanded, &mut trial_subst).is_ok()
}

/// Infers a local receiver-method call.
///
/// Inputs:
/// - `expr`: syntax-output call expression whose callee may be field access.
/// - `arg_types`: inferred non-receiver argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference context.
///
/// Output:
/// - `Some(Type)` for a resolved local receiver method or a method-shaped call
///   that has candidates but no matching receiver.
/// - `None` when the expression is not a receiver-method call known to the
///   current module.
///
/// Transformation:
/// - Reads `receiver.method(args...)` from the field-access callee, infers the
///   receiver type, selects a receiver-method signature by method/arity and
///   receiver unification, then checks the non-receiver arguments with the
///   existing function-scheme inference path.
fn infer_syntax_receiver_method_call(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let candidates = ctx
        .receiver_methods
        .get(&(method.to_string(), arg_types.len()))?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_expr(receiver, locals, ctx, subst, errors);

    for candidate in candidates {
        let mut trial_subst = subst.clone();
        if unify(&candidate.receiver_type, &receiver_type, &mut trial_subst).is_err() {
            continue;
        }
        match infer_function_with_bounds(
            &candidate.scheme,
            Some(method),
            arg_types,
            ctx,
            &mut trial_subst,
        ) {
            Ok(ty) => {
                *subst = trial_subst;
                return Some(ty);
            }
            Err(message) => {
                errors.push(message);
                return Some(Type::Dynamic);
            }
        }
    }

    let candidate_types = candidates
        .iter()
        .map(|candidate| pretty_type(&candidate.receiver_type))
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "no receiver method `{}` / {} for {}; candidates: {}",
        method,
        arg_types.len(),
        pretty_type(&receiver_type),
        candidate_types
    ));
    Some(Type::Dynamic)
}

/// Infers compiler-known primitive receiver method calls.
///
/// Inputs:
/// - `expr`: syntax-output call expression whose callee may be field access.
/// - `arg_types`: inferred non-receiver argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference context.
///
/// Output:
/// - `Some(Type)` for supported primitive receiver calls.
/// - `None` when the expression is not a supported primitive receiver call.
///
/// Transformation:
/// - Reads the receiver type from the field-access callee, prepends that type to
///   the argument check, validates the primitive method's arity and parameter
///   types, and returns the method result type.
fn infer_syntax_primitive_receiver_method_call(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_expr(receiver, locals, ctx, subst, errors);
    let scheme = primitive_receiver_method_scheme(&receiver_type, method, arg_types.len())?;
    infer_function_with_bounds(&scheme, Some(method), arg_types, ctx, subst)
        .map(Some)
        .unwrap_or_else(|message| {
            errors.push(message);
            Some(Type::Dynamic)
        })
}

/// Unwraps macro-specific return wrappers.
///
/// Inputs:
/// - `ty`: inferred macro implementation return type.
///
/// Output:
/// - User-visible macro expansion result type.
///
/// Transformation:
/// - Removes one known macro wrapper layer and leaves all other types
///   unchanged.
fn unwrap_macro_return_type(ty: Type) -> Type {
    match ty {
        Type::Named {
            module,
            name: tag,
            args,
        } if module.is_none() && tag == "Ast" && args.len() == 1 => {
            args.into_iter().next().unwrap_or(Type::Dynamic)
        }
        other => other,
    }
}

/// Infers a local named call.
///
/// Inputs:
/// - `expr`: call expression without an explicit remote qualifier.
/// - `arg_types`: inferred argument types.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Resolved local call return type.
///
/// Transformation:
/// - Checks constructors, local functions, imports, aliases, trait shorthands,
///   receiver forms, and intrinsics in source-call priority order.
fn infer_syntax_local_call(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if is_removed_implicit_builtin_call(function_name, arg_types.len()) {
        errors.push(format!(
            "`{function_name}/{}` is not part of the implicit prelude; import or define it explicitly",
            arg_types.len()
        ));
        return Type::Dynamic;
    }

    if let Some(scheme) = builtin_call(function_name, arg_types.len()) {
        if let Err(message) =
            infer_function_with_bounds(&scheme, Some(function_name), arg_types, ctx, subst)
        {
            errors.push(message);
        }
        return scheme.ret;
    }

    if let Some(ty) =
        infer_syntax_imported_function_call(function_name, arg_types, ctx, subst, errors)
    {
        return ty;
    }

    if let Some(scheme) = ctx
        .signatures
        .get(&(function_name.to_string(), arg_types.len()))
    {
        match infer_function_with_bounds(scheme, Some(function_name), arg_types, ctx, subst) {
            Ok(ty) => return ty,
            Err(message) => {
                errors.push(message);
                return Type::Dynamic;
            }
        }
    }

    if let Some(symbol) = ctx
        .local_fns
        .get(&(function_name.to_string(), arg_types.len()))
    {
        if let Some(scheme) = parse_symbol_scheme(symbol) {
            match infer_function_with_bounds(&scheme, Some(function_name), arg_types, ctx, subst) {
                Ok(ty) => return ty,
                Err(message) => {
                    errors.push(message);
                    return Type::Dynamic;
                }
            }
        }
    }

    Type::Dynamic
}

/// Infers a selected imported function call.
///
/// Inputs:
/// - `function_name`: local call name from source, possibly an import alias.
/// - `arg_types`: already inferred argument types.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type)` when the local name is a selected function import.
/// - `None` when the local name is not imported as a function.
///
/// Transformation:
/// - Resolves the local import target to its provider module interface, parses
///   the public function signature for the call arity, and reuses ordinary
///   function-call inference so argument mismatches are reported before backend
///   emission.
fn infer_syntax_imported_function_call(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let target = ctx.function_imports.get(function_name)?;
    let resolved_module = ctx
        .module_aliases
        .get(&target.module)
        .map(String::as_str)
        .unwrap_or(target.module.as_str());
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        errors.push(spanned_expression_error(
            target.span,
            missing_imported_function_interface_message(
                resolved_module,
                &target.function,
                ctx.interface_map,
            ),
        ));
        return Some(Type::Dynamic);
    };

    let Some(signature) = interface
        .functions
        .get(&(target.function.clone(), arg_types.len()))
    else {
        errors.push(spanned_expression_error(
            target.span,
            missing_imported_function_message(interface, &target.function, arg_types.len()),
        ));
        return Some(Type::Dynamic);
    };

    let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) else {
        errors.push(format!(
            "cannot parse imported function signature {}.{} / {}",
            resolved_module,
            target.function,
            arg_types.len()
        ));
        return Some(Type::Dynamic);
    };

    match infer_function_with_bounds(&scheme, Some(function_name), arg_types, ctx, subst) {
        Ok(ty) => Some(ty),
        Err(message) => {
            errors.push(message);
            Some(Type::Dynamic)
        }
    }
}

/// Infers a case expression.
///
/// Inputs:
/// - `expr`: syntax-output case expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Union of branch body types.
///
/// Transformation:
/// - Infers the scrutinee, type-checks each pattern against it with scoped
///   locals, applies guards, and normalizes branch body types.
fn infer_syntax_case_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let scrutinee_type = expr
        .children
        .first()
        .map(|scrutinee| infer_syntax_expr(scrutinee, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let branches = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            if let Some(pattern) = clause.patterns.first() {
                if let Err(message) = check_syntax_pattern(
                    pattern,
                    &scrutinee_type,
                    ctx.aliases,
                    Some(ctx),
                    &mut clause_locals,
                    &mut clause_subst,
                ) {
                    errors.push(message);
                }
            }

            if let Some(guard) = clause.guard.as_ref() {
                refine_by_syntax_guard(guard, &mut clause_locals, ctx.aliases, &mut clause_subst);
            }

            let branch_type =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        })
        .collect::<Vec<_>>();

    normalize_union(branches)
}

/// Infers a try expression.
///
/// Inputs:
/// - `expr`: syntax-output try expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Union of try body, catch body, and after body types.
///
/// Transformation:
/// - Infers the body and each catch/after clause in scoped environments while
///   preserving recoverable diagnostics.
fn infer_syntax_try_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let protected_type = expr
        .children
        .first()
        .map(|body| infer_syntax_expr(body, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let mut branches = Vec::new();

    if expr.clauses.is_empty() {
        branches.push(protected_type.clone());
    } else {
        branches.extend(expr.clauses.iter().map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            if let Some(pattern) = clause.patterns.first() {
                if let Err(message) = check_syntax_pattern(
                    pattern,
                    &protected_type,
                    ctx.aliases,
                    Some(ctx),
                    &mut clause_locals,
                    &mut clause_subst,
                ) {
                    errors.push(message);
                }
            }

            if let Some(guard) = clause.guard.as_ref() {
                refine_by_syntax_guard(guard, &mut clause_locals, ctx.aliases, &mut clause_subst);
            }

            let branch_type =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        }));
    }

    branches.extend(expr.catch_clauses.iter().map(|clause| {
        let mut clause_locals = locals.clone();
        let mut clause_subst = subst.clone();
        if let Some(pattern) = clause.patterns.first() {
            if let Err(message) = check_syntax_pattern(
                pattern,
                &Type::Dynamic,
                ctx.aliases,
                Some(ctx),
                &mut clause_locals,
                &mut clause_subst,
            ) {
                errors.push(message);
            }
        }

        if let Some(guard) = clause.guard.as_ref() {
            refine_by_syntax_guard(guard, &mut clause_locals, ctx.aliases, &mut clause_subst);
        }

        let branch_type =
            infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
        apply_subst(&branch_type, &clause_subst)
    }));

    if let Some(after) = expr.try_after.as_ref() {
        let _ = infer_syntax_expr(&after.trigger, locals, ctx, subst, errors);
        let _ = infer_syntax_expr(&after.body, locals, ctx, subst, errors);
    }

    normalize_union(branches)
}

/// Infers an if expression.
///
/// Inputs:
/// - `expr`: syntax-output if expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Union of branch body types.
///
/// Transformation:
/// - Requires boolean-like conditions, refines branch locals through guards,
///   and normalizes branch result types.
fn infer_syntax_if_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let branches = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_subst = subst.clone();
            if let Some(condition) = clause.guard.as_ref() {
                let condition_type =
                    infer_syntax_expr(condition, locals, ctx, &mut clause_subst, errors);
                if let Err(message) = unify(&Type::Bool, &condition_type, &mut clause_subst) {
                    errors.push(message);
                }
            }
            let branch_type =
                infer_syntax_expr(&clause.body, locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        })
        .collect::<Vec<_>>();

    normalize_union(branches)
}

/// Infers a list comprehension expression.
///
/// Inputs:
/// - `expr`: syntax-output list comprehension.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - List type containing the inferred yielded element type.
///
/// Transformation:
/// - Infers the source iterable, binds generator pattern locals, checks the
///   optional guard, and infers the yielded expression in item scope.
fn infer_syntax_list_comprehension(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let source_type = expr
        .children
        .get(1)
        .map(|source| infer_syntax_expr(source, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let element_type = match expand_type_aliases(&source_type, ctx.aliases) {
        Type::List(elem) => *elem,
        Type::Dynamic | Type::Term => Type::Dynamic,
        other => {
            if let Some(iterable_item_type) = infer_iterable_comprehension_element_type(&other, ctx)
            {
                iterable_item_type
            } else {
                errors.push(format!(
                    "list comprehension source must be List or Iterable, found {}",
                    pretty_type(&other)
                ));
                Type::Dynamic
            }
        }
    };
    let mut item_locals = locals.clone();
    let mut item_subst = subst.clone();
    if let Some(pattern) = expr.patterns.first() {
        if let Err(message) = check_syntax_pattern(
            pattern,
            &element_type,
            ctx.aliases,
            Some(ctx),
            &mut item_locals,
            &mut item_subst,
        ) {
            errors.push(message);
        }
    }
    if let Some(guard) = expr.children.get(2) {
        refine_by_syntax_guard(guard, &mut item_locals, ctx.aliases, &mut item_subst);
        let guard_type = infer_syntax_expr(guard, &item_locals, ctx, &mut item_subst, errors);
        if let Err(message) = unify(&Type::Bool, &guard_type, &mut item_subst) {
            errors.push(format!("list comprehension filter {}", message));
        }
    }
    let item_type = expr
        .children
        .first()
        .map(|item| infer_syntax_expr(item, &item_locals, ctx, &mut item_subst, errors))
        .unwrap_or(Type::Dynamic);

    Type::List(Box::new(apply_subst(&item_type, &item_subst)))
}

/// Infers the element type produced by an iterable comprehension source.
///
/// Inputs:
/// - `source_type`: inferred source collection type.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Element type yielded by the source.
///
/// Transformation:
/// - Handles built-in list-like sources and delegates target-neutral sources to
///   visible `Iterable`/`Iterator` trait information.
fn infer_iterable_comprehension_element_type(
    source_type: &Type,
    ctx: &ExprInferContext,
) -> Option<Type> {
    let source_type = expand_type_aliases(source_type, ctx.aliases);

    if let Some(impl_args_by_type) = ctx.trait_bound_impl_type_args.get("Iterable") {
        for impl_args in impl_args_by_type {
            if impl_args.len() < 2 {
                continue;
            }

            let collection_arg = expand_type_aliases(&impl_args[0], ctx.aliases);
            let item_arg = expand_type_aliases(&impl_args[1], ctx.aliases);
            let mut local_subst = HashMap::new();

            if unify(&collection_arg, &source_type, &mut local_subst).is_ok() {
                return Some(apply_subst(&item_arg, &local_subst));
            }
        }
    }

    for bound in ctx.current_bounds.iter() {
        if bound.trait_name != "Iterable" || bound.trait_args.len() < 2 {
            continue;
        }

        let collection_arg = expand_type_aliases(&bound.trait_args[0], ctx.aliases);
        let item_arg = expand_type_aliases(&bound.trait_args[1], ctx.aliases);
        let mut local_subst = HashMap::new();

        if unify(&collection_arg, &source_type, &mut local_subst).is_ok() {
            return Some(apply_subst(&item_arg, &local_subst));
        }
    }

    None
}

/// Infers an anonymous function expression.
///
/// Inputs:
/// - `expr`: syntax-output function expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Function type or union of compatible clause function types.
///
/// Transformation:
/// - Creates scoped locals for clause patterns, infers each body, and returns a
///   function type preserving parameter count and return type.
fn infer_syntax_fun_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let union = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            for pattern in &clause.patterns {
                let _ = check_syntax_pattern(
                    pattern,
                    &Type::Dynamic,
                    ctx.aliases,
                    Some(ctx),
                    &mut clause_locals,
                    &mut clause_subst,
                );
            }
            let inferred =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            Type::Function {
                params: vec![Type::Dynamic; clause.patterns.len()],
                ret: Box::new(apply_subst(&inferred, &clause_subst)),
            }
        })
        .collect::<Vec<_>>();
    normalize_union(union)
}

/// Infers a syntax-output let expression.
///
/// Inputs:
/// - `expr`: syntax-output let node with binding names in `patterns`, binding
///   values in `children`, and a required final body child.
/// - `locals`: local type environment visible before the let expression.
/// - `ctx`, `subst`, `errors`: inference context, substitution state, and
///   diagnostics accumulator.
///
/// Output:
/// - Inferred explicit body type.
///
/// Transformation:
/// - Infers binding values left-to-right, extending a scoped local environment
///   after each binding. The caller's `locals` map is not mutated.
fn infer_syntax_let_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if expr.patterns.is_empty() || expr.children.len() != expr.patterns.len() + 1 {
        errors.push("malformed let expression".to_string());
        return Type::Dynamic;
    }

    let mut scoped = locals.clone();
    for (pattern, value) in expr.patterns.iter().zip(expr.children.iter()) {
        let value_type = infer_syntax_expr(value, &scoped, ctx, subst, errors);
        let binding_type = apply_subst(&value_type, subst);
        match pattern.text.as_deref() {
            Some(name) => {
                scoped.insert(name.to_string(), binding_type);
            }
            None => errors.push("malformed let binding name".to_string()),
        }
    }

    infer_syntax_expr(
        &expr.children[expr.patterns.len()],
        &scoped,
        ctx,
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
