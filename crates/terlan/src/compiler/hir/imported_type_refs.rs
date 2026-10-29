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
        if items.is_empty() {
            if let Some(name) = module_name.rsplit('.').next().filter(|name| {
                name.starts_with(char::is_uppercase)
                    && (include_default_imports || !is_prelude_type(name))
            }) {
                refs.insert(name.to_string(), module_name.clone());
            }
        }
        for item in items {
            if !item.name.starts_with(char::is_uppercase) {
                continue;
            }
            let selected_from_provider = *is_selected
                && module_name
                    .rsplit('.')
                    .next()
                    .is_some_and(|name| name.starts_with(char::is_uppercase));
            if !include_default_imports && !selected_from_provider && is_prelude_type(&item.name) {
                continue;
            }
            let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
            if let Some(target) = imported_type_ref_target(module_name, &item.name, *is_selected) {
                refs.insert(local_name, target);
            }
        }
    }
    if let Some(default_name) = module.module_name.rsplit('.').next() {
        let declares_default_type = module.declarations.iter().any(|declaration| {
            matches!(
                &declaration.payload,
                SyntaxDeclarationPayload::Type { name, .. }
                    | SyntaxDeclarationPayload::Struct { name, .. }
                    if name == default_name
            )
        });
        if declares_default_type {
            refs.insert(
                module.module_name.clone(),
                format!("{}.{}", module.module_name, default_name),
            );
        }
    }
    refs
}

/// Collects selected type names and module-prefix aliases used by callable
/// annotations without treating a default module import as a type alias.
pub(crate) fn collect_syntax_callable_type_refs(
    module: &SyntaxModuleOutput,
) -> HashMap<String, String> {
    let mut refs = collect_syntax_type_refs(module, false);
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
        if items.is_empty() {
            if let Some(local_name) = module_name.rsplit('.').next() {
                refs.insert(format!("{local_name}."), module_name.clone());
            }
            continue;
        }
        if !*is_selected {
            for item in items {
                let local_name = item.as_alias.as_deref().unwrap_or(&item.name);
                refs.insert(
                    format!("{local_name}."),
                    format!("{module_name}.{}", item.name),
                );
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
    let mut qualified_heads = imported_type_refs
        .iter()
        .filter(|(visible, target)| {
            visible.contains('.') && !visible.ends_with('.') && *visible != *target
        })
        .collect::<Vec<_>>();
    qualified_heads.sort_by(|(left, _), (right, _)| {
        right.len().cmp(&left.len()).then_with(|| left.cmp(right))
    });

    while index < text.len() {
        let ch = text[index..].chars().next().expect("valid type text index");
        if ch == '"' || ch == '\'' {
            qualified.push(ch);
            index += ch.len_utf8();
            while index < text.len() {
                let next = text[index..]
                    .chars()
                    .next()
                    .expect("valid quoted type text");
                qualified.push(next);
                index += next.len_utf8();
                if next == '\\' && index < text.len() {
                    let escaped = text[index..]
                        .chars()
                        .next()
                        .expect("valid escaped type text");
                    qualified.push(escaped);
                    index += escaped.len_utf8();
                } else if next == ch {
                    break;
                }
            }
            continue;
        }
        if ch == '_' || ch.is_ascii_alphabetic() {
            if let Some((visible, target)) = qualified_heads.iter().find(|(visible, _)| {
                let remaining = &text[index..];
                if !remaining.starts_with(visible.as_str()) {
                    return false;
                }
                remaining[visible.len()..]
                    .chars()
                    .next()
                    .is_none_or(|next| next != '.' && next != '_' && !next.is_ascii_alphanumeric())
            }) {
                qualified.push_str(target);
                index += visible.len();
                continue;
            }
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
            let preceded_by_dot = text[..start].ends_with('.');
            let followed_by_dot = text[index..].starts_with('.');
            if !preceded_by_dot && followed_by_dot {
                let member_start = index + 1;
                let mut member_end = member_start;
                while member_end < text.len() {
                    let next = text[member_end..]
                        .chars()
                        .next()
                        .expect("valid qualified type text index");
                    if next == '_' || next.is_ascii_alphanumeric() {
                        member_end += next.len_utf8();
                    } else {
                        break;
                    }
                }
                let visible_name = &text[start..member_end];
                if !text[member_end..].starts_with('.') {
                    if let Some(target) = imported_type_refs.get(visible_name) {
                        qualified.push_str(target);
                        index = member_end;
                        continue;
                    }
                }
                if let Some(target) = imported_type_refs.get(&format!("{name}.")) {
                    qualified.push_str(target);
                    continue;
                }
                if let Some(target) = imported_type_refs.get(name) {
                    let duplicated_default_type = format!(".{name}.{name}");
                    let module_target = target
                        .strip_suffix(&duplicated_default_type)
                        .map(|prefix| format!("{prefix}.{name}"))
                        .unwrap_or_else(|| target.clone());
                    qualified.push_str(&module_target);
                    continue;
                }
            } else if !preceded_by_dot {
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
/// - Qualifies selected types and namespace-group default types.
/// - Uses the provider module path for its same-named primary type.
/// - Preserves prelude type heads when importing their helper modules.
fn imported_type_ref_target(
    module_name: &str,
    item_name: &str,
    is_selected: bool,
) -> Option<String> {
    let provider_is_module = is_selected
        && module_name
            .rsplit('.')
            .next()?
            .starts_with(char::is_uppercase);
    Some(if provider_is_module {
        super::naming::public_type_path(module_name, item_name)
    } else {
        format!("{module_name}.{item_name}")
    })
}

/// Prelude heads retain their language-owned meaning when importing helper modules.
fn is_prelude_type(name: &str) -> bool {
    matches!(
        name,
        "Bool"
            | "Int"
            | "Float"
            | "String"
            | "Text"
            | "Atom"
            | "Dynamic"
            | "Never"
            | "Term"
            | "Unit"
            | "List"
            | "Map"
            | "Set"
            | "Option"
            | "Result"
    )
}

#[cfg(test)]
#[path = "imported_type_refs_test.rs"]
mod tests;
