use std::collections::{HashMap, HashSet};

use terlan_hir::{ConstructorSignature, FunctionSignature, FunctionSymbol, ModuleInterface};

use crate::{
    atom_type_literal_payload, normalize_type_param_name, pretty_type, primitive_type_names,
    ConstructorScheme, FunctionScheme, MapFieldType, QualifiedTypeName, Type, TypeAlias, TypeVarId,
};

pub(super) fn is_map_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    match expand_type_aliases(ty, aliases) {
        Type::Named { name, .. } if name == "Map" || name == "map" => true,
        Type::Map(_) => true,
        Type::Dynamic | Type::Term => true,
        _ => false,
    }
}

pub(super) fn parse_symbol_scheme(symbol: &FunctionSymbol) -> Option<FunctionScheme> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let params = symbol
        .params
        .iter()
        .filter_map(|param| {
            parse_type_expr(&param.annotation, &HashSet::new(), &mut vars, &mut next_var)
        })
        .collect::<Vec<_>>();
    let ret = parse_type_expr(
        &symbol.return_type,
        &HashSet::new(),
        &mut vars,
        &mut next_var,
    )?;

    Some(FunctionScheme {
        params,
        ret,
        bounds: Vec::new(),
    })
}

pub(super) fn parse_interface_signature(
    signature: &FunctionSignature,
    interface: &ModuleInterface,
    global_aliases: &HashMap<String, TypeAlias>,
) -> Option<FunctionScheme> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let alias_names = interface_type_names(interface);
    let qualified_names = interface_qualified_type_names(interface);
    let interface_aliases = interface_type_aliases(interface);

    let params = signature
        .params
        .iter()
        .filter_map(|param| {
            parse_type_expr(&param.annotation, &alias_names, &mut vars, &mut next_var)
        })
        .map(|param| expand_type_aliases(&param, &interface_aliases))
        .map(|param| expand_interface_global_aliases(&param, global_aliases))
        .map(|param| qualify_type_names(&param, &qualified_names))
        .collect::<Vec<_>>();

    let ret = parse_type_expr(
        &signature.return_type,
        &alias_names,
        &mut vars,
        &mut next_var,
    )?;
    let ret = expand_type_aliases(&ret, &interface_aliases);
    let ret = expand_interface_global_aliases(&ret, global_aliases);
    let ret = qualify_type_names(&ret, &qualified_names);
    Some(FunctionScheme {
        params,
        ret,
        bounds: Vec::new(),
    })
}

/// Expands aliases visible through the global interface map.
///
/// Inputs:
/// - `ty`: type parsed from a public interface signature.
/// - `global_aliases`: fully qualified aliases loaded from dependency
///   interfaces.
///
/// Output:
/// - Type with fully qualified aliases expanded, plus unqualified aliases
///   expanded only when their short name is unique globally.
///
/// Transformation:
/// - Preserves local/interface parsing while allowing checked std summaries
///   such as `Option[T]` to resolve to the single loaded
///   `std.core.Option.Option[T]` alias without requiring every summary to spell
///   fully qualified type names.
pub(super) fn expand_interface_global_aliases(
    ty: &Type,
    global_aliases: &HashMap<String, TypeAlias>,
) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if let Some(alias) = unique_global_alias(name, global_aliases) {
                if alias.params.len() != args.len() {
                    return Type::Named {
                        module: None,
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| expand_interface_global_aliases(arg, global_aliases))
                            .collect(),
                    };
                }
                let args = args
                    .iter()
                    .map(|arg| expand_interface_global_aliases(arg, global_aliases))
                    .collect::<Vec<_>>();
                let mapping = alias
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                if alias.is_opaque {
                    return substitute_type_vars(&alias.body, &mapping);
                }
                return expand_interface_global_aliases(
                    &substitute_type_vars(&alias.body, &mapping),
                    global_aliases,
                );
            }
            expand_type_aliases(ty, global_aliases)
        }
        Type::Named { .. } => expand_type_aliases(ty, global_aliases),
        Type::List(inner) => Type::List(Box::new(expand_interface_global_aliases(
            inner,
            global_aliases,
        ))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| expand_interface_global_aliases(item, global_aliases))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| expand_interface_global_aliases(item, global_aliases))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: expand_interface_global_aliases(&field.value, global_aliases),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| expand_interface_global_aliases(param, global_aliases))
                .collect(),
            ret: Box::new(expand_interface_global_aliases(ret, global_aliases)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(expand_interface_global_aliases(elem, global_aliases)),
        },
        other => other.clone(),
    }
}

