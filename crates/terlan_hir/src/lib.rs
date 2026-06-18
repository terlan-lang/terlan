use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use terlan_syntax::{
    extract_native_function_signatures, span::Span, SyntaxDeclarationOutput,
    SyntaxDeclarationPayload, SyntaxImportItem, SyntaxModuleOutput, SyntaxParamOutput,
    SyntaxSourceKind,
};

mod interface_render;

use interface_render::normalize_type_text;

#[derive(Debug, Clone)]
/// Public module interface used by downstream resolver/typecheck phases.
///
/// Inputs: syntax-output module or `.terli`/`.typi` summary. Output: importable
/// module surface. Transformation: strips implementation bodies while
/// preserving exported types, constructors, functions, traits, conformances,
/// docs, and overload metadata.
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
    pub function_overloads: HashMap<(String, usize), Vec<FunctionSignature>>,
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
/// Constructor signature exported through a module interface.
///
/// Inputs: syntax-output constructor clause. Output: callable constructor
/// signature. Transformation: records fixed parameters, optional vararg,
/// return type, body summary, arity policy, visibility, and docs.
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
/// Function or receiver-method signature exported through an interface.
///
/// Inputs: syntax-output function, method, or native config signature. Output:
/// callable signature metadata. Transformation: records normalized parameter
/// and return annotations plus receiver/visibility/doc metadata.
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
/// Trait signature exported through an interface.
///
/// Inputs: public syntax-output trait declaration. Output: trait methods and
/// inheritance metadata. Transformation: keeps type params, super traits,
/// method signatures, and docs without implementation bodies.
pub struct TraitSignature {
    pub name: String,
    pub type_params: Vec<String>,
    pub super_traits: Vec<String>,
    pub methods: HashMap<String, TraitMethodSignature>,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone)]
