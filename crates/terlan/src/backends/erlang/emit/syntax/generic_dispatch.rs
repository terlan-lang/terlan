use super::*;

/// Bounded generic function metadata used by syntax-bridge call lowering.
///
/// Inputs:
/// - Source-visible parameter annotations and trait bounds from a function
///   declaration.
///
/// Output:
/// - Backend-local target metadata used to synthesize hidden trait dictionary
///   arguments for concrete calls.
///
/// Transformation:
/// - Keeps the source parameter type variables and parsed trait bounds
///   together so call lowering can infer concrete bound dictionaries.
#[derive(Debug, Clone)]
pub(super) struct SyntaxGenericFunctionTarget {
    pub(super) params: Vec<String>,
    pub(super) bounds: Vec<SyntaxGenericFunctionBound>,
}

/// Parsed trait bound for a bounded generic function.
///
/// Inputs:
/// - Source bound text such as `Equal[A]`.
///
/// Output:
/// - Trait name plus normalized type arguments.
///
/// Transformation:
/// - Converts textual bounds into the compact form needed by Erlang hidden
///   trait dictionary lowering.
#[derive(Debug, Clone)]
pub(super) struct SyntaxGenericFunctionBound {
    pub(super) trait_name: String,
    pub(super) type_args: Vec<String>,
}

/// Collects bounded generic local functions.
///
/// Inputs:
/// - `module`: syntax-output module containing function declarations.
///
/// Output:
/// - Map keyed by source-visible `(function name, arity)` with parameter type
///   annotations and parsed trait bounds.
///
/// Transformation:
/// - Reads `generic_bounds` from formal syntax output and stores only bounds
///   that parse as named trait applications, enabling backend call lowering to
///   synthesize hidden trait dictionaries for concrete local calls.
pub(super) fn collect_syntax_generic_functions(
    module: &SyntaxModuleOutput,
) -> BTreeMap<(String, usize), SyntaxGenericFunctionTarget> {
    module
        .declarations
        .iter()
        .filter_map(|decl| {
            let SyntaxDeclarationPayload::Function {
                name,
                params,
                generic_bounds,
                ..
            } = &decl.payload
            else {
                return None;
            };
            if generic_bounds.is_empty() {
                return None;
            }
            let bounds = generic_bounds
                .iter()
                .filter_map(|bound| parse_syntax_generic_function_bound(bound))
                .collect::<Vec<_>>();
            if bounds.is_empty() {
                return None;
            }
            Some((
                (name.clone(), params.len()),
                SyntaxGenericFunctionTarget {
                    params: params
                        .iter()
                        .map(|param| normalize_trait_type_text(&param.annotation.text))
                        .collect(),
                    bounds,
                },
            ))
        })
        .collect()
}

/// Parses one generic function trait-bound reference.
///
/// Inputs:
/// - `text`: source bound text such as `Eq[A]`.
///
/// Output:
/// - Parsed trait head and normalized type arguments.
///
/// Transformation:
/// - Splits named type application text using the backend type utility parser
///   so bounded generic function lowering can reason about trait dictionaries
///   without reparsing source declarations.
pub(super) fn parse_syntax_generic_function_bound(
    text: &str,
) -> Option<SyntaxGenericFunctionBound> {
    let compact = compact_type_application(&compact_spaces(text));
    let (trait_name, type_args) = parse_named_type_args(&compact)?;
    Some(SyntaxGenericFunctionBound {
        trait_name: trait_name.to_string(),
        type_args: type_args
            .into_iter()
            .map(|arg| normalize_trait_type_text(&arg))
            .collect(),
    })
}

/// Builds hidden generic-bound parameter names.
///
/// Inputs:
/// - `generic_bounds`: source bound texts from a function declaration.
///
/// Output:
/// - Stable Erlang variable names, one per source bound.
///
/// Transformation:
/// - Parses each bound when possible and includes the trait/type shape in the
///   hidden name for readable generated code; malformed fallback names retain
///   deterministic positional identity.
pub(super) fn generic_bound_param_names(generic_bounds: &[String]) -> Vec<String> {
    generic_bounds
        .iter()
        .enumerate()
        .map(|(index, bound)| {
            if let Some(parsed) = parse_syntax_generic_function_bound(bound) {
                let suffix = std::iter::once(parsed.trait_name)
                    .chain(parsed.type_args)
                    .map(|part| sanitize_erlang_fn_name(&part))
                    .collect::<Vec<_>>()
                    .join("_");
                format!("_TyperTraitDict{}", to_erlang_type_name(&suffix))
            } else {
                format!("_TyperTraitDict{}", index)
            }
        })
        .collect()
}

