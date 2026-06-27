use super::*;
use crate::terlan_syntax::SyntaxImportItem;

/// Collects public imported trait conformances selected by this module.
///
/// Inputs:
/// - `module`: syntax-output module containing import declarations.
/// - `interfaces`: provider interfaces keyed by source module name.
///
/// Output:
/// - Map from local imported trait name to consumer-qualified implementation
///   type keys and provider-local wrapper type keys.
///
/// Transformation:
/// - Matches selected imports such as `import provider.{Named}` against public
///   provider conformance facts, rewrites the trait key to the local import
///   name or alias, stores qualified keys for call-site matching, and stores
///   provider-local type keys for remote wrapper symbol generation.
pub(super) fn collect_imported_trait_conformances(
    module: &SyntaxModuleOutput,
    interfaces: &BTreeMap<String, ModuleInterface>,
) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut conformances: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind: SyntaxImportKind::Module,
            module_name,
            items,
            is_type: false,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let Some(interface) = interfaces.get(module_name) else {
            continue;
        };

        for item in items {
            if !interface.traits.contains_key(&item.name) {
                continue;
            }
            let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
            for conformance in &interface.trait_conformances {
                if !conformance.public {
                    continue;
                }
                let Some(trait_name) = syntax_type_head_name(&conformance.trait_ref) else {
                    continue;
                };
                if trait_name != item.name {
                    continue;
                }
                conformances.entry(local_name.clone()).or_default().insert(
                    qualify_imported_type_text(
                        &normalize_trait_type_text(&conformance.for_type),
                        &collect_interface_type_refs(interface),
                    ),
                    normalize_trait_type_text(&conformance.for_type),
                );
            }
        }
    }

    conformances
}

/// Collects selected imported type references from provider interfaces.
///
/// Inputs:
/// - `module`: syntax-output module containing selected imports.
/// - `interfaces`: loaded provider interfaces keyed by module name.
///
/// Output:
/// - Map from local imported type name or alias to fully qualified Terlan type
///   text such as `people.Provider.ExternalUser`.
///
/// Transformation:
/// - Reads selected module imports, keeps public type and opaque type items,
///   applies local aliases, and records the provider-qualified type identity
///   needed by BEAM spec lowering.
pub(super) fn collect_imported_type_refs(
    module: &SyntaxModuleOutput,
    interfaces: &BTreeMap<String, ModuleInterface>,
) -> BTreeMap<String, String> {
    let mut refs = BTreeMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind: SyntaxImportKind::Module,
            module_name,
            items,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let mut default_imported_items = BTreeSet::new();
        for item in items {
            if let Some((local_name, qualified)) =
                imported_default_type_ref(module_name, item, interfaces)
            {
                refs.insert(local_name, qualified);
                default_imported_items.insert(item.name.clone());
            }
        }

        if let Some(interface) = interfaces.get(module_name) {
            for item in items {
                if default_imported_items.contains(&item.name) {
                    continue;
                }
                if !interface.public_types.contains(&item.name)
                    && !interface.opaque_types.contains(&item.name)
                {
                    continue;
                }
                let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
                refs.insert(local_name, format!("{}.{}", module_name, item.name));
            }
            continue;
        }
    }

    refs
}

