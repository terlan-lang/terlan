use super::imports::{resolve_syntax_import, ImportResolutionTables};
use super::interface_conversion::{
    add_syntax_function_symbol, builtin_interfaces, collect_export_payloads,
    resolve_exports_against_defs, syntax_module_output_to_prepared_interface,
};
use super::*;

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
    let mut imported_constants = HashMap::new();

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
                    &mut ImportResolutionTables {
                        types: &mut imported_types,
                        traits: &mut imported_traits,
                        constants: &mut imported_constants,
                        diagnostics: &mut diagnostics,
                    },
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
                                    default: None,
                                    default_text: None,
                                })
                                .collect(),
                            return_type: native_sig.return_type.clone(),
                            generic_bounds: Vec::new(),
                            receiver_method: false,
                            receiver_mutable: false,
                            public: true,
                            exported: true,
                            pure: false,
                            docs: declaration.docs.clone(),
                            span: declaration.span.into(),
                        },
                    );
                }
            }
            SyntaxDeclarationPayload::Constructor { .. }
            | SyntaxDeclarationPayload::Constant { .. }
            | SyntaxDeclarationPayload::ConstFunction { .. }
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
        let key = (symbol.name.clone(), symbol.arity);
        let inferred_pure = interface
            .functions
            .get(&key)
            .is_some_and(|signature| signature.pure);
        let signature = FunctionSignature {
            name: symbol.name.clone(),
            generic_params: symbol.generic_params.clone(),
            params: symbol.params.clone(),
            return_type: symbol.return_type.clone(),
            generic_bounds: symbol.generic_bounds.clone(),
            receiver_method: symbol.receiver_method,
            receiver_mutable: symbol.receiver_mutable,
            public: symbol.public,
            pure: symbol.pure || inferred_pure,
            docs: symbol.docs.clone(),
        };
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
            imported_constants,
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
    if module.source_kind == SyntaxSourceKind::Module
        && crate::value_lifecycle::module_requires_value_lifecycle_pass(module, &HashMap::new())
    {
        let mut prepared = module.clone();
        let _ = crate::value_lifecycle::evaluate_and_substitute_module_constants(&mut prepared);
        return syntax_module_output_to_prepared_interface(&prepared);
    }
    syntax_module_output_to_prepared_interface(module)
}