/// Builds hidden dictionaries for a concrete bounded generic function call.
///
/// Inputs:
/// - `target`: generic function metadata from the callee declaration.
/// - `args`: source-visible call arguments.
/// - `ctx`, `env`: syntax lowering context and caller lexical environment.
///
/// Output:
/// - One Erlang dictionary expression per declared generic bound.
/// - `None` when the concrete call cannot select a visible local impl wrapper.
///
/// Transformation:
/// - Infers simple type-variable substitutions from parameter annotations and
///   call arguments, resolves each bound to a concrete type, and creates a map
///   from trait method atoms to local typed wrapper function atoms.
pub(super) fn lower_syntax_generic_bound_dictionaries(
    target: &SyntaxGenericFunctionTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    let substitutions = infer_generic_function_type_substitutions(&target.params, args, ctx, env)?;
    target
        .bounds
        .iter()
        .map(|bound| lower_syntax_generic_bound_dictionary(bound, &substitutions, ctx))
        .collect()
}

/// Infers generic type substitutions for a simple local generic call.
///
/// Inputs:
/// - `params`: callee parameter annotation texts.
/// - `args`: source-visible call arguments.
/// - `env`: caller lexical environment containing inferred value types.
///
/// Output:
/// - Map from type variable name to concrete inferred type key.
///
/// Transformation:
/// - Handles the first executable P0.5e.4 ABI shape where a parameter
///   annotation is a direct type variable such as `A`; more structural
///   matching can move into this helper later without changing the ABI.
fn infer_generic_function_type_substitutions(
    params: &[String],
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<BTreeMap<String, String>> {
    if params.len() != args.len() {
        return None;
    }
    let mut substitutions = BTreeMap::new();
    for (param, arg) in params.iter().zip(args.iter()) {
        if !is_generic_type_var(param) {
            continue;
        }
        let arg_type = infer_syntax_trait_dispatch_type(arg, ctx, env)?;
        substitutions.insert(param.clone(), arg_type);
    }
    Some(substitutions)
}

/// Builds one hidden dictionary for a concrete generic bound.
///
/// Inputs:
/// - `bound`: parsed source trait bound.
/// - `substitutions`: type variable substitutions inferred from call args.
/// - `ctx`: syntax lowering context containing local trait methods and impl
///   wrappers.
///
/// Output:
/// - Erlang map expression from method atom to typed impl wrapper atom.
///
/// Transformation:
/// - Resolves a one-argument trait bound such as `Eq[A]` to `Eq[Int]`, then
///   maps every known local trait method to its concrete typed wrapper.
fn lower_syntax_generic_bound_dictionary(
    bound: &SyntaxGenericFunctionBound,
    substitutions: &BTreeMap<String, String>,
    ctx: &SyntaxLowerCtx,
) -> Option<ErlExpr> {
    if bound.type_args.len() != 1 {
        return None;
    }
    let type_arg = substitutions
        .get(&bound.type_args[0])
        .cloned()
        .unwrap_or_else(|| bound.type_args[0].clone());
    let methods = ctx.local_trait_methods.get(&bound.trait_name)?;
    let entries = methods
        .iter()
        .map(|method| {
            let wrapper = ctx.typed_trait_method_wrapper(&bound.trait_name, method, &type_arg)?;
            Some(format!("{} => {}", render_atom_expr(method), wrapper))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(ErlExpr::Raw(format!("#{{{}}}", entries.join(", "))))
}

/// Lowers a trait method call satisfied by the current function's bounds.
///
/// Inputs:
/// - `trait_name`: trait qualifier from `Trait.method(...)`.
/// - `method`: method name.
/// - `args`: source call arguments.
/// - `ctx`, `env`: syntax lowering context and current function environment.
///
/// Output:
/// - Erlang expression that applies the function stored in the hidden bound
///   dictionary.
///
/// Transformation:
/// - Uses the first argument's inferred type key to find the matching hidden
///   dictionary, then emits `apply(?MODULE, maps:get(Method, Dict), [Dict|Args])`
///   so generic source code can call trait methods without knowing the concrete
///   implementation wrapper.
pub(super) fn lower_syntax_bound_trait_method_call(
    trait_name: &str,
    method: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let type_arg = args
        .first()
        .and_then(|arg| infer_syntax_trait_dispatch_type(arg, ctx, env))?;
    let dict = env
        .trait_bound_dicts
        .get(&(trait_name.to_string(), type_arg))?;
    let mut rendered_args = Vec::with_capacity(args.len() + 1);
    rendered_args.push(dict.clone());
    rendered_args.extend(
        args.iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env).map(|expr| expr.render()))
            .collect::<Option<Vec<_>>>()?,
    );
    Some(ErlExpr::Raw(format!(
        "apply(?MODULE, maps:get({}, {}), [{}])",
        render_atom_expr(method),
        dict,
        rendered_args.join(", ")
    )))
}