/// Finds a globally unique alias by short type name.
///
/// Inputs:
/// - `name`: unqualified type name from an interface signature.
/// - `global_aliases`: fully qualified aliases keyed as `module.Type`.
///
/// Output:
/// - The alias when exactly one global alias has the requested final segment.
///
/// Transformation:
/// - Scans fully qualified alias keys by their final dotted segment and rejects
///   ambiguous short names so interface summaries do not accidentally bind to
///   the wrong module.
pub(super) fn unique_global_alias<'a>(
    name: &str,
    global_aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a TypeAlias> {
    let mut matches = global_aliases.iter().filter_map(|(qualified, alias)| {
        qualified
            .rsplit_once('.')
            .and_then(|(_, short)| (short == name).then_some(alias))
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}

pub(super) fn parse_interface_constructor_schemes(
    signatures: Option<&[ConstructorSignature]>,
    interface: &ModuleInterface,
) -> Option<Vec<ConstructorScheme>> {
    let signatures = signatures?;
    let alias_names = interface_type_names(interface);
    let qualified_names = interface_qualified_type_names(interface);
    let interface_aliases = interface_type_aliases(interface);

    let schemes = signatures
        .iter()
        .filter(|signature| signature.public && signature.min_arity == signature.params.len())
        .map(|signature| {
            let mut vars = HashMap::new();
            let mut next_var: TypeVarId = 0;

            let fixed_params = signature
                .params
                .iter()
                .map(|param| {
                    parse_type_expr(&param.annotation, &alias_names, &mut vars, &mut next_var)
                        .unwrap_or(Type::Dynamic)
                })
                .map(|param| expand_type_aliases(&param, &interface_aliases))
                .map(|param| qualify_type_names(&param, &qualified_names))
                .collect::<Vec<_>>();

            let vararg = signature.vararg.as_ref().map(|param| {
                let parsed =
                    parse_type_expr(&param.annotation, &alias_names, &mut vars, &mut next_var)
                        .unwrap_or(Type::Dynamic);
                let parsed = expand_type_aliases(&parsed, &interface_aliases);
                qualify_type_names(&parsed, &qualified_names)
            });

            let ret = parse_type_expr(
                &signature.return_type,
                &alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);
            let ret = expand_type_aliases(&ret, &interface_aliases);
            let ret = qualify_type_names(&ret, &qualified_names);

            ConstructorScheme {
                min_arity: signature.min_arity,
                fixed_params,
                vararg,
                ret,
            }
        })
        .collect::<Vec<_>>();

    Some(schemes)
}

pub(super) fn interface_type_names(interface: &ModuleInterface) -> HashSet<String> {
    let mut names: HashSet<String> = interface
        .public_types
        .iter()
        .chain(interface.opaque_types.iter())
        .cloned()
        .collect();
    names.extend(primitive_type_names());
    names
}

pub(super) fn interface_type_aliases(interface: &ModuleInterface) -> HashMap<String, TypeAlias> {
    let mut aliases = HashMap::new();
    let alias_names = interface_type_names(interface);

    for (name, variants) in &interface.type_bodies {
        if interface.opaque_types.contains(name) {
            continue;
        }

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        let mut params = Vec::new();
        for param in interface.type_params.get(name).into_iter().flatten() {
            vars.insert(normalize_type_param_name(param), next_var);
            params.push(next_var);
            next_var += 1;
        }

        let body = normalize_union(
            variants
                .iter()
                .filter_map(|variant| {
                    parse_type_expr(variant, &alias_names, &mut vars, &mut next_var)
                })
                .collect(),
        );
        aliases.insert(
            name.clone(),
            TypeAlias {
                params,
                body,
                is_opaque: false,
            },
        );
    }

    aliases
}

pub(super) fn interface_qualified_type_names(
    interface: &ModuleInterface,
) -> HashMap<String, QualifiedTypeName> {
    interface
        .public_types
        .iter()
        .chain(interface.opaque_types.iter())
        .map(|name| {
            (
                name.clone(),
                QualifiedTypeName {
                    module: interface.module.clone(),
                    name: name.clone(),
                },
            )
        })
        .collect()
}

pub(super) fn qualify_type_names(
    ty: &Type,
    qualified_names: &HashMap<String, QualifiedTypeName>,
) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            let args = args
                .iter()
                .map(|arg| qualify_type_names(arg, qualified_names))
                .collect();
            if let Some(qualified) = qualified_names.get(name) {
                Type::Named {
                    module: Some(qualified.module.clone()),
                    name: qualified.name.clone(),
                    args,
                }
            } else {
                Type::Named {
                    module: None,
                    name: name.clone(),
                    args,
                }
            }
        }
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| qualify_type_names(arg, qualified_names))
                .collect(),
        },
        Type::List(inner) => Type::List(Box::new(qualify_type_names(inner, qualified_names))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| qualify_type_names(item, qualified_names))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| qualify_type_names(item, qualified_names))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: qualify_type_names(&field.value, qualified_names),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| qualify_type_names(param, qualified_names))
                .collect(),
            ret: Box::new(qualify_type_names(ret, qualified_names)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(qualify_type_names(elem, qualified_names)),
        },
        other => other.clone(),
    }
}

