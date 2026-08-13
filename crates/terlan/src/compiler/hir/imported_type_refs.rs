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
    collect_syntax_type_refs(module, true)
}

/// Collects only direct selected type references for callable signatures.
pub(super) fn collect_syntax_selected_type_refs(
    module: &SyntaxModuleOutput,
) -> HashMap<String, String> {
    collect_syntax_type_refs(module, false)
}

fn collect_syntax_type_refs(
    module: &SyntaxModuleOutput,
    include_default_imports: bool,
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
            if !include_default_imports && !*is_selected {
                continue;
            }
            let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
            if let Some(target) = imported_type_ref_target(module_name, &item.name, *is_selected) {
                refs.insert(local_name, target);
            }
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
pub(crate) fn qualify_syntax_type_text(
    text: &str,
    imported_type_refs: &HashMap<String, String>,
) -> String {
    let mut qualified = String::with_capacity(text.len());
    let mut index = 0usize;

    while index < text.len() {
        let ch = text[index..].chars().next().expect("valid type text index");
        if ch == '_' || ch.is_ascii_alphabetic() {
            let start = index;
            index += ch.len_utf8();
            while index < text.len() {
                let next = text[index..].chars().next().expect("valid type text index");
                if next == '_' || next.is_ascii_alphanumeric() {
                    index += next.len_utf8();
                } else {
                    break;
                }
            }
            let name = &text[start..index];
            let already_qualified = text[..start].ends_with('.') || text[index..].starts_with('.');
            if !already_qualified {
                if let Some(target) = imported_type_refs.get(name) {
                    qualified.push_str(target);
                    continue;
                }
            }
            qualified.push_str(name);
            continue;
        }
        qualified.push(ch);
        index += ch.len_utf8();
    }

    qualified
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
fn imported_type_ref_target(
    module_name: &str,
    item_name: &str,
    is_selected: bool,
) -> Option<String> {
    if is_selected {
        let provider_name = module_name.rsplit('.').next()?;
        let provider_is_module = provider_name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase());
        (provider_is_module && provider_name != item_name)
            .then(|| format!("{module_name}.{item_name}"))
    } else {
        Some(format!("{module_name}.{item_name}.{item_name}"))
    }
}
