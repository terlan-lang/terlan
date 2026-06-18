use std::collections::{HashMap, HashSet};

use terlan_hir::{ConstructorSignature, FunctionSignature, FunctionSymbol, ModuleInterface};

mod interface;

pub(super) use interface::{
    interface_qualified_type_names, interface_type_aliases, interface_type_names,
    parse_interface_constructor_schemes, parse_interface_signature, parse_symbol_scheme,
    qualify_type_names,
};

use crate::{
    atom_type_literal_payload, normalize_type_param_name, pretty_type, primitive_type_names,
    ConstructorScheme, FunctionScheme, MapFieldType, QualifiedTypeName, Type, TypeAlias, TypeVarId,
};

/// Reports whether a type can be treated as a map-like value.
///
/// Inputs:
/// - `ty`: resolved type to inspect.
/// - `aliases`: type aliases available in the current module.
///
/// Output:
/// - `true` for concrete map types, `Map` aliases, and dynamic top types.
///
/// Transformation:
/// - Expands transparent aliases before checking map compatibility.
pub(super) fn is_map_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    match expand_type_aliases(ty, aliases) {
        Type::Named { name, .. } if name == "Map" || name == "map" => true,
        Type::Map(_) => true,
        Type::Dynamic | Type::Term => true,
        _ => false,
    }
}

/// Parses a textual type expression into the internal type model.
///
/// Inputs:
/// - `input`: source text for a type expression.
/// - `aliases`: visible alias names that should stay named.
/// - `vars`: mutable mapping from type-variable names to ids.
/// - `next_var`: next available type-variable id.
///
/// Output:
/// - `Some(Type)` when the expression is recognized.
/// - `None` for malformed generic, fixed-array, map, or nested forms.
///
/// Transformation:
/// - Removes insignificant whitespace, recognizes literals and composite
///   types, and allocates type variables consistently through `vars`.
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

/// Parses one tuple type element.
///
/// Inputs:
/// - `input`: tuple element text, optionally `name: Type` or `_: Type`.
/// - `aliases`: visible alias names.
/// - `vars`: mutable type-variable mapping.
/// - `next_var`: next available type-variable id.
///
/// Output:
/// - Parsed element type, ignoring valid tuple field labels.
///
/// Transformation:
/// - Strips supported tuple labels before delegating to `parse_type_expr`.
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

/// Splits a named tuple type element at its top-level colon.
///
/// Inputs:
/// - `input`: tuple element text.
///
/// Output:
/// - `Some((label, value))` when a top-level label separator is found.
/// - `None` when the colon is absent or nested inside another construct.
///
/// Transformation:
/// - Tracks bracket depth and quoted strings so nested colons are preserved.
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

/// Parses an atom literal used in type position.
///
/// Inputs:
/// - `input`: candidate atom literal text.
///
/// Output:
/// - Atom payload without source delimiters when the spelling is valid.
///
/// Transformation:
/// - Accepts canonical `Atom["name"]`, shorthand `:name`, and quoted
///   interop atoms while rejecting empty or invalid constructor names.
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

/// Removes quote delimiters from a quoted atom payload.
///
/// Inputs:
/// - `text`: quoted atom text including leading and trailing single quotes.
///
/// Output:
/// - Unescaped atom payload, or `None` when delimiters are missing.
///
/// Transformation:
/// - Copies escaped characters literally after dropping the escape marker.
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

/// Parses a map type expression.
///
/// Inputs:
/// - `input`: candidate `#{...}` map type text.
/// - `aliases`: visible alias names.
/// - `vars`: mutable type-variable mapping.
/// - `next_var`: next available type-variable id.
///
/// Output:
/// - `Some(Type::Map(_))` for valid map type syntax.
/// - `None` for non-map input or malformed fields.
///
/// Transformation:
/// - Splits top-level fields and parses each required or optional field type.
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