pub(super) fn parse_type_expr(
    input: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<Type> {
    let src = compact_spaces(input);
    if src.is_empty() {
        return Some(Type::Dynamic);
    }

    if let Some(atom) = parse_type_atom_literal(&src) {
        return Some(Type::LiteralAtom(atom));
    }

    if let Some((params, ret)) = split_top_level_arrow(&src) {
        let params = strip_wrapping_parens(&params).unwrap_or(params.as_str());
        let params = if params.trim().is_empty() {
            Vec::new()
        } else {
            split_top_level_csv(params)
                .into_iter()
                .map(|param| parse_type_expr(param.trim(), aliases, vars, next_var))
                .collect::<Option<Vec<_>>>()?
        };
        let ret = parse_type_expr(ret.trim(), aliases, vars, next_var)?;
        return Some(Type::Function {
            params,
            ret: Box::new(ret),
        });
    }

    if is_union_type(&src) {
        let variants = split_top_level_union(&src)
            .into_iter()
            .map(|variant| parse_type_expr(variant.trim(), aliases, vars, next_var))
            .collect::<Option<Vec<_>>>()?;
        return Some(normalize_union(variants));
    }

    if is_list_type(&src) {
        let inner = &src[1..src.len() - 1];
        return Some(Type::List(Box::new(
            parse_type_expr(inner.trim(), aliases, vars, next_var).unwrap_or(Type::Dynamic),
        )));
    }

    if let Some((base, args)) = split_named_type(&src) {
        if base == "List" {
            let mut args = split_top_level_csv(&args).into_iter();
            let inner = args.next()?;
            if args.next().is_some() {
                return None;
            }
            return Some(Type::List(Box::new(parse_type_expr(
                inner.trim(),
                aliases,
                vars,
                next_var,
            )?)));
        }

        if base == "FixedArray" {
            let mut args = split_top_level_csv(&args).into_iter();
            let first = args.next()?;
            let second = args.next()?;
            if args.next().is_some() {
                return None;
            }

            let size = parse_type_expr(first.trim(), aliases, vars, next_var)?;
            if let Type::LiteralInt(size_value) = size {
                let size = usize::try_from(size_value).ok()?;
                let elem = parse_type_expr(second.trim(), aliases, vars, next_var)?;
                return Some(Type::FixedArray {
                    size,
                    elem: Box::new(elem),
                });
            }

            return None;
        }

        let args = split_top_level_csv(&args)
            .into_iter()
            .map(|arg| parse_type_expr(arg.trim(), aliases, vars, next_var))
            .collect::<Option<Vec<_>>>()?;
        let (module, name) = split_module_name(base);
        return Some(Type::Named { module, name, args });
    }

    if is_map_type_expr(&src) {
        return parse_map_type_expression(&src, aliases, vars, next_var);
    }

    if is_tuple_type(&src) {
        let inner = &src[1..src.len() - 1];
        return Some(Type::Tuple(
            split_top_level_csv(inner)
                .into_iter()
                .map(|item| parse_tuple_type_elem(item.trim(), aliases, vars, next_var))
                .collect::<Option<Vec<_>>>()?,
        ));
    }

    Some(map_named_or_var(&src, aliases, vars, next_var))
}

pub(super) fn parse_tuple_type_elem(
    input: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<Type> {
    if let Some((label, value)) = split_named_tuple_type_elem(input) {
        if label == "_" || is_lower_identifier(label) {
            return parse_type_expr(value.trim(), aliases, vars, next_var);
        }
    }

    parse_type_expr(input, aliases, vars, next_var)
}

pub(super) fn split_named_tuple_type_elem(input: &str) -> Option<(&str, &str)> {
    let mut depth_p = 0usize;
    let mut depth_b = 0usize;
    let mut depth_br = 0usize;
    let mut quote = None;
    let mut escape = false;

    for (i, ch) in input.char_indices() {
        if let Some(quote_ch) = quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == quote_ch {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth_p += 1,
            ')' => depth_p = depth_p.saturating_sub(1),
            '[' => depth_b += 1,
            ']' => depth_b = depth_b.saturating_sub(1),
            '{' => depth_br += 1,
            '}' => depth_br = depth_br.saturating_sub(1),
            ':' if i > 0 && depth_p == 0 && depth_b == 0 && depth_br == 0 => {
                return Some((&input[..i], &input[i + ch.len_utf8()..]));
            }
            _ => {}
        }
    }

    None
}

pub(super) fn parse_type_atom_literal(input: &str) -> Option<String> {
    if let Some(atom) = atom_type_literal_payload(input) {
        return Some(atom);
    }

    let atom = input.strip_prefix(':')?;
    if atom.is_empty() {
        return None;
    }
    if is_type_constructor_atom(atom) {
        return Some(atom.to_string());
    }
    if atom.len() >= 2 && atom.starts_with('\'') && atom.ends_with('\'') {
        return unquote_type_atom(atom);
    }
    None
}

pub(super) fn unquote_type_atom(text: &str) -> Option<String> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                output.push(escaped);
            }
        } else {
            output.push(ch);
        }
    }
    Some(output)
}

