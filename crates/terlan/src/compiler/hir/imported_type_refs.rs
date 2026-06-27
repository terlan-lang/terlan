use std::collections::HashMap;

use crate::terlan_syntax::{SyntaxDeclarationPayload, SyntaxImportKind, SyntaxModuleOutput};

/// Collects source-visible imported type references from import declarations.
///
/// Inputs:
/// - `module`: syntax-output module whose import declarations are scanned.
///
/// Output:
/// - Map from local imported type name or alias to fully qualified type name.
///
/// Transformation:
/// - Uses the import declaration shape directly so interface extraction can
///   qualify conformance facts without requiring a full resolver pass or a
///   serialized interface schema change.
pub(super) fn collect_syntax_imported_type_refs(
    module: &SyntaxModuleOutput,
) -> HashMap<String, String> {
    let mut refs = HashMap::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind: SyntaxImportKind::Module,
            module_name,
            items,
            is_selected,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        for item in items {
            if item.name == "*" {
                continue;
            }
            let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
            refs.insert(
                local_name,
                imported_type_ref_target(module_name, &item.name, *is_selected),
            );
        }
    }
    refs
}

/// Qualifies imported type heads inside one conformance type expression.
///
/// Inputs:
/// - `text`: normalized type text from a trait conformance.
/// - `imported_type_refs`: local imported names mapped to qualified names.
///
/// Output:
/// - Type text with imported heads rewritten.
///
/// Transformation:
/// - Rewrites exact imported heads and recursively rewrites top-level generic
///   arguments. Generic variables and higher-kinded variables are preserved.
pub(super) fn qualify_syntax_type_text(
    text: &str,
    imported_type_refs: &HashMap<String, String>,
) -> String {
    let trimmed = text.trim();
    if let Some(qualified) = imported_type_refs.get(trimmed) {
        return qualified.clone();
    }
    let Some((head, args_text)) = trimmed.split_once('[') else {
        return trimmed.to_string();
    };
    let Some(args_text) = args_text.strip_suffix(']') else {
        return trimmed.to_string();
    };
    let qualified_head = imported_type_refs
        .get(head.trim())
        .cloned()
        .unwrap_or_else(|| head.trim().to_string());
    let args = split_top_level_type_args(args_text)
        .into_iter()
        .map(|arg| qualify_syntax_type_text(arg, imported_type_refs))
        .collect::<Vec<_>>();
    format!("{}[{}]", qualified_head, args.join(", "))
}

/// Builds the fully qualified target for one imported type-like reference.
///
/// Inputs:
/// - `module_name`: syntax-output module prefix from the import declaration.
/// - `item_name`: imported symbol name.
/// - `is_selected`: whether the source used selected import syntax.
///
/// Output:
/// - Fully qualified type reference used in generated interface summaries.
///
/// Transformation:
/// - Preserves selected imports as `module.Item`.
/// - Expands default type imports such as `import std.collections.List.` from
///   parser shape `module_name = "std.collections", item = "List"` into the
///   default exported type `std.collections.List.List`.
fn imported_type_ref_target(module_name: &str, item_name: &str, is_selected: bool) -> String {
    if is_selected {
        format!("{module_name}.{item_name}")
    } else {
        format!("{module_name}.{item_name}.{item_name}")
    }
}

/// Splits generic type arguments at top-level commas.
///
/// Inputs:
/// - `text`: bracket contents from a type application.
///
/// Output:
/// - Borrowed argument slices without surrounding whitespace.
///
/// Transformation:
/// - Tracks nested brackets, braces, and parentheses so commas inside nested
///   type applications or function types do not split the outer list.
fn split_top_level_type_args(text: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (index, ch) in text.char_indices() {
        match ch {
            '[' | '{' | '(' => depth += 1,
            ']' | '}' | ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(text[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        args.push(tail);
    }
    args
}