/// Trait method signature exported through an interface.
///
/// Inputs: syntax-output trait method declaration. Output: method signature.
/// Transformation: records params, return type, generic bounds, default-body
/// availability, and docs.
pub struct TraitMethodSignature {
    pub params: Vec<ParamSignature>,
    pub return_type: String,
    pub generic_bounds: Vec<String>,
    pub has_default: bool,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Trait conformance fact exported through an interface.
///
/// Inputs: declaration-site `implements` or explicit impl declaration. Output:
/// normalized conformance metadata. Transformation: records trait, owner type,
/// source category, and visibility for imported conformance checks.
pub struct TraitConformanceSignature {
    pub trait_ref: String,
    pub for_type: String,
    pub source: TraitConformanceSource,
    pub public: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// Source form that introduced a trait conformance.
///
/// Inputs: syntax declaration kind. Output: implements or explicit-impl tag.
/// Transformation: classifies conformance provenance without affecting backend
/// lowering.
pub enum TraitConformanceSource {
    Implements,
    ExplicitImpl,
}

#[derive(Debug, Clone)]
/// Parameter signature used by HIR interfaces.
///
/// Inputs: syntax-output parameter. Output: name, normalized annotation, and
/// mutability flag. Transformation: removes spans/defaults and preserves
/// callable type shape.
pub struct ParamSignature {
    pub name: String,
    pub annotation: String,
    pub is_mutable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Visibility of a type name in HIR.
///
/// Inputs: declaration visibility. Output: public or private tag.
/// Transformation: normalizes source visibility for import and export checks.
pub enum TypeVisibility {
    Public,
    Private,
}

#[derive(Debug, Clone)]
/// Function symbol resolved in the current module.
///
/// Inputs: syntax-output function or method declaration. Output: local symbol
/// table entry. Transformation: records callable shape, export/public flags,
/// docs, and source span for duplicate/export diagnostics.
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
/// Imported type or trait item resolved from an interface.
///
/// Inputs: import item and provider interface. Output: local import binding.
/// Transformation: records local alias, provider module/name, visibility, and
/// source span for duplicate diagnostics.
pub struct ImportedItem {
    pub local_name: String,
    pub source_module: String,
    pub source_name: String,
    pub visibility: TypeVisibility,
    pub span: Span,
}

#[derive(Debug, Clone)]
/// Fully resolved module summary.
///
/// Inputs: syntax-output module plus visible interfaces. Output: local symbols,
/// imports, generated interface, and diagnostics. Transformation: resolves
/// imports/exports/types/functions while preserving loaded interface metadata.
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
/// HIR diagnostic.
///
/// Inputs: resolver error condition. Output: span/message diagnostic.
/// Transformation: attaches HIR messages to source spans for later display.
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
}

#[derive(Debug)]
/// Resolver result wrapper.
///
/// Inputs: syntax-output module resolution. Output: resolved module.
/// Transformation: packages the resolved module for callers while leaving room
/// for future resolver metadata.
pub struct ResolveResult {
    pub module: ResolvedModule,
}

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
        let signature = FunctionSignature {
            name: symbol.name.clone(),
            params: symbol.params.clone(),
            return_type: symbol.return_type.clone(),
            receiver_method: symbol.receiver_method,
            receiver_mutable: symbol.receiver_mutable,
            public: symbol.public,
            docs: symbol.docs.clone(),
        };
        let key = (symbol.name.clone(), symbol.arity);
        interface.functions.insert(key.clone(), signature.clone());
        interface.function_overloads.insert(key, vec![signature]);
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

/// Parses one interface file into a module interface.
///
/// Inputs: path to `.terli` or `.typi`. Output: module name plus interface when
/// parsing succeeds. Transformation: reads source, parses interface syntax
/// output, and converts it to an interface summary.
pub fn parse_interface_file(path: &Path) -> Option<(String, ModuleInterface)> {
    let content = fs::read_to_string(path).ok()?;
    let parsed = terlan_syntax::parse_interface_module_as_syntax_output(&content).ok()?;
    let module_name = parsed.module_name.clone();
    let interface = syntax_module_output_to_interface(&parsed);
    Some((module_name, interface))
}

/// Loads interface summaries from one directory.
///
/// Inputs: directory path and accumulator. Output: accumulator is updated.
/// Transformation: reads direct `.terli` and `.typi` files and inserts richer
/// duplicate summaries preferentially.
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

/// Loads interfaces visible to one source file.
///
/// Inputs: source file path. Output: interface map. Transformation: scans the
/// source directory and nearest/std fallback trees for `.terli`/`.typi`
/// summaries.
pub fn load_interfaces_from_file_set(file_path: &str) -> HashMap<String, ModuleInterface> {
    let mut interfaces = HashMap::new();
    let current = Path::new(file_path);
    let base = current.parent().unwrap_or(Path::new("."));
    load_interfaces_from_dir(base, &mut interfaces);
    load_std_interfaces(current, &mut interfaces);
    interfaces
}

/// Loads standard-library interfaces visible from a source path.
///
/// Inputs: current source path and accumulator. Output: accumulator is updated.
/// Transformation: walks upward looking for a `std` tree, falling back to
/// `./std` from the current working directory.
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

/// Loads interfaces from a standard-library tree.
///
/// Inputs: std root and accumulator. Output: number of newly added interfaces.
/// Transformation: scans child directories for interface files using the same
/// directory loader as project sources.
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
                params,
                return_type,
                is_public,
                ..
            } => {
                let key = (name.clone(), params.len());
                let signature = FunctionSignature {
                    name: name.clone(),
                    params: syntax_param_signatures(params),
                    return_type: normalize_type_text(&return_type.text),
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
                params,
                return_type,
                is_public,
                ..
            } => {
                let signature_params = syntax_method_param_signatures(receiver, params);
                let key = (name.clone(), signature_params.len());
                let signature = FunctionSignature {
                    name: name.clone(),
                    params: signature_params,
                    return_type: normalize_type_text(&return_type.text),
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
        })
        .collect()
}

/// Resolves one syntax-output import declaration.
///
/// Inputs: module name, selected items, type-import flag, visible interfaces,
/// mutable import tables, and diagnostics sink. Output: import tables and
/// diagnostics are updated. Transformation: validates public type/trait/default
/// imports and records local aliases.
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
