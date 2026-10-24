use crate::terlan_typeck::{CoreModule, CoreTupleTypeElem, CoreType, CoreVisibility};

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
#[cfg(test)]
pub(crate) fn emit_core_module_to_typescript_declarations(module: &CoreModule) -> String {
    emit_declarations(module, None)
}

pub(crate) fn emit_core_module_to_typescript_declarations_at(
    module: &CoreModule,
    path: &std::path::Path,
) -> String {
    emit_declarations(module, Some(path))
}

fn emit_declarations(module: &CoreModule, path: Option<&std::path::Path>) -> String {
    let mut module = module.clone();
    let mut out = qualify_typescript_imports(&mut module, path);
    let module = &module;
    let mut declared_alias_groups = std::collections::HashSet::new();
    for type_decl in &module.types {
        if !matches!(type_decl.visibility, CoreVisibility::Public) {
            continue;
        }
        let mut params = typescript_generic_params(&type_decl.params);
        if type_decl.name == "Result" && type_decl.params.as_slice() == ["T", "E"] {
            out.push_str(
                "export type Result<T, E> = { tag: \"ok\"; value: T } | { tag: \"error\"; error: E };\n\n",
            );
            continue;
        }
        let mut body = type_decl
            .core_body
            .as_ref()
            .map(core_type_to_typescript)
            .unwrap_or_else(|| type_body_text_to_typescript(&type_decl.body));
        if mentions_private_type(module, &body) {
            if let Some(declared) = module.interface.type_bodies.get(&type_decl.name) {
                body = type_body_text_to_typescript(declared);
            }
            if let Some(fields) = module.interface.struct_fields.get(&type_decl.name) {
                body = format!(
                    "{{ {} }}",
                    fields
                        .iter()
                        .map(|field| {
                            let ty = crate::terlan_typeck::core_type_from_text(&field.annotation);
                            format!(
                                "{}: {}",
                                field.name,
                                core_type_or_text_to_typescript(ty.as_ref(), &field.annotation)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("; ")
                );
            }
            if let Some(declared) = module.interface.type_params.get(&type_decl.name) {
                params = typescript_generic_params(declared);
            }
        }
        out.push_str(&format!(
            "export type {}{} = {};\n\n",
            type_decl.name, params, body
        ));
    }

    let private_alias_groups = module
        .functions
        .iter()
        .filter(|function| function.public)
        .filter(|function| {
            std::iter::once(function.return_type.as_str())
                .chain(function.params.iter().map(|param| param.ty.as_str()))
                .any(|text| mentions_private_type(module, text))
        })
        .map(|function| (function.name.clone(), function.arity))
        .collect::<std::collections::HashSet<_>>();
    for function in &module.functions {
        if !function.public {
            continue;
        }
        let key = (function.name.clone(), function.arity);
        if private_alias_groups.contains(&key) {
            if let Some(signatures) = module.interface.function_overloads.get(&key) {
                if declared_alias_groups.insert(key) {
                    for signature in signatures.iter().filter(|signature| signature.public) {
                        let params = signature
                            .params
                            .iter()
                            .map(|param| {
                                let ty =
                                    crate::terlan_typeck::core_type_from_text(&param.annotation);
                                format!(
                                    "{}: {}",
                                    param.name,
                                    core_type_or_text_to_typescript(ty.as_ref(), &param.annotation)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        let result =
                            crate::terlan_typeck::core_type_from_text(&signature.return_type);
                        out.push_str(&format!(
                            "export function {}{}({}): {};\n",
                            signature.name,
                            typescript_generic_params(&signature.generic_params),
                            params,
                            core_type_or_text_to_typescript(
                                result.as_ref(),
                                &signature.return_type
                            )
                        ));
                    }
                }
                continue;
            }
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
            "export function {}{}({}): {};\n",
            function.name,
            typescript_generic_params(&function.generic_params),
            params,
            core_type_or_text_to_typescript(
                function.core_return_type.as_ref(),
                &function.return_type
            )
        ));
    }
    out
}

fn qualify_typescript_imports(module: &mut CoreModule, path: Option<&std::path::Path>) -> String {
    use std::collections::BTreeMap;
    fn rewrite(
        ty: &mut CoreType,
        current: &str,
        option_names: &std::collections::HashSet<String>,
        imports: &mut BTreeMap<String, String>,
    ) {
        // Native options cross the JS boundary as their tagged runtime values.
        if let CoreType::Apply { constructor, args } = ty {
            if option_names.contains(constructor) && args.len() == 1 {
                let value = args.remove(0);
                let some = CoreType::Tuple(vec![
                    CoreTupleTypeElem::Type(CoreType::AtomLiteral("some".into())),
                    CoreTupleTypeElem::Type(value),
                ]);
                *ty = if constructor.ends_with("Some") {
                    some
                } else {
                    CoreType::Union(vec![CoreType::AtomLiteral("none".into()), some])
                };
            }
        }
        if let CoreType::Named(name) = ty {
            if option_names.contains(name) && name.ends_with("None") {
                *ty = CoreType::AtomLiteral("none".into());
            }
        }
        match ty {
            CoreType::Named(name) if name == "std.js.Number.JsNumber" => *ty = CoreType::Number,
            CoreType::Named(name) if name == "std.js.String.JsString" => *ty = CoreType::String,
            CoreType::Named(name) if name == "std.core.Unit.Unit" => *name = "Unit".into(),
            CoreType::Named(name) if name.contains('.') => {
                let (provider, local) = name.rsplit_once('.').expect("qualified type");
                if provider == current {
                    *name = local.to_string();
                } else {
                    let next = imports.len();
                    let alias = imports
                        .entry(name.clone())
                        .or_insert_with(|| format!("$imported{next}"));
                    *name = alias.clone();
                }
            }
            CoreType::Apply { constructor, args } => {
                let mut name = CoreType::Named(constructor.clone());
                rewrite(&mut name, current, option_names, imports);
                if let CoreType::Named(name) = name {
                    *constructor = name;
                }
                for argument in args {
                    rewrite(argument, current, option_names, imports);
                }
            }
            CoreType::Union(args) => {
                for argument in args {
                    rewrite(argument, current, option_names, imports);
                }
            }
            CoreType::List(item) => rewrite(item, current, option_names, imports),
            CoreType::Tuple(items) => {
                for item in items {
                    let (CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. }) = item;
                    rewrite(ty, current, option_names, imports);
                }
            }
            CoreType::Struct { fields, .. } => {
                for field in fields {
                    rewrite(&mut field.ty, current, option_names, imports);
                }
            }
            CoreType::Arrow {
                params,
                return_type,
            } => {
                for parameter in params {
                    rewrite(parameter, current, option_names, imports);
                }
                rewrite(return_type, current, option_names, imports);
            }
            _ => {}
        }
    }
    let mut option_names = ["Option", "Some", "None"]
        .into_iter()
        .map(|name| format!("std.core.Option.{name}"))
        .collect::<std::collections::HashSet<_>>();
    if module
        .imports
        .iter()
        .any(|import| import.module == "std.core.Option")
    {
        for name in ["Option", "Some", "None"] {
            if !module
                .types
                .iter()
                .any(|declaration| declaration.name == name)
            {
                option_names.insert(name.into());
            }
        }
    }
    let mut imports = BTreeMap::new();
    for declaration in module
        .types
        .iter_mut()
        .filter(|decl| matches!(decl.visibility, CoreVisibility::Public))
    {
        if let Some(ty) = &mut declaration.core_body {
            rewrite(ty, &module.module, &option_names, &mut imports);
        }
    }
    for function in module
        .functions
        .iter_mut()
        .filter(|function| function.public)
    {
        for parameter in &mut function.params {
            if let Some(ty) = &mut parameter.core_ty {
                rewrite(ty, &module.module, &option_names, &mut imports);
            }
        }
        if let Some(ty) = &mut function.core_return_type {
            rewrite(ty, &module.module, &option_names, &mut imports);
        }
    }
    let directory = module.module.split('.').collect::<Vec<_>>();
    let directory = &directory[..directory.len().saturating_sub(1)];
    let mut output = String::new();
    for (name, alias) in imports {
        let (provider, ty) = name.rsplit_once('.').expect("qualified import");
        if let Some((source, declaration)) = path.and_then(|path| {
            let declaration = crate::support::js_binding_registry::typescript_type(provider)?;
            let source = relative_typescript_source(path, &declaration.source)?;
            Some((source, declaration))
        }) {
            output.push_str(&format!(
                "import type {{ {} as {alias} }} from {};\n",
                declaration.name,
                typescript_string_literal(&source)
            ));
            continue;
        }
        let target = provider.split('.').collect::<Vec<_>>();
        let common = directory
            .iter()
            .zip(&target)
            .take_while(|(a, b)| a == b)
            .count();
        let mut path = "../".repeat(directory.len() - common);
        if path.is_empty() {
            path.push_str("./");
        }
        let mut target = target[common..]
            .iter()
            .map(|part| crate::terlan_hir::source_name_to_terlan_identifier(part))
            .collect::<Vec<_>>();
        if let Some(last) = target.last_mut() {
            *last = provider
                .rsplit('.')
                .next()
                .expect("module name")
                .to_string();
        }
        path.push_str(&target.join("/"));
        path.push_str(".js");
        output.push_str(&format!(
            "import type {{ {ty} as {alias} }} from {};\n",
            typescript_string_literal(&path)
        ));
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

/// Keeps generated SDK imports relative to their original declaration source.
fn relative_typescript_source(
    output: &std::path::Path,
    source: &std::path::Path,
) -> Option<String> {
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(output)
    };
    let directory = output.parent()?.components().collect::<Vec<_>>();
    let source = source.components().collect::<Vec<_>>();
    let common = directory
        .iter()
        .zip(&source)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = "../".repeat(directory.len() - common);
    if relative.is_empty() {
        relative.push_str("./");
    }
    relative.push_str(
        &source[common..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    );
    Some(format!("{}.js", relative.strip_suffix(".d.ts")?))
}

fn mentions_private_type(module: &CoreModule, text: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '.')
        .any(|name| module.interface.private_types.contains(name))
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
    crate::terlan_syntax::quoted_string_literal(value)
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
        "Text" | "Binary" | "String" => "string".to_string(),
        "Unit" => "void".to_string(),
        "Bool" => "boolean".to_string(),
        "Term" | "Dynamic" => "unknown".to_string(),
        "ok" => "\"ok\"".to_string(),
        atom if atom.chars().next().is_some_and(|ch| ch.is_lowercase()) => {
            typescript_string_literal(atom)
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
