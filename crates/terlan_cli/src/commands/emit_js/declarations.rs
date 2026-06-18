use terlan_typeck::{CoreModule, CoreTupleTypeElem, CoreType, CoreVisibility};

/// Serializes public CoreIR type/function signatures into TypeScript declarations.
///
/// Inputs:
/// - `module`: backend-independent CoreIR module produced by the formal
///   compile pipeline.
///
/// Output:
/// - TypeScript declaration source text.
///
/// Transformation:
/// - Filters private CoreIR declarations, emits public type aliases when the
///   current CoreIR payload can describe them, and emits public function
///   signatures from CoreIR parameter/return metadata.
pub(crate) fn emit_core_module_to_typescript_declarations(module: &CoreModule) -> String {
    let mut out = String::new();
    for type_decl in &module.types {
        if !matches!(type_decl.visibility, CoreVisibility::Public) {
            continue;
        }
        let params = typescript_generic_params(&type_decl.params);
        if type_decl.name == "Result" && type_decl.params.as_slice() == ["T", "E"] {
            out.push_str(
                "export type Result<T, E> = { tag: \"ok\"; value: T } | { tag: \"error\"; error: E };\n\n",
            );
            continue;
        }
        let body = type_decl
            .core_body
            .as_ref()
            .map(core_type_to_typescript)
            .unwrap_or_else(|| type_body_text_to_typescript(&type_decl.body));
        out.push_str(&format!(
            "export type {}{} = {};\n\n",
            type_decl.name, params, body
        ));
    }

    for function in &module.functions {
        if !function.public {
            continue;
        }
        let params = function
            .params
            .iter()
            .map(|param| {
                format!(
                    "{}: {}",
                    param.name,
                    core_type_or_text_to_typescript(param.core_ty.as_ref(), &param.ty)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "export function {}({}): {};\n",
            function.name,
            params,
            core_type_or_text_to_typescript(
                function.core_return_type.as_ref(),
                &function.return_type
            )
        ));
    }
    out
}

/// Renders type parameters as a TypeScript generic suffix.
///
/// Inputs:
/// - `params`: CoreIR type parameter names.
///
/// Output:
/// - Empty string when there are no type parameters, or a `<T, U>` suffix.
///
/// Transformation:
/// - Joins CoreIR parameter names without changing their identity.
fn typescript_generic_params(params: &[String]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

/// Maps a typed CoreIR type payload to TypeScript text.
///
/// Inputs:
/// - `ty`: structured CoreIR type.
///
/// Output:
/// - TypeScript type text for declaration emission.
///
/// Transformation:
/// - Converts backend-independent CoreIR type variants into conservative
///   TypeScript declarations without assuming a runtime-specific JS layout
///   beyond tuples/lists/struct-like field objects.
fn core_type_to_typescript(ty: &CoreType) -> String {
    match ty {
        CoreType::Int | CoreType::Float | CoreType::Number => "number".to_string(),
        CoreType::String | CoreType::Binary => "string".to_string(),
        CoreType::Bool => "boolean".to_string(),
        CoreType::Term | CoreType::Dynamic | CoreType::Atom => "unknown".to_string(),
        CoreType::Never => "never".to_string(),
        CoreType::AtomLiteral(atom) => typescript_string_literal(atom),
        CoreType::Named(name) => typer_type_to_typescript(name),
        CoreType::Apply { constructor, args } => {
            let args = args
                .iter()
                .map(core_type_to_typescript)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", constructor, args)
        }
        CoreType::List(item) => format!("Array<{}>", core_type_to_typescript(item)),
        CoreType::Tuple(items) => {
            let items = items
                .iter()
                .map(core_tuple_type_elem_to_typescript)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", items)
        }
        CoreType::Struct { fields, .. } => {
            let fields = fields
                .iter()
                .map(|field| format!("{}: {}", field.name, core_type_to_typescript(&field.ty)))
                .collect::<Vec<_>>()
                .join("; ");
            format!("{{ {} }}", fields)
        }
        CoreType::Map(_) => "Record<string, unknown>".to_string(),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            let params = params
                .iter()
                .enumerate()
                .map(|(index, param)| format!("arg{}: {}", index, core_type_to_typescript(param)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({}) => {}", params, core_type_to_typescript(return_type))
        }
        CoreType::Union(items) => items
            .iter()
            .map(core_type_to_typescript)
            .collect::<Vec<_>>()
            .join(" | "),
    }
}

/// Maps a CoreIR tuple type element to TypeScript tuple element text.
///
/// Inputs:
/// - `elem`: one structured CoreIR tuple element.
///
/// Output:
/// - TypeScript tuple element text, preserving labels when present.
///
/// Transformation:
/// - Converts unlabeled tuple elements as plain tuple members and named tuple
///   fields as TypeScript labeled tuple members.
fn core_tuple_type_elem_to_typescript(elem: &CoreTupleTypeElem) -> String {
    match elem {
        CoreTupleTypeElem::Type(ty) => core_type_to_typescript(ty),
        CoreTupleTypeElem::Field { name, ty } if name == "_" => core_type_to_typescript(ty),
        CoreTupleTypeElem::Field { name, ty } => {
            format!("{}: {}", name, core_type_to_typescript(ty))
        }
    }
}

/// Maps CoreIR type metadata to TypeScript with a text fallback.
///
/// Inputs:
/// - `core`: optional structured CoreIR type.
/// - `text`: original resolver/typechecker type text.
///
/// Output:
/// - TypeScript type text.
///
/// Transformation:
/// - Prefers structured CoreIR type mapping and falls back to the existing
///   lightweight textual mapper when the CoreIR payload is not yet available.
fn core_type_or_text_to_typescript(core: Option<&CoreType>, text: &str) -> String {
    core.map(core_type_to_typescript)
        .unwrap_or_else(|| typer_type_to_typescript(text))
}

/// Maps textual type body variants to TypeScript.
///
/// Inputs:
/// - `body`: resolver-preserved type body variants.
///
/// Output:
/// - TypeScript union text, or `unknown` when no body is available.
///
/// Transformation:
/// - Applies the existing text mapper to each variant and joins multiple
///   variants as a TypeScript union.
fn type_body_text_to_typescript(body: &[String]) -> String {
    if body.is_empty() {
        "unknown".to_string()
    } else {
        body.iter()
            .map(|item| typer_type_to_typescript(item))
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Renders a TypeScript string literal.
///
/// Inputs:
/// - `value`: unescaped atom/string payload.
///
/// Output:
/// - Double-quoted TypeScript string literal text.
///
/// Transformation:
/// - Escapes backslashes, double quotes, and common control characters.
fn typescript_string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Maps a Terlan type annotation to a TypeScript type string.
///
/// Inputs:
/// - `input`: type text as produced by the parser (compact or spaced).
///
/// Output:
/// - A TypeScript-friendly type string.
///
/// Transformation:
/// - Performs lightweight structural normalization for builtins and handles
///   `Result[... , ...]` generics recursively.
pub(crate) fn typer_type_to_typescript(input: &str) -> String {
    let compact = input
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    let trimmed = compact.as_str();
    if let Some(inner) = trimmed
        .strip_prefix("Result[")
        .and_then(|value| value.strip_suffix(']'))
    {
        let args = split_top_level_args(inner);
        if args.len() == 2 {
            return format!(
                "Result<{}, {}>",
                typer_type_to_typescript(&args[0]),
                typer_type_to_typescript(&args[1])
            );
        }
    }

    match trimmed {
        "Int" | "Float" | "Number" => "number".to_string(),
        "Text" | "Binary" => "string".to_string(),
        "Bool" => "boolean".to_string(),
        "Term" | "Dynamic" => "unknown".to_string(),
        "ok" => "\"ok\"".to_string(),
        atom if atom.chars().next().is_some_and(|ch| ch.is_lowercase()) => {
            format!("\"{}\"", atom)
        }
        other => other.to_string(),
    }
}

/// Splits a generic argument list by commas at top nesting level.
///
/// Inputs:
/// - `input`: a string like `Result[Int, Text], User`.
///
/// Output:
/// - Vector of top-level comma-separated segments.
///
/// Transformation:
/// - Tracks bracket nesting depth and only splits at depth 0 commas.
pub(crate) fn split_top_level_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (index, ch) in input.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                args.push(input[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    args.push(input[start..].trim().to_string());
    args
}