pub(super) fn parse_map_type_expression(
    input: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<Type> {
    let src = input;
    if !is_map_type_expr(src) {
        return None;
    }

    let inner = &src[2..src.len() - 1];
    if inner.trim().is_empty() {
        return Some(Type::Map(Vec::new()));
    }

    let fields = split_top_level_csv(inner)
        .into_iter()
        .map(|field| parse_map_type_field(field.trim(), aliases, vars, next_var))
        .collect::<Option<Vec<_>>>()?;

    Some(Type::Map(fields))
}

pub(super) fn parse_map_type_field(
    input: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<MapFieldType> {
    let (raw_key, raw_value, required) = split_map_field(input)?;
    let key = raw_key.trim().to_string();
    let value = parse_type_expr(raw_value.trim(), aliases, vars, next_var)?;
    Some(MapFieldType {
        key,
        value,
        required,
    })
}

pub(super) fn split_map_field(input: &str) -> Option<(&str, &str, bool)> {
    let bytes = input.as_bytes();
    let mut depth_p = 0usize;
    let mut depth_b = 0usize;
    let mut depth_br = 0usize;

    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => depth_p += 1,
            b')' => depth_p = depth_p.saturating_sub(1),
            b'[' => depth_b += 1,
            b']' => depth_b = depth_b.saturating_sub(1),
            b'{' => depth_br += 1,
            b'}' => depth_br = depth_br.saturating_sub(1),
            b':' if i + 1 < bytes.len()
                && bytes[i + 1] == b'='
                && depth_p == 0
                && depth_b == 0
                && depth_br == 0 =>
            {
                return Some((&input[..i], &input[i + 2..], true));
            }
            b'=' if i + 1 < bytes.len()
                && bytes[i + 1] == b'>'
                && depth_p == 0
                && depth_b == 0
                && depth_br == 0 =>
            {
                return Some((&input[..i], &input[i + 2..], false));
            }
            _ => {}
        }
    }

    None
}

pub(super) fn split_named_type(input: &str) -> Option<(&str, String)> {
    let bytes = input.as_bytes();
    if !bytes.contains(&b'[') || !input.ends_with(']') {
        return None;
    }

    let mut depth = 0usize;
    for (i, byte) in bytes.iter().enumerate() {
        match *byte {
            b'[' => {
                if depth == 0 {
                    return split_named_type_inner(input, i);
                }
                depth += 1;
            }
            b']' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    None
}

pub(super) fn split_named_type_inner(input: &str, open_index: usize) -> Option<(&str, String)> {
    if !input.ends_with(']') {
        return None;
    }

    let name = input[..open_index].trim();
    let args = input[open_index + 1..input.len() - 1].trim();

    if name.is_empty() {
        None
    } else {
        Some((name, args.to_string()))
    }
}

pub(super) fn is_union_type(input: &str) -> bool {
    split_top_level_union(input).len() > 1
}

pub(super) fn map_named_or_var(
    text: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Type {
    match text {
        "Int" => Type::Int,
        "Float" => Type::Float,
        "Number" => Type::Number,
        "Binary" => Type::Binary,
        "String" | "Text" => Type::Binary,
        "Atom" => Type::Atom,
        "Bool" => Type::Bool,
        "Term" => Type::Term,
        "Dynamic" => Type::Dynamic,
        "Never" => Type::Never,
        _ if text.chars().all(|c| c.is_ascii_digit()) => {
            if let Ok(value) = text.parse::<i64>() {
                Type::LiteralInt(value)
            } else {
                Type::Dynamic
            }
        }
        _ if text.contains('.') => {
            let (module, name) = split_module_name(text);
            Type::Named {
                module,
                name,
                args: Vec::new(),
            }
        }
        _ => {
            if let Some(existing) = vars.get(text) {
                return Type::Var(*existing);
            }
            if aliases.contains(text) {
                return Type::Named {
                    module: None,
                    name: text.to_string(),
                    args: Vec::new(),
                };
            }
            if let Some(id) = fresh_type_var(text, vars, next_var) {
                Type::Var(id)
            } else if is_type_constructor_atom(text) {
                Type::LiteralAtom(text.to_string())
            } else {
                Type::Named {
                    module: None,
                    name: text.to_string(),
                    args: Vec::new(),
                }
            }
        }
    }
}

pub(super) fn fresh_type_var(
    text: &str,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<TypeVarId> {
    if !text.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    if text.contains('.') {
        return None;
    }
    if let Some(existing) = vars.get(text) {
        return Some(*existing);
    }

    let id = *next_var;
    vars.insert(text.to_string(), id);
    *next_var += 1;
    Some(id)
}

pub(super) fn expand_type_aliases(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if let Some(alias) = aliases.get(name) {
                if alias.is_opaque {
                    return Type::Named {
                        module: None,
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| expand_type_aliases(arg, aliases))
                            .collect(),
                    };
                }
                if alias.params.len() != args.len() {
                    return ty.clone();
                }
                let args = args
                    .iter()
                    .map(|arg| expand_type_aliases(arg, aliases))
                    .collect::<Vec<_>>();
                let mapping = alias
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                expand_type_aliases(&substitute_type_vars(&alias.body, &mapping), aliases)
            } else {
                Type::Named {
                    module: None,
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| expand_type_aliases(arg, aliases))
                        .collect(),
                }
            }
        }
        Type::Named {
            module: Some(module),
            name,
            args,
        } => {
            let qualified_name = format!("{}.{}", module, name);
            if let Some(alias) = aliases.get(&qualified_name) {
                if alias.is_opaque {
                    return Type::Named {
                        module: Some(module.clone()),
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| expand_type_aliases(arg, aliases))
                            .collect(),
                    };
                }
                if alias.params.len() != args.len() {
                    return ty.clone();
                }
                let args = args
                    .iter()
                    .map(|arg| expand_type_aliases(arg, aliases))
                    .collect::<Vec<_>>();
                let mapping = alias
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                expand_type_aliases(&substitute_type_vars(&alias.body, &mapping), aliases)
            } else {
                Type::Named {
                    module: Some(module.clone()),
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| expand_type_aliases(arg, aliases))
                        .collect(),
                }
            }
        }
        Type::List(inner) => Type::List(Box::new(expand_type_aliases(inner, aliases))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| expand_type_aliases(item, aliases))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: expand_type_aliases(&field.value, aliases),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| expand_type_aliases(item, aliases))
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| expand_type_aliases(param, aliases))
                .collect(),
            ret: Box::new(expand_type_aliases(ret, aliases)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(expand_type_aliases(elem, aliases)),
        },
        other => other.clone(),
    }
}