/// Parses one map type field.
///
/// Inputs:
/// - `input`: field text using `:=` for required or `=>` for optional.
/// - `aliases`: visible alias names.
/// - `vars`: mutable type-variable mapping.
/// - `next_var`: next available type-variable id.
///
/// Output:
/// - A map-field type descriptor with key, value type, and required flag.
///
/// Transformation:
/// - Splits the field separator and parses the value side as a type.
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

/// Splits a map type field into key, value, and requiredness.
///
/// Inputs:
/// - `input`: map field text.
///
/// Output:
/// - `Some((key, value, true))` for top-level `:=`.
/// - `Some((key, value, false))` for top-level `=>`.
/// - `None` when no top-level map-field separator exists.
///
/// Transformation:
/// - Tracks nested delimiters so separators inside type arguments are ignored.
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

/// Splits a generic named type into base name and argument text.
///
/// Inputs:
/// - `input`: candidate type text such as `Result[T, E]`.
///
/// Output:
/// - Base type name and raw argument list without outer brackets.
///
/// Transformation:
/// - Finds the first top-level `[` and validates the closing `]`.
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

/// Completes generic named-type splitting after the opening bracket is known.
///
/// Inputs:
/// - `input`: full type text.
/// - `open_index`: byte index of the top-level opening bracket.
///
/// Output:
/// - Base type name and raw argument list when both sides are non-empty enough.
///
/// Transformation:
/// - Trims the base name and removes the outer generic brackets.
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

/// Reports whether type text contains a top-level union separator.
///
/// Inputs:
/// - `input`: compacted type-expression text.
///
/// Output:
/// - `true` when splitting on top-level `|` yields multiple variants.
///
/// Transformation:
/// - Delegates to the depth-aware union splitter.
pub(super) fn is_union_type(input: &str) -> bool {
    split_top_level_union(input).len() > 1
}

/// Maps a bare type token to a primitive, named type, literal, or type variable.
///
/// Inputs:
/// - `text`: compacted bare type token.
/// - `aliases`: visible alias names.
/// - `vars`: mutable type-variable mapping.
/// - `next_var`: next available type-variable id.
///
/// Output:
/// - Internal type corresponding to the token.
///
/// Transformation:
/// - Recognizes primitives first, then qualified names, aliases, fresh generic
///   variables, constructor atoms, and finally ordinary named types.
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

/// Substitutes type variables according to a concrete mapping.
///
/// Inputs:
/// - `ty`: type tree that may contain variables.
/// - `mapping`: variable-id to replacement-type mapping.
///
/// Output:
/// - A cloned type tree with mapped variables replaced.
///
/// Transformation:
/// - Recursively walks all composite type forms and leaves unmapped variables
///   unchanged.
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

/// Normalizes a set of union variants.
///
/// Inputs:
/// - `types`: candidate union variants that may include nested unions.
///
/// Output:
/// - `Never` for an empty union, a single type for singleton unions, or a
///   deduplicated `Union`.
///
/// Transformation:
/// - Flattens nested unions, removes `Never`, short-circuits on `Term`, and
///   drops variants covered by wider supertypes.
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

/// Checks the current structural subtype relation.
///
/// Inputs:
/// - `lhs`: candidate subtype.
/// - `rhs`: expected supertype.
///
/// Output:
/// - `true` when `lhs` is assignable to `rhs` without conversion.
///
/// Transformation:
/// - Applies primitive widening, literal widening, Unit equivalence, fixed-array
///   compatibility, and structural map-field compatibility.
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

/// Checks structural subtype compatibility for map fields.
///
/// Inputs:
/// - `lhs`: fields present on the candidate map type.
/// - `rhs`: fields required or allowed by the expected map type.
///
/// Output:
/// - `true` when all required expected fields are present with compatible types.
///
/// Transformation:
/// - Treats optional expected fields as skippable and rejects optional candidate
///   fields where the expected type requires the field.
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

