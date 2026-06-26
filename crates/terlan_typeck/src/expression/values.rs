use super::*;

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
pub(super) fn infer_syntax_var(
    name: &str,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
) -> Type {
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
