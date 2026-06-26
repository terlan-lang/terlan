use std::collections::{HashMap, HashSet};

use terlan_syntax::{
    extract_native_function_signatures, span::Span, SyntaxDeclarationOutput,
    SyntaxDeclarationPayload, SyntaxModuleOutput, SyntaxParamOutput, SyntaxSourceKind,
};

mod imports;
mod interface_loading;
mod interface_render;
mod model;
mod naming;

use imports::resolve_syntax_import;
pub use interface_loading::{
    load_interfaces_from_dir, load_interfaces_from_file_set, parse_interface_file,
};
use interface_render::normalize_type_text;
pub use model::{
    ConstructorSignature, Diagnostic, FunctionSignature, FunctionSymbol, ImportedItem,
    ModuleInterface, ParamSignature, ResolveResult, ResolvedModule, StructFieldSignature,
    TraitConformanceSignature, TraitConformanceSource, TraitMethodSignature, TraitSignature,
    TypeVisibility,
};
pub use naming::{identifier_to_snake, module_path_to_safe_native_module};

/// Resolves syntax output using built-in interfaces only.
///
/// Inputs: syntax-output module. Output: resolver result. Transformation:
/// loads built-in interfaces and delegates to the interface-aware resolver.
pub fn resolve_syntax_module_output(module: &SyntaxModuleOutput) -> ResolveResult {
    let interfaces = builtin_interfaces();
    resolve_syntax_module_output_with_interfaces(module, &interfaces)
}