/// Unifies two types and updates type-variable substitutions.
///
/// Inputs:
/// - `left`: first type constraint.
/// - `right`: second type constraint.
/// - `subst`: mutable substitution table for type variables.
///
/// Output:
/// - `Ok(())` when the types can be made compatible.
/// - `Err(message)` with a human-readable mismatch when unification fails.
///
/// Transformation:
/// - Applies existing substitutions, binds variables with occurs checks, and
///   recursively unifies composite type structure.
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

/// Unifies two structural map field lists.
///
/// Inputs:
/// - `lhs`: candidate map fields.
/// - `rhs`: expected map fields.
/// - `subst`: mutable substitution table for field value types.
///
/// Output:
/// - `Ok(())` when required fields and value types can unify.
/// - `Err(message)` when a required field is missing or incompatible.
///
/// Transformation:
/// - Matches fields by key, enforces requiredness, and delegates value
///   compatibility to `unify`.
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

/// Binds a type variable to a concrete type.
///
/// Inputs:
/// - `id`: variable id being constrained.
/// - `value`: type to bind after generic literal widening.
/// - `subst`: mutable substitution table.
///
/// Output:
/// - `Ok(())` when the binding is accepted.
/// - `Err(message)` for recursive bindings or incompatible existing bindings.
///
/// Transformation:
/// - Widens literal bindings, unifies with any existing binding, checks occurs,
///   and records the substitution.
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

/// Checks whether a type variable occurs inside a candidate binding.
///
/// Inputs:
/// - `var`: variable id being tested.
/// - `value`: candidate type value.
/// - `subst`: current substitutions to apply before traversal.
///
/// Output:
/// - `true` when binding would create a recursive type.
///
/// Transformation:
/// - Applies substitutions and recursively scans composite type children.
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

/// Reveals opaque alias bodies for internal compatibility checks.
///
/// Inputs:
/// - `ty`: type tree that may reference opaque aliases.
/// - `aliases`: known type aliases.
///
/// Output:
/// - Type tree with directly referenced local opaque aliases substituted.
///
/// Transformation:
/// - Replaces matching opaque aliases with parameter-substituted bodies and
///   recursively processes composite type children.
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

/// Applies type-variable substitutions to a type tree.
///
/// Inputs:
/// - `ty`: type that may contain variables.
/// - `subst`: variable substitutions produced during unification.
///
/// Output:
/// - Type tree with all reachable substitutions applied.
///
/// Transformation:
/// - Recursively follows variable bindings and rewrites composite type children.
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

/// Looks up an implicit compiler builtin call.
///
/// Inputs:
/// - `name`: unqualified source-level call name.
/// - `arity`: number of call arguments.
///
/// Output:
/// - Function scheme for supported implicit builtins, otherwise `None`.
///
/// Transformation:
/// - Maps the small always-available builtin surface to typed function schemes.
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
/// - Keeps the minimal implicit prelude closed while producing a clearer
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

/// Reports whether a bare atom payload is a supported literal atom.
///
/// Inputs:
/// - `name`: atom payload without source delimiters.
///
/// Output:
/// - `true` for built-in singleton atoms and valid constructor-style atoms.
///
/// Transformation:
/// - Keeps legacy boolean and nil payloads recognizable while delegating general
///   constructor-atom validation to `is_type_constructor_atom`.
pub(super) fn is_literal_atom(name: &str) -> bool {
    matches!(name, "ok" | "error" | "true" | "false" | "nil") || is_type_constructor_atom(name)
}

/// Widens literal element types inferred inside list literals.
///
/// Inputs:
/// - `ty`: element type inferred from one list item.
///
/// Output:
/// - A list-compatible element type.
///
/// Transformation:
/// - Converts integer literals to `Int`, boolean atom literals to `Bool`, and
///   other atom literals to `Atom`.
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

/// Validates a constructor-style atom payload.
///
/// Inputs:
/// - `name`: atom payload without source delimiters.
///
/// Output:
/// - `true` when the payload starts lowercase and uses allowed atom characters.
///
/// Transformation:
/// - Enforces Terlan's conservative atom spelling for type-literal payloads.
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

