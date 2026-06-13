//! Shared Erlang backend lowering utilities.
//!
//! This module owns target-specific helper functions that are used by multiple
//! emitter paths: formal syntax-output lowering, CoreIR expression lowering,
//! and Erlang render-model formatting.

use super::*;

pub(super) fn is_bool_literal_name(name: &str) -> bool {
    matches!(name, "true" | "false")
}

pub(super) fn lower_type_to_spec(input: &str) -> ErlType {
    let src = compact_type_application(&compact_spaces(input));

    if src.is_empty() {
        return ErlType::Raw("any()".to_string());
    }

    if let Some(atom) = parse_type_atom_literal(&src) {
        return ErlType::Raw(render_atom_expr(&atom));
    }

    if let Some((params, ret)) = split_top_level_arrow(&src) {
        let params = strip_wrapping_parens(&params)
            .map(str::to_string)
            .unwrap_or(params);
        let args = split_top_level_csv(&params)
            .into_iter()
            .map(|param| lower_type_to_spec(&param))
            .collect::<Vec<_>>();
        return ErlType::Fun {
            args,
            ret: Box::new(lower_type_to_spec(&ret)),
        };
    }

    if is_union(&src) {
        return ErlType::Union(
            split_top_level_union(&src)
                .into_iter()
                .map(|branch| lower_type_to_spec(&branch))
                .collect(),
        )
        .normalized();
    }

    if let Some((head, args)) = parse_named_type_args(&src) {
        if head == "FixedArray" {
            if let Some((size, elem_type)) = split_fixed_array_args(args) {
                if let Ok(size) = size.parse::<usize>() {
                    let element = lower_type_to_spec(&elem_type);
                    return ErlType::Tuple(vec![element; size]);
                }
            }
            return ErlType::Raw("tuple()".to_string());
        }
        if head == "List" {
            let inner = args
                .first()
                .map(|arg| lower_type_to_spec(arg))
                .unwrap_or_else(|| ErlType::Raw("any()".to_string()));
            return ErlType::List(Box::new(inner));
        }
        if matches!(head, "Map" | "Set") {
            return ErlType::Raw("map()".to_string());
        }
        if head == "Option" {
            return ErlType::Named {
                name: "std_core_option:typer_option".to_string(),
                args: args
                    .into_iter()
                    .map(|arg| lower_type_to_spec(&arg))
                    .collect(),
            };
        }
        if head == "Result" {
            return ErlType::Named {
                name: "std_core_result:result".to_string(),
                args: args
                    .into_iter()
                    .map(|arg| lower_type_to_spec(&arg))
                    .collect(),
            };
        }
        if is_generic_type_var(head) {
            return ErlType::Raw("term()".to_string());
        }
        return ErlType::Named {
            name: map_type_name(head),
            args: args
                .into_iter()
                .map(|arg| lower_type_to_spec(&arg))
                .collect(),
        };
    }

    if src.starts_with('[') && src.ends_with(']') {
        let inner = &src[1..src.len() - 1];
        if inner.trim().is_empty() {
            return ErlType::Raw("[]".to_string());
        }
        return ErlType::List(Box::new(lower_type_to_spec(inner)));
    }

    if src.starts_with('{') && src.ends_with('}') {
        let inner = &src[1..src.len() - 1];
        return ErlType::Tuple(
            split_top_level_csv(inner)
                .into_iter()
                .map(|field| lower_tuple_type_elem_to_spec(&field))
                .collect(),
        );
    }

    if src.starts_with("#{") && src.ends_with('}') {
        let inner = &src[2..src.len() - 1];
        if inner.trim().is_empty() {
            return ErlType::Raw("map()".to_string());
        }
        if let Some(fields) = split_top_level_csv(inner)
            .into_iter()
            .map(|field| lower_map_type_elem_to_spec(&field))
            .collect::<Option<Vec<_>>>()
        {
            return ErlType::Map(fields);
        }
        return ErlType::Raw("map()".to_string());
    }

    lower_bare_type_to_spec(&src)
}