/// Resolves syntax output with explicit external interfaces.
///
/// Inputs: syntax-output module and external interface map. Output: resolver
/// result. Transformation: merges built-ins with external interfaces, resolves
/// imports/types/functions/exports, and builds the module interface.
pub fn resolve_syntax_module_output_with_interfaces(
    module: &SyntaxModuleOutput,
    external_interfaces: &HashMap<String, ModuleInterface>,
) -> ResolveResult {
    let mut diagnostics = Vec::new();
    let mut interfaces = builtin_interfaces();
    for (name, interface) in external_interfaces {
        interfaces.insert(name.clone(), interface.clone());
    }

    let mut function_symbols = HashMap::new();
    let mut local_type_names = HashMap::new();
    let mut imported_types = HashMap::new();
    let mut imported_traits = HashMap::new();

    let mut exported_fns = HashSet::new();
    let mut exported_types = HashSet::new();
    let mut private_types = HashSet::new();
    let mut exported_opaques = HashSet::new();

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                module_name,
                items,
                is_type,
                ..
            } => {
                resolve_syntax_import(
                    module_name,
                    items,
                    *is_type,
                    &interfaces,
                    &mut imported_types,
                    &mut imported_traits,
                    &mut diagnostics,
                );
            }
            SyntaxDeclarationPayload::Export { items } => collect_export_payloads(
                module.source_kind,
                items,
                declaration.span.into(),
                &mut exported_fns,
                &mut diagnostics,
            ),
            SyntaxDeclarationPayload::Type {
                name,
                is_public,
                is_opaque,
                ..
            } => {
                let existing = local_type_names.get(name).cloned();
                if existing.is_some() {
                    diagnostics.push(Diagnostic {
                        span: declaration.span.into(),
                        message: format!("duplicate type declaration: {name}"),
                    });
                } else {
                    let visibility = if *is_public {
                        TypeVisibility::Public
                    } else {
                        TypeVisibility::Private
                    };
                    local_type_names.insert(name.clone(), visibility);
                    if visibility == TypeVisibility::Public {
                        exported_types.insert(name.clone());
                    } else {
                        private_types.insert(name.clone());
                    }
                    if *is_opaque && visibility == TypeVisibility::Public {
                        exported_opaques.insert(name.clone());
                    }
                }
            }
            SyntaxDeclarationPayload::Struct {
                name, is_public, ..
            } => {
                let existing = local_type_names.get(name).cloned();
                if existing.is_some() {
                    diagnostics.push(Diagnostic {
                        span: declaration.span.into(),
                        message: format!("duplicate type declaration: {name}"),
                    });
                } else {
                    let visibility = if *is_public {
                        TypeVisibility::Public
                    } else {
                        TypeVisibility::Private
                    };
                    local_type_names.insert(name.clone(), visibility);
                    if visibility == TypeVisibility::Public {
                        exported_types.insert(name.clone());
                    } else {
                        private_types.insert(name.clone());
                    }
                }
            }
            SyntaxDeclarationPayload::Function { .. } | SyntaxDeclarationPayload::Method { .. } => {
                add_syntax_function_symbol(declaration, &mut function_symbols, &mut diagnostics);
            }
            SyntaxDeclarationPayload::Config { name, text, .. } if name == "native" => {
                for native_sig in extract_native_function_signatures(text) {
                    let key = (native_sig.name.clone(), native_sig.arity);
                    if function_symbols.contains_key(&key) {
                        diagnostics.push(Diagnostic {
                            span: declaration.span.into(),
                            message: format!(
                                "duplicate function declaration: {} / {}",
                                native_sig.name, native_sig.arity
                            ),
                        });
                        continue;
                    }
                    function_symbols.insert(
                        key,
                        FunctionSymbol {
                            name: native_sig.name.clone(),
                            arity: native_sig.arity,
                            generic_params: Vec::new(),
                            params: native_sig
                                .params
                                .iter()
                                .map(|(param_name, annotation)| ParamSignature {
                                    name: param_name.clone(),
                                    annotation: annotation.clone(),
                                    is_mutable: false,
                                    default_text: None,
                                })
                                .collect(),
                            return_type: native_sig.return_type.clone(),
                            generic_bounds: Vec::new(),
                            receiver_method: false,
                            receiver_mutable: false,
                            public: true,
                            exported: true,
                            docs: declaration.docs.clone(),
                            span: declaration.span.into(),
                        },
                    );
                }
            }
            SyntaxDeclarationPayload::Constructor { .. }
            | SyntaxDeclarationPayload::Trait { .. }
            | SyntaxDeclarationPayload::TraitImpl { .. }
            | SyntaxDeclarationPayload::AnnotationSchema { .. }
            | SyntaxDeclarationPayload::Template { .. }
            | SyntaxDeclarationPayload::Config { .. }
            | SyntaxDeclarationPayload::Raw { .. } => {}
        }
    }

    let mut interface = syntax_module_output_to_interface(module);
    for symbol in function_symbols.values() {
        let signature = FunctionSignature {
            name: symbol.name.clone(),
            generic_params: symbol.generic_params.clone(),
            params: symbol.params.clone(),
            return_type: symbol.return_type.clone(),
            generic_bounds: symbol.generic_bounds.clone(),
            receiver_method: symbol.receiver_method,
            receiver_mutable: symbol.receiver_mutable,
            public: symbol.public,
            docs: symbol.docs.clone(),
        };
        let key = (symbol.name.clone(), symbol.arity);
        interface.functions.insert(key.clone(), signature.clone());
        interface
            .function_overloads
            .entry(key)
            .or_insert_with(|| vec![signature]);
    }

    interface.public_types = exported_types;
    interface.private_types = private_types;
    interface.opaque_types = exported_opaques;

    resolve_exports_against_defs(&exported_fns, &mut function_symbols, &mut diagnostics);

    ResolveResult {
        module: ResolvedModule {
            name: module.module_name.clone(),
            function_symbols,
            local_type_names,
            imported_types,
            imported_traits,
            interface_map: interfaces,
            interface,
            diagnostics,
        },
    }
}