/// Validates a lowercase Terlan identifier.
///
/// Inputs:
/// - `name`: candidate identifier text.
///
/// Output:
/// - `true` when the identifier starts lowercase and contains only ASCII
///   alphanumerics or underscores.
///
/// Transformation:
/// - Applies the value-level identifier shape used by tuple field labels.
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

/// Removes insignificant whitespace from a type expression.
///
/// Inputs:
/// - `input`: raw type-expression text.
///
/// Output:
/// - Text with whitespace outside quoted strings removed.
///
/// Transformation:
/// - Preserves quoted payloads and escape sequences while compacting structural
///   type syntax.
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

/// Removes one pair of parentheses that wraps an entire expression.
///
/// Inputs:
/// - `input`: candidate parenthesized text.
///
/// Output:
/// - Inner text when the outer parentheses enclose the whole input.
///
/// Transformation:
/// - Tracks parenthesis depth and rejects partial wrapping.
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

/// Splits a function type at a top-level `->`.
///
/// Inputs:
/// - `input`: compacted type-expression text.
///
/// Output:
/// - Parameter-side and return-side text when a top-level arrow exists.
///
/// Transformation:
/// - Ignores arrows nested inside parentheses, brackets, or braces.
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

/// Splits comma-separated text at top-level commas.
///
/// Inputs:
/// - `input`: list text without surrounding delimiters.
///
/// Output:
/// - Non-empty trimmed items.
///
/// Transformation:
/// - Tracks nested delimiters so commas inside nested type forms are preserved.
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

/// Splits text at top-level plus signs.
///
/// Inputs:
/// - `input`: expression-like text to split.
///
/// Output:
/// - Non-empty trimmed items.
///
/// Transformation:
/// - Tracks nested delimiters so plus signs inside nested forms are preserved.
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

/// Splits type text at top-level union separators.
///
/// Inputs:
/// - `input`: type-expression text.
///
/// Output:
/// - Non-empty trimmed union variant text.
///
/// Transformation:
/// - Tracks nested delimiters so `|` inside lists, tuples, maps, or function
///   parameters does not split the outer union.
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

/// Splits a qualified type name into module path and base name.
///
/// Inputs:
/// - `name`: type name that may contain dots.
///
/// Output:
/// - Optional module path and unqualified base name.
///
/// Transformation:
/// - Uses the final dot as the boundary so nested module paths remain intact.
pub(super) fn split_module_name(name: &str) -> (Option<String>, String) {
    if let Some((module, base)) = name.rsplit_once('.') {
        (Some(module.to_string()), base.to_string())
    } else {
        (None, name.to_string())
    }
}

/// Reports whether compacted text has list type syntax.
///
/// Inputs:
/// - `input`: compacted type-expression text.
///
/// Output:
/// - `true` for bracketed list type syntax that is not a list-comprehension
///   marker.
///
/// Transformation:
/// - Checks only the outer delimiters and excludes `||`.
pub(super) fn is_list_type(input: &str) -> bool {
    input.starts_with('[') && input.ends_with(']') && !input.contains("||")
}

/// Reports whether compacted text has tuple type syntax.
///
/// Inputs:
/// - `input`: compacted type-expression text.
///
/// Output:
/// - `true` when the text is wrapped in tuple braces.
///
/// Transformation:
/// - Performs a delimiter-shape check before full tuple parsing.
pub(super) fn is_tuple_type(input: &str) -> bool {
    input.starts_with('{') && input.ends_with('}')
}

/// Reports whether compacted text has map type syntax.
///
/// Inputs:
/// - `input`: compacted type-expression text.
///
/// Output:
/// - `true` for `#{...}` map type expressions.
///
/// Transformation:
/// - Performs a delimiter-shape check before full map-field parsing.
pub(super) fn is_map_type_expr(input: &str) -> bool {
    input.starts_with("#{") && input.ends_with('}') && input.len() >= 3
}