pub(super) fn lower_tuple_type_elem_to_spec(input: &str) -> ErlType {
    if let Some((label, ty)) = split_named_tuple_type_elem(input) {
        if is_lower_identifier(label) || label == "_" {
            return lower_type_to_spec(ty);
        }
    }

    lower_type_to_spec(input)
}

/// Lowers one Terlan map type field to an Erlang map field type.
///
/// Inputs:
/// - `input`: one map type field such as `name := String`.
///
/// Output:
/// - `Some(ErlMapTypeField)` when a top-level `:=` or `=>` separator is found.
/// - `None` when the field is malformed.
///
/// Transformation:
/// - Keeps the source key spelling and lowers the value type through the same
///   type-spec mapper used by ordinary annotations.
fn lower_map_type_elem_to_spec(input: &str) -> Option<ErlMapTypeField> {
    let (key, value, required) = split_map_type_elem(input)?;
    Some(ErlMapTypeField {
        key: key.to_string(),
        value: lower_type_to_spec(value),
        required,
    })
}

pub(super) fn lower_bare_type_to_spec(src: &str) -> ErlType {
    if src.contains('.') {
        ErlType::Named {
            name: map_type_name(src),
            args: Vec::new(),
        }
    } else if is_generic_type_var(src) {
        ErlType::Raw(src.to_string())
    } else if is_builtin_type_name(src) {
        ErlType::Raw(map_type_name(src))
    } else if src == "Option" {
        ErlType::Named {
            name: "std_core_option:typer_option".to_string(),
            args: Vec::new(),
        }
    } else if src == "Result" {
        ErlType::Named {
            name: "std_core_result:result".to_string(),
            args: Vec::new(),
        }
    } else if is_custom_type_name(src) {
        ErlType::Named {
            name: map_type_name(src),
            args: Vec::new(),
        }
    } else {
        ErlType::Raw(map_type_name(src))
    }
}

pub(super) fn compact_type_application(input: &str) -> String {
    input
        .replace("# {", "#{")
        .replace(" [", "[")
        .replace("[ ", "[")
        .replace(" ]", "]")
        .replace(" :", ":")
        .replace(": ", ":")
        .replace(" ,", ",")
        .replace(", ", ",")
}

pub(super) fn split_fixed_array_args(mut args: Vec<String>) -> Option<(String, String)> {
    if args.len() != 2 {
        return None;
    }
    Some((args.remove(0), args.remove(0)))
}

pub(super) fn erlang_type_param_name(param: &str) -> String {
    let compact = compact_type_application(&compact_spaces(param));
    compact
        .split_once('[')
        .map(|(name, _)| name.to_string())
        .unwrap_or(compact)
}

pub(super) fn strip_wrapping_parens(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return None;
    }

    let mut depth = 0usize;
    for (idx, ch) in trimmed.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && idx != trimmed.len() - 1 {
                    return None;
                }
            }
            _ => {}
        }
    }

    Some(&trimmed[1..trimmed.len() - 1])
}

pub(super) fn compact_spaces(input: &str) -> String {
    let mut out = String::new();
    let mut in_token = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if in_token {
                out.push(' ');
            }
            in_token = false;
        } else {
            out.push(ch);
            in_token = true;
        }
    }

    let mut result = out;
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }
    result
}

pub(super) fn map_type_name(name: &str) -> String {
    match name {
        "Int" => "integer()".to_string(),
        "Float" => "float()".to_string(),
        "Number" => "number()".to_string(),
        "String" => "binary()".to_string(),
        "Binary" => "binary()".to_string(),
        "Text" => "binary()".to_string(),
        "Atom" => "atom()".to_string(),
        "Bool" => "boolean()".to_string(),
        "Unit" => "unit".to_string(),
        "Term" => "term()".to_string(),
        "Dynamic" => "dynamic()".to_string(),
        "Never" => "none()".to_string(),
        "Option" => "typer_option".to_string(),
        "Result" => "result".to_string(),
        _ if name == "Pid" => "pid".to_string(),
        _ if name.contains('.') => {
            let (module, base) = name.rsplit_once('.').unwrap_or(("", name));
            if module.is_empty() {
                map_type_name(base)
            } else {
                format!("{}:{}", map_module_name(module), map_type_name(base))
            }
        }
        _ if is_custom_type_name(name) => to_erlang_type_name(name),
        _ => name.to_string(),
    }
}

