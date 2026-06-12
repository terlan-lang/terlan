use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use terlan_syntax::{
    extract_native_function_signatures, span::Span, SyntaxDeclarationOutput,
    SyntaxDeclarationPayload, SyntaxImportItem, SyntaxModuleOutput, SyntaxParamOutput,
    SyntaxSourceKind,
};

#[derive(Debug, Clone)]
pub struct ModuleInterface {
    pub module: String,
    pub docs: Vec<String>,
    pub public_types: HashSet<String>,
    pub private_types: HashSet<String>,
    pub opaque_types: HashSet<String>,
    pub type_params: HashMap<String, Vec<String>>,
    pub type_bodies: HashMap<String, Vec<String>>,
    pub type_docs: HashMap<String, Vec<String>>,
    pub traits: HashMap<String, TraitSignature>,
    pub constructors: HashMap<String, Vec<ConstructorSignature>>,
    pub functions: HashMap<(String, usize), FunctionSignature>,
}

#[derive(Debug, Clone)]
pub struct ConstructorSignature {
    pub name: String,
    pub params: Vec<ParamSignature>,
    pub vararg: Option<ParamSignature>,
    pub return_type: String,
    pub body: String,
    pub min_arity: usize,
    pub varargs: bool,
    pub public: bool,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub params: Vec<ParamSignature>,
    pub return_type: String,
    pub public: bool,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TraitSignature {
    pub name: String,
    pub type_params: Vec<String>,
    pub super_traits: Vec<String>,
    pub methods: HashMap<String, TraitMethodSignature>,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TraitMethodSignature {
    pub params: Vec<ParamSignature>,
    pub return_type: String,
    pub generic_bounds: Vec<String>,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParamSignature {
    pub name: String,
    pub annotation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeVisibility {
    Public,
    Private,
}

#[derive(Debug, Clone)]
pub struct FunctionSymbol {
    pub name: String,
    pub arity: usize,
    pub params: Vec<ParamSignature>,
    pub return_type: String,
    pub public: bool,
    pub exported: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportedItem {
    pub local_name: String,
    pub source_module: String,
    pub source_name: String,
    pub visibility: TypeVisibility,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub name: String,
    pub function_symbols: HashMap<(String, usize), FunctionSymbol>,
    pub local_type_names: HashMap<String, TypeVisibility>,
    pub imported_types: HashMap<String, ImportedItem>,
    pub imported_traits: HashMap<String, ImportedItem>,
    pub interface_map: HashMap<String, ModuleInterface>,
    pub interface: ModuleInterface,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
}

#[derive(Debug)]
pub struct ResolveResult {
    pub module: ResolvedModule,
}

pub fn resolve_syntax_module_output(module: &SyntaxModuleOutput) -> ResolveResult {
    let interfaces = builtin_interfaces();
    resolve_syntax_module_output_with_interfaces(module, &interfaces)
}

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
                            params: native_sig
                                .params
                                .iter()
                                .map(|(param_name, annotation)| ParamSignature {
                                    name: param_name.clone(),
                                    annotation: annotation.clone(),
                                })
                                .collect(),
                            return_type: native_sig.return_type.clone(),
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
            | SyntaxDeclarationPayload::Template { .. }
            | SyntaxDeclarationPayload::Config { .. }
            | SyntaxDeclarationPayload::Raw { .. } => {}
        }
    }

    let mut interface = syntax_module_output_to_interface(module);
    for symbol in function_symbols.values() {
        interface.functions.insert(
            (symbol.name.clone(), symbol.arity),
            FunctionSignature {
                name: symbol.name.clone(),
                params: symbol.params.clone(),
                return_type: symbol.return_type.clone(),
                public: symbol.public,
                docs: symbol.docs.clone(),
            },
        );
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

pub fn parse_interface_file(path: &Path) -> Option<(String, ModuleInterface)> {
    let content = fs::read_to_string(path).ok()?;
    let parsed = terlan_syntax::parse_interface_module_as_syntax_output(&content).ok()?;
    let module_name = parsed.module_name.clone();
    let interface = syntax_module_output_to_interface(&parsed);
    Some((module_name, interface))
}

pub fn load_interfaces_from_dir(dir: &Path, acc: &mut HashMap<String, ModuleInterface>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        if extension == "tli" || extension == "typi" {
            if let Some((module_name, interface)) = parse_interface_file(&path) {
                insert_interface_if_not_poorer(acc, module_name, interface);
            }
        }
    }
}

/// Inserts an interface without replacing a richer duplicate.
///
/// Inputs:
/// - `acc`: accumulated interfaces keyed by module name.
/// - `module_name`: module identity parsed from the interface file.
/// - `interface`: parsed interface candidate.
///
/// Output:
/// - `acc` contains the candidate when no existing interface is present or when
///   the candidate carries at least as much public surface as the existing one.
///
/// Transformation:
/// - Scores interfaces by public type, function, constructor, trait, and type
///   body payload counts, then ignores duplicate candidates that would erase a
///   richer summary discovered earlier in the same load pass.
fn insert_interface_if_not_poorer(
    acc: &mut HashMap<String, ModuleInterface>,
    module_name: String,
    interface: ModuleInterface,
) {
    let incoming_score = interface_payload_score(&interface);
    let existing_score = acc
        .get(&module_name)
        .map(interface_payload_score)
        .unwrap_or(0);
    if incoming_score >= existing_score {
        acc.insert(module_name, interface);
    }
}

/// Computes a coarse public-payload score for duplicate interface resolution.
///
/// Inputs:
/// - `interface`: parsed interface candidate.
///
/// Output:
/// - Count of public surface payload buckets present in the interface.
///
/// Transformation:
/// - Sums exported type, opaque/private type, type body, trait, constructor,
///   and function counts so duplicate resolution prefers the interface with
///   more usable compiler metadata.
fn interface_payload_score(interface: &ModuleInterface) -> usize {
    interface.public_types.len()
        + interface.private_types.len()
        + interface.opaque_types.len()
        + interface.type_bodies.len()
        + interface.traits.len()
        + interface.constructors.len()
        + interface.functions.len()
}

pub fn load_interfaces_from_file_set(file_path: &str) -> HashMap<String, ModuleInterface> {
    let mut interfaces = HashMap::new();
    let current = Path::new(file_path);
    let base = current.parent().unwrap_or(Path::new("."));
    load_interfaces_from_dir(base, &mut interfaces);
    load_std_interfaces(current, &mut interfaces);
    interfaces
}

fn load_std_interfaces(current: &Path, acc: &mut HashMap<String, ModuleInterface>) {
    let mut dir = current.parent();
    while let Some(candidate) = dir {
        let std_dir = candidate.join("std");
        if std_dir.is_dir() {
            if load_interfaces_from_std_tree(&std_dir, acc) > 0 {
                return;
            }
        }
        dir = candidate.parent();
    }

    let cwd_std = Path::new("std");
    if cwd_std.is_dir() {
        load_interfaces_from_std_tree(cwd_std, acc);
    }
}

fn load_interfaces_from_std_tree(
    std_dir: &Path,
    acc: &mut HashMap<String, ModuleInterface>,
) -> usize {
    let before = acc.len();
    let entries = match fs::read_dir(std_dir) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_interfaces_from_dir(&path, acc);
        }
    }

    acc.len().saturating_sub(before)
}

pub fn syntax_module_output_to_interface(module: &SyntaxModuleOutput) -> ModuleInterface {
    let mut public_types = HashSet::new();
    let mut private_types = HashSet::new();
    let mut opaque_types = HashSet::new();
    let type_params = collect_syntax_type_params(module);
    let type_bodies = collect_syntax_type_bodies(module);
    let mut functions = HashMap::new();
    let traits = collect_syntax_trait_signatures(module);
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
                params,
                return_type,
                is_public,
                ..
            } => {
                let key = (name.clone(), params.len());
                functions.insert(
                    key,
                    FunctionSignature {
                        name: name.clone(),
                        params: syntax_param_signatures(params),
                        return_type: return_type.text.clone(),
                        public: *is_public,
                        docs: declaration.docs.clone(),
                    },
                );
            }
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                return_type,
                is_public,
                ..
            } => {
                let signature_params = syntax_method_param_signatures(receiver, params);
                let key = (name.clone(), signature_params.len());
                functions.insert(
                    key,
                    FunctionSignature {
                        name: name.clone(),
                        params: signature_params,
                        return_type: return_type.text.clone(),
                        public: *is_public,
                        docs: declaration.docs.clone(),
                    },
                );
            }
            SyntaxDeclarationPayload::Config { name, text, .. } if name == "native" => {
                for native_sig in extract_native_function_signatures(text) {
                    let key = (native_sig.name.clone(), native_sig.arity);
                    functions.entry(key).or_insert_with(|| FunctionSignature {
                        name: native_sig.name.clone(),
                        params: native_sig
                            .params
                            .iter()
                            .map(|(param_name, annotation)| ParamSignature {
                                name: param_name.clone(),
                                annotation: annotation.clone(),
                            })
                            .collect(),
                        return_type: native_sig.return_type.clone(),
                        public: true,
                        docs: declaration.docs.clone(),
                    });
                }
            }
            SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. }
            | SyntaxDeclarationPayload::Constructor { .. }
            | SyntaxDeclarationPayload::Trait { .. }
            | SyntaxDeclarationPayload::TraitImpl { .. }
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
        type_docs: collect_syntax_type_docs(module),
        traits,
        constructors,
        functions,
    }
}

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
                    params: method
                        .params
                        .iter()
                        .map(|param| ParamSignature {
                            name: param.name.clone(),
                            annotation: normalize_type_text(&param.annotation.text),
                        })
                        .collect(),
                    return_type: normalize_type_text(&method.return_type.text),
                    generic_bounds: method.generic_bounds.clone(),
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

