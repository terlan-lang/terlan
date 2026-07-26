use std::collections::{HashMap, HashSet};

use crate::terlan_hir::{ModuleInterface, ResolvedModule};
use crate::terlan_syntax::{
    span::Span, SyntaxDeclarationPayload, SyntaxImportKind, SyntaxModuleOutput,
};

use super::{
    alias_constructor_param_names_from_variants, expand_interface_global_aliases,
    expand_type_aliases, interface_qualified_type_names, interface_type_aliases,
    interface_type_names, normalize_type_param_name, normalize_union,
    parse_structural_implication_bounds, parse_type_expr, qualify_type_names, type_param_variances,
    QualifiedTypeName, Type, TypeAlias, TypeVarId,
};

/// Import maps consumed by expression and declaration type checking.
///
/// Inputs:
/// - Source import declarations from syntax output.
///
/// Output:
/// - Grouped aliases for modules, raw file imports, markdown imports, and
///   selected function imports.
///
/// Transformation:
/// - Normalizes import declarations into lookup tables so later inference
///   code does not need to inspect raw syntax declaration payloads.
#[derive(Debug, Clone, Default)]
pub(super) struct TypeCheckImportMaps {
    pub(super) module_aliases: HashMap<String, String>,
    pub(super) file_imports: HashMap<String, String>,
    pub(super) markdown_imports: HashMap<String, String>,
    pub(super) function_imports: HashMap<String, Vec<ImportedFunctionTarget>>,
}

/// Selected function import target visible under a local call name.
///
/// Inputs:
/// - Produced from source imports such as `import std.io.Console.{println}` or
///   `import module.{source as local}`.
///
/// Output:
/// - Source module/function identity used by call inference.
///
/// Transformation:
/// - Keeps the source function separate from the local alias so selected
///   imports can be typechecked against the provider interface before backend
///   emission rewrites the call target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImportedFunctionTarget {
    pub(super) module: String,
    pub(super) function: String,
    pub(super) span: Span,
}

/// Returns the exact provider modules explicitly imported by one source module.
pub(super) fn source_imported_module_names(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> HashSet<String> {
    let mut providers = HashSet::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Import {
            import_kind: SyntaxImportKind::Module,
            module_name,
            items,
            ..
        } = &declaration.payload
        {
            if interfaces.contains_key(module_name) {
                providers.insert(module_name.clone());
            }
            for item in items {
                let nested = format!("{module_name}.{}", item.name);
                if interfaces.contains_key(&nested) {
                    providers.insert(nested);
                }
            }
        }
    }
    providers
}

