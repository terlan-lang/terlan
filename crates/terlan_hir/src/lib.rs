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
    pub struct_fields: HashMap<String, Vec<StructFieldSignature>>,
    pub type_docs: HashMap<String, Vec<String>>,
    pub traits: HashMap<String, TraitSignature>,
    pub trait_conformances: Vec<TraitConformanceSignature>,
    pub constructors: HashMap<String, Vec<ConstructorSignature>>,
    pub functions: HashMap<(String, usize), FunctionSignature>,
}

/// Public field signature for a struct exported through a module interface.
///
/// Inputs:
/// - One source struct field from syntax output.
///
/// Output:
/// - Stable interface metadata containing the field name and normalized type.
///
/// Transformation:
/// - Drops source spans and default expressions so imported modules can type
///   check and expand derived struct shape without depending on implementation
///   source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructFieldSignature {
    pub name: String,
    pub annotation: String,
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
    pub receiver_method: bool,
    pub receiver_mutable: bool,
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
    pub has_default: bool,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TraitConformanceSignature {
    pub trait_ref: String,
    pub for_type: String,
    pub source: TraitConformanceSource,
    pub public: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TraitConformanceSource {
    Implements,
    ExplicitImpl,
}

#[derive(Debug, Clone)]
pub struct ParamSignature {
    pub name: String,
    pub annotation: String,
    pub is_mutable: bool,
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
    pub receiver_method: bool,
    pub receiver_mutable: bool,
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
                                    is_mutable: false,
                                })
                                .collect(),
                            return_type: native_sig.return_type.clone(),
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
        interface.functions.insert(
            (symbol.name.clone(), symbol.arity),
            FunctionSignature {
                name: symbol.name.clone(),
                params: symbol.params.clone(),
                return_type: symbol.return_type.clone(),
                receiver_method: symbol.receiver_method,
                receiver_mutable: symbol.receiver_mutable,
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
        if extension == "terli" || extension == "typi" {
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
    let struct_fields = collect_syntax_struct_fields(module);
    let mut functions = HashMap::new();
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
                        return_type: normalize_type_text(&return_type.text),
                        receiver_method: false,
                        receiver_mutable: false,
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
                        return_type: normalize_type_text(&return_type.text),
                        receiver_method: true,
                        receiver_mutable: receiver.is_mutable,
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
                                is_mutable: false,
                            })
                            .collect(),
                        return_type: native_sig.return_type.clone(),
                        receiver_method: false,
                        receiver_mutable: false,
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
///   `derives` is intentionally excluded because it derives struct shape, not
///   trait conformance.
fn collect_syntax_trait_conformances(
    module: &SyntaxModuleOutput,
) -> Vec<TraitConformanceSignature> {
    let mut conformances = Vec::new();

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
                            trait_ref: normalize_type_text(&trait_ref.text),
                            for_type: normalize_type_text(name),
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
                trait_ref: normalize_type_text(&trait_ref.text),
                for_type: normalize_type_text(&for_type.text),
                source: TraitConformanceSource::ExplicitImpl,
                public: *is_public,
            });
        }
    }

    conformances.sort();
    conformances.dedup();
    conformances
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
                            is_mutable: param.is_mutable,
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
                        is_mutable: false,
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
///   metadata so downstream modules can expand `struct Child derives Parent`
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
                })
                .collect(),
        );
    }

    structs
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
            annotation: normalize_type_text(&param.annotation.text),
            is_mutable: param.is_mutable,
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