/// Resolves one imported module-default type reference for backend specs.
///
/// Inputs:
/// - `module_name`: parser module prefix, such as `std.core`.
/// - `item`: parser import item, such as `Task`.
/// - `interfaces`: loaded provider interfaces.
///
/// Output:
/// - Local type head and provider-qualified type text when
///   `module_name.item.name` is an interface module that exports public type
///   `item.name`.
/// - `None` when the import should not be treated as a default type export.
///
/// Transformation:
/// - Mirrors HIR default type import resolution for the transitional syntax
///   bridge so annotations like `Task[Int]` lower to
///   `std_core_task:task(integer())` after `import std.core.Task.`.
fn imported_default_type_ref(
    module_name: &str,
    item: &SyntaxImportItem,
    interfaces: &BTreeMap<String, ModuleInterface>,
) -> Option<(String, String)> {
    let default_module = default_type_import_module_name(module_name, &item.name)?;
    let interface = interfaces.get(&default_module)?;
    if !interface.public_types.contains(&item.name) && !interface.opaque_types.contains(&item.name)
    {
        return None;
    }
    let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
    Some((local_name, format!("{}.{}", default_module, item.name)))
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
/// - Joins the parser prefix and final imported item so the syntax bridge can
///   test the same default-export module shape as HIR.
fn default_type_import_module_name(module_name: &str, item_name: &str) -> Option<String> {
    (!module_name.is_empty()).then(|| format!("{module_name}.{item_name}"))
}

/// Collects provider-local type names for qualification.
///
/// Inputs:
/// - `interface`: provider module interface.
///
/// Output:
/// - Map from public provider type head to provider-qualified type text.
///
/// Transformation:
/// - Converts interface public and opaque type sets into the same local-to-full
///   type map used for selected imports.
fn collect_interface_type_refs(interface: &ModuleInterface) -> BTreeMap<String, String> {
    interface
        .public_types
        .iter()
        .chain(interface.opaque_types.iter())
        .map(|name| (name.clone(), format!("{}.{}", interface.module, name)))
        .collect()
}

/// Lowers a syntax annotation to a BEAM spec with import-aware type names.
///
/// Inputs:
/// - `text`: source annotation text.
/// - `ctx`: syntax lowering context containing selected type imports.
///
/// Output:
/// - Erlang type-spec model for the annotation.
///
/// Transformation:
/// - Qualifies selected imported type heads before delegating to the ordinary
///   BEAM type-spec lowering helper.
pub(super) fn lower_syntax_type_to_spec(text: &str, ctx: &SyntaxLowerCtx) -> ErlType {
    lower_type_to_spec(&qualify_imported_type_text(text, &ctx.imported_type_refs))
}

/// Qualifies imported type heads inside one annotation text.
///
/// Inputs:
/// - `text`: Terlan type annotation text.
/// - `imported_type_refs`: local type names mapped to provider-qualified names.
///
/// Output:
/// - Annotation text with imported heads rewritten when needed.
///
/// Transformation:
/// - Handles exact imported type names and generic type applications
///   recursively. Other type forms are returned unchanged so this helper stays
///   conservative until the full type AST owns backend spec rendering.
pub(super) fn qualify_imported_type_text(
    text: &str,
    imported_type_refs: &BTreeMap<String, String>,
) -> String {
    let normalized = normalize_trait_type_text(text);
    if let Some(qualified) = imported_type_refs.get(&normalized) {
        return qualified.clone();
    }

    let compact = compact_type_application(&compact_spaces(&normalized));
    let Some((head, args)) = parse_named_type_args(&compact) else {
        return normalized;
    };
    let qualified_head = imported_type_refs
        .get(head)
        .cloned()
        .unwrap_or_else(|| head.to_string());
    let qualified_args = args
        .iter()
        .map(|arg| qualify_imported_type_text(arg, imported_type_refs))
        .collect::<Vec<_>>();
    format!("{}[{}]", qualified_head, qualified_args.join(", "))
}

/// Extracts the source trait/type head from a type expression string.
///
/// Inputs:
/// - `text`: syntax-output type text such as `Identity[ExternalUser]` or
///   `std.core.Show[User]`.
///
/// Output:
/// - The non-empty type head before type arguments.
///
/// Transformation:
/// - Trims the type expression and keeps the prefix before the first `[` so
///   wrapper maps use source-visible trait names rather than full type
///   application text.
pub(super) fn syntax_type_head_name(text: &str) -> Option<String> {
    let head = text
        .split_once('[')
        .map(|(head, _)| head)
        .unwrap_or(text)
        .trim();
    (!head.is_empty()).then(|| head.to_string())
}

/// Splits an explicit trait-call target into trait alias and type argument.
///
/// Inputs:
/// - `remote`: remote qualifier from a syntax-output call, such as `Parse[Int]`
///   or `Show`.
///
/// Output:
/// - Tuple containing the trait alias and optional normalized first type
///   argument.
///
/// Transformation:
/// - Parses the closed `Trait[Type]` shape used by explicit target calls while
///   leaving ordinary remote qualifiers untouched. Multi-argument trait targets
///   are preserved in the returned type text only when they can be parsed by
///   the shared type-application helper.
pub(super) fn split_explicit_trait_call_target(remote: &str) -> (String, Option<String>) {
    let compact = compact_type_application(&compact_spaces(remote));
    let Some((head, args)) = parse_named_type_args(&compact) else {
        return (remote.to_string(), None);
    };
    let Some(first) = args.first() else {
        return (remote.to_string(), None);
    };
    (head.to_string(), Some(normalize_trait_type_text(first)))
}