/// Collects all import maps needed by syntax-output type inference.
///
/// Inputs:
/// - `module`: syntax-output module containing import declarations.
///
/// Output:
/// - Grouped module, asset, markdown, and selected-function imports.
///
/// Transformation:
/// - Delegates each import category to its specialized collector so callers
///   can build one inference context without knowing import declaration shape.
pub(super) fn collect_syntax_import_maps(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> TypeCheckImportMaps {
    TypeCheckImportMaps {
        module_aliases: collect_syntax_module_aliases(module, interfaces),
        file_imports: collect_syntax_file_imports(module),
        markdown_imports: collect_syntax_markdown_imports(module),
        function_imports: collect_syntax_function_imports(module, interfaces),
    }
}

/// Collects imported type names visible under local aliases.
///
/// Inputs:
/// - `resolved`: resolved module context with imported type symbols.
///
/// Output:
/// - Map from local type name to provider module/name pair.
///
/// Transformation:
/// - Repackages resolver import metadata into the typechecker's compact
///   qualified-name representation.
pub(super) fn imported_type_names(resolved: &ResolvedModule) -> HashMap<String, QualifiedTypeName> {
    resolved
        .imported_types
        .iter()
        .map(|(local_name, imported)| {
            (
                local_name.clone(),
                QualifiedTypeName {
                    module: imported.source_module.clone(),
                    name: imported.source_name.clone(),
                },
            )
        })
        .collect()
}

/// Collects imported type aliases and identity aliases from interfaces.
///
/// Inputs:
/// - `resolved`: resolved module context with provider interfaces.
///
/// Output:
/// - Type aliases available to the current module.
///
/// Transformation:
/// - Imports explicit provider aliases, adds qualified identity aliases for
///   exported opaque/struct types, and exposes selected imports under their
///   local source alias without expanding opaque representations.
pub(super) fn imported_type_aliases(resolved: &ResolvedModule) -> HashMap<String, TypeAlias> {
    let global_interface_aliases = resolved
        .interface_map
        .values()
        .flat_map(|interface| {
            interface_type_aliases(interface)
                .into_iter()
                .map(|(name, alias)| (format!("{}.{}", interface.module, name), alias))
        })
        .collect::<HashMap<_, _>>();
    let mut aliases = HashMap::new();
    for interface in resolved.interface_map.values() {
        let interface_aliases = interface_type_aliases(interface);
        for (name, alias) in interface_type_aliases(interface) {
            aliases.insert(
                format!("{}.{}", interface.module, name),
                normalize_imported_provider_alias(
                    &alias,
                    interface,
                    &interface_aliases,
                    &global_interface_aliases,
                ),
            );
        }
        for name in interface
            .public_types
            .iter()
            .chain(interface.opaque_types.iter())
        {
            let qualified_name = format!("{}.{}", interface.module, name);
            aliases
                .entry(qualified_name)
                .or_insert_with(|| interface_identity_type_alias(interface, name));
        }
    }
    for (local_name, imported) in &resolved.imported_types {
        let Some(interface) = resolved.interface_map.get(&imported.source_module) else {
            continue;
        };
        if interface.opaque_types.contains(&imported.source_name) {
            aliases.insert(
                local_name.clone(),
                interface_identity_type_alias(interface, &imported.source_name),
            );
            continue;
        }
        let interface_aliases = interface_type_aliases(interface);
        if let Some(alias) = interface_aliases.get(&imported.source_name) {
            aliases.insert(
                local_name.clone(),
                normalize_imported_provider_alias(
                    alias,
                    interface,
                    &interface_aliases,
                    &global_interface_aliases,
                ),
            );
        }
    }
    aliases
}

/// Expands provider-local references inside one imported alias body.
///
/// Inputs:
/// - `alias`: type alias selected through an import declaration.
/// - `interface`: provider that owns the alias and its local nominal names.
/// - `interface_aliases`: aliases declared by the provider module that owns
///   the selected alias.
/// - `global_aliases`: aliases from every loaded provider interface, keyed by
///   fully qualified `module.Type`.
///
/// Output:
/// - Cloned alias with the same parameters, constructor metadata, and opacity,
///   but with nested provider-local and imported aliases expanded in the body.
///
/// Transformation:
/// - Resolves dependencies such as `Config.mode: Mode` inside the provider
///   module, qualifies remaining provider-local nominal names, then resolves
///   unique global dependencies such as
///   `Option[String]`, before the alias is exposed under the importing
///   module's local name. This preserves imported outer identity while keeping
///   structural field comparisons from seeing unresolved provider-local names.
fn normalize_imported_provider_alias(
    alias: &TypeAlias,
    interface: &ModuleInterface,
    interface_aliases: &HashMap<String, TypeAlias>,
    global_aliases: &HashMap<String, TypeAlias>,
) -> TypeAlias {
    let body = expand_type_aliases(&alias.body, interface_aliases);
    let body = qualify_type_names(&body, &interface_qualified_type_names(interface));
    let body = expand_interface_global_aliases(&body, global_aliases);
    TypeAlias {
        params: alias.params.clone(),
        param_variance: alias.param_variance.clone(),
        bounds: alias.bounds.clone(),
        body,
        constructor_param_names: alias.constructor_param_names.clone(),
        is_opaque: alias.is_opaque,
    }
}

/// Builds an identity alias for an exported interface type.
///
/// Inputs:
/// - `interface`: provider interface exporting the type.
/// - `name`: exported type name within the provider interface.
///
/// Output:
/// - Alias whose body is the fully qualified type application with the same
///   type-parameter arity.
///
/// Transformation:
/// - Converts an exported opaque or struct type such as
///   `std.collections.Iterator.Iterator[T]` into an identity alias. This lets
///   import-erased interface summaries resolve globally unique type names
///   without exposing or expanding the provider representation.
fn interface_identity_type_alias(interface: &ModuleInterface, name: &str) -> TypeAlias {
    let generic_params = interface.type_params.get(name).cloned().unwrap_or_default();
    let params = generic_params
        .iter()
        .enumerate()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let vars = generic_params
        .iter()
        .enumerate()
        .map(|(index, param)| (normalize_type_param_name(param), index))
        .collect::<HashMap<_, _>>();
    let bounds = parse_structural_implication_bounds(
        &generic_params,
        &vars,
        &interface_type_names(interface),
    );
    let body_args = params.iter().map(|param| Type::Var(*param)).collect();

    TypeAlias {
        param_variance: interface
            .type_params
            .get(name)
            .map(|params| type_param_variances(params))
            .unwrap_or_default(),
        params,
        bounds,
        body: Type::Named {
            module: Some(interface.module.clone()),
            name: name.to_string(),
            args: body_args,
        },
        constructor_param_names: Vec::new(),
        is_opaque: true,
    }
}

/// Collects module-name aliases available to remote-call type inference.
///
/// Inputs:
/// - `module`: syntax-output module containing source import declarations.
///
/// Output:
/// - Map from source-visible module alias to fully qualified imported module
///   path.
///
/// Transformation:
/// - Treats bare module imports such as `import std.collections.Set.` as
///   binding the leaf name `Set` to `std.collections.Set`.
/// - Treats single imported upper names such as `import std.collections.Set.`
///   as binding `Set` to `std.collections.Set`.
/// - Treats selected module aliases such as `import std.text.{format as fmt}.`
///   as binding `fmt` to `std.text.format`.
/// - Skips type-only and asset imports because they do not create value-level
///   module call targets.
fn collect_syntax_module_aliases(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> HashMap<String, String> {
    let mut aliases = HashMap::new();

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

        if items.is_empty() {
            if let Some(leaf) = module_name
                .rsplit('.')
                .next()
                .filter(|leaf| !leaf.is_empty())
            {
                aliases.insert(leaf.to_string(), module_name.clone());
            }
            continue;
        }

        for item in items {
            if interfaces
                .get(module_name)
                .is_some_and(|interface| interface.traits.contains_key(&item.name))
            {
                continue;
            }
            let full_module_name = format!("{}.{}", module_name, item.name);
            if interfaces
                .get(&full_module_name)
                .is_some_and(|interface| interface.traits.contains_key(&item.name))
            {
                continue;
            }
            if let Some(alias) = &item.as_alias {
                aliases.insert(alias.clone(), full_module_name);
            } else if item
                .name
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
            {
                aliases.insert(item.name.clone(), full_module_name);
            }
        }
    }

    aliases
}

/// Collects selected function imports visible as local call names.
///
/// Inputs:
/// - `module`: syntax-output module containing source import declarations.
///
/// Output:
/// - Map from local call name to imported source module/function target.
///
/// Transformation:
/// - Scans module imports such as `import foo.Bar.{baz}` and
///   `import foo.Bar.{baz as qux}`, skips type-only imports and non-module
///   asset imports, and preserves aliases so local calls can be checked
///   against the provider interface.
fn collect_syntax_function_imports(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> HashMap<String, Vec<ImportedFunctionTarget>> {
    let mut imports = HashMap::new();
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

        for item in items {
            if item.name == "*" {
                let Some(interface) = interfaces.get(module_name) else {
                    continue;
                };
                for signature in interface.functions.values() {
                    if !signature.public || signature.receiver_method {
                        continue;
                    }
                    imports.entry(signature.name.clone()).or_insert_with(|| {
                        vec![ImportedFunctionTarget {
                            module: module_name.clone(),
                            function: signature.name.clone(),
                            span: item.span.into(),
                        }]
                    });
                }
                continue;
            }
            let local_name = item.as_alias.as_ref().unwrap_or(&item.name).clone();
            imports
                .entry(local_name)
                .or_insert_with(Vec::new)
                .push(ImportedFunctionTarget {
                    module: module_name.clone(),
                    function: item.name.clone(),
                    span: item.span.into(),
                });
        }
    }
    imports
}

/// Collects imported file and CSS assets visible by local alias.
///
/// Inputs:
/// - `module`: syntax-output module containing asset import declarations.
///
/// Output:
/// - Map from local alias to source path.
///
/// Transformation:
/// - Selects file/css imports that have a source path and first import item,
///   ignoring malformed or unrelated declarations.
fn collect_syntax_file_imports(module: &SyntaxModuleOutput) -> HashMap<String, String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::File | SyntaxImportKind::Css,
                items,
                source_path: Some(source_path),
                ..
            } => {
                let alias = items.first()?;
                Some((alias.name.clone(), source_path.clone()))
            }
            _ => None,
        })
        .collect()
}