fn add_syntax_function_symbol(
    declaration: &SyntaxDeclarationOutput,
    function_symbols: &mut HashMap<(String, usize), FunctionSymbol>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (name, params, return_type, receiver_method, receiver_mutable, is_public) =
        match &declaration.payload {
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
                false,
                false,
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
                true,
                receiver.is_mutable,
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
        receiver_method,
        receiver_mutable,
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
            annotation: normalize_type_text(&param.annotation.text),
            is_mutable: param.is_mutable,
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
            } else if let Some(fields) = self.struct_fields.get(ty) {
                out.push_str(&format!("pub struct {} {{\n", ty));
                for (index, field) in fields.iter().enumerate() {
                    let suffix = if index + 1 == fields.len() { "" } else { "," };
                    out.push_str(&format!(
                        "    {}: {}{}\n",
                        field.name,
                        normalize_type_text(&field.annotation),
                        suffix
                    ));
                }
                out.push_str("}.\n\n");
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
            if function.receiver_method && !function.params.is_empty() {
                let receiver = &function.params[0];
                let receiver_mut = if function.receiver_mutable {
                    "mut "
                } else {
                    ""
                };
                let params = function
                    .params
                    .iter()
                    .skip(1)
                    .map(render_param_signature)
                    .collect::<Vec<_>>();
                out.push_str(&format!(
                    "pub ({}{}: {}) {}({}): {}.\n\n",
                    receiver_mut,
                    receiver.name,
                    normalize_type_text(&receiver.annotation),
                    function.name,
                    params.join(", "),
                    normalize_type_text(&function.return_type)
                ));
                continue;
            }
            let params = function
                .params
                .iter()
                .map(render_param_signature)
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
                    .map(render_param_signature)
                    .collect::<Vec<_>>();
                out.push_str(&format!("    {}", method_name));
                if !method.generic_bounds.is_empty() {
                    out.push_str(&format!("<{}>", method.generic_bounds.join(", ")));
                }
                out.push_str(&format!(
                    "({}): {}",
                    params.join(", "),
                    normalize_type_text(&method.return_type)
                ));
                if method.has_default {
                    out.push_str(" ->\n        terlan_interface_default");
                }
                out.push_str(".\n");
            }

            out.push_str("}.\n\n");
        }

        let mut public_conformances: Vec<_> = self
            .trait_conformances
            .iter()
            .filter(|conformance| conformance.public)
            .collect();
        public_conformances.sort();

        for conformance in public_conformances {
            out.push_str(&format!(
                "pub impl {} for {} {{\n}}.\n\n",
                normalize_type_text(&conformance.trait_ref),
                normalize_type_text(&conformance.for_type)
            ));
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
                .map(render_param_signature)
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

/// Renders one public interface parameter.
///
/// Inputs:
/// - `param`: HIR parameter signature carrying name, type annotation, and
///   source mutability.
///
/// Output:
/// - A source-like parameter fragment such as `value: Int` or
///   `mut collection: C`.
///
/// Transformation:
/// - Normalizes the type text and preserves `mut` so generated `.typi`
///   summaries do not erase trait or function parameter mutability.
fn render_param_signature(param: &ParamSignature) -> String {
    let mut_prefix = if param.is_mutable { "mut " } else { "" };
    format!(
        "{}{}: {}",
        mut_prefix,
        param.name,
        normalize_type_text(&param.annotation)
    )
}

fn render_type_params(params: Option<&Vec<String>>) -> String {
    match params {
        Some(params) if !params.is_empty() => format!("[{}]", params.join(", ")),
        _ => String::new(),
    }
}

/// Appends Terlan doc comments to rendered interface text.
///
/// Inputs:
/// - `out`: rendered interface buffer being built.
/// - `marker`: doc-comment marker suffix, either `!` for module docs or `/`
///   for item docs.
/// - `docs`: normalized documentation blocks collected from source syntax.
///
/// Output:
/// - `out` contains comment-prefixed documentation lines followed by a blank
///   separator when at least one doc block is present.
///
/// Transformation:
/// - Splits multiline documentation blocks into physical lines and prefixes
///   every line with `//!` or `///` so generated `.typi` files remain valid
///   Terlan interface source.
fn push_doc_lines(out: &mut String, marker: &str, docs: &[String]) {
    for block in docs {
        for line in block.lines() {
            out.push_str(&format!("//{} {}\n", marker, line));
        }
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
        resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
        ModuleInterface, TraitConformanceSource,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use terlan_syntax::cached_canonical_terlan_syntax_contract;
    use terlan_syntax::canonical_terlan_syntax_contract;
    use terlan_syntax::ebnf::EbnfGrammarExprKind;
    use terlan_syntax::parse_interface_module_as_syntax_output;
    use terlan_syntax::parse_module_as_syntax_output;
    use terlan_syntax::validate_syntax_contract;
    use terlan_syntax::SyntaxSourceKind;

    /// Verifies type-only imports support module-default type exports.
    ///
    /// Inputs:
    /// - A provider interface named `std.core.Task` that exports public opaque
    ///   type `Task`.
    /// - A consumer module using `import std.core.Task.` and an aliased form
    ///   `import std.core.Task as AsyncTask.`.
    ///
    /// Output:
    /// - Test passes when both local type names resolve to provider type
    ///   `std.core.Task.Task`.
    ///
    /// Transformation:
    /// - Parses the consumer through syntax output, resolves it against the
    ///   provider interface map, and checks that the resolver collapses the
    ///   repeated module/type name only when the provider module exports the
    ///   matching default type.
    #[test]
    fn type_import_resolves_module_default_type_export() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module std.core.Task.\n\
\n\
pub opaque type Task[T].\n",
        )
        .expect("parse task provider interface");
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "std.core.Task".to_string(),
            syntax_module_output_to_interface(&provider),
        );
        let consumer = parse_module_as_syntax_output(
            "\
module default_type_import_consumer.\n\
\n\
import std.core.Task.\n\
import std.core.Task as AsyncTask.\n\
\n\
pub identity(task: Task[Int]): AsyncTask[Int] ->\n\
    task.\n",
        )
        .expect("parse default type import consumer");

        let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;

        let task = resolved
            .imported_types
            .get("Task")
            .expect("default Task import");
        assert_eq!(task.source_module, "std.core.Task");
        assert_eq!(task.source_name, "Task");
        let async_task = resolved
            .imported_types
            .get("AsyncTask")
            .expect("aliased default Task import");
        assert_eq!(async_task.source_module, "std.core.Task");
        assert_eq!(async_task.source_name, "Task");
        assert!(
            resolved.diagnostics.is_empty(),
            "unexpected default type import diagnostics: {:?}",
            resolved.diagnostics
        );
    }

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
        let source_path = test_core.join("result_test.terl");
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

    /// Verifies release collection summaries load through std discovery.
    ///
    /// Inputs:
    /// - A temporary workspace containing a `std/summaries` directory populated
    ///   from the release Map/List/Set `.typi` summaries.
    /// - A source file path under the same temporary workspace.
    ///
    /// Output:
    /// - Test passes when `load_interfaces_from_file_set` discovers all three
    ///   collection interfaces and preserves receiver-method mutability.
    ///
    /// Transformation:
    /// - Writes release summaries into a throwaway std tree, runs the normal
    ///   interface discovery algorithm, and checks the resulting module
    ///   interfaces through the same path external projects use.
    #[test]
    fn std_interface_loading_discovers_release_core_collection_contracts() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan_hir_collection_summaries_{}_{}",
            std::process::id(),
            nanos
        ));
        let source_dir = root.join("src/app");
        let summaries = root.join("std/summaries");
        fs::create_dir_all(&source_dir).expect("create source fixture");
        fs::create_dir_all(&summaries).expect("create summaries fixture");
        let source_path = source_dir.join("Main.terl");
        fs::write(&source_path, "module app.Main.\n").expect("write source fixture");

        for (file_name, text) in [
            (
                "std.collections.Map.typi",
                include_str!("../../../std/summaries/std.collections.Map.typi"),
            ),
            (
                "std.collections.List.typi",
                include_str!("../../../std/summaries/std.collections.List.typi"),
            ),
            (
                "std.collections.Set.typi",
                include_str!("../../../std/summaries/std.collections.Set.typi"),
            ),
        ] {
            fs::write(summaries.join(file_name), text)
                .unwrap_or_else(|err| panic!("write {file_name}: {err}"));
        }

        let interfaces = load_interfaces_from_file_set(
            source_path
                .to_str()
                .expect("temporary source path should be utf-8"),
        );
        let _ = fs::remove_dir_all(&root);

        assert_collection_summary_signature(
            &interfaces,
            "std.collections.Map",
            "put",
            3,
            "Unit",
            "map",
            "Map[K, V]",
            true,
            true,
        );
        assert_collection_summary_signature(
            &interfaces,
            "std.collections.List",
            "clear",
            1,
            "Unit",
            "list",
            "List[T]",
            true,
            true,
        );
        assert_collection_summary_signature(
            &interfaces,
            "std.collections.Set",
            "add",
            2,
            "Unit",
            "set",
            "Set[T]",
            true,
            true,
        );
    }

    /// Verifies release iterator/iterable summaries load through std discovery.
    ///
    /// Inputs:
    /// - A temporary workspace containing a `std/summaries` directory populated
    ///   from the release Iterator/Iterable `.typi` summaries.
    /// - A source file path under the same temporary workspace.
    ///
    /// Output:
    /// - Test passes when `load_interfaces_from_file_set` discovers both
    ///   interfaces and preserves `Iterator.next` plus `Iterable.iterator`.
    ///
    /// Transformation:
    /// - Writes release summaries into a throwaway std tree, runs normal
    ///   interface discovery, and checks the resulting module interfaces
    ///   from the checked-in release std summaries.
    #[test]
    fn std_interface_loading_discovers_release_traversal_contracts() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan_hir_collection_trait_summaries_{}_{}",
            std::process::id(),
            nanos
        ));
        let source_dir = root.join("src/app");
        let summaries = root.join("std/summaries");
        fs::create_dir_all(&source_dir).expect("create source fixture");
        fs::create_dir_all(&summaries).expect("create summaries fixture");
        let source_path = source_dir.join("Main.terl");
        fs::write(&source_path, "module app.Main.\n").expect("write source fixture");

        for (file_name, text) in [
            (
                "std.collections.Iterator.typi",
                include_str!("../../../std/summaries/std.collections.Iterator.typi"),
            ),
            (
                "std.collections.Iterable.typi",
                include_str!("../../../std/summaries/std.collections.Iterable.typi"),
            ),
        ] {
            fs::write(summaries.join(file_name), text)
                .unwrap_or_else(|err| panic!("write {file_name}: {err}"));
        }

        let interfaces = load_interfaces_from_file_set(
            source_path
                .to_str()
                .expect("temporary source path should be utf-8"),
        );
        let _ = fs::remove_dir_all(&root);

        assert_collection_summary_signature(
            &interfaces,
            "std.collections.Iterator",
            "next",
            1,
            "Option[Step[T]]",
            "iterator",
            "Iterator[T]",
            false,
            false,
        );
        assert_trait_method_signature(
            &interfaces,
            "std.collections.Iterable",
            "Iterable",
            "iterator",
            "Iterator[T]",
            "collection",
            "C",
        );
    }

    /// Asserts one loaded collection summary function signature.
    ///
    /// Inputs:
    /// - `interfaces`: discovered module interfaces keyed by module name.
    /// - `module_name`: expected collection module name.
    /// - `function_name`: expected function/method name.
    /// - `arity`: expected receiver-first callable arity.
    /// - `return_type`: expected normalized return type text.
    /// - `receiver_name`: expected receiver parameter name.
    /// - `receiver_type`: expected normalized receiver annotation text.
    /// - `receiver_method`: expected receiver-method syntax marker.
    /// - `receiver_mutable`: expected receiver mutability marker.
    ///
    /// Output:
    /// - Panics when the interface, function, return type, receiver-first
    ///   parameter shape, or receiver mutability does not match.
    ///
    /// Transformation:
    /// - Reads a function signature from an already loaded interface and
    ///   compares the receiver-first shape plus mutability metadata used by
    ///   downstream compiler phases.
    fn assert_collection_summary_signature(
        interfaces: &HashMap<String, ModuleInterface>,
        module_name: &str,
        function_name: &str,
        arity: usize,
        return_type: &str,
        receiver_name: &str,
        receiver_type: &str,
        receiver_method: bool,
        receiver_mutable: bool,
    ) {
        let interface = interfaces
            .get(module_name)
            .unwrap_or_else(|| panic!("missing interface {module_name}"));
        let signature = interface
            .functions
            .get(&(function_name.to_string(), arity))
            .unwrap_or_else(|| panic!("missing signature {module_name}.{function_name}/{arity}"));

        assert_eq!(signature.return_type, return_type);
        assert_eq!(signature.params[0].name, receiver_name);
        assert_eq!(signature.params[0].annotation, receiver_type);
        assert_eq!(signature.receiver_method, receiver_method);
        assert_eq!(signature.receiver_mutable, receiver_mutable);
    }

    /// Asserts one loaded trait method signature.
    ///
    /// Inputs:
    /// - `interfaces`: discovered module interfaces keyed by module name.
    /// - `module_name`: expected module containing the trait.
    /// - `trait_name`: expected trait name.
    /// - `method_name`: expected trait method name.
    /// - `return_type`: expected normalized method return type.
    /// - `param_name`: expected first parameter name.
    /// - `param_type`: expected first parameter annotation.
    ///
    /// Output:
    /// - Panics when the interface, trait, method, return type, or parameter
    ///   shape does not match.
    ///
    /// Transformation:
    /// - Reads a trait method signature from an already loaded interface and
    ///   compares the shape used by downstream conformance checks.
    fn assert_trait_method_signature(
        interfaces: &HashMap<String, ModuleInterface>,
        module_name: &str,
        trait_name: &str,
        method_name: &str,
        return_type: &str,
        param_name: &str,
        param_type: &str,
    ) {
        let interface = interfaces
            .get(module_name)
            .unwrap_or_else(|| panic!("missing interface {module_name}"));
        let trait_signature = interface
            .traits
            .get(trait_name)
            .unwrap_or_else(|| panic!("missing trait {module_name}.{trait_name}"));
        let method = trait_signature
            .methods
            .get(method_name)
            .unwrap_or_else(|| panic!("missing trait method {trait_name}.{method_name}"));

        assert_eq!(method.return_type, return_type);
        assert_eq!(method.params[0].name, param_name);
        assert_eq!(method.params[0].annotation, param_type);
    }

    /// Verifies interface snapshots preserve public trait conformance facts.
    ///
    /// Inputs:
    /// - A source module containing one declaration-site `implements`
    ///   conformance and one explicit `impl Trait[...] for Type` conformance.
    ///
    /// Output:
    /// - Test passes when both conformance facts appear in the direct interface
    ///   and survive rendering/parsing as `.typi` interface text.
    ///
    /// Transformation:
    /// - Converts syntax output to `ModuleInterface`, renders it as interface
    ///   text, reparses that text through the interface parser, and converts it
    ///   back to `ModuleInterface` to prove the metadata is stable.
    #[test]
    fn interface_rendering_preserves_public_trait_conformances() {
        let module = parse_module_as_syntax_output(
            "\
module interface_trait_conformance.\n\
\n\
pub trait Show[T] {\n\
    show(value: T): String.\n\
}.\n\
\n\
pub type User implements Show[User] = {name: String}.\n\
\n\
pub impl Show[Int] for Int {\n\
    show(value: Int): String ->\n\
        \"int\".\n\
}.\n",
        )
        .expect("parse conformance source fixture");

        let interface = syntax_module_output_to_interface(&module);
        assert_trait_conformance(
            &interface,
            "Show[User]",
            "User",
            TraitConformanceSource::Implements,
        );
        assert_trait_conformance(
            &interface,
            "Show[Int]",
            "Int",
            TraitConformanceSource::ExplicitImpl,
        );

        let rendered = interface.to_terlan_interface_text();
        assert!(
            rendered.contains("pub impl Show[User] for User"),
            "rendered interface should preserve declaration-site conformance:\n{}",
            rendered
        );
        assert!(
            rendered.contains("pub impl Show[Int] for Int"),
            "rendered interface should preserve explicit impl conformance:\n{}",
            rendered
        );

        let reparsed = parse_interface_module_as_syntax_output(&rendered)
            .expect("parse rendered conformance interface");
        let reparsed_interface = syntax_module_output_to_interface(&reparsed);
        assert_trait_conformance(
            &reparsed_interface,
            "Show[User]",
            "User",
            TraitConformanceSource::ExplicitImpl,
        );
        assert_trait_conformance(
            &reparsed_interface,
            "Show[Int]",
            "Int",
            TraitConformanceSource::ExplicitImpl,
        );
    }

    /// Verifies interface rendering preserves trait default-method markers.
    ///
    /// Inputs:
    /// - A public trait with one required method and one default method.
    ///
    /// Output:
    /// - Test passes when direct and rendered/reparsed interfaces mark only the
    ///   default method as having a default implementation.
    ///
    /// Transformation:
    /// - Converts source syntax to an interface, renders the `.typi` summary
    ///   with a placeholder default body, reparses that summary, and verifies
    ///   downstream interface extraction still sees the default marker.
    #[test]
    fn interface_rendering_preserves_trait_default_method_markers() {
        let module = parse_module_as_syntax_output(
            "\
module interface_trait_defaults.\n\
\n\
pub trait Lifecycle[T] {\n\
    start(value: T): T.\n\
    stop(value: T): Unit -> Unit.\n\
}.\n",
        )
        .expect("parse default trait method source fixture");

        let interface = syntax_module_output_to_interface(&module);
        let lifecycle = interface
            .traits
            .get("Lifecycle")
            .expect("direct lifecycle trait");
        assert!(!lifecycle.methods["start"].has_default);
        assert!(lifecycle.methods["stop"].has_default);

        let rendered = interface.to_terlan_interface_text();
        assert!(
            rendered.contains("stop(value: T): Unit ->"),
            "rendered summary should contain a placeholder default body:\n{}",
            rendered
        );

        let reparsed = parse_interface_module_as_syntax_output(&rendered)
            .expect("parse rendered default trait interface");
        let reparsed_interface = syntax_module_output_to_interface(&reparsed);
        let reparsed_lifecycle = reparsed_interface
            .traits
            .get("Lifecycle")
            .expect("reparsed lifecycle trait");
        assert!(!reparsed_lifecycle.methods["start"].has_default);
        assert!(reparsed_lifecycle.methods["stop"].has_default);
    }

    /// Verifies public struct fields survive `.typi` rendering and parsing.
    ///
    /// Inputs:
    /// - A syntax-output module containing one public struct with two fields.
    ///
    /// Output:
    /// - Test passes when direct and reparsed interfaces both expose the public
    ///   struct field signatures.
    ///
    /// Transformation:
    /// - Converts source to interface metadata, renders that metadata as
    ///   Terlan interface text, reparses it, and compares the resulting
    ///   span-free field signatures.
    #[test]
    fn interface_rendering_preserves_public_struct_fields() {
        let module = parse_module_as_syntax_output(
            "\
module interface_struct_fields.\n\
\n\
pub struct Error {\n\
    code: Atom,\n\
    message: String\n\
}.\n",
        )
        .expect("parse struct field source fixture");

        let interface = syntax_module_output_to_interface(&module);
        let fields = interface
            .struct_fields
            .get("Error")
            .expect("direct struct field metadata");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "code");
        assert_eq!(fields[0].annotation, "Atom");
        assert_eq!(fields[1].name, "message");
        assert_eq!(fields[1].annotation, "String");

        let rendered = interface.to_terlan_interface_text();
        assert!(
            rendered.contains("pub struct Error"),
            "rendered interface should preserve struct declaration:\n{}",
            rendered
        );
        let reparsed = parse_interface_module_as_syntax_output(&rendered)
            .expect("parse rendered struct interface");
        let reparsed_interface = syntax_module_output_to_interface(&reparsed);
        let reparsed_fields = reparsed_interface
            .struct_fields
            .get("Error")
            .expect("reparsed struct field metadata");
        assert_eq!(reparsed_fields, fields);
    }

    /// Verifies generated provider summaries with constructors and empty impls parse.
    ///
    /// Inputs:
    /// - Interface text matching a cached provider `.typi` summary for a module
    ///   with a public struct, constructor, trait, explicit impl, and function.
    ///
    /// Output:
    /// - Test passes when interface parsing and HIR extraction preserve the
    ///   provider module interface.
    ///
    /// Transformation:
    /// - Parses generated interface text and converts it back into a
    ///   `ModuleInterface`, catching cache summary shapes that would otherwise
    ///   be silently skipped by interface loading.
    #[test]
    fn generated_provider_interface_with_empty_impl_parses() {
        let source = "\
module people.Provider.\n\
\n\
pub type ExternalUser.\n\
\n\
pub new_user(name: String): ExternalUser.\n\
\n\
pub trait Named[T] {\n\
    name(value: T): String.\n\
}.\n\
\n\
pub impl Named[ExternalUser] for ExternalUser {\n\
}.\n\
\n\
pub constructor ExternalUser {\n\
    (name: String): ExternalUser ->\n\
        terlan_interface_constructor\n\
}.\n";

        let parsed = parse_interface_module_as_syntax_output(source)
            .expect("parse generated provider interface summary");
        let interface = syntax_module_output_to_interface(&parsed);

        assert_eq!(interface.module, "people.Provider");
        assert!(interface.public_types.contains("ExternalUser"));
        assert!(interface.traits.contains_key("Named"));
        assert_eq!(interface.trait_conformances.len(), 1);
        assert!(interface
            .functions
            .contains_key(&("new_user".to_string(), 1)));
    }

    /// Asserts one trait conformance fact exists in an interface snapshot.
    ///
    /// Inputs:
    /// - `interface`: module interface to inspect.
    /// - `trait_ref`: expected normalized trait reference.
    /// - `for_type`: expected normalized implementation type.
    /// - `source`: expected conformance source category.
    ///
    /// Output:
    /// - Panics when the conformance fact is missing.
    ///
    /// Transformation:
    /// - Performs an exact metadata lookup without inspecting source text.
    fn assert_trait_conformance(
        interface: &ModuleInterface,
        trait_ref: &str,
        for_type: &str,
        source: TraitConformanceSource,
    ) {
        assert!(
            interface.trait_conformances.iter().any(|conformance| {
                conformance.trait_ref == trait_ref
                    && conformance.for_type == for_type
                    && conformance.source == source
                    && conformance.public
            }),
            "missing conformance {trait_ref} for {for_type} via {:?}: {:?}",
            source,
            interface.trait_conformances
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
            .any(|diagnostic| diagnostic.message == "syntax rule Expr must reference AssignExpr"));
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
        assert_eq!(interface.constructors.get("Items").map(Vec::len), Some(1));
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

    /// Verifies release core collection contracts produce stable interfaces.
    ///
    /// Inputs:
    /// - Release source contracts for `std.collections.Map`, `std.collections.List`, and
    ///   `std.collections.Set`.
    /// - Matching release `.typi` summaries using bodyless receiver method
    ///   signatures.
    ///
    /// Output:
    /// - Test passes when source-contract extraction and summary parsing expose
    ///   the same key function arities, return types, and receiver mutability.
    ///
    /// Transformation:
    /// - Converts source and summary receiver methods into HIR's callable
    ///   `method(receiver, args...)` convention while preserving `mut`.
    #[test]
    fn hir_extracts_release_core_collection_contracts_as_receiver_first_interfaces() {
        let contracts = [
            (
                "std.collections.Map",
                include_str!("../../../std/collections/map.terl"),
                include_str!("../../../std/summaries/std.collections.Map.typi"),
                vec![
                    ("put", 3, "Unit", "map", "Map[K, V]", true),
                    ("remove", 2, "Unit", "map", "Map[K, V]", true),
                    ("clear", 1, "Unit", "map", "Map[K, V]", true),
                ],
            ),
            (
                "std.collections.List",
                include_str!("../../../std/collections/list.terl"),
                include_str!("../../../std/summaries/std.collections.List.typi"),
                vec![
                    ("push", 2, "Unit", "list", "List[T]", true),
                    ("clear", 1, "Unit", "list", "List[T]", true),
                ],
            ),
            (
                "std.collections.Set",
                include_str!("../../../std/collections/set.terl"),
                include_str!("../../../std/summaries/std.collections.Set.typi"),
                vec![
                    ("add", 2, "Unit", "set", "Set[T]", true),
                    ("remove", 2, "Unit", "set", "Set[T]", true),
                    ("clear", 1, "Unit", "set", "Set[T]", true),
                ],
            ),
        ];

        for (module_name, source, summary, expected_functions) in contracts {
            let source_module =
                parse_module_as_syntax_output(source).expect("parse release collection source");
            let summary_module = parse_interface_module_as_syntax_output(summary)
                .expect("parse release collection summary");
            let source_interface = syntax_module_output_to_interface(&source_module);
            let summary_interface = syntax_module_output_to_interface(&summary_module);

            assert_eq!(source_interface.module, module_name);
            assert_eq!(summary_interface.module, module_name);

            for (function_name, arity, return_type, receiver_name, receiver_type, mutable) in
                expected_functions
            {
                let key = (function_name.to_string(), arity);
                let source_signature = source_interface
                    .functions
                    .get(&key)
                    .unwrap_or_else(|| panic!("missing source signature {module_name}.{key:?}"));
                let summary_signature = summary_interface
                    .functions
                    .get(&key)
                    .unwrap_or_else(|| panic!("missing summary signature {module_name}.{key:?}"));

                assert_eq!(source_signature.return_type, return_type);
                assert_eq!(summary_signature.return_type, return_type);
                assert_eq!(source_signature.params[0].name, receiver_name);
                assert_eq!(summary_signature.params[0].name, receiver_name);
                assert_eq!(source_signature.params[0].annotation, receiver_type);
                assert_eq!(summary_signature.params[0].annotation, receiver_type);
                assert!(source_signature.receiver_method);
                assert!(summary_signature.receiver_method);
                assert_eq!(source_signature.receiver_mutable, mutable);
                assert_eq!(summary_signature.receiver_mutable, mutable);
            }
        }
    }

    /// Verifies release iterator/iterable contracts produce stable interfaces.
    ///
    /// Inputs:
    /// - Release interface contracts for `std.collections.Iterator` and
    ///   `std.collections.Iterable`.
    /// - Matching release `.typi` summaries.
    ///
    /// Output:
    /// - Test passes when source-contract extraction and summary parsing expose
    ///   the same key function and trait method signatures.
    ///
    /// Transformation:
    /// - Converts release interface syntax into HIR module interfaces and
    ///   compares those interfaces with the bodyless summaries planned for
    ///   later compiler phases.
    #[test]
    fn hir_extracts_release_traversal_contracts_as_interfaces() {
        let iterator_source =
            parse_module_as_syntax_output(include_str!("../../../std/collections/iterator.terl"))
                .expect("parse iterator source contract");
        let iterator_summary = parse_interface_module_as_syntax_output(include_str!(
            "../../../std/summaries/std.collections.Iterator.typi"
        ))
        .expect("parse iterator summary");
        let iterator_source_interface = syntax_module_output_to_interface(&iterator_source);
        let iterator_summary_interface = syntax_module_output_to_interface(&iterator_summary);

        assert_eq!(iterator_source_interface.module, "std.collections.Iterator");
        assert_eq!(
            iterator_summary_interface.module,
            "std.collections.Iterator"
        );
        assert_eq!(
            iterator_source_interface.functions[&("next".to_string(), 1)].return_type,
            "Option[Step[T]]"
        );
        assert_eq!(
            iterator_summary_interface.functions[&("next".to_string(), 1)].return_type,
            "Option[Step[T]]"
        );

        let iterable_source =
            parse_module_as_syntax_output(include_str!("../../../std/collections/iterable.terl"))
                .expect("parse iterable source contract");
        let iterable_summary = parse_interface_module_as_syntax_output(include_str!(
            "../../../std/summaries/std.collections.Iterable.typi"
        ))
        .expect("parse iterable summary");
        let iterable_source_interface = syntax_module_output_to_interface(&iterable_source);
        let iterable_summary_interface = syntax_module_output_to_interface(&iterable_summary);

        assert_eq!(iterable_source_interface.module, "std.collections.Iterable");
        assert_eq!(
            iterable_summary_interface.module,
            "std.collections.Iterable"
        );
        assert_eq!(
            iterable_source_interface.traits["Iterable"].methods["iterator"].return_type,
            "Iterator[T]"
        );
        assert_eq!(
            iterable_summary_interface.traits["Iterable"].methods["iterator"].return_type,
            "Iterator[T]"
        );
    }
}