/// Converts syntax output to an importable module interface.
///
/// Inputs: syntax-output module. Output: module interface. Transformation:
/// collects public/private types, type bodies, struct fields, traits,
/// conformances, constructors, functions, overloads, docs, and module metadata.
pub fn syntax_module_output_to_interface(module: &SyntaxModuleOutput) -> ModuleInterface {
    let mut public_types = HashSet::new();
    let mut private_types = HashSet::new();
    let mut opaque_types = HashSet::new();
    let type_params = collect_syntax_type_params(module);
    let type_bodies = collect_syntax_type_bodies(module);
    let struct_fields = collect_syntax_struct_fields(module);
    let mut functions = HashMap::new();
    let mut function_overloads = HashMap::new();
    let traits = collect_syntax_trait_signatures(module);
    let trait_conformances = collect_syntax_trait_conformances(module);
    let constructors = collect_syntax_constructor_signatures(module);

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name,
                is_public,
                is_opaque,
                ..
            } => {
                if *is_public {
                    public_types.insert(name.clone());
                    if *is_opaque {
                        opaque_types.insert(name.clone());
                    }
                } else {
                    private_types.insert(name.clone());
                }
            }
            SyntaxDeclarationPayload::Struct {
                name, is_public, ..
            } => {
                if *is_public {
                    public_types.insert(name.clone());
                } else {
                    private_types.insert(name.clone());
                }
            }
            SyntaxDeclarationPayload::Function {
                name,
                generic_params,
                params,
                return_type,
                generic_bounds,
                is_public,
                ..
            } => {
                let key = (name.clone(), params.len());
                let signature = FunctionSignature {
                    name: name.clone(),
                    generic_params: generic_params.clone(),
                    params: syntax_param_signatures(params),
                    return_type: normalize_type_text(&return_type.text),
                    generic_bounds: generic_bounds.clone(),
                    receiver_method: false,
                    receiver_mutable: false,
                    public: *is_public,
                    docs: declaration.docs.clone(),
                };
                functions
                    .entry(key.clone())
                    .or_insert_with(|| signature.clone());
                function_overloads
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(signature);
            }
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                generic_params,
                params,
                return_type,
                generic_bounds,
                is_public,
                ..
            } => {
                let signature_params = syntax_method_param_signatures(receiver, params);
                let key = (name.clone(), signature_params.len());
                let signature = FunctionSignature {
                    name: name.clone(),
                    generic_params: generic_params.clone(),
                    params: signature_params,
                    return_type: normalize_type_text(&return_type.text),
                    generic_bounds: generic_bounds.clone(),
                    receiver_method: true,
                    receiver_mutable: receiver.is_mutable,
                    public: *is_public,
                    docs: declaration.docs.clone(),
                };
                functions
                    .entry(key.clone())
                    .or_insert_with(|| signature.clone());
                function_overloads
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(signature);
            }
            SyntaxDeclarationPayload::Config { name, text, .. } if name == "native" => {
                for native_sig in extract_native_function_signatures(text) {
                    let key = (native_sig.name.clone(), native_sig.arity);
                    let signature = FunctionSignature {
                        name: native_sig.name.clone(),
                        generic_params: Vec::new(),
                        params: native_sig
                            .params
                            .iter()
                            .map(|(param_name, annotation)| ParamSignature {
                                name: param_name.clone(),
                                annotation: annotation.clone(),
                                is_mutable: false,
                                default_text: None,
                            })
                            .collect(),
                        return_type: native_sig.return_type.clone(),
                        generic_bounds: Vec::new(),
                        receiver_method: false,
                        receiver_mutable: false,
                        public: true,
                        docs: declaration.docs.clone(),
                    };
                    functions
                        .entry(key.clone())
                        .or_insert_with(|| signature.clone());
                    function_overloads
                        .entry(key)
                        .or_insert_with(Vec::new)
                        .push(signature);
                }
            }
            SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. }
            | SyntaxDeclarationPayload::Constructor { .. }
            | SyntaxDeclarationPayload::Trait { .. }
            | SyntaxDeclarationPayload::TraitImpl { .. }
            | SyntaxDeclarationPayload::AnnotationSchema { .. }
            | SyntaxDeclarationPayload::Template { .. }
            | SyntaxDeclarationPayload::Config { .. }
            | SyntaxDeclarationPayload::Raw { .. } => {}
        }
    }

    ModuleInterface {
        module: module.module_name.clone(),
        docs: module.docs.clone(),
        public_types,
        private_types,
        opaque_types,
        type_params,
        type_bodies,
        struct_fields,
        type_docs: collect_syntax_type_docs(module),
        traits,
        trait_conformances,
        constructors,
        functions,
        function_overloads,
    }
}

