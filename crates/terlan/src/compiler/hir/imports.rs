use std::collections::{BTreeSet, HashMap};

use crate::terlan_syntax::{span::Span, SyntaxImportItem};

use crate::terlan_hir::{Diagnostic, ImportedItem, ModuleInterface, TypeVisibility};

/// Resolves one syntax-output import declaration.
///
/// Inputs: module name, selected items, type-import flag, visible interfaces,
/// mutable import tables, and diagnostics sink. Output: import tables and
/// diagnostics are updated. Transformation: validates public type/trait/default
/// imports and records local aliases.
pub(crate) fn resolve_syntax_import(
    module_name: &str,
    items: &[SyntaxImportItem],
    is_type: bool,
    interfaces: &HashMap<String, ModuleInterface>,
    imported_types: &mut HashMap<String, ImportedItem>,
    imported_traits: &mut HashMap<String, ImportedItem>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let iface = interfaces.get(module_name);
    for item in items {
        if item.name == "*" {
            match iface {
                Some(iface) => resolve_wildcard_import(
                    module_name,
                    item,
                    is_type,
                    iface,
                    imported_types,
                    imported_traits,
                    diagnostics,
                ),
                None if is_type => diagnostics.push(Diagnostic {
                    span: item.span.into(),
                    message: format!("cannot find interface for module {module_name}"),
                }),
                None => {}
            }
            continue;
        }

        if let Some(default_import) =
            resolve_default_type_import(module_name, item, interfaces, imported_types)
        {
            if let Err(diagnostic) =
                insert_imported_type(default_import, item.span.into(), imported_types)
            {
                diagnostics.push(diagnostic);
            }
            continue;
        }

        let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());

        match iface {
            Some(iface) => {
                let has_public_type = iface.public_types.contains(&item.name)
                    || iface.opaque_types.contains(&item.name);
                let has_public_constructor = iface
                    .constructors
                    .get(&item.name)
                    .is_some_and(|signatures| signatures.iter().any(|signature| signature.public));
                let has_public_trait = iface.traits.contains_key(&item.name);

                if iface.private_types.contains(&item.name) {
                    diagnostics.push(Diagnostic {
                        span: item.span.into(),
                        message: format!("type {module_name}.{} is private", item.name),
                    });
                    continue;
                }

                if is_type {
                    if !has_public_type && !has_public_trait {
                        diagnostics.push(Diagnostic {
                            span: item.span.into(),
                            message: format!(
                                "cannot find type {module_name}.{} in interface",
                                item.name
                            ),
                        });
                        continue;
                    }
                } else if !has_public_type && !has_public_constructor && !has_public_trait {
                    continue;
                }

                if has_public_type {
                    let imported = ImportedItem {
                        local_name: local_name.clone(),
                        source_module: module_name.to_string(),
                        source_name: item.name.clone(),
                        visibility: TypeVisibility::Public,
                        span: item.span.into(),
                    };
                    if let Err(diagnostic) =
                        insert_imported_type(imported, item.span.into(), imported_types)
                    {
                        diagnostics.push(diagnostic);
                        continue;
                    }
                }

                if has_public_trait {
                    if let Some(existing) = imported_traits.get(&local_name) {
                        if existing.source_module == module_name
                            && existing.source_name == item.name
                        {
                            continue;
                        }
                        diagnostics.push(Diagnostic {
                            span: item.span.into(),
                            message: format!(
                                "duplicate imported trait name '{}', already imported from {}",
                                local_name, existing.source_module
                            ),
                        });
                        continue;
                    }
                    imported_traits.insert(
                        local_name.clone(),
                        ImportedItem {
                            local_name: local_name.clone(),
                            source_module: module_name.to_string(),
                            source_name: item.name.clone(),
                            visibility: TypeVisibility::Public,
                            span: item.span.into(),
                        },
                    );
                }

                if !is_type && has_public_constructor && !has_public_type && !has_public_trait {
                    let imported = ImportedItem {
                        local_name,
                        source_module: module_name.to_string(),
                        source_name: item.name.clone(),
                        visibility: TypeVisibility::Public,
                        span: item.span.into(),
                    };
                    if let Err(diagnostic) =
                        insert_imported_type(imported, item.span.into(), imported_types)
                    {
                        diagnostics.push(diagnostic);
                        continue;
                    }
                }
            }
            None => {
                if is_type {
                    diagnostics.push(Diagnostic {
                        span: item.span.into(),
                        message: format!("cannot find interface for module {module_name}"),
                    });
                }
            }
        }
    }
}