/// Converts a Terlan module path into an Erlang module atom spelling.
///
/// Inputs:
/// - `name`: Terlan source module path, such as `std.core.Bool`.
///
/// Output:
/// - Erlang-compatible lowercase module name, such as `std_core_bool`.
///
/// Transformation:
/// - Replaces Terlan namespace dots with underscores and lowercases the result
///   so uppercase Terlan namespace segments do not leak into BEAM artifacts.
pub(super) fn map_module_name(name: &str) -> String {
    name.replace('.', "_").to_ascii_lowercase()
}

pub(super) fn is_generic_type_var(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_uppercase()) && chars.next().is_none()
}

pub(super) fn is_upper_identifier(name: &str) -> bool {
    matches!(name.chars().next(), Some(ch) if ch.is_ascii_uppercase())
}

pub(super) fn is_builtin_type_name(name: &str) -> bool {
    matches!(
        name,
        "Int"
            | "Float"
            | "Number"
            | "String"
            | "Binary"
            | "Text"
            | "Atom"
            | "Bool"
            | "Unit"
            | "Term"
            | "Dynamic"
            | "Never"
            | "Option"
            | "Result"
            | "Pid"
    )
}

pub(super) fn is_custom_type_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_uppercase()) && !is_generic_type_var(name)
}

pub(super) fn is_lower_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(super) fn is_raw_atom_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' || ch == '-')
}