/// Collects public and private trait conformance facts from syntax output.
///
/// Inputs:
/// - `module`: syntax-output module containing type, struct, or explicit impl
///   declarations.
///
/// Output:
/// - Sorted, deduplicated trait conformance summaries.
///
/// Transformation:
/// - Converts declaration-site `implements` and explicit `impl Trait[...] for
///   Type` declarations into stable interface metadata so imported modules can
///   expose conformance without exposing implementation bodies. Struct
///   `includes` is intentionally excluded because it expands struct shape, not
///   trait conformance.
fn collect_syntax_trait_conformances(
    module: &SyntaxModuleOutput,
) -> Vec<TraitConformanceSignature> {
    let mut conformances = Vec::new();
    let imported_type_refs = collect_syntax_imported_type_refs(module);

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name,
                implements,
                is_public,
                ..
            }
            | SyntaxDeclarationPayload::Struct {
                name,
                implements,
                is_public,
                ..
            } => {
                conformances.extend(
                    implements
                        .iter()
                        .map(|trait_ref| TraitConformanceSignature {
                            trait_ref: qualify_syntax_type_text(
                                &normalize_type_text(&trait_ref.text),
                                &imported_type_refs,
                            ),
                            for_type: qualify_syntax_type_text(
                                &normalize_type_text(name),
                                &imported_type_refs,
                            ),
                            source: TraitConformanceSource::Implements,
                            public: *is_public,
                        }),
                );
            }
            _ => {}
        }

        if let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_public,
            ..
        } = &declaration.payload
        {
            conformances.push(TraitConformanceSignature {
                trait_ref: qualify_syntax_type_text(
                    &normalize_type_text(&trait_ref.text),
                    &imported_type_refs,
                ),
                for_type: qualify_syntax_type_text(
                    &normalize_type_text(&for_type.text),
                    &imported_type_refs,
                ),
                source: TraitConformanceSource::ExplicitImpl,
                public: *is_public,
            });
        }
    }

    conformances.sort();
    conformances.dedup();
    conformances
}

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
fn collect_syntax_imported_type_refs(module: &SyntaxModuleOutput) -> HashMap<String, String> {
    let mut refs = HashMap::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind: terlan_syntax::SyntaxImportKind::Module,
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
fn qualify_syntax_type_text(text: &str, imported_type_refs: &HashMap<String, String>) -> String {
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

/// Collects public trait signatures from syntax output.
///
/// Inputs: syntax-output module. Output: map from trait name to signature.
/// Transformation: ignores private traits and converts public trait methods to
/// span-free interface signatures.
fn collect_syntax_trait_signatures(module: &SyntaxModuleOutput) -> HashMap<String, TraitSignature> {
    let mut traits = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Trait {
            name,
            params,
            super_traits,
            is_public,
            methods,
        } = &declaration.payload
        else {
            continue;
        };

        if !*is_public {
            continue;
        }

        let mut method_signatures = HashMap::new();
        for method in methods {
            method_signatures.insert(
                method.name.clone(),
                TraitMethodSignature {
                    generic_params: method.generic_params.clone(),
                    params: method
                        .params
                        .iter()
                        .map(|param| ParamSignature {
                            name: param.name.clone(),
                            annotation: normalize_type_text(&param.annotation.text),
                            is_mutable: param.is_mutable,
                            default_text: param.default_text.clone(),
                        })
                        .collect(),
                    return_type: normalize_type_text(&method.return_type.text),
                    generic_bounds: method.generic_bounds.clone(),
                    has_default: method.default_body.is_some(),
                    docs: method.docs.clone(),
                },
            );
        }

        traits.insert(
            name.clone(),
            TraitSignature {
                name: name.clone(),
                type_params: params.clone(),
                super_traits: super_traits.clone(),
                methods: method_signatures,
                docs: declaration.docs.clone(),
            },
        );
    }

    traits
}

/// Collects constructor signatures from syntax output.
///
/// Inputs: syntax-output module. Output: constructor signatures keyed by type
/// name. Transformation: converts constructor clauses into fixed/vararg
/// signature metadata and computes minimum arity from defaults.
fn collect_syntax_constructor_signatures(
    module: &SyntaxModuleOutput,
) -> HashMap<String, Vec<ConstructorSignature>> {
    let mut constructors = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Constructor {
            name,
            params: type_params,
            clauses,
            is_public,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let signatures = clauses
            .iter()
            .map(|clause| {
                let params = clause
                    .params
                    .iter()
                    .filter(|param| !param.is_varargs)
                    .map(|param| ParamSignature {
                        name: param.name.clone(),
                        annotation: param.annotation.text.clone(),
                        is_mutable: false,
                        default_text: param.default_text.clone(),
                    })
                    .collect::<Vec<_>>();
                let vararg = clause
                    .params
                    .iter()
                    .find(|param| param.is_varargs)
                    .map(|param| ParamSignature {
                        name: param.name.clone(),
                        annotation: param.annotation.text.clone(),
                        is_mutable: false,
                        default_text: param.default_text.clone(),
                    });
                let min_arity = clause
                    .params
                    .iter()
                    .filter(|param| !param.is_varargs && !param.has_default)
                    .count();
                ConstructorSignature {
                    name: name.clone(),
                    type_params: type_params.clone(),
                    params,
                    vararg,
                    return_type: clause.return_type.text.clone(),
                    body: clause.body_text.clone(),
                    min_arity,
                    varargs: clause.params.iter().any(|param| param.is_varargs),
                    public: *is_public,
                    docs: declaration.docs.clone(),
                }
            })
            .collect::<Vec<_>>();

        constructors.insert(name.clone(), signatures);
    }

    constructors
}

/// Collects public type parameters from syntax output.
///
/// Inputs: syntax-output module. Output: public type name to type parameters.
/// Transformation: ignores private type aliases and preserves source parameter
/// order.
fn collect_syntax_type_params(module: &SyntaxModuleOutput) -> HashMap<String, Vec<String>> {
    let mut params = HashMap::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Type {
            name,
            params: type_params,
            is_public,
            ..
        } = &declaration.payload
        {
            if *is_public {
                params.insert(name.clone(), type_params.clone());
            }
        }
    }
    params
}