/// Collects imported markdown assets visible by local alias.
///
/// Inputs:
/// - `module`: syntax-output module containing markdown import declarations.
///
/// Output:
/// - Map from local alias to source path.
///
/// Transformation:
/// - Selects markdown imports that have a source path and first import item,
///   ignoring malformed or unrelated declarations.
fn collect_syntax_markdown_imports(module: &SyntaxModuleOutput) -> HashMap<String, String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Markdown,
                items,
                source_path: Some(source_path),
                ..
            } => {
                let alias = items.first()?;
                Some((alias.name.clone(), source_path.clone()))
            }
            _ => None,
        })
        .collect()
}

/// Collects local type aliases declared by the current syntax module.
///
/// Inputs:
/// - `module`: syntax-output module containing type declarations.
///
/// Output:
/// - Map from local type name to typechecker alias metadata.
///
/// Transformation:
/// - Builds the local alias-name scope, parses each type declaration variant,
///   normalizes unions, and records parameter ids plus opacity.
pub(super) fn collect_syntax_type_aliases(
    module: &SyntaxModuleOutput,
) -> HashMap<String, TypeAlias> {
    let mut aliases = HashMap::new();
    let alias_names = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, .. }
            | SyntaxDeclarationPayload::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect::<HashSet<String>>();

    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Type {
            name,
            params,
            variants,
            is_opaque,
            representation,
            ..
        } = &declaration.payload
        {
            if representation.is_some() {
                continue;
            }
            let mut vars = HashMap::new();
            let mut next_var: TypeVarId = 0;
            let mut type_params = Vec::new();

            for param in params {
                vars.insert(normalize_type_param_name(param), next_var);
                type_params.push(next_var);
                next_var += 1;
            }

            let body = normalize_union(
                variants
                    .iter()
                    .filter_map(|variant| {
                        parse_type_expr(&variant.text, &alias_names, &mut vars, &mut next_var)
                    })
                    .collect(),
            );

            aliases.insert(
                name.clone(),
                TypeAlias {
                    params: type_params,
                    param_variance: type_param_variances(params),
                    bounds: parse_structural_implication_bounds(params, &vars, &alias_names),
                    body,
                    constructor_param_names: alias_constructor_param_names_from_variants(
                        &variants
                            .iter()
                            .map(|variant| variant.text.clone())
                            .collect::<Vec<_>>(),
                    ),
                    is_opaque: *is_opaque,
                },
            );
        }
    }

    aliases
}

/// Collects additional local names that should parse as type aliases.
///
/// Inputs:
/// - `module`: syntax-output module containing struct declarations.
///
/// Output:
/// - Set of struct names.
///
/// Transformation:
/// - Exposes local struct names to type parsing as alias-like names so struct
///   references are accepted consistently with aliases.
pub(super) fn collect_syntax_alias_extra_names(module: &SyntaxModuleOutput) -> HashSet<String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}