pub(super) fn substitute_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => mapping.get(id).cloned().unwrap_or(Type::Var(*id)),
        Type::List(inner) => Type::List(Box::new(substitute_type_vars(inner, mapping))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| substitute_type_vars(item, mapping))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| substitute_type_vars(item, mapping))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: substitute_type_vars(&field.value, mapping),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| substitute_type_vars(arg, mapping))
                .collect(),
        },
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| substitute_type_vars(param, mapping))
                .collect(),
            ret: Box::new(substitute_type_vars(ret, mapping)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(substitute_type_vars(elem, mapping)),
        },
        other => other.clone(),
    }
}

pub(super) fn normalize_union(mut types: Vec<Type>) -> Type {
    let mut expanded = Vec::new();
    while let Some(ty) = types.pop() {
        match ty {
            Type::Union(items) => expanded.extend(items),
            other => expanded.push(other),
        }
    }

    let mut normalized: Vec<Type> = Vec::new();
    for candidate in expanded {
        if candidate == Type::Never {
            continue;
        }
        if candidate == Type::Term {
            return Type::Term;
        }
        if normalized
            .iter()
            .any(|existing| is_subtype(&candidate, existing))
        {
            continue;
        }
        normalized.retain(|existing| !is_subtype(existing, &candidate));
        normalized.push(candidate);
    }

    if normalized.is_empty() {
        Type::Never
    } else if normalized.len() == 1 {
        normalized.into_iter().next().unwrap()
    } else {
        Type::Union(normalized)
    }
}

/// Checks whether a type denotes Terlan's canonical Unit type.
///
/// Inputs:
/// - `ty`: resolved type representation.
///
/// Output:
/// - `true` for local `Unit` and fully-qualified `std.core.Unit.Unit`.
/// - `false` for all other named types and literal atoms.
///
/// Transformation:
/// - Recognizes only zero-argument Unit names so `Unit[T]` or unrelated
///   aliases do not become singleton-unit equivalents.
pub(super) fn is_unit_named_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Named {
            module: None,
            name,
            args,
        } if name == "Unit" && args.is_empty()
    ) || matches!(
        ty,
        Type::Named {
            module: Some(module),
            name,
            args,
        } if module == "std.core.Unit" && name == "Unit" && args.is_empty()
    )
}

/// Checks whether a type denotes the canonical Unit singleton representation.
///
/// Inputs:
/// - `ty`: resolved type representation.
///
/// Output:
/// - `true` for the explicit `Atom["unit"]` literal type.
/// - `false` for all other atoms and named types.
///
/// Transformation:
/// - Keeps the equivalence at the type level; expression parsing still rejects
///   bare lowercase `unit` as a source-level Unit synonym.
pub(super) fn is_unit_literal_type(ty: &Type) -> bool {
    matches!(ty, Type::LiteralAtom(atom) if atom == "unit")
}

/// Checks whether two types are equivalent Unit spellings.
///
/// Inputs:
/// - `left`: first resolved type.
/// - `right`: second resolved type.
///
/// Output:
/// - `true` when one side is named Unit and the other is `Atom["unit"]`.
/// - `false` for non-Unit atom aliases and unrelated named types.
///
/// Transformation:
/// - Bridges the public `std.core.Unit.Unit = Atom["unit"]` alias to the
///   compiler's singleton representation during type comparison only.
pub(super) fn are_unit_equivalent_types(left: &Type, right: &Type) -> bool {
    (is_unit_named_type(left) && is_unit_literal_type(right))
        || (is_unit_literal_type(left) && is_unit_named_type(right))
}

pub(super) fn is_subtype(lhs: &Type, rhs: &Type) -> bool {
    if lhs == rhs {
        return true;
    }
    if are_unit_equivalent_types(lhs, rhs) {
        return true;
    }
    match (lhs, rhs) {
        (_, Type::Dynamic) => true,
        (_, Type::Term) => true,
        (Type::Int, Type::Number) => true,
        (Type::Float, Type::Number) => true,
        (Type::LiteralInt(_), Type::Int) => true,
        (Type::LiteralAtom(_), Type::Atom) => true,
        (
            Type::FixedArray {
                size: lhs_size,
                elem: lhs_elem,
            },
            Type::FixedArray {
                size: rhs_size,
                elem: rhs_elem,
            },
        ) => lhs_size == rhs_size && is_subtype(lhs_elem, rhs_elem),
        (Type::Map(lhs), Type::Map(rhs)) => map_fields_is_subtype(lhs, rhs),
        (Type::Never, _) => true,
        _ => false,
    }
}

