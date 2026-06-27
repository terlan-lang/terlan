//! Terlan type-text lowering helpers for Erlang specs.

use super::*;
use crate::terlan_hir::identifier_to_snake;
use crate::terlan_typeck::type_system::parser::parse_type_atom_literal;

/// Lowers Terlan type text into an Erlang type-spec model.
///
/// Inputs: `input` is a Terlan type expression. Output: an `ErlType` suitable
/// for `-spec` or `-type` rendering. Transformation: normalizes spacing,
/// recognizes atoms, functions, unions, collections, tuples, maps, and named
/// types, then maps each form to the Erlang backend representation.
pub(in crate::backends::erlang::emit) fn lower_type_to_spec(input: &str) -> ErlType {
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
        if is_native_vector_type_head(head) {
            return ErlType::Named {
                name: "std_native_collections_vector_safe_native:vector".to_string(),
                args: args
                    .into_iter()
                    .map(|arg| lower_type_to_spec(&arg))
                    .collect(),
            };
        }
        if let Some(mapped) = beam_opaque_type_spec(head) {
            return mapped;
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

/// Lowers one tuple type element to an Erlang type.
///
/// Inputs: `input` is a tuple element type, optionally named as `field: Type`.
/// Output: the lowered element type. Transformation: strips supported tuple
/// field labels before delegating to normal type lowering.
pub(in crate::backends::erlang::emit) fn lower_tuple_type_elem_to_spec(input: &str) -> ErlType {
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

/// Lowers a bare type name with no surrounding syntax.
///
/// Inputs: `src` is a compact bare type name. Output: an `ErlType` for the
/// name. Transformation: distinguishes built-ins, generic variables, module
/// qualified names, and custom type names before mapping to Erlang spelling.
pub(in crate::backends::erlang::emit) fn lower_bare_type_to_spec(src: &str) -> ErlType {
    if let Some(mapped) = beam_opaque_type_spec(src) {
        mapped
    } else if src.contains('.') {
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
    } else if is_native_vector_type_head(src) {
        ErlType::Named {
            name: "std_native_collections_vector_safe_native:vector".to_string(),
            args: vec![ErlType::Raw("term()".to_string())],
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

/// Returns whether a source type head denotes `std.native.collections.Vector`.
///
/// Inputs:
/// - `head`: compact Terlan type head text from a bare type or type
///   application.
///
/// Output:
/// - `true` when the type head is the imported shorthand `Vector` or the fully
///   qualified `std.native.collections.Vector.Vector`.
///
/// Transformation:
/// - Keeps runtime bridge knowledge localized to the spec mapper so generated
///   Erlang specs reference the exported SafeNative vector type instead of
///   inventing a local `vector/1` type in every emitted module.
pub(in crate::backends::erlang::emit) fn is_native_vector_type_head(head: &str) -> bool {
    matches!(
        head,
        "Vector" | "std.native.collections.Vector.Vector" | "std.native.collections.Vector"
    )
}

/// Maps BEAM-specific opaque Terlan types to native Erlang spec types.
///
/// Inputs:
/// - `head`: compact Terlan type head text from a bare type or type
///   application.
///
/// Output:
/// - `Some(ErlType)` for BEAM opaque types with target-owned representation.
/// - `None` for ordinary source types.
///
/// Transformation:
/// - Keeps socket, port, byte, and timeout representation knowledge localized
///   to the backend type-spec mapper so generated specs do not invent local
///   opaque aliases in caller modules.
fn beam_opaque_type_spec(head: &str) -> Option<ErlType> {
    match head {
        "Bytes" | "std.beam.Bytes.Bytes" | "std.beam.Bytes" => {
            Some(ErlType::Raw("binary()".to_string()))
        }
        "Timeout" | "std.beam.Timeout.Timeout" | "std.beam.Timeout" => {
            Some(ErlType::Raw("timeout()".to_string()))
        }
        "TcpSocket" | "std.beam.Tcp.TcpSocket" => Some(ErlType::Raw("port()".to_string())),
        "Port" | "std.beam.Port.Port" => Some(ErlType::Raw("port()".to_string())),
        _ => None,
    }
}

/// Removes insignificant spacing around type application punctuation.
///
/// Inputs: `input` is type text. Output: compacted type text. Transformation:
/// rewrites known whitespace-sensitive punctuation pairs without parsing the
/// type tree.
pub(in crate::backends::erlang::emit) fn compact_type_application(input: &str) -> String {
    input
        .replace("# {", "#{")
        .replace(" [", "[")
        .replace("[ ", "[")
        .replace(" ]", "]")
        .replace(" <", "<")
        .replace("< ", "<")
        .replace(" >", ">")
        .replace(" :", ":")
        .replace(": ", ":")
        .replace(" ,", ",")
        .replace(", ", ",")
}

/// Splits fixed-array type arguments into size and element type.
///
/// Inputs: `args` is the parsed argument list. Output: `Some(size, elem)` for
/// exactly two arguments. Transformation: consumes the vector in source order.
pub(in crate::backends::erlang::emit) fn split_fixed_array_args(
    mut args: Vec<String>,
) -> Option<(String, String)> {
    if args.len() != 2 {
        return None;
    }
    Some((args.remove(0), args.remove(0)))
}

/// Extracts the Erlang type parameter name from Terlan type parameter text.
///
/// Inputs: `param` is a parameter or type application. Output: the head name.
/// Transformation: compacts spaces/application punctuation and drops any
/// bracketed type arguments.
pub(in crate::backends::erlang::emit) fn erlang_type_param_name(param: &str) -> String {
    let compact = compact_type_application(&compact_spaces(param));
    compact
        .split_once('[')
        .map(|(name, _)| name.to_string())
        .unwrap_or(compact)
}

/// Removes one balanced outer parenthesis pair.
///
/// Inputs: `input` is candidate parenthesized text. Output: the inner slice
/// when the outer pair wraps the whole input. Transformation: tracks
/// parenthesis depth to reject partial wrappers.
pub(in crate::backends::erlang::emit) fn strip_wrapping_parens(input: &str) -> Option<&str> {
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

/// Collapses repeated whitespace inside source fragments.
///
/// Inputs: `input` is source or type text. Output: text with whitespace runs
/// collapsed to single spaces. Transformation: scans characters and preserves
/// token boundaries without trimming non-whitespace characters.
pub(in crate::backends::erlang::emit) fn compact_spaces(input: &str) -> String {
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

/// Maps a Terlan type name to Erlang type-spec spelling.
///
/// Inputs: `name` is a Terlan type reference. Output: Erlang type-spec name
/// text. Transformation: handles built-ins, qualified names, and custom
/// PascalCase type names.
pub(in crate::backends::erlang::emit) fn map_type_name(name: &str) -> String {
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
pub(in crate::backends::erlang::emit) fn map_module_name(name: &str) -> String {
    name.replace('.', "_").to_ascii_lowercase()
}

/// Returns whether a type name is a single-letter generic variable.
///
/// Inputs: `name` is a type identifier. Output: `true` for one uppercase ASCII
/// letter. Transformation: checks identifier shape only.
pub(in crate::backends::erlang::emit) fn is_generic_type_var(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_uppercase()) && chars.next().is_none()
}

/// Returns whether an identifier starts with an uppercase ASCII letter.
///
/// Inputs: `name` is source identifier text. Output: uppercase-head flag.
/// Transformation: inspects only the first character.
pub(in crate::backends::erlang::emit) fn is_upper_identifier(name: &str) -> bool {
    matches!(name.chars().next(), Some(ch) if ch.is_ascii_uppercase())
}

/// Returns whether a name is a Terlan built-in type known to Erlang lowering.
///
/// Inputs: `name` is a type identifier. Output: built-in membership flag.
/// Transformation: performs an exact match against the backend type table.
pub(in crate::backends::erlang::emit) fn is_builtin_type_name(name: &str) -> bool {
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

/// Returns whether a type name should lower as a custom type.
///
/// Inputs: `name` is a type identifier. Output: custom-type flag.
/// Transformation: accepts uppercase identifiers while excluding one-letter
/// generic variables.
pub(in crate::backends::erlang::emit) fn is_custom_type_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_uppercase()) && !is_generic_type_var(name)
}

/// Returns whether a name is a lowercase Terlan identifier.
///
/// Inputs: `name` is identifier text. Output: lowercase identifier flag.
/// Transformation: validates the leading character and ASCII identifier tail.
pub(in crate::backends::erlang::emit) fn is_lower_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Converts a Terlan PascalCase type name to Erlang snake_case.
///
/// Inputs: `name` is a custom type name. Output: Erlang type atom text.
/// Transformation: delegates to the shared Terlan identifier normalizer so
/// backend type names use the same acronym and word-boundary rules as
/// generated bindings.
pub(in crate::backends::erlang::emit) fn to_erlang_type_name(name: &str) -> String {
    identifier_to_snake(name)
}

/// Maps a Terlan struct name to its Erlang record/type stem.
///
/// Inputs: `name` is a Terlan struct type name. Output: lowercase Erlang stem.
/// Transformation: lowercases the full name for backend record use.
pub(in crate::backends::erlang::emit) fn map_struct_name(name: &str) -> String {
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
pub(in crate::backends::erlang::emit) fn parse_named_type_args(
    src: &str,
) -> Option<(&str, Vec<String>)> {
    if let Some((name, rest)) = src.split_once('[') {
        if name.is_empty() {
            return None;
        }
        if let Some(args) = rest.strip_suffix(']') {
            return Some((name, split_top_level_csv(args)));
        }
    }
    if let Some((name, rest)) = src.split_once('<') {
        if name.is_empty() {
            return None;
        }
        if let Some(args) = rest.strip_suffix('>') {
            return Some((name, split_top_level_csv(args)));
        }
    }
    None
}

/// Splits comma-separated text at top-level commas only.
///
/// Inputs: `input` is comma-separated source text. Output: trimmed non-empty
/// fields. Transformation: tracks parentheses, brackets, and braces so nested
/// commas remain part of their containing expression.
pub(in crate::backends::erlang::emit) fn split_top_level_csv(input: &str) -> Vec<String> {
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

/// Splits union type text at top-level `|` separators.
///
/// Inputs: `input` is Terlan type text. Output: trimmed union branch text.
/// Transformation: tracks nested parentheses, brackets, and braces so nested
/// `|` characters do not split the outer union.
pub(in crate::backends::erlang::emit) fn split_top_level_union(input: &str) -> Vec<String> {
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

/// Returns whether type text contains a top-level union.
///
/// Inputs: `input` is Terlan type text. Output: union membership flag.
/// Transformation: reuses top-level union splitting and checks for multiple
/// branches.
pub(in crate::backends::erlang::emit) fn is_union(input: &str) -> bool {
    split_top_level_union(input).len() > 1
}

/// Splits function type text at a top-level `->`.
///
/// Inputs: `input` is Terlan type text. Output: `Some(params, return_type)`
/// when a top-level arrow exists. Transformation: tracks nested delimiters and
/// trims both sides of the arrow.
pub(in crate::backends::erlang::emit) fn split_top_level_arrow(
    input: &str,
) -> Option<(String, String)> {
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

/// Splits named tuple element text at a top-level colon.
///
/// Inputs: `input` is tuple element type text. Output: `Some(label, type)` for
/// named elements. Transformation: scans with delimiter and quote tracking so
/// nested colons do not split the element.
pub(in crate::backends::erlang::emit) fn split_named_tuple_type_elem(
    input: &str,
) -> Option<(&str, &str)> {
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
