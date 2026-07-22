use std::collections::HashMap;

use crate::terlan_syntax::{
    ebnf::EbnfCompileResult, expand_shape_imports, syntax_module_import_identity,
    SyntaxDeclarationPayload, SyntaxImportKind, SyntaxModuleOutput, SyntaxShapeImport,
};

use super::ShapeSignature;

/// Expands shape aliases selected from loaded module interfaces.
///
/// Inputs:
/// - `module`: mutable consumer syntax output.
/// - `interfaces`: loaded dependency interfaces keyed by module identity.
///
/// Output:
/// - Success after every selected or wildcard-imported shape call is expanded.
///
/// Transformation:
/// - Resolves source import spelling to provider interfaces, preserves local
///   aliases, and delegates canonical pattern expansion to the syntax layer.
/// - Ignores ordinary function/type imports whose provider does not export a
///   same-named shape.
pub fn expand_syntax_shape_imports(
    module: &mut SyntaxModuleOutput,
    interfaces: &HashMap<String, super::ModuleInterface>,
) -> EbnfCompileResult<()> {
    let mut imports = Vec::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind: SyntaxImportKind::Module,
            module_name,
            items,
            is_type: false,
            is_selected,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let provider_name = if *is_selected {
            module_name.clone()
        } else {
            syntax_module_import_identity(module_name, items)
        };
        let Some(interface) = interfaces.get(&provider_name) else {
            continue;
        };
        if interface.shapes.is_empty() {
            continue;
        }
        let mut provider_signatures = interface
            .shapes
            .values()
            .map(|shape| shape.signature.clone())
            .collect::<Vec<_>>();
        provider_signatures.sort_unstable();

        for item in items {
            if item.name == "*" {
                let mut names = interface.shapes.keys().cloned().collect::<Vec<_>>();
                names.sort_unstable();
                for name in names {
                    imports.push(SyntaxShapeImport {
                        local_name: name.clone(),
                        source_module: provider_name.clone(),
                        source_name: name,
                        provider_signatures: provider_signatures.clone(),
                    });
                }
            } else if interface.shapes.contains_key(&item.name) {
                imports.push(SyntaxShapeImport {
                    local_name: item.as_alias.clone().unwrap_or_else(|| item.name.clone()),
                    source_module: provider_name.clone(),
                    source_name: item.name.clone(),
                    provider_signatures: provider_signatures.clone(),
                });
            }
        }
    }

    expand_shape_imports(&mut module.declarations, &imports)
}

/// Collects public raw shape signatures from syntax output.
pub(super) fn collect_syntax_shape_signatures(
    module: &SyntaxModuleOutput,
) -> HashMap<String, ShapeSignature> {
    let mut shapes = HashMap::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Raw { raw_kind, text } = &declaration.payload {
            let Some((name, is_public, signature)) = parse_raw_shape_signature(raw_kind, text)
            else {
                continue;
            };
            if is_public {
                shapes.insert(
                    name.clone(),
                    ShapeSignature {
                        name,
                        signature,
                        docs: declaration.docs.clone(),
                    },
                );
            }
        }
    }
    shapes
}

/// Parses declaration-head metadata from one raw shape declaration.
fn parse_raw_shape_signature(raw_kind: &str, text: &str) -> Option<(String, bool, String)> {
    if raw_kind != "shape" {
        return None;
    }

    let trimmed = text.trim();
    let (is_public, after_visibility) =
        if let Some(rest) = trimmed.strip_prefix("pub").and_then(trim_keyword_rest) {
            (true, rest)
        } else {
            (false, trimmed)
        };
    let after_shape = after_visibility
        .strip_prefix("shape")
        .and_then(trim_keyword_rest)?;
    let name = after_shape
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if name.is_empty() {
        return None;
    }

    let signature = trimmed.strip_suffix('.').unwrap_or(trimmed).trim_end();
    Some((name, is_public, format!("{signature}.")))
}

/// Trims required whitespace after a recognized keyword token.
fn trim_keyword_rest(rest: &str) -> Option<&str> {
    let mut chars = rest.chars();
    let first = chars.next()?;
    if !first.is_whitespace() {
        return None;
    }
    Some(chars.as_str().trim_start())
}