fn collect_syntax_constructor_signatures(
    module: &SyntaxModuleOutput,
) -> HashMap<String, Vec<ConstructorSignature>> {
    let mut constructors = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Constructor {
            name,
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
                    })
                    .collect::<Vec<_>>();
                let vararg = clause
                    .params
                    .iter()
                    .find(|param| param.is_varargs)
                    .map(|param| ParamSignature {
                        name: param.name.clone(),
                        annotation: param.annotation.text.clone(),
                    });
                let min_arity = clause
                    .params
                    .iter()
                    .filter(|param| !param.is_varargs && !param.has_default)
                    .count();
                ConstructorSignature {
                    name: name.clone(),
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

fn syntax_param_signatures(params: &[SyntaxParamOutput]) -> Vec<ParamSignature> {
    params
        .iter()
        .map(|param| ParamSignature {
            name: param.name.clone(),
            annotation: param.annotation.text.clone(),
        })
        .collect()
}

fn resolve_syntax_import(
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
                    if let Some(existing) = imported_types.get(&local_name) {
                        if existing.source_module == module_name
                            && existing.source_name == item.name
                        {
                            continue;
                        }
                        diagnostics.push(Diagnostic {
                            span: item.span.into(),
                            message: format!(
                                "duplicate imported type name '{}', already imported from {}",
                                local_name, existing.source_module
                            ),
                        });
                        continue;
                    }
                    imported_types.insert(
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
                    if let Some(existing) = imported_types.get(&local_name) {
                        if existing.source_module == module_name
                            && existing.source_name == item.name
                        {
                            continue;
                        }
                        diagnostics.push(Diagnostic {
                            span: item.span.into(),
                            message: format!(
                                "duplicate imported type name '{}', already imported from {}",
                                local_name, existing.source_module
                            ),
                        });
                        continue;
                    }
                    imported_types.insert(
                        local_name.clone(),
                        ImportedItem {
                            local_name,
                            source_module: module_name.to_string(),
                            source_name: item.name.clone(),
                            visibility: TypeVisibility::Public,
                            span: item.span.into(),
                        },
                    );
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

fn add_syntax_function_symbol(
    declaration: &SyntaxDeclarationOutput,
    function_symbols: &mut HashMap<(String, usize), FunctionSymbol>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (name, params, return_type, is_public) = match &declaration.payload {
        SyntaxDeclarationPayload::Function {
            name,
            params,
            return_type,
            is_public,
            ..
        } => (
            name,
            syntax_param_signatures(params),
            return_type.text.as_str(),
            *is_public,
        ),
        SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            return_type,
            is_public,
            ..
        } => (
            name,
            syntax_method_param_signatures(receiver, params),
            return_type.text.as_str(),
            *is_public,
        ),
        _ => return,
    };

    let key = (name.clone(), params.len());
    if function_symbols.contains_key(&key) {
        diagnostics.push(Diagnostic {
            span: declaration.span.into(),
            message: format!("duplicate function definition: {} / {}", name, params.len()),
        });
        return;
    }

    let symbol = FunctionSymbol {
        name: name.clone(),
        arity: params.len(),
        params,
        return_type: return_type.to_string(),
        public: is_public,
        exported: is_public,
        docs: declaration.docs.clone(),
        span: declaration.span.into(),
    };
    function_symbols.insert(key, symbol);
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
            annotation: param.annotation.text.clone(),
        })
        .collect()
}

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