pub(super) fn map_fields_is_subtype(lhs: &[MapFieldType], rhs: &[MapFieldType]) -> bool {
    for rhs_field in rhs {
        let Some(lhs_field) = lhs.iter().find(|field| field.key == rhs_field.key) else {
            if rhs_field.required {
                return false;
            }
            continue;
        };

        if rhs_field.required && !lhs_field.required {
            return false;
        }

        if !is_subtype(&lhs_field.value, &rhs_field.value) {
            return false;
        }
    }

    true
}

pub(super) fn unify(
    left: &Type,
    right: &Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    let left = apply_subst(left, subst);
    let right = apply_subst(right, subst);

    if are_unit_equivalent_types(&left, &right) {
        return Ok(());
    }

    match (&left, &right) {
        (Type::Dynamic, _) | (_, Type::Dynamic) => Ok(()),
        (Type::Term, _) => Ok(()),
        (_, Type::Never) => Ok(()),
        (Type::Var(left_id), Type::Var(right_id)) if left_id == right_id => Ok(()),
        (Type::Var(id), rhs) => bind_var(*id, rhs.clone(), subst),
        (lhs, Type::Var(id)) => bind_var(*id, lhs.clone(), subst),
        (Type::Union(left), Type::Union(right)) => {
            for l in left {
                let mut trial_ok = false;
                for r in right {
                    let mut trial_subst = subst.clone();
                    if unify(l, r, &mut trial_subst).is_ok() {
                        *subst = trial_subst;
                        trial_ok = true;
                        break;
                    }
                }
                if !trial_ok {
                    return Err(format!(
                        "expected {} but could not match {}",
                        pretty_type(&Type::Union(right.to_vec())),
                        pretty_type(l)
                    ));
                }
            }
            Ok(())
        }
        (Type::Union(left), rhs) => {
            for l in left {
                let mut trial_subst = subst.clone();
                if unify(l, rhs, &mut trial_subst).is_ok() {
                    *subst = trial_subst;
                    return Ok(());
                }
            }
            Err(format!(
                "expected {} found {}",
                pretty_type(&Type::Union(left.clone())),
                pretty_type(rhs)
            ))
        }
        (lhs, Type::Union(right)) => {
            for r in right {
                let mut trial_subst = subst.clone();
                if unify(lhs, r, &mut trial_subst).is_ok() {
                    *subst = trial_subst;
                    return Ok(());
                }
            }
            Err(format!(
                "expected {} found {}",
                pretty_type(lhs),
                pretty_type(&Type::Union(right.clone()))
            ))
        }
        (Type::Int, Type::Number) => Ok(()),
        (Type::Float, Type::Number) => Ok(()),
        (Type::LiteralInt(_), Type::Number) => Ok(()),
        (Type::Number, Type::LiteralInt(_)) => Ok(()),
        (Type::Number, Type::Int) | (Type::Number, Type::Float) => {
            Err("expected Number but found Int/Float".to_string())
        }
        (Type::LiteralAtom(left_atom), Type::LiteralAtom(right_atom))
            if left_atom == right_atom =>
        {
            Ok(())
        }
        (Type::LiteralInt(left_int), Type::LiteralInt(right_int)) if left_int == right_int => {
            Ok(())
        }
        (Type::Int, Type::Int)
        | (Type::Float, Type::Float)
        | (Type::Atom, Type::Atom)
        | (Type::Atom, Type::LiteralAtom(_))
        | (Type::LiteralAtom(_), Type::Atom)
        | (Type::Int, Type::LiteralInt(_))
        | (Type::LiteralInt(_), Type::Int)
        | (Type::Binary, Type::Binary)
        | (Type::Bool, Type::Bool) => Ok(()),
        (Type::List(lhs), Type::List(rhs)) => unify(lhs, rhs, subst),
        (Type::Map(lhs_fields), Type::Map(rhs_fields)) => {
            unify_map_fields(lhs_fields, rhs_fields, subst)
        }
        (Type::Tuple(lhs), Type::Tuple(rhs)) => {
            if lhs.len() != rhs.len() {
                return Err(format!(
                    "tuple arity mismatch: expected {} elements, found {}",
                    lhs.len(),
                    rhs.len()
                ));
            }
            for (left_item, right_item) in lhs.iter().zip(rhs.iter()) {
                unify(left_item, right_item, subst)?;
            }
            Ok(())
        }
        (
            Type::Named {
                module: m1,
                name: n1,
                args: args1,
            },
            Type::Named {
                module: m2,
                name: n2,
                args: args2,
            },
        ) => {
            if m1 == m2 && n1 == n2 && args1.len() == args2.len() {
                for (a, b) in args1.iter().zip(args2.iter()) {
                    unify(a, b, subst)?;
                }
                Ok(())
            } else {
                Err(format!(
                    "expected {} found {}",
                    pretty_type(&Type::Named {
                        module: m1.clone(),
                        name: n1.clone(),
                        args: args1.clone(),
                    }),
                    pretty_type(&Type::Named {
                        module: m2.clone(),
                        name: n2.clone(),
                        args: args2.clone(),
                    })
                ))
            }
        }
        (
            Type::Function {
                params: params_a,
                ret: ret_a,
            },
            Type::Function {
                params: params_b,
                ret: ret_b,
            },
        ) => {
            if params_a.len() != params_b.len() {
                return Err(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params_a.len(),
                    params_b.len()
                ));
            }
            for (a, b) in params_a.iter().zip(params_b.iter()) {
                unify(a, b, subst)?;
            }
            unify(ret_a.as_ref(), ret_b.as_ref(), subst)
        }
        (
            Type::FixedArray {
                size: size_a,
                elem: elem_a,
            },
            Type::FixedArray {
                size: size_b,
                elem: elem_b,
            },
        ) => {
            if size_a != size_b {
                return Err(format!(
                    "expected {} found {}",
                    pretty_type(&Type::FixedArray {
                        size: *size_a,
                        elem: elem_a.clone(),
                    }),
                    pretty_type(&Type::FixedArray {
                        size: *size_b,
                        elem: elem_b.clone(),
                    })
                ));
            }
            unify(elem_a, elem_b, subst)
        }
        _ => Err(format!(
            "expected {} found {}",
            pretty_type(&left),
            pretty_type(&right)
        )),
    }
}

