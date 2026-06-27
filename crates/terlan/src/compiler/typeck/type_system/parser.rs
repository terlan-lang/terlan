use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{atom_type_literal_payload, MapFieldType, Type, TypeAlias, TypeVarId};

use super::text::{
    compact_spaces, is_list_type, is_map_type_expr, is_tuple_type, split_module_name,
    split_top_level_arrow, split_top_level_csv, split_top_level_union, strip_wrapping_parens,
};
use super::{expand_type_aliases, normalize_union};

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
pub(crate) fn is_map_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
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
pub(crate) fn parse_type_expr(
    input: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<Type> {
    let raw = input.trim();
    if let Some((params, body)) = split_existential_type(raw) {
        return parse_existential_type(params, body, aliases, vars, next_var);
    }

    let src = compact_spaces(input);
    if src.is_empty() {
        return Some(Type::Dynamic);
    }
    if src == "_" {
        return Some(Type::Placeholder);
    }
    if let Some(inner) = strip_wrapping_parens(&src) {
        return parse_type_expr(inner.trim(), aliases, vars, next_var);
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
        if let Some(constructor) = vars.get(base).copied() {
            return Some(Type::Apply { constructor, args });
        }
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

/// Parses a split existential type into the internal model.
///
/// Inputs:
/// - `params`: raw comma-separated binder list.
/// - `body`: raw existential body text.
/// - `aliases`, `vars`, and `next_var`: visible type context.
///
/// Output:
/// - Internal existential type when every binder and the body are valid.
///
/// Transformation:
/// - Allocates binders in a nested type-variable scope, parses the body in
///   that scope, and leaves the caller's outer variable map unchanged.
fn parse_existential_type(
    params: String,
    body: String,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<Type> {
    let mut scoped_vars = vars.clone();
    let mut bound_params = Vec::new();
    for param in split_top_level_csv(&params) {
        let param = param.trim();
        if !is_type_var_identifier(param) {
            return None;
        }
        let id = fresh_type_var(param, &mut scoped_vars, next_var)?;
        bound_params.push(id);
    }
    if bound_params.is_empty() {
        return None;
    }
    let body = parse_type_expr(body.trim(), aliases, &mut scoped_vars, next_var)?;
    Some(Type::Existential {
        params: bound_params,
        body: Box::new(body),
    })
}

/// Splits an existential type into binder list and body.
///
/// Inputs:
/// - `input`: compacted type expression text.
///
/// Output:
/// - `Some((params, body))` for `exists T, U. Body`.
/// - `None` when the expression is not an existential package type.
///
/// Transformation:
/// - Finds the first top-level dot after the `exists` keyword while respecting
///   nested delimiters in the eventual body text.
pub(crate) fn split_existential_type(input: &str) -> Option<(String, String)> {
    let rest = input.strip_prefix("exists ")?;
    let mut depth_p = 0usize;
    let mut depth_b = 0usize;
    let mut depth_br = 0usize;

    for (index, ch) in rest.char_indices() {
        match ch {
            '(' => depth_p += 1,
            ')' => depth_p = depth_p.saturating_sub(1),
            '[' => depth_b += 1,
            ']' => depth_b = depth_b.saturating_sub(1),
            '{' => depth_br += 1,
            '}' => depth_br = depth_br.saturating_sub(1),
            '.' if depth_p == 0 && depth_b == 0 && depth_br == 0 => {
                let params = rest[..index].trim();
                let body = rest[index + ch.len_utf8()..].trim();
                if params.is_empty() || body.is_empty() {
                    return None;
                }
                return Some((params.to_string(), body.to_string()));
            }
            _ => {}
        }
    }

    None
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
pub(crate) fn parse_tuple_type_elem(
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

/// Extracts source constructor labels from a single-shape type alias.
///
/// Inputs:
/// - `variants`: source type variants from a type alias declaration or
///   interface summary.
///
/// Output:
/// - Tuple field labels after the leading atom tag for eligible tagged tuple
///   aliases.
/// - Empty vector for atom aliases or non-eligible aliases.
///
/// Transformation:
/// - Reads labels from source text before tuple labels are erased by the
///   internal `Type` model, preserving only lowercase constructor argument
///   names that can be used by named call-site arguments.
pub(crate) fn alias_constructor_param_names_from_variants(variants: &[String]) -> Vec<String> {
    if variants.len() != 1 {
        return Vec::new();
    }
    let src = compact_spaces(&variants[0]);
    if is_union_type(&src) || !(src.starts_with('{') && src.ends_with('}')) {
        return Vec::new();
    }

    let inner = &src[1..src.len() - 1];
    let mut items = split_top_level_csv(inner).into_iter();
    let Some(tag) = items.next() else {
        return Vec::new();
    };
    if parse_type_atom_literal(tag.trim()).is_none() {
        return Vec::new();
    }

    let mut labels = Vec::new();
    for item in items {
        let Some((label, _ty)) = split_named_tuple_type_elem(item.trim()) else {
            return Vec::new();
        };
        if !is_lower_identifier(label) {
            return Vec::new();
        }
        labels.push(label.to_string());
    }
    labels
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
pub(crate) fn split_named_tuple_type_elem(input: &str) -> Option<(&str, &str)> {
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
pub(crate) fn parse_type_atom_literal(input: &str) -> Option<String> {
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
pub(crate) fn unquote_type_atom(text: &str) -> Option<String> {
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
pub(crate) fn parse_map_type_expression(
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
pub(crate) fn parse_map_type_field(
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
pub(crate) fn split_map_field(input: &str) -> Option<(&str, &str, bool)> {
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
pub(crate) fn split_named_type(input: &str) -> Option<(&str, String)> {
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
pub(crate) fn split_named_type_inner(input: &str, open_index: usize) -> Option<(&str, String)> {
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
pub(crate) fn is_union_type(input: &str) -> bool {
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
pub(crate) fn map_named_or_var(
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

/// Allocates or returns a type variable for a bare uppercase type token.
///
/// Inputs:
/// - `text`: candidate type-variable name.
/// - `vars`: current variable-name mapping.
/// - `next_var`: next allocatable variable id.
///
/// Output:
/// - Existing or newly allocated variable id, or `None` for non-variable text.
///
/// Transformation:
/// - Accepts uppercase unqualified names and mutates the caller-owned mapping
///   so repeated uses of the same name share the same type variable id.
pub(crate) fn fresh_type_var(
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

/// Validates a source type-variable identifier.
///
/// Inputs:
/// - `text`: candidate type variable name.
///
/// Output:
/// - `true` when the name is an uppercase unqualified type variable token.
///
/// Transformation:
/// - Mirrors `fresh_type_var` eligibility without mutating the variable table.
fn is_type_var_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !text.contains('.')
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
pub(crate) fn is_type_constructor_atom(name: &str) -> bool {
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
pub(crate) fn is_lower_identifier(name: &str) -> bool {
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