/// Collects public non-opaque type bodies.
///
/// Inputs: syntax-output module. Output: public type name to variant text.
/// Transformation: excludes opaque/private/empty bodies and preserves variant
/// order for imported constructor/type checks.
fn collect_syntax_type_bodies(module: &SyntaxModuleOutput) -> HashMap<String, Vec<String>> {
    let mut bodies = HashMap::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Type {
            name,
            variants,
            is_public,
            is_opaque,
            ..
        } = &declaration.payload
        {
            if *is_public && !*is_opaque && !variants.is_empty() {
                bodies.insert(
                    name.clone(),
                    variants
                        .iter()
                        .map(|variant| variant.text.clone())
                        .collect(),
                );
            }
        }
    }
    bodies
}

/// Collects public struct fields for interface summaries.
///
/// Inputs:
/// - `module`: syntax-output module containing source declarations.
///
/// Output:
/// - Map from public struct name to ordered field signatures.
///
/// Transformation:
/// - Converts public syntax-output struct fields into span-free interface
///   metadata so downstream modules can expand `struct Child includes Parent`
///   across module boundaries.
fn collect_syntax_struct_fields(
    module: &SyntaxModuleOutput,
) -> HashMap<String, Vec<StructFieldSignature>> {
    let mut structs = HashMap::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Struct {
            name,
            fields,
            is_public,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        if !*is_public {
            continue;
        }

        structs.insert(
            name.clone(),
            fields
                .iter()
                .map(|field| StructFieldSignature {
                    name: field.name.clone(),
                    annotation: normalize_type_text(&field.annotation.text),
                    is_private: field.is_private,
                })
                .collect(),
        );
    }

    structs
}