fn builtin_interfaces() -> HashMap<String, ModuleInterface> {
    HashMap::new()
}

impl ModuleInterface {
    pub fn to_terlan_interface_text(&self) -> String {
        self.render_terlan_interface_text(true)
    }

    pub fn to_terlan_interface_type_text(&self) -> String {
        self.render_terlan_interface_text(false)
    }

    pub fn to_terlan_interface_doc_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("module {}\n", self.module));
        push_doc_lines(&mut out, "!", &self.docs);

        let mut public_types: Vec<_> = self.public_types.iter().cloned().collect();
        public_types.sort();
        for ty in &public_types {
            push_doc_lines(
                &mut out,
                "/",
                self.type_docs.get(ty).map(Vec::as_slice).unwrap_or(&[]),
            );
        }

        let mut public_functions: Vec<_> = self
            .functions
            .iter()
            .filter(|(_, signature)| signature.public)
            .collect();
        public_functions.sort_by(|(lhs, _), (rhs, _)| match lhs.0.cmp(&rhs.0) {
            std::cmp::Ordering::Equal => lhs.1.cmp(&rhs.1),
            ord => ord,
        });

        for (_key, function) in public_functions {
            push_doc_lines(&mut out, "/", &function.docs);
        }

        out
    }

    fn render_terlan_interface_text(&self, include_docs: bool) -> String {
        let mut out = String::new();
        if include_docs {
            push_doc_lines(&mut out, "!", &self.docs);
        }
        out.push_str(&format!("module {}.\n\n", self.module));

        let mut public_types: Vec<_> = self.public_types.iter().cloned().collect();
        public_types.sort();
        for ty in &public_types {
            if include_docs {
                push_doc_lines(
                    &mut out,
                    "/",
                    self.type_docs.get(ty).map(Vec::as_slice).unwrap_or(&[]),
                );
            }
            if self.opaque_types.contains(ty) {
                out.push_str(&format!(
                    "pub opaque type {}{}.\n\n",
                    ty,
                    render_type_params(self.type_params.get(ty))
                ));
            } else {
                let params = render_type_params(self.type_params.get(ty));
                if let Some(body) = self.type_bodies.get(ty) {
                    out.push_str(&format!(
                        "pub type {}{} =\n    {}.\n\n",
                        ty,
                        params,
                        body.iter()
                            .map(|variant| normalize_type_text(variant))
                            .collect::<Vec<_>>()
                            .join("\n  | ")
                    ));
                } else {
                    out.push_str(&format!("pub type {}{}.\n\n", ty, params));
                }
            }
        }

        let mut public_functions: Vec<_> = self
            .functions
            .iter()
            .filter(|(_, signature)| signature.public)
            .collect();
        public_functions.sort_by(|(lhs, _), (rhs, _)| match lhs.0.cmp(&rhs.0) {
            std::cmp::Ordering::Equal => lhs.1.cmp(&rhs.1),
            ord => ord,
        });

        for (_key, function) in public_functions {
            if include_docs {
                push_doc_lines(&mut out, "/", &function.docs);
            }
            let params = function
                .params
                .iter()
                .map(|param| format!("{}: {}", param.name, normalize_type_text(&param.annotation)))
                .collect::<Vec<_>>();
            out.push_str(&format!(
                "pub {}({}): {}.\n\n",
                function.name,
                params.join(", "),
                normalize_type_text(&function.return_type)
            ));
        }

        let mut public_traits: Vec<_> = self.traits.values().collect();
        public_traits.sort_by(|left, right| left.name.cmp(&right.name));

        for trait_signature in public_traits {
            if include_docs {
                push_doc_lines(&mut out, "/", &trait_signature.docs);
            }

            let mut methods: Vec<_> = trait_signature.methods.iter().collect();
            methods.sort_by(|(left, _), (right, _)| left.cmp(right));

            let params = render_type_params(Some(&trait_signature.type_params));
            out.push_str(&format!("pub trait {}{}", trait_signature.name, params));
            if !trait_signature.super_traits.is_empty() {
                out.push_str(&format!(
                    " extends {}",
                    trait_signature.super_traits.join(", ")
                ));
            }
            out.push_str(" {\n");

            for (method_name, method) in methods {
                if include_docs {
                    push_doc_lines(&mut out, "/", &method.docs);
                }
                let params = method
                    .params
                    .iter()
                    .map(|param| {
                        format!("{}: {}", param.name, normalize_type_text(&param.annotation))
                    })
                    .collect::<Vec<_>>();
                out.push_str(&format!("    {}", method_name));
                if !method.generic_bounds.is_empty() {
                    out.push_str(&format!("<{}>", method.generic_bounds.join(", ")));
                }
                out.push_str(&format!(
                    "({}): {}.\n",
                    params.join(", "),
                    normalize_type_text(&method.return_type)
                ));
            }

            out.push_str("}.\n\n");
        }

        let mut public_constructors: Vec<_> = self
            .constructors
            .values()
            .flatten()
            .filter(|signature| signature.public)
            .collect();
        public_constructors.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.params.len().cmp(&right.params.len()))
                .then(left.varargs.cmp(&right.varargs))
        });

        let mut current_constructor = String::new();
        for (idx, constructor) in public_constructors.iter().enumerate() {
            if current_constructor != constructor.name {
                if !current_constructor.is_empty() {
                    out.push_str("}.\n\n");
                }
                current_constructor = constructor.name.clone();
                if include_docs {
                    push_doc_lines(&mut out, "/", &constructor.docs);
                }
                out.push_str(&format!("pub constructor {} {{\n", constructor.name));
            }

            let mut params = constructor
                .params
                .iter()
                .map(|param| format!("{}: {}", param.name, normalize_type_text(&param.annotation)))
                .collect::<Vec<_>>();
            if let Some(vararg) = &constructor.vararg {
                params.push(format!(
                    "...{}: {}",
                    vararg.name,
                    normalize_type_text(&vararg.annotation)
                ));
            }

            out.push_str(&format!(
                "    ({}): {} ->\n        {}",
                params.join(", "),
                normalize_type_text(&constructor.return_type),
                normalize_expr_text(&constructor.body)
            ));

            let next_is_same_constructor = public_constructors
                .get(idx + 1)
                .is_some_and(|next| next.name == constructor.name);
            if next_is_same_constructor {
                out.push_str(";\n\n");
            } else {
                out.push('\n');
            }
        }
        if !current_constructor.is_empty() {
            out.push_str("}.\n\n");
        }

        let mut private_types: Vec<_> = self.private_types.iter().cloned().collect();
        private_types.sort();
        if !private_types.is_empty() {
            out.push_str("export type ");
            out.push_str(&private_types.join(", "));
            out.push_str(".\n\n");
        }

        out
    }
}