pub(super) fn unify_map_fields(
    lhs: &[MapFieldType],
    rhs: &[MapFieldType],
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    for rhs_field in rhs {
        let Some(lhs_field) = lhs.iter().find(|field| field.key == rhs_field.key) else {
            if rhs_field.required {
                return Err(format!("missing required map field: {}", rhs_field.key));
            }
            continue;
        };

        if rhs_field.required && !lhs_field.required {
            return Err(format!(
                "required map field {} cannot match optional",
                rhs_field.key
            ));
        }

        unify(&lhs_field.value, &rhs_field.value, subst)?;
    }

    for lhs_field in lhs {
        if lhs_field.required {
            let present = rhs.iter().any(|rhs_field| rhs_field.key == lhs_field.key);
            if !present {
                return Err(format!("missing required map field: {}", lhs_field.key));
            }
        }
    }

    Ok(())
}

pub(super) fn bind_var(
    id: TypeVarId,
    value: Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    let value = widen_type_var_binding(value);
    if let Some(existing) = subst.get(&id).cloned() {
        return unify(&existing, &value, subst);
    }
    if occurs(id, &value, subst) {
        return Err("recursive type".to_string());
    }
    subst.insert(id, value);
    Ok(())
}

/// Widens overly specific literal types when binding generic variables.
///
/// Inputs:
/// - `value`: inferred type about to bind a type variable.
///
/// Output:
/// - A type suitable for reuse across generic call arguments.
///
/// Transformation:
/// - Converts integer literal singleton types into `Int` so generic calls such
///   as `Some(1)` and `Some(2)` can agree on `T = Int`; leaves atom literals
///   unchanged because atom literals carry closed-shape domain information.
pub(super) fn widen_type_var_binding(value: Type) -> Type {
    match value {
        Type::LiteralInt(_) => Type::Int,
        other => other,
    }
}

pub(super) fn occurs(var: TypeVarId, value: &Type, subst: &HashMap<TypeVarId, Type>) -> bool {
    match apply_subst(value, subst) {
        Type::Var(other) => other == var,
        Type::List(inner) => occurs(var, &inner, subst),
        Type::Tuple(items) => items.iter().any(|item| occurs(var, item, subst)),
        Type::Union(items) => items.iter().any(|item| occurs(var, item, subst)),
        Type::Named { args, .. } => args.iter().any(|arg| occurs(var, arg, subst)),
        Type::Map(fields) => fields.iter().any(|field| occurs(var, &field.value, subst)),
        Type::Function { params, ret } => {
            params.iter().any(|param| occurs(var, param, subst)) || occurs(var, &ret, subst)
        }
        _ => false,
    }
}

pub(super) fn reveal_opaque_aliases(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if let Some(alias) = aliases.get(name) {
                if alias.is_opaque && alias.params.len() == args.len() {
                    let mapping = alias
                        .params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    return substitute_type_vars(&alias.body, &mapping);
                }
            }
            Type::Named {
                module: None,
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| reveal_opaque_aliases(arg, aliases))
                    .collect(),
            }
        }
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| reveal_opaque_aliases(arg, aliases))
                .collect(),
        },
        Type::List(inner) => Type::List(Box::new(reveal_opaque_aliases(inner, aliases))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| reveal_opaque_aliases(item, aliases))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| reveal_opaque_aliases(item, aliases))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: reveal_opaque_aliases(&field.value, aliases),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| reveal_opaque_aliases(param, aliases))
                .collect(),
            ret: Box::new(reveal_opaque_aliases(ret, aliases)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(reveal_opaque_aliases(elem, aliases)),
        },
        other => other.clone(),
    }
}

pub(super) fn apply_subst(ty: &Type, subst: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => match subst.get(id) {
            Some(inner) => apply_subst(inner, subst),
            None => Type::Var(*id),
        },
        Type::List(inner) => Type::List(Box::new(apply_subst(inner, subst))),
        Type::Tuple(items) => {
            Type::Tuple(items.iter().map(|item| apply_subst(item, subst)).collect())
        }
        Type::Union(items) => {
            Type::Union(items.iter().map(|item| apply_subst(item, subst)).collect())
        }
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: apply_subst(&field.value, subst),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|arg| apply_subst(arg, subst)).collect(),
        },
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| apply_subst(param, subst))
                .collect(),
            ret: Box::new(apply_subst(ret, subst)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(apply_subst(elem, subst)),
        },
        other => other.clone(),
    }
}