/// Collects public type and struct documentation.
///
/// Inputs: syntax-output module. Output: public type name to docs.
/// Transformation: copies declaration docs for public type-like declarations.
fn collect_syntax_type_docs(module: &SyntaxModuleOutput) -> HashMap<String, Vec<String>> {
    let mut docs = HashMap::new();
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name, is_public, ..
            }
            | SyntaxDeclarationPayload::Struct {
                name, is_public, ..
            } if *is_public => {
                docs.insert(name.clone(), declaration.docs.clone());
            }
            _ => {}
        }
    }
    docs
}

/// Converts syntax parameters to HIR parameter signatures.
///
/// Inputs: syntax-output params. Output: span-free parameter signatures.
/// Transformation: normalizes type text and preserves parameter names and
/// mutability flags.
fn syntax_param_signatures(params: &[SyntaxParamOutput]) -> Vec<ParamSignature> {
    params
        .iter()
        .map(|param| ParamSignature {
            name: param.name.clone(),
            annotation: normalize_type_text(&param.annotation.text),
            is_mutable: param.is_mutable,
            default_text: param.default_text.clone(),
        })
        .collect()
}

/// Adds a function or method declaration to the local symbol table.
///
/// Inputs: declaration, mutable symbol table, and diagnostics sink. Output:
/// symbol table or diagnostics are updated. Transformation: extracts callable
/// shape, detects duplicate shapes, and records exported/public metadata.
fn add_syntax_function_symbol(
    declaration: &SyntaxDeclarationOutput,
    function_symbols: &mut HashMap<(String, usize), FunctionSymbol>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (
        name,
        generic_params,
        params,
        return_type,
        generic_bounds,
        receiver_method,
        receiver_mutable,
        is_public,
    ) = match &declaration.payload {
        SyntaxDeclarationPayload::Function {
            name,
            generic_params,
            params,
            return_type,
            generic_bounds,
            is_public,
            ..
        } => (
            name,
            generic_params.clone(),
            syntax_param_signatures(params),
            return_type.text.as_str(),
            generic_bounds.clone(),
            false,
            false,
            *is_public,
        ),
        SyntaxDeclarationPayload::Method {
            receiver,
            name,
            generic_params,
            params,
            return_type,
            generic_bounds,
            is_public,
            ..
        } => (
            name,
            generic_params.clone(),
            syntax_method_param_signatures(receiver, params),
            return_type.text.as_str(),
            generic_bounds.clone(),
            true,
            receiver.is_mutable,
            *is_public,
        ),
        _ => return,
    };

    let key = (name.clone(), params.len());
    if let Some(existing) = function_symbols.get(&key) {
        if function_symbol_shape_matches(
            existing,
            &params,
            return_type,
            receiver_method,
            receiver_mutable,
        ) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!("duplicate function definition: {} / {}", name, params.len()),
            });
        }
        return;
    }

    let symbol = FunctionSymbol {
        name: name.clone(),
        arity: params.len(),
        generic_params,
        params,
        return_type: return_type.to_string(),
        generic_bounds,
        receiver_method,
        receiver_mutable,
        public: is_public,
        exported: is_public,
        docs: declaration.docs.clone(),
        span: declaration.span.into(),
    };
    function_symbols.insert(key, symbol);
}