/// Expands one wildcard import through a loaded module interface.
///
/// Inputs:
/// - `module_name`: provider module named by the import declaration.
/// - `item`: wildcard import item used only for source span diagnostics.
/// - `is_type`: whether the import was written as a type-only import.
/// - `iface`: provider interface to expand.
/// - Mutable import tables and diagnostics sink.
///
/// Output:
/// - Import tables contain every public type, opaque type, trait, and
///   constructor-only symbol selected by the wildcard.
///
/// Transformation:
/// - Reuses ordinary selected-import insertion rules after expanding `*` into
///   stable sorted names. Type-only wildcard imports skip constructors.
fn resolve_wildcard_import(
    module_name: &str,
    item: &SyntaxImportItem,
    is_type: bool,
    iface: &ModuleInterface,
    imported_types: &mut HashMap<String, ImportedItem>,
    imported_traits: &mut HashMap<String, ImportedItem>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut type_names = BTreeSet::new();
    type_names.extend(iface.public_types.iter().cloned());
    type_names.extend(iface.opaque_types.iter().cloned());

    for name in type_names {
        let imported = ImportedItem {
            local_name: name.clone(),
            source_module: module_name.to_string(),
            source_name: name,
            visibility: TypeVisibility::Public,
            span: item.span.into(),
        };
        if let Err(diagnostic) = insert_imported_type(imported, item.span.into(), imported_types) {
            diagnostics.push(diagnostic);
        }
    }

    let trait_names = iface.traits.keys().cloned().collect::<BTreeSet<_>>();
    for name in trait_names {
        if let Some(existing) = imported_traits.get(&name) {
            if existing.source_module == module_name && existing.source_name == name {
                continue;
            }
            diagnostics.push(Diagnostic {
                span: item.span.into(),
                message: format!(
                    "duplicate imported trait name '{}', already imported from {}",
                    name, existing.source_module
                ),
            });
            continue;
        }
        imported_traits.insert(
            name.clone(),
            ImportedItem {
                local_name: name.clone(),
                source_module: module_name.to_string(),
                source_name: name,
                visibility: TypeVisibility::Public,
                span: item.span.into(),
            },
        );
    }

    if is_type {
        return;
    }

    let constructor_names = iface.constructors.keys().cloned().collect::<BTreeSet<_>>();
    for name in constructor_names {
        if type_names_contains(iface, &name) || iface.traits.contains_key(&name) {
            continue;
        }
        let has_public_constructor = iface
            .constructors
            .get(&name)
            .is_some_and(|signatures| signatures.iter().any(|signature| signature.public));
        if !has_public_constructor {
            continue;
        }
        let imported = ImportedItem {
            local_name: name.clone(),
            source_module: module_name.to_string(),
            source_name: name,
            visibility: TypeVisibility::Public,
            span: item.span.into(),
        };
        if let Err(diagnostic) = insert_imported_type(imported, item.span.into(), imported_types) {
            diagnostics.push(diagnostic);
        }
    }
}

/// Returns whether a provider interface exports a type-like symbol.
fn type_names_contains(iface: &ModuleInterface, name: &str) -> bool {
    iface.public_types.contains(name) || iface.opaque_types.contains(name)
}

/// Resolves a module-default type import.
///
/// Inputs:
/// - `module_name`: parser module prefix, such as `std.core`.
/// - `item`: parser import item, such as `Task`.
/// - `interfaces`: loaded provider interfaces keyed by full module name.
/// - `imported_types`: already imported type names for duplicate checks.
///
/// Output:
/// - `Some(ImportedItem)` when `module_name.item.name` is an interface module
///   that publicly exports a type with the same final segment.
/// - `None` when the import should use the ordinary selected-import path.
///
/// Transformation:
/// - Reinterprets `import std.core.Task.` and `import type std.core.Task.` as
///   the default type export `std.core.Task.Task` only when the module and type
///   names exactly match. Aliases such as
///   `import std.core.Task as AsyncTask.` preserve the requested local alias
///   while still pointing at the default exported type.
fn resolve_default_type_import(
    module_name: &str,
    item: &SyntaxImportItem,
    interfaces: &HashMap<String, ModuleInterface>,
    imported_types: &HashMap<String, ImportedItem>,
) -> Option<ImportedItem> {
    let default_module = default_type_import_module_name(module_name, &item.name)?;
    let iface = interfaces.get(&default_module)?;
    if iface.private_types.contains(&item.name) {
        return None;
    }
    if !iface.public_types.contains(&item.name) && !iface.opaque_types.contains(&item.name) {
        return None;
    }

    let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
    if imported_types.get(&local_name).is_some_and(|existing| {
        existing.source_module == default_module && existing.source_name == item.name
    }) {
        return Some(ImportedItem {
            local_name,
            source_module: default_module,
            source_name: item.name.clone(),
            visibility: TypeVisibility::Public,
            span: item.span.into(),
        });
    }

    Some(ImportedItem {
        local_name,
        source_module: default_module,
        source_name: item.name.clone(),
        visibility: TypeVisibility::Public,
        span: item.span.into(),
    })
}

/// Builds the candidate module path for a default type import.
///
/// Inputs:
/// - `module_name`: parser module prefix.
/// - `item_name`: parser import item.
///
/// Output:
/// - Full module path candidate, or `None` when the parser did not produce a
///   module prefix.
///
/// Transformation:
/// - Joins the prefix and item with a dot, preserving source spelling so the
///   resolver can test whether that full path is an actual interface module.
fn default_type_import_module_name(module_name: &str, item_name: &str) -> Option<String> {
    (!module_name.is_empty()).then(|| format!("{module_name}.{item_name}"))
}

/// Inserts a resolved imported type while enforcing duplicate import rules.
///
/// Inputs:
/// - `imported`: resolved imported type metadata.
/// - `span`: source span used for diagnostics.
/// - `imported_types`: mutable local import table.
///
/// Output:
/// - `Ok(())` when the type was inserted or already imported from the same
///   provider.
/// - `Err(Diagnostic)` when the local name is already bound to a different
///   provider type.
///
/// Transformation:
/// - Centralizes duplicate handling for selected type imports and default type
///   exports so both forms produce identical resolver behavior.
fn insert_imported_type(
    imported: ImportedItem,
    span: Span,
    imported_types: &mut HashMap<String, ImportedItem>,
) -> Result<(), Diagnostic> {
    if let Some(existing) = imported_types.get(&imported.local_name) {
        if existing.source_module == imported.source_module
            && existing.source_name == imported.source_name
        {
            return Ok(());
        }
        return Err(Diagnostic {
            span,
            message: format!(
                "duplicate imported type name '{}', already imported from {}",
                imported.local_name, existing.source_module
            ),
        });
    }

    imported_types.insert(imported.local_name.clone(), imported);
    Ok(())
}