pub(super) fn builtin_call(name: &str, arity: usize) -> Option<FunctionScheme> {
    match (name, arity) {
        ("type_of", 1) => Some(FunctionScheme {
            params: vec![Type::Dynamic],
            ret: Type::Named {
                module: None,
                name: "Type".to_string(),
                args: Vec::new(),
            },
            bounds: Vec::new(),
        }),
        ("is_type", 2) => Some(FunctionScheme {
            params: vec![
                Type::Dynamic,
                Type::Named {
                    module: None,
                    name: "Type".to_string(),
                    args: Vec::new(),
                },
            ],
            ret: Type::Bool,
            bounds: Vec::new(),
        }),
        _ => None,
    }
}

/// Reports whether a call name used to be an implicit legacy builtin.
///
/// Inputs:
/// - `name`: local call name from source.
/// - `arity`: number of source arguments.
///
/// Output:
/// - `true` for legacy Erlang-shaped helper names that are no longer admitted
///   into Terlan's implicit prelude.
///
/// Transformation:
/// - Keeps the minimal 0.0.3 implicit prelude closed while producing a clearer
///   diagnostic than a later backend failure for old helper spellings.
pub(super) fn is_removed_implicit_builtin_call(name: &str, arity: usize) -> bool {
    matches!(
        (name, arity),
        ("integer_to_binary", 1)
            | ("is_integer", 1)
            | ("is_binary", 1)
            | ("is_atom", 1)
            | ("is_boolean", 1)
            | ("is_list", 1)
            | ("is_map", 1)
            | ("is_tuple", 1)
    )
}

pub(super) fn is_literal_atom(name: &str) -> bool {
    matches!(name, "ok" | "error" | "true" | "false" | "nil") || is_type_constructor_atom(name)
}

pub(super) fn widen_list_literal_element_type(ty: Type) -> Type {
    match ty {
        Type::LiteralInt(_) => Type::Int,
        Type::LiteralAtom(atom) => {
            if atom == "true" || atom == "false" {
                Type::Bool
            } else {
                Type::Atom
            }
        }
        other => other,
    }
}

pub(super) fn is_type_constructor_atom(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => {
            if !c.is_ascii_lowercase() {
                return false;
            }
        }
        None => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '-')
}

pub(super) fn is_lower_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(super) fn compact_spaces(input: &str) -> String {
    let mut output = String::new();
    let mut quote = None;
    let mut escape = false;

    for ch in input.chars() {
        if let Some(quote_ch) = quote {
            output.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == quote_ch {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                output.push(ch);
            }
            ch if ch.is_whitespace() => {}
            _ => output.push(ch),
        }
    }

    output
}

pub(super) fn strip_wrapping_parens(input: &str) -> Option<&str> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
        return None;
    }

    let mut depth = 0usize;
    for (idx, byte) in bytes.iter().enumerate() {
        match *byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && idx != bytes.len() - 1 {
                    return None;
                }
            }
            _ => {}
        }
    }

    Some(&input[1..input.len() - 1])
}

pub(super) fn split_top_level_arrow(input: &str) -> Option<(String, String)> {
    let bytes = input.as_bytes();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;

    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b'-' if i + 1 < bytes.len()
                && bytes[i + 1] == b'>'
                && p_depth == 0
                && b_depth == 0
                && t_depth == 0 =>
            {
                let left = String::from_utf8_lossy(&bytes[..i]).to_string();
                let right = String::from_utf8_lossy(&bytes[i + 2..]).to_string();
                return Some((left.trim().to_string(), right.trim().to_string()));
            }
            _ => {}
        }
    }

    None
}

pub(super) fn split_top_level_csv(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in bytes.iter().enumerate() {
        match *ch {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b',' if p_depth == 0 && b_depth == 0 && t_depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..idx]).to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(String::from_utf8_lossy(&bytes[start..]).to_string());
    out.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(super) fn split_top_level_plus(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in bytes.iter().enumerate() {
        match *ch {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b'+' if p_depth == 0 && b_depth == 0 && t_depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..idx]).to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(String::from_utf8_lossy(&bytes[start..]).to_string());
    out.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(super) fn split_top_level_union(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in bytes.iter().enumerate() {
        match *ch {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b'|' if p_depth == 0 && b_depth == 0 && t_depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..idx]).to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(String::from_utf8_lossy(&bytes[start..]).to_string());
    out.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(super) fn split_module_name(name: &str) -> (Option<String>, String) {
    if let Some((module, base)) = name.rsplit_once('.') {
        (Some(module.to_string()), base.to_string())
    } else {
        (None, name.to_string())
    }
}

pub(super) fn is_list_type(input: &str) -> bool {
    input.starts_with('[') && input.ends_with(']') && !input.contains("||")
}

pub(super) fn is_tuple_type(input: &str) -> bool {
    input.starts_with('{') && input.ends_with('}')
}

pub(super) fn is_map_type_expr(input: &str) -> bool {
    input.starts_with("#{") && input.ends_with('}') && input.len() >= 3
}