/// Checks whether two same-name same-arity function declarations have the same shape.
///
/// Inputs:
/// - `existing`: first HIR function symbol already recorded for the name and
///   arity.
/// - `params`, `return_type`, `receiver_method`, and `receiver_mutable`: shape
///   of the later declaration being considered.
///
/// Output:
/// - `true` when the later declaration is a duplicate of the existing symbol.
/// - `false` when it is a distinct overload candidate.
///
/// Transformation:
/// - Compares callable kind, receiver mutability, return annotation, and
///   parameter annotations. Parameter names are intentionally ignored so
///   overload identity is based on callable type shape, not local binding names.
fn function_symbol_shape_matches(
    existing: &FunctionSymbol,
    params: &[ParamSignature],
    return_type: &str,
    receiver_method: bool,
    receiver_mutable: bool,
) -> bool {
    existing.receiver_method == receiver_method
        && existing.receiver_mutable == receiver_mutable
        && existing.return_type == return_type
        && existing.params.len() == params.len()
        && existing
            .params
            .iter()
            .zip(params.iter())
            .all(|(left, right)| {
                left.annotation == right.annotation && left.is_mutable == right.is_mutable
            })
}

/// Converts method syntax parameters into callable HIR parameters.
///
/// Inputs:
/// - `receiver`: receiver parameter from a syntax-output method declaration.
/// - `params`: ordinary method parameters.
///
/// Output:
/// - Parameter signatures with the receiver first.
///
/// Transformation:
/// - Rewrites source-level receiver syntax into the backend/interface calling
///   convention `method(receiver, params...)`.
fn syntax_method_param_signatures(
    receiver: &SyntaxParamOutput,
    params: &[SyntaxParamOutput],
) -> Vec<ParamSignature> {
    std::iter::once(receiver)
        .chain(params.iter())
        .map(|param| ParamSignature {
            name: param.name.clone(),
            annotation: normalize_type_text(&param.annotation.text),
            is_mutable: param.is_mutable,
            default_text: param.default_text.clone(),
        })
        .collect()
}

/// Resolves interface export summaries against local function definitions.
///
/// Inputs: export set, mutable function symbols, and diagnostics sink. Output:
/// function export flags or diagnostics are updated. Transformation: marks
/// matching symbols exported and reports missing definitions.
fn resolve_exports_against_defs(
    exported_fns: &HashSet<(String, usize)>,
    function_symbols: &mut HashMap<(String, usize), FunctionSymbol>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (name, arity) in exported_fns {
        if let Some(symbol) = function_symbols.get_mut(&(name.clone(), *arity)) {
            symbol.exported = true;
        } else {
            diagnostics.push(Diagnostic {
                span: Span::new(0, 0),
                message: format!("exported function {}/{} is not defined", name, arity),
            });
        }
    }
}

/// Collects interface-only export summaries into the resolver export set.
///
/// Inputs:
/// - `source_kind`: whether the syntax output came from a source module or an
///   interface summary.
/// - `items`: interface export-summary entries preserved by the syntax-output
///   frontend.
/// - `span`: source span for diagnostics.
/// - `exported_fns`: mutable export set keyed by function name and arity.
/// - `diagnostics`: mutable HIR diagnostic sink.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Interface outputs may preserve export summaries, so their entries are
///   copied into `exported_fns`.
/// - Normal module outputs must use declaration-site `pub`; an export payload
///   in module mode is reported and ignored.
fn collect_export_payloads(
    source_kind: SyntaxSourceKind,
    items: &[terlan_syntax::SyntaxExportItem],
    span: Span,
    exported_fns: &mut HashSet<(String, usize)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match source_kind {
        SyntaxSourceKind::Interface => {
            for entry in items {
                exported_fns.insert((entry.name.clone(), entry.arity));
            }
        }
        SyntaxSourceKind::Module => diagnostics.push(Diagnostic {
            span,
            message:
                "source export declarations are not part of canonical Terlan; use `pub` on declarations"
                    .to_string(),
        }),
    }
}

/// Returns compiler-provided interfaces available without project files.
///
/// Inputs: none. Output: built-in interface map. Transformation: currently
/// returns an empty map because release std interfaces are loaded from source
/// summaries rather than hard-coded HIR metadata.
fn builtin_interfaces() -> HashMap<String, ModuleInterface> {
    HashMap::new()
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