fn render_type_params(params: Option<&Vec<String>>) -> String {
    match params {
        Some(params) if !params.is_empty() => format!("[{}]", params.join(", ")),
        _ => String::new(),
    }
}

fn push_doc_lines(out: &mut String, marker: &str, docs: &[String]) {
    for line in docs {
        out.push_str(&format!("//{} {}\n", marker, line));
    }
    if !docs.is_empty() {
        out.push('\n');
    }
}

fn normalize_type_text(input: &str) -> String {
    input
        .replace(" [", "[")
        .replace("[ ", "[")
        .replace(" ]", "]")
        .replace(" }", "}")
        .replace(" ,", ",")
        .replace(",", ", ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(", ]", "]")
}

fn normalize_expr_text(input: &str) -> String {
    normalize_type_text(input)
}

#[cfg(test)]
mod tests {
    use super::{
        load_interfaces_from_file_set, resolve_syntax_module_output,
        syntax_module_output_to_interface,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use terlan_syntax::cached_canonical_terlan_syntax_contract;
    use terlan_syntax::canonical_terlan_syntax_contract;
    use terlan_syntax::ebnf::EbnfGrammarExprKind;
    use terlan_syntax::parse_module_as_syntax_output;
    use terlan_syntax::validate_syntax_contract;
    use terlan_syntax::SyntaxSourceKind;

    /// Verifies test-layout `std` directories do not shadow root std summaries.
    ///
    /// Inputs:
    /// - A temporary workspace containing `tests/std` without summaries.
    /// - A root `std/summaries` directory containing `std_core_result.typi`.
    /// - A source path under `tests/std/core`.
    ///
    /// Output:
    /// - Test passes when `load_interfaces_from_file_set` still loads the root
    ///   stdlib summary.
    ///
    /// Transformation:
    /// - Builds the workspace on disk, runs normal interface discovery from the
    ///   test source path, and removes the workspace afterward.
    #[test]
    fn std_interface_loading_skips_empty_test_std_shadow_tree() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan_hir_std_shadow_{}_{}",
            std::process::id(),
            nanos
        ));
        let test_core = root.join("tests/std/core");
        let summaries = root.join("std/summaries");
        fs::create_dir_all(&test_core).expect("create test std fixture");
        fs::create_dir_all(&summaries).expect("create std summaries fixture");
        let source_path = test_core.join("result_test.tl");
        fs::write(&source_path, "module result_test.\n").expect("write test source fixture");
        fs::write(
            summaries.join("std_core_result.typi"),
            "\
module std_core_result.\n\
pub type Ok[T] = {:ok, T}.\n\
pub constructor Ok[T] {\n\
    (value: T): Ok[T] -> {:ok, value}\n\
}.\n",
        )
        .expect("write std summary fixture");

        let interfaces = load_interfaces_from_file_set(
            source_path
                .to_str()
                .expect("temporary source path should be utf-8"),
        );
        let _ = fs::remove_dir_all(&root);

        assert!(
            interfaces.contains_key("std_core_result"),
            "interfaces: {:?}",
            interfaces.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn hir_accepts_canonical_syntax_contract() {
        let contract =
            cached_canonical_terlan_syntax_contract().expect("cached canonical syntax contract");

        let diagnostics = validate_syntax_contract(contract);
        assert!(
            diagnostics.is_empty(),
            "unexpected syntax contract diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn hir_rejects_broken_syntax_contract() {
        let mut contract =
            canonical_terlan_syntax_contract().expect("compile canonical syntax contract");
        contract.entry_rule = Some("Program".to_string());
        let expr_rule = contract.rule("Expr").expect("Expr rule").clone();
        let expr_rule_index = contract
            .rules
            .iter()
            .position(|rule| rule.name == expr_rule.name)
            .expect("Expr rule index");
        contract.rules[expr_rule_index].expr.kind = EbnfGrammarExprKind::Terminal {
            value: "broken".to_string(),
        };

        let diagnostics = validate_syntax_contract(&contract);
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("entry rule")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "syntax rule Expr must reference SendExpr"));
    }

    #[test]
    fn resolve_syntax_output_records_function_symbols() {
        let syntax_module = parse_module_as_syntax_output(
            r#"
module syntax_resolve.

pub add(Value: Int): Int ->
    Value + 1.
"#,
        )
        .expect("parse syntax output");

        let resolved = resolve_syntax_module_output(&syntax_module);
        let symbol = resolved
            .module
            .function_symbols
            .get(&("add".to_string(), 1))
            .expect("add symbol");
        assert_eq!(symbol.return_type, "Int");
        assert!(symbol.exported);
        assert!(resolved.module.diagnostics.is_empty());
    }

    #[test]
    fn resolve_syntax_output_rejects_source_export_payloads() {
        let mut syntax_module = terlan_syntax::parse_interface_module_as_syntax_output(
            r#"
module syntax_resolve_source_export_payload.

export add/1.
"#,
        )
        .expect("parse interface syntax output");
        syntax_module.source_kind = SyntaxSourceKind::Module;

        let resolved = resolve_syntax_module_output(&syntax_module);
        assert!(resolved
            .module
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .message
                .contains("source export declarations are not part of canonical Terlan")));
        assert!(!resolved
            .module
            .function_symbols
            .contains_key(&("add".to_string(), 1)));
    }

    /// Verifies source-tree resolution rejects source-mode export
    /// payloads.
    ///
    /// Inputs:
    /// - An interface-parsed AST module containing an `Export` payload.
    ///
    /// Output:
    /// - Test passes when AST resolution reports the canonical source-export
    ///   diagnostic and does not create a function symbol from the interface
    ///   export summary.
    ///
    /// Transformation:
    /// - Feeds an interface export-summary AST payload through the source-oriented
    ///   compatibility resolver to prove it no longer treats `export` as a
    ///   normal source visibility mechanism.
    #[test]
    fn formal_hir_syntax_output_resolves_interface_surface() {
        let syntax_module = parse_module_as_syntax_output(
            r#"
//! Public docs.
module formal_syntax_iface.

/// Item collection.
pub type Items[T] =
    List[T].

/// Builds item collections.
pub constructor Items[T] {
    (Values: List[T]): Items[T] ->
        Values;

    (...Values: T): Items[T] ->
        Values
}.

/// Shows values.
	pub trait Show[A] {
	  /// Converts to text.
	  show(Value: A): Text.
	}.
	
	/// Adds one.
	pub add(Value: Int): Int ->
	    Value + 1.
"#,
        )
        .expect("parse syntax output");

        let interface = syntax_module_output_to_interface(&syntax_module);
        assert_eq!(interface.module, "formal_syntax_iface");
        assert_eq!(interface.docs, vec!["Public docs."]);
        assert!(interface.public_types.contains("Items"));
        assert_eq!(
            interface.type_params.get("Items"),
            Some(&vec!["T".to_string()])
        );
        assert_eq!(interface.constructors.get("Items").map(Vec::len), Some(2));
        assert_eq!(
            interface.traits["Show"].methods["show"].docs,
            vec!["Converts to text."]
        );
        assert_eq!(
            interface.functions[&("add".to_string(), 1)].return_type,
            "Int"
        );

        let resolved = resolve_syntax_module_output(&syntax_module);
        let symbol = resolved
            .module
            .function_symbols
            .get(&("add".to_string(), 1))
            .expect("add symbol");
        assert!(symbol.exported);
        assert!(resolved.module.diagnostics.is_empty());
    }
}