pub(super) fn to_erlang_type_name(name: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn map_struct_name(name: &str) -> String {
    name.to_lowercase()
}

/// Parses a named generic type application.
///
/// Inputs:
/// - `src`: compact Terlan type text such as `Option[Int]`.
///
/// Output:
/// - `Some((name, args))` when `src` has a non-empty named head followed by
///   bracketed type arguments.
/// - `None` for non-generic types and list shorthand such as `[Int]`.
///
/// Transformation:
/// - Splits once at the first `[` and top-level-comma splits the argument
///   payload, deliberately rejecting empty heads so list shorthand remains
///   available to the dedicated list-type branch.
pub(super) fn parse_named_type_args(src: &str) -> Option<(&str, Vec<String>)> {
    if let Some((name, rest)) = src.split_once('[') {
        if name.is_empty() {
            return None;
        }
        if let Some(args) = rest.strip_suffix(']') {
            return Some((name, split_top_level_csv(args)));
        }
    }
    None
}

pub(super) fn split_top_level_csv(input: &str) -> Vec<String> {
    let mut depth_p = 0;
    let mut depth_b = 0;
    let mut depth_br = 0;
    let mut start = 0usize;
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();

    for (idx, ch) in chars.iter().enumerate() {
        match ch {
            '(' => depth_p += 1,
            ')' => {
                if depth_p > 0 {
                    depth_p -= 1;
                }
            }
            '[' => depth_b += 1,
            ']' => {
                if depth_b > 0 {
                    depth_b -= 1;
                }
            }
            '{' => depth_br += 1,
            '}' => {
                if depth_br > 0 {
                    depth_br -= 1;
                }
            }
            ',' if depth_p == 0 && depth_b == 0 && depth_br == 0 => {
                let item: String = chars[start..idx].iter().collect();
                if !item.trim().is_empty() {
                    out.push(item.trim().to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }
    }

    let last = chars[start..].iter().collect::<String>();
    if !last.trim().is_empty() {
        out.push(last.trim().to_string());
    }

    out
}

pub(super) fn split_top_level_union(input: &str) -> Vec<String> {
    let mut depth_p = 0;
    let mut depth_b = 0;
    let mut depth_br = 0;
    let mut start = 0usize;
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();

    for (idx, ch) in chars.iter().enumerate() {
        match ch {
            '(' => depth_p += 1,
            ')' => {
                if depth_p > 0 {
                    depth_p -= 1;
                }
            }
            '[' => depth_b += 1,
            ']' => {
                if depth_b > 0 {
                    depth_b -= 1;
                }
            }
            '{' => depth_br += 1,
            '}' => {
                if depth_br > 0 {
                    depth_br -= 1;
                }
            }
            '|' if depth_p == 0 && depth_b == 0 && depth_br == 0 => {
                let item: String = chars[start..idx].iter().collect();
                if !item.trim().is_empty() {
                    out.push(item.trim().to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }
    }

    let last = chars[start..].iter().collect::<String>();
    if !last.trim().is_empty() {
        out.push(last.trim().to_string());
    }

    out
}

pub(super) fn is_union(input: &str) -> bool {
    split_top_level_union(input).len() > 1
}

pub(super) fn split_top_level_arrow(input: &str) -> Option<(String, String)> {
    let chars: Vec<char> = input.chars().collect();
    let mut depth_p = 0;
    let mut depth_b = 0;
    let mut depth_br = 0;

    for i in 0..chars.len() {
        if i + 1 >= chars.len() {
            continue;
        }
        match chars[i] {
            '(' => depth_p += 1,
            ')' if depth_p > 0 => depth_p -= 1,
            '[' => depth_b += 1,
            ']' if depth_b > 0 => depth_b -= 1,
            '{' => depth_br += 1,
            '}' if depth_br > 0 => depth_br -= 1,
            _ => {}
        }

        if chars[i] == '-' && chars[i + 1] == '>' && depth_p == 0 && depth_b == 0 && depth_br == 0 {
            let left = chars[..i].iter().collect::<String>().trim().to_string();
            let right = chars[i + 2..].iter().collect::<String>().trim().to_string();
            return Some((left, right));
        }
    }

    None
}

#[derive(Debug, Clone)]
pub(super) struct LowerTemplate {
    pub(super) nodes: Vec<terlan_html::HtmlNode>,
    pub(super) props: BTreeMap<String, String>,
}

pub(super) fn constructor_function_name(name: &str, fixed_arity: usize, varargs: bool) -> String {
    if varargs {
        format!(
            "typer_ctor_{}_varargs_{}",
            to_erlang_type_name(name),
            fixed_arity
        )
    } else {
        format!("typer_ctor_{}_{}", to_erlang_type_name(name), fixed_arity)
    }
}

pub(super) fn lower_raw_decl_text(kind: &str, text: &str) -> String {
    match kind {
        "native" => {
            let signatures = extract_native_function_signatures(text);
            if signatures.is_empty() {
                return format!("% terlan native: {}\n\n", text);
            }

            let native_module =
                extract_native_module_name(text).unwrap_or_else(|| "terlan_native".to_string());
            let mut names = BTreeSet::new();
            for signature in &signatures {
                names.insert((signature.name.clone(), signature.arity));
            }

            let mut out = String::new();
            out.push_str(&format!(
                "-on_load(load/0).\n-export([load/0, {}]).\n\n",
                names
                    .iter()
                    .map(|(name, arity)| format!("{}/{}", name, arity))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            out.push_str(&format!(
                "load() ->\n    erlang:load_nif(filename:join([code:priv_dir(?MODULE), \"{}.so\"]), 0).\n\n",
                native_module
            ));
            for (name, arity) in names {
                let args = (0..arity)
                    .map(|idx| format!("A{}", idx + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "{}({}) ->\n    erlang:nif_error(nif_not_loaded).\n\n",
                    name, args
                ));
            }
            out
        }
        _ => format!("% terlan {}: {}\n\n", kind, text),
    }
}

pub(super) fn simple_template_type_name(type_text: &str) -> Option<&str> {
    let mut chars = type_text.chars();
    let first = chars.next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    if chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        Some(type_text)
    } else {
        None
    }
}

pub(super) fn is_template_html_type(type_text: &str) -> bool {
    let trimmed = type_text.trim();
    trimmed == "Html" || trimmed.starts_with("Html[")
}

pub(super) fn static_html_attr_binary_text(value: &str) -> String {
    if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
    {
        return inner.to_string();
    }
    if let Some(inner) = value
        .strip_prefix("<<\"")
        .and_then(|text| text.strip_suffix("\">>"))
    {
        return inner.to_string();
    }
    value.to_string()
}

pub(super) fn erlang_binary_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "<<>>".to_string();
    }

    format!(
        "<<{}>>",
        bytes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub(super) fn html_binary(text: &str) -> ErlExpr {
    ErlExpr::Binary(format!("<<\"{}\">>", escape_erlang_binary_string(text)))
}

pub(super) fn escape_erlang_binary_string(text: &str) -> String {
    text.chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

pub(super) fn escape_html_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn split_named_tuple_type_elem(input: &str) -> Option<(&str, &str)> {
    let mut depth_p = 0usize;
    let mut depth_b = 0usize;
    let mut depth_br = 0usize;
    let mut quote = None;
    let mut escape = false;

    for (idx, ch) in input.char_indices() {
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
            ':' if idx > 0 && depth_p == 0 && depth_b == 0 && depth_br == 0 => {
                return Some((input[..idx].trim(), input[idx + ch.len_utf8()..].trim()));
            }
            _ => {}
        }
    }

    None
}

/// Splits one Terlan map type field at the top-level map separator.
///
/// Inputs:
/// - `input`: map field source text.
///
/// Output:
/// - `Some((key, value, required))` where `required` is true for `:=`.
/// - `None` when no top-level `:=` or `=>` exists.
///
/// Transformation:
/// - Scans while tracking parentheses, brackets, braces, strings, and escapes
///   so nested type expressions do not get split accidentally.
fn split_map_type_elem(input: &str) -> Option<(&str, &str, bool)> {
    let mut depth_p = 0usize;
    let mut depth_b = 0usize;
    let mut depth_br = 0usize;
    let mut quote = None;
    let mut escape = false;
    let bytes = input.as_bytes();
    let mut idx = 0usize;

    while idx < bytes.len() {
        let ch = input[idx..].chars().next()?;
        if let Some(quote_ch) = quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == quote_ch {
                quote = None;
            }
            idx += ch.len_utf8();
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
            ':' | '=' if idx + 1 < bytes.len() && depth_p == 0 && depth_b == 0 && depth_br == 0 => {
                let sep = &input[idx..idx + 2];
                if sep == ":=" || sep == "=>" {
                    return Some((input[..idx].trim(), input[idx + 2..].trim(), sep == ":="));
                }
            }
            _ => {}
        }
        idx += ch.len_utf8();
    }

    None
}

pub(super) fn parse_type_atom_literal(input: &str) -> Option<String> {
    if let Some(atom) = atom_type_literal_payload(input) {
        return Some(atom);
    }

    let atom = input.trim().strip_prefix(':')?;
    if atom.is_empty() {
        return None;
    }
    if is_raw_atom_name(atom) {
        return Some(atom.to_string());
    }
    if atom.len() >= 2 && atom.starts_with('\'') && atom.ends_with('\'') {
        return unquote_type_atom(atom);
    }
    None
}

/// Extracts the canonical Terlan `Atom["name"]` singleton payload.
///
/// Inputs:
/// - `input`: compacted or raw type expression text.
///
/// Output:
/// - `Some(String)` for non-empty `Atom["name"]` payloads.
/// - `None` for non-Atom type expressions.
///
/// Transformation:
/// - Parses the language-neutral atom primitive and unescapes the contained
///   string literal for backend atom rendering.
fn atom_type_literal_payload(input: &str) -> Option<String> {
    let inner = input
        .trim()
        .strip_prefix("Atom[")?
        .strip_suffix(']')?
        .trim();
    parse_atom_string_literal(inner)
}

/// Parses the string literal inside `Atom["name"]`.
///
/// Inputs:
/// - `input`: candidate string literal source including quotes.
///
/// Output:
/// - The unescaped non-empty atom name, or `None`.
///
/// Transformation:
/// - Handles simple backslash escaping without interpreting backend syntax.
fn parse_atom_string_literal(input: &str) -> Option<String> {
    let inner = input.strip_prefix('"')?.strip_suffix('"')?;
    if inner.is_empty() {
        return None;
    }
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

pub(super) fn normalize_trait_type_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn trait_method_wrapper_name(trait_name: &str, method_name: &str) -> String {
    format!(
        "typer_trait_{}_{}_dict",
        sanitize_erlang_fn_name(trait_name),
        sanitize_erlang_fn_name(method_name),
    )
}

/// Builds a typed trait-method dictionary wrapper name.
///
/// Inputs:
/// - `trait_name`: source trait name.
/// - `method_name`: trait method name.
/// - `type_arg`: concrete implementation type argument.
///
/// Output:
/// - Erlang-safe wrapper function name that is distinct per trait/method/type.
///
/// Transformation:
/// - Sanitizes every source segment and appends the type identity before the
///   dictionary suffix so multiple impls for the same method can coexist.
pub(super) fn typed_trait_method_wrapper_name(
    trait_name: &str,
    method_name: &str,
    type_arg: &str,
) -> String {
    format!(
        "typer_trait_{}_{}_{}_dict",
        sanitize_erlang_fn_name(trait_name),
        sanitize_erlang_fn_name(method_name),
        sanitize_erlang_fn_name(type_arg),
    )
}

pub(super) fn sanitize_erlang_fn_name(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_".to_string()
    } else if out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("_{}", out)
    } else {
        out
    }
}

pub(super) fn sanitize_erlang_var(name: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if idx == 0 && ch.is_ascii_lowercase() {
                out.push(ch.to_ascii_uppercase());
            } else {
                out.push(ch);
            }
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_arg".to_string()
    } else if out == "_" || out.starts_with('_') {
        out
    } else if out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("_{}", out)
    } else if out.chars().next().is_some_and(|ch| ch.is_ascii_lowercase()) {
        let mut chars = out.chars();
        let first = chars.next().unwrap().to_ascii_uppercase();
        format!("{}{}", first, chars.collect::<String>())
    } else {
        out
    }
}

pub(super) fn trait_dictionary_expr(trait_name: &str, method_name: &str) -> ErlExpr {
    ErlExpr::Map(vec![
        ErlMapField {
            key: "\"__typer_trait\"".to_string(),
            value: ErlExpr::Atom(sanitize_erlang_fn_name(trait_name)),
            required: false,
        },
        ErlMapField {
            key: "\"__typer_method\"".to_string(),
            value: ErlExpr::Atom(sanitize_erlang_fn_name(method_name)),
            required: false,
        },
    ])
}

pub(super) fn lower_syntax_binary_op(operator: Option<&str>) -> ErlBinaryOp {
    match operator.unwrap_or("=") {
        "+" => ErlBinaryOp::Add,
        "-" => ErlBinaryOp::Sub,
        "*" => ErlBinaryOp::Mul,
        "/" => ErlBinaryOp::Div,
        "=" => ErlBinaryOp::Eq,
        "==" => ErlBinaryOp::EqEq,
        "=:=" => ErlBinaryOp::EqEqEq,
        "!=" | "/=" => ErlBinaryOp::NotEq,
        "=/=" => ErlBinaryOp::NotEqEq,
        ">=" => ErlBinaryOp::GtEq,
        "<" => ErlBinaryOp::Lt,
        ">" => ErlBinaryOp::Gt,
        "<=" => ErlBinaryOp::LtEq,
        "div" => ErlBinaryOp::DivRem,
        "rem" => ErlBinaryOp::Rem,
        "and" | "&&" => ErlBinaryOp::And,
        "or" | "||" => ErlBinaryOp::Or,
        "|>" => ErlBinaryOp::PipeForward,
        "!" => ErlBinaryOp::Send,
        _ => ErlBinaryOp::Eq,
    }
}

pub(super) fn lower_syntax_unary_op(operator: Option<&str>) -> ErlUnaryOp {
    match operator.unwrap_or("") {
        "-" => ErlUnaryOp::Neg,
        "not" | "!" => ErlUnaryOp::Not,
        _ => ErlUnaryOp::Not,
    }
}

#[cfg(test)]
pub(super) fn lower_syntax_binary_op_render(operator: &str) -> &'static str {
    lower_syntax_binary_op(Some(operator)).render()
}
