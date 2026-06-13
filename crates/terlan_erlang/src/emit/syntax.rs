//! Formal syntax-output to Erlang lowering.
//!
//! This module owns the direct `SyntaxModuleOutput` bridge emitter used
//! by the CoreIR-gated Erlang backend while CoreIR executable payload coverage
//! is still being expanded. It lowers compiler-facing syntax output into the
//! internal Erlang render model without routing through the source AST adapter.

use super::*;
use terlan_typeck::CorePrimitiveIntrinsic;

pub(super) struct SyntaxLowerCtx {
    constructors: BTreeMap<String, Vec<SyntaxConstructorTarget>>,
    imported_constructor_targets: BTreeMap<String, Vec<SyntaxRemoteConstructorTarget>>,
    remote_constructor_targets: BTreeMap<String, Vec<SyntaxRemoteConstructorTarget>>,
    constructor_patterns: BTreeMap<String, Vec<SyntaxConstructorPatternTarget>>,
    alias_constructor_targets: BTreeMap<String, SyntaxAliasConstructorTarget>,
    remote_alias_constructor_targets: BTreeMap<String, SyntaxAliasConstructorTarget>,
    imported_functions: BTreeMap<(String, usize), (String, String)>,
    opaque_constructors: BTreeSet<String>,
    trait_method_wrappers: BTreeMap<String, BTreeMap<String, String>>,
    typed_trait_method_wrappers: BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>,
    generic_functions: BTreeMap<(String, usize), SyntaxGenericFunctionTarget>,
    local_function_values: BTreeMap<String, usize>,
    imported_trait_aliases: BTreeMap<String, (String, String)>,
    imported_trait_conformances: BTreeMap<String, BTreeMap<String, String>>,
    imported_type_refs: BTreeMap<String, String>,
    local_trait_methods: BTreeMap<String, BTreeSet<String>>,
    receiver_methods: BTreeMap<(String, usize), BTreeMap<String, SyntaxReceiverMethodTarget>>,
    module_aliases: BTreeMap<String, String>,
    file_imports: BTreeMap<String, Vec<u8>>,
    markdown_imports: BTreeMap<String, terlan_html::MarkdownDocument>,
    templates: BTreeMap<String, LowerTemplate>,
    struct_field_types: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone)]
pub(super) struct SyntaxReceiverMethodTarget {
    pub(super) mutable: bool,
}

#[derive(Debug, Clone)]
struct SyntaxGenericFunctionTarget {
    params: Vec<String>,
    bounds: Vec<SyntaxGenericFunctionBound>,
}

#[derive(Debug, Clone)]
struct SyntaxGenericFunctionBound {
    trait_name: String,
    type_args: Vec<String>,
}

#[derive(Clone, Default)]
struct SyntaxLowerEnv {
    struct_locals: BTreeMap<String, String>,
    value_locals: BTreeSet<String>,
    value_types: BTreeMap<String, String>,
    trait_bound_dicts: BTreeMap<(String, String), String>,
    value_replacements: BTreeMap<String, ErlExpr>,
}

impl SyntaxLowerCtx {
    fn empty() -> Self {
        Self {
            constructors: BTreeMap::new(),
            imported_constructor_targets: BTreeMap::new(),
            remote_constructor_targets: BTreeMap::new(),
            constructor_patterns: BTreeMap::new(),
            alias_constructor_targets: BTreeMap::new(),
            remote_alias_constructor_targets: BTreeMap::new(),
            imported_functions: BTreeMap::new(),
            opaque_constructors: BTreeSet::new(),
            trait_method_wrappers: BTreeMap::new(),
            typed_trait_method_wrappers: BTreeMap::new(),
            generic_functions: BTreeMap::new(),
            local_function_values: BTreeMap::new(),
            imported_trait_aliases: BTreeMap::new(),
            imported_trait_conformances: BTreeMap::new(),
            imported_type_refs: BTreeMap::new(),
            local_trait_methods: BTreeMap::new(),
            receiver_methods: BTreeMap::new(),
            module_aliases: BTreeMap::new(),
            file_imports: BTreeMap::new(),
            markdown_imports: BTreeMap::new(),
            templates: BTreeMap::new(),
            struct_field_types: BTreeMap::new(),
        }
    }

    pub(super) fn new(
        module: &SyntaxModuleOutput,
        interfaces: &BTreeMap<String, ModuleInterface>,
        file_imports: &BTreeMap<String, Vec<u8>>,
        templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
        markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    ) -> Self {
        let module_aliases = module
            .declarations
            .iter()
            .flat_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Import {
                    import_kind: SyntaxImportKind::Module,
                    module_name,
                    items,
                    is_type: false,
                    ..
                } => {
                    let aliases = items
                        .iter()
                        .filter_map(|item| {
                            let target = format!("{}.{}", module_name, item.name);
                            if let Some(alias) = &item.as_alias {
                                return Some((alias.clone(), target));
                            }
                            is_upper_identifier(&item.name).then(|| (item.name.clone(), target))
                        })
                        .collect::<Vec<_>>();
                    aliases
                }
                _ => Vec::new(),
            })
            .collect();

        let imported_trait_aliases = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Import {
                    import_kind: SyntaxImportKind::Module,
                    module_name,
                    items,
                    ..
                } => Some((module_name, items, interfaces.get(module_name)?)),
                _ => None,
            })
            .flat_map(|(module_name, items, interface)| {
                items
                    .iter()
                    .filter_map(|item| {
                        if !interface.traits.contains_key(&item.name) {
                            return None;
                        }
                        Some((
                            item.as_alias.clone().unwrap_or_else(|| item.name.clone()),
                            (module_name.clone(), item.name.clone()),
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let imported_trait_conformances = collect_imported_trait_conformances(module, interfaces);
        let imported_type_refs = collect_imported_type_refs(module, interfaces);

        let imported_functions = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Import {
                    import_kind: SyntaxImportKind::Module,
                    module_name,
                    items,
                    is_type: false,
                    ..
                } => Some((module_name, items, interfaces.get(module_name)?)),
                _ => None,
            })
            .flat_map(|(module_name, items, interface)| {
                items
                    .iter()
                    .flat_map(|item| {
                        let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
                        interface
                            .functions
                            .iter()
                            .filter(move |((name, _), signature)| {
                                name == &item.name && signature.public
                            })
                            .map(move |((_, arity), _)| {
                                (
                                    (local_name.clone(), *arity),
                                    (module_name.clone(), item.name.clone()),
                                )
                            })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let constructors = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Constructor { name, clauses, .. } => Some((
                    name.clone(),
                    clauses
                        .iter()
                        .map(|clause| {
                            let fixed_arity = clause
                                .params
                                .iter()
                                .filter(|param| !param.is_varargs)
                                .count();
                            let min_arity = clause
                                .params
                                .iter()
                                .filter(|param| !param.is_varargs && param.default.is_none())
                                .count();
                            let defaults = clause
                                .params
                                .iter()
                                .filter(|param| !param.is_varargs)
                                .map(|param| param.default.clone())
                                .collect();
                            let varargs = clause.params.iter().any(|param| param.is_varargs);
                            SyntaxConstructorTarget {
                                function: constructor_function_name(name, fixed_arity, varargs),
                                fixed_arity,
                                min_arity,
                                defaults,
                                varargs,
                            }
                        })
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            })
            .collect();

        let constructor_patterns = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Constructor { name, clauses, .. } => Some((
                    name.clone(),
                    clauses
                        .iter()
                        .filter(|clause| !clause.params.iter().any(|param| param.is_varargs))
                        .map(|clause| SyntaxConstructorPatternTarget {
                            params: clause
                                .params
                                .iter()
                                .map(|param| param.name.clone())
                                .collect(),
                            body: clause.body.clone(),
                        })
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            })
            .collect();

        let remote_constructor_targets = interfaces
            .iter()
            .flat_map(|(module_name, interface)| {
                interface
                    .constructors
                    .iter()
                    .flat_map(|(name, signatures)| {
                        let targets = signatures
                            .iter()
                            .filter(|signature| signature.public)
                            .map(|signature| {
                                syntax_remote_constructor_target_from_signature(
                                    module_name,
                                    name,
                                    signature,
                                )
                            })
                            .collect::<Vec<_>>();
                        if targets.is_empty() {
                            None
                        } else {
                            Some((format!("{}.{}", module_name, name), targets))
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let imported_constructor_targets = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Import {
                    import_kind: SyntaxImportKind::Module,
                    module_name,
                    items,
                    ..
                } => Some((module_name, items, interfaces.get(module_name)?)),
                _ => None,
            })
            .flat_map(|(module_name, items, interface)| {
                items
                    .iter()
                    .filter_map(|item| {
                        let signatures = interface.constructors.get(&item.name)?;
                        let targets = signatures
                            .iter()
                            .filter(|signature| signature.public)
                            .map(|signature| {
                                syntax_remote_constructor_target_from_signature(
                                    module_name,
                                    &item.name,
                                    signature,
                                )
                            })
                            .collect::<Vec<_>>();
                        if targets.is_empty() {
                            None
                        } else {
                            Some((
                                item.as_alias.clone().unwrap_or_else(|| item.name.clone()),
                                targets,
                            ))
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let mut alias_constructor_targets = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Type {
                    name,
                    variants,
                    is_opaque: false,
                    ..
                } => parse_syntax_type_alias_constructor_target_texts(
                    &variants
                        .iter()
                        .map(|variant| variant.text.clone())
                        .collect::<Vec<_>>(),
                )
                .map(|target| (name.clone(), target)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();

        for decl in &module.declarations {
            let SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                items,
                ..
            } = &decl.payload
            else {
                continue;
            };
            let Some(interface) = interfaces.get(module_name) else {
                continue;
            };
            for item in items {
                if !is_custom_type_name(&item.name)
                    || interface.constructors.contains_key(&item.name)
                {
                    continue;
                }
                let Some(type_body) = interface.type_bodies.get(&item.name) else {
                    continue;
                };
                let Some(target) = parse_syntax_type_alias_constructor_target_texts(type_body)
                else {
                    continue;
                };
                let alias = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
                alias_constructor_targets.insert(alias, target);
            }
        }

        let remote_alias_constructor_targets = interfaces
            .iter()
            .flat_map(|(module_name, interface)| {
                interface
                    .type_bodies
                    .iter()
                    .filter(|(name, _)| !interface.constructors.contains_key(*name))
                    .filter_map(|(name, variants)| {
                        parse_syntax_type_alias_constructor_target_texts(variants)
                            .map(|target| (format!("{}.{}", module_name, name), target))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let opaque_constructors = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Type {
                    name,
                    is_opaque: true,
                    ..
                } => Some(name.clone()),
                _ => None,
            })
            .collect();

        let templates = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Template { name, props, .. } = &decl.payload else {
                    return None;
                };
                let parsed = templates.get(name)?;
                Some((
                    name.clone(),
                    LowerTemplate {
                        nodes: parsed.nodes.clone(),
                        props: props
                            .iter()
                            .map(|prop| (prop.name.clone(), prop.annotation.text.clone()))
                            .collect(),
                    },
                ))
            })
            .collect();

        let struct_field_types = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Struct { name, fields, .. } = &decl.payload else {
                    return None;
                };
                Some((
                    name.clone(),
                    fields
                        .iter()
                        .map(|field| (field.name.clone(), field.annotation.text.clone()))
                        .collect::<BTreeMap<_, _>>(),
                ))
            })
            .collect();

        let local_trait_methods = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Trait { name, methods, .. } = &decl.payload else {
                    return None;
                };
                Some((
                    name.clone(),
                    methods
                        .iter()
                        .map(|method| method.name.clone())
                        .collect::<BTreeSet<_>>(),
                ))
            })
            .collect();

        let typed_trait_method_wrappers = collect_syntax_typed_trait_method_wrappers(module);
        let generic_functions = collect_syntax_generic_functions(module);
        let local_function_values = collect_syntax_local_function_values(module);

        let receiver_methods = module
            .declarations
            .iter()
            .filter_map(|decl| {
                let SyntaxDeclarationPayload::Method {
                    receiver,
                    name,
                    params,
                    ..
                } = &decl.payload
                else {
                    return None;
                };
                Some((
                    (name.clone(), params.len()),
                    normalize_trait_type_text(&receiver.annotation.text),
                    SyntaxReceiverMethodTarget {
                        mutable: receiver.is_mutable,
                    },
                ))
            })
            .fold(
                BTreeMap::<(String, usize), BTreeMap<String, SyntaxReceiverMethodTarget>>::new(),
                |mut methods, (key, receiver_type, target)| {
                    methods
                        .entry(key)
                        .or_default()
                        .insert(receiver_type, target);
                    methods
                },
            );

        Self {
            constructors,
            imported_constructor_targets,
            remote_constructor_targets,
            constructor_patterns,
            alias_constructor_targets,
            remote_alias_constructor_targets,
            imported_functions,
            opaque_constructors,
            trait_method_wrappers: BTreeMap::new(),
            typed_trait_method_wrappers,
            generic_functions,
            local_function_values,
            imported_trait_aliases,
            imported_trait_conformances,
            imported_type_refs,
            local_trait_methods,
            receiver_methods,
            module_aliases,
            file_imports: file_imports.clone(),
            markdown_imports: markdown_imports.clone(),
            templates,
            struct_field_types,
        }
    }

    fn resolve_remote_module(&self, module: &str) -> String {
        self.module_aliases
            .get(module)
            .cloned()
            .unwrap_or_else(|| module.to_string())
    }

    /// Resolves a selected imported function call target.
    ///
    /// Inputs:
    /// - `name`: local function name or import alias used at the call site.
    /// - `arity`: number of call arguments.
    ///
    /// Output:
    /// - Imported source module and source function name when the local call
    ///   resolves to a selected public function import.
    ///
    /// Transformation:
    /// - Looks up the local name/arity pair captured from import declarations
    ///   and interface signatures without rewriting local functions or remote
    ///   call syntax.
    fn imported_function_target(&self, name: &str, arity: usize) -> Option<(&String, &String)> {
        self.imported_functions
            .get(&(name.to_string(), arity))
            .map(|(module, function)| (module, function))
    }

    /// Resolves metadata for a bounded generic local function.
    ///
    /// Inputs:
    /// - `name`: local function name used at the call site.
    /// - `arity`: source-visible argument count, excluding hidden dictionaries.
    ///
    /// Output:
    /// - Generic function metadata when the local function declares one or more
    ///   trait bounds.
    ///
    /// Transformation:
    /// - Looks up the source-visible function key captured from formal syntax
    ///   output without exposing hidden backend dictionary parameters to source
    ///   callers.
    fn generic_function_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxGenericFunctionTarget> {
        self.generic_functions.get(&(name.to_string(), arity))
    }

    fn alias_constructor_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxAliasConstructorTarget> {
        self.alias_constructor_targets
            .get(name)
            .filter(|target| target.params.len() == arity)
    }

    /// Resolves a transparent singleton alias value by source name.
    ///
    /// Inputs:
    /// - `name`: local source name from a bare variable expression.
    ///
    /// Output:
    /// - The zero-payload alias target when `name` represents a singleton atom
    ///   value such as `None` or `Unit`.
    /// - `None` when the alias carries associated values or is not present.
    ///
    /// Transformation:
    /// - Looks up the existing alias-constructor target table but restricts
    ///   bare value lowering to targets with no parameters.
    fn singleton_alias_value_target(&self, name: &str) -> Option<&SyntaxAliasConstructorTarget> {
        self.alias_constructor_targets
            .get(name)
            .filter(|target| target.params.is_empty())
    }

    /// Resolves an alias call target for constructor syntax.
    ///
    /// Inputs:
    /// - `name`: local call head.
    /// - `arity`: number of supplied call arguments.
    ///
    /// Output:
    /// - The alias target when constructor syntax carries associated values.
    /// - `None` for zero-payload aliases so `None()` and `Unit()` do not lower.
    ///
    /// Transformation:
    /// - Reuses alias target metadata while enforcing that call syntax is only
    ///   for associated values, not singleton atom aliases.
    fn alias_constructor_call_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxAliasConstructorTarget> {
        self.alias_constructor_target(name, arity)
            .filter(|target| !target.params.is_empty())
    }

    fn remote_alias_constructor_target(
        &self,
        module: &str,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxAliasConstructorTarget> {
        let key = format!("{}.{}", self.resolve_remote_module(module), name);
        self.remote_alias_constructor_targets
            .get(&key)
            .filter(|target| target.params.len() == arity)
    }

    fn constructor_target(&self, name: &str, arity: usize) -> Option<&SyntaxConstructorTarget> {
        self.constructors.get(name)?.iter().find(|target| {
            if target.varargs {
                arity >= target.fixed_arity
            } else {
                arity >= target.min_arity && arity <= target.fixed_arity
            }
        })
    }

    fn imported_constructor_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxRemoteConstructorTarget> {
        self.imported_constructor_targets
            .get(name)?
            .iter()
            .find(|target| target.accepts_arity(arity))
    }

    fn remote_constructor_target(
        &self,
        module: &str,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxRemoteConstructorTarget> {
        let key = format!("{}.{}", self.resolve_remote_module(module), name);
        self.remote_constructor_targets
            .get(&key)?
            .iter()
            .find(|target| target.accepts_arity(arity))
    }

    fn constructor_pattern_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxConstructorPatternTarget> {
        self.constructor_patterns
            .get(name)?
            .iter()
            .find(|target| target.params.len() == arity)
    }

    fn trait_method_wrapper(&self, trait_name: &str, method: &str) -> Option<&String> {
        let key = trait_name.trim_matches('.').trim();
        if let Some(wrapper) = self
            .trait_method_wrappers
            .get(key)
            .and_then(|methods| methods.get(method))
        {
            return Some(wrapper);
        }
        let key = key.rsplit('.').next().unwrap_or(key);
        self.trait_method_wrappers
            .get(key)
            .and_then(|methods| methods.get(method))
    }

    /// Returns the generated wrapper for a typed trait-method implementation.
    ///
    /// Inputs:
    /// - `trait_name`: trait name from a local or imported trait-method call.
    /// - `method`: trait method name.
    /// - `type_arg`: normalized concrete implementation type.
    ///
    /// Output:
    /// - `Some(wrapper_name)` when the trait, method, and type argument resolve
    ///   to a typed implementation wrapper.
    /// - `None` when the call must fall back to the untyped dispatch path.
    ///
    /// Transformation:
    /// - Looks up the exact trait name first, then falls back to the final
    ///   qualified segment so source-qualified names can share local wrappers.
    fn typed_trait_method_wrapper(
        &self,
        trait_name: &str,
        method: &str,
        type_arg: &str,
    ) -> Option<&String> {
        let key = trait_name.trim_matches('.').trim();
        if let Some(wrapper) = self
            .typed_trait_method_wrappers
            .get(key)
            .and_then(|methods| methods.get(method))
            .and_then(|types| types.get(type_arg))
        {
            return Some(wrapper);
        }
        let key = key.rsplit('.').next().unwrap_or(key);
        self.typed_trait_method_wrappers
            .get(key)
            .and_then(|methods| methods.get(method))
            .and_then(|types| types.get(type_arg))
    }

    /// Returns whether a local trait declares a method name.
    ///
    /// Inputs:
    /// - `trait_name`: source-visible local trait name.
    /// - `method`: source-visible trait method name.
    ///
    /// Output:
    /// - `true` when the current module declares the trait and method.
    ///
    /// Transformation:
    /// - Performs a syntax-output inventory lookup so backend lowering only
    ///   rewrites calls that are actually trait-shaped in source.
    fn has_local_trait_method(&self, trait_name: &str, method: &str) -> bool {
        self.local_trait_methods
            .get(trait_name)
            .is_some_and(|methods| methods.contains(method))
    }

    /// Returns provider metadata for an imported trait alias.
    ///
    /// Inputs:
    /// - `name`: local trait alias used as the remote call head, or the
    ///   qualified selected-import spelling produced by module alias
    ///   resolution.
    ///
    /// Output:
    /// - Provider module and provider-local trait name when the call head
    ///   identifies an imported trait.
    /// - `None` when the call head is not an imported trait.
    ///
    /// Transformation:
    /// - Looks up the call head directly first, then falls back to its final
    ///   dotted segment so imports such as `std.collections.Enumerable.{Enumerable}`
    ///   still dispatch as traits after uppercase selected-import alias
    ///   qualification.
    fn imported_trait_alias(&self, name: &str) -> Option<(&str, &str)> {
        let key = name.trim_matches('.').trim();
        if let Some((module, source_name)) = self.imported_trait_aliases.get(key) {
            return Some((module.as_str(), source_name.as_str()));
        }
        let key = key.rsplit('.').next().unwrap_or(key);
        self.imported_trait_aliases
            .get(key)
            .map(|(module, source_name)| (module.as_str(), source_name.as_str()))
    }

    /// Returns the provider-local wrapper type for an imported conformance.
    ///
    /// Inputs:
    /// - `trait_name`: local imported trait name or alias used at the call site.
    /// - `type_arg`: normalized concrete first-argument type.
    ///
    /// Output:
    /// - Provider-local type key used in the remote wrapper function name.
    /// - `None` when provider interface metadata exposes no public conformance.
    ///
    /// Transformation:
    /// - Looks up by the consumer-qualified type key while returning the
    ///   provider-local type key, because remote wrapper symbols are generated
    ///   in the provider module's local namespace.
    fn imported_trait_conformance_wrapper_type(
        &self,
        trait_name: &str,
        type_arg: &str,
    ) -> Option<&str> {
        let key = trait_name.trim_matches('.').trim();
        if let Some(wrapper) = self
            .imported_trait_conformances
            .get(key)
            .and_then(|types| types.get(type_arg))
        {
            return Some(wrapper.as_str());
        }
        let key = key.rsplit('.').next().unwrap_or(key);
        self.imported_trait_conformances
            .get(key)
            .and_then(|types| types.get(type_arg))
            .map(String::as_str)
    }

    /// Returns metadata for a local receiver-method declaration.
    ///
    /// Inputs:
    /// - `receiver_type`: normalized type key inferred for the call receiver.
    /// - `method`: source method name.
    /// - `arity`: number of non-receiver call arguments.
    ///
    /// Output:
    /// - Receiver-method metadata when the current module declares the selected
    ///   receiver type, method name, and non-receiver arity.
    /// - `None` when no matching receiver method exists.
    ///
    /// Transformation:
    /// - Performs the same exact inventory lookup as `has_receiver_method`, but
    ///   keeps the receiver mutability bit available to backend rebinding
    ///   lowering without reparsing declarations.
    pub(super) fn receiver_method_target(
        &self,
        receiver_type: &str,
        method: &str,
        arity: usize,
    ) -> Option<&SyntaxReceiverMethodTarget> {
        self.receiver_methods
            .get(&(method.to_string(), arity))
            .and_then(|receivers| receivers.get(receiver_type))
    }
}

/// Collects typed trait-method wrapper names for explicit impl declarations.
///
/// Inputs:
/// - `module`: syntax-output module containing zero or more
///   `impl Trait[...] for Type` declarations.
///
/// Output:
/// - Map from trait name, method name, and concrete implementation type to the
///   generated Erlang wrapper function name.
///
/// Transformation:
/// - Reads only structured syntax output, extracts the head trait name and
///   normalized `for` type, and assigns deterministic wrapper names so
///   `Trait.method(value)` dispatch can resolve before ordinary remote calls.
fn collect_syntax_typed_trait_method_wrappers(
    module: &SyntaxModuleOutput,
) -> BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>> {
    let mut wrappers = BTreeMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            methods,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let Some(trait_name) = syntax_type_head_name(&trait_ref.text) else {
            continue;
        };
        let type_arg = normalize_trait_type_text(&for_type.text);

        for method in methods {
            wrappers
                .entry(trait_name.clone())
                .or_insert_with(BTreeMap::new)
                .entry(method.name.clone())
                .or_insert_with(BTreeMap::new)
                .insert(
                    type_arg.clone(),
                    typed_trait_method_wrapper_name(&trait_name, &method.name, &type_arg),
                );
        }
    }

    wrappers
}

/// Collects bounded generic local functions.
///
/// Inputs:
/// - `module`: syntax-output module containing function declarations.
///
/// Output:
/// - Map keyed by source-visible `(function name, arity)` with parameter type
///   annotations and parsed trait bounds.
///
/// Transformation:
/// - Reads `generic_bounds` from formal syntax output and stores only bounds
///   that parse as named trait applications, enabling backend call lowering to
///   synthesize hidden trait dictionaries for concrete local calls.
fn collect_syntax_generic_functions(
    module: &SyntaxModuleOutput,
) -> BTreeMap<(String, usize), SyntaxGenericFunctionTarget> {
    module
        .declarations
        .iter()
        .filter_map(|decl| {
            let SyntaxDeclarationPayload::Function {
                name,
                params,
                generic_bounds,
                ..
            } = &decl.payload
            else {
                return None;
            };
            if generic_bounds.is_empty() {
                return None;
            }
            let bounds = generic_bounds
                .iter()
                .filter_map(|bound| parse_syntax_generic_function_bound(bound))
                .collect::<Vec<_>>();
            if bounds.is_empty() {
                return None;
            }
            Some((
                (name.clone(), params.len()),
                SyntaxGenericFunctionTarget {
                    params: params
                        .iter()
                        .map(|param| normalize_trait_type_text(&param.annotation.text))
                        .collect(),
                    bounds,
                },
            ))
        })
        .collect()
}

/// Collects local functions that can be captured as first-class values.
///
/// Inputs:
/// - `module`: syntax-output module containing function declarations.
///
/// Output:
/// - Map from source function name to its single source arity.
///
/// Transformation:
/// - Scans non-generic local functions and keeps only names that appear with
///   exactly one arity. Multi-clause functions with the same arity remain
///   capturable, while overloaded names and generic-bound functions are left
///   out so backend lowering cannot pick the wrong BEAM function reference.
fn collect_syntax_local_function_values(module: &SyntaxModuleOutput) -> BTreeMap<String, usize> {
    let mut arities_by_name = BTreeMap::<String, BTreeSet<usize>>::new();
    for decl in &module.declarations {
        let SyntaxDeclarationPayload::Function {
            name,
            params,
            generic_bounds,
            ..
        } = &decl.payload
        else {
            continue;
        };
        if !generic_bounds.is_empty() {
            continue;
        }
        arities_by_name
            .entry(name.clone())
            .or_default()
            .insert(params.len());
    }

    arities_by_name
        .into_iter()
        .filter_map(|(name, arities)| {
            if arities.len() == 1 {
                arities.iter().next().copied().map(|arity| (name, arity))
            } else {
                None
            }
        })
        .collect()
}

/// Parses one generic function trait-bound reference.
///
/// Inputs:
/// - `text`: source bound text such as `Eq[A]`.
///
/// Output:
/// - Parsed trait head and normalized type arguments.
///
/// Transformation:
/// - Splits named type application text using the backend type utility parser
///   so bounded generic function lowering can reason about trait dictionaries
///   without reparsing source declarations.
fn parse_syntax_generic_function_bound(text: &str) -> Option<SyntaxGenericFunctionBound> {
    let compact = compact_type_application(&compact_spaces(text));
    let (trait_name, type_args) = parse_named_type_args(&compact)?;
    Some(SyntaxGenericFunctionBound {
        trait_name: trait_name.to_string(),
        type_args: type_args
            .into_iter()
            .map(|arg| normalize_trait_type_text(&arg))
            .collect(),
    })
}

/// Collects imported trait conformances from provider interfaces.
///
/// Inputs:
/// - `module`: syntax-output module containing selected trait imports.
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
fn collect_imported_trait_conformances(
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
fn collect_imported_type_refs(
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
        let Some(interface) = interfaces.get(module_name) else {
            continue;
        };

        for item in items {
            if !interface.public_types.contains(&item.name)
                && !interface.opaque_types.contains(&item.name)
            {
                continue;
            }
            let local_name = item.as_alias.clone().unwrap_or_else(|| item.name.clone());
            refs.insert(local_name, format!("{}.{}", module_name, item.name));
        }
    }

    refs
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
fn lower_syntax_type_to_spec(text: &str, ctx: &SyntaxLowerCtx) -> ErlType {
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
fn qualify_imported_type_text(text: &str, imported_type_refs: &BTreeMap<String, String>) -> String {
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
fn syntax_type_head_name(text: &str) -> Option<String> {
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
fn split_explicit_trait_call_target(remote: &str) -> (String, Option<String>) {
    let compact = compact_type_application(&compact_spaces(remote));
    let Some((head, args)) = parse_named_type_args(&compact) else {
        return (remote.to_string(), None);
    };
    let Some(first) = args.first() else {
        return (remote.to_string(), None);
    };
    (head.to_string(), Some(normalize_trait_type_text(first)))
}

#[derive(Debug, Clone)]
struct SyntaxConstructorTarget {
    function: String,
    fixed_arity: usize,
    min_arity: usize,
    defaults: Vec<Option<SyntaxExprOutput>>,
    varargs: bool,
}

#[derive(Debug, Clone)]
struct SyntaxRemoteConstructorTarget {
    module: String,
    function: String,
    fixed_arity: usize,
    varargs: bool,
}

impl SyntaxRemoteConstructorTarget {
    fn accepts_arity(&self, arity: usize) -> bool {
        if self.varargs {
            arity >= self.fixed_arity
        } else {
            arity == self.fixed_arity
        }
    }
}

#[derive(Debug, Clone)]
struct SyntaxConstructorPatternTarget {
    params: Vec<String>,
    body: SyntaxExprOutput,
}

#[derive(Debug, Clone)]
struct SyntaxAliasConstructorTarget {
    params: Vec<String>,
    body: SyntaxExprOutput,
}

fn syntax_remote_constructor_target_from_signature(
    module_name: &str,
    name: &str,
    signature: &terlan_hir::ConstructorSignature,
) -> SyntaxRemoteConstructorTarget {
    let fixed_arity = signature.params.len();
    SyntaxRemoteConstructorTarget {
        module: module_name.to_string(),
        function: constructor_function_name(name, fixed_arity, signature.varargs),
        fixed_arity,
        varargs: signature.varargs,
    }
}

fn parse_syntax_type_alias_constructor_target_texts(
    variants: &[String],
) -> Option<SyntaxAliasConstructorTarget> {
    if variants.len() != 1 {
        return None;
    }
    let src = compact_type_application(&compact_spaces(&variants[0]));
    if is_union(&src) {
        return None;
    }
    if let Some(atom) = parse_type_atom_literal(&src) {
        return Some(SyntaxAliasConstructorTarget {
            params: Vec::new(),
            body: syntax_alias_expr_leaf(SyntaxExprKind::Atom, atom),
        });
    }
    if !(src.starts_with('{') && src.ends_with('}')) {
        return None;
    }

    let inner = &src[1..src.len() - 1];
    let mut items = split_top_level_csv(inner).into_iter();
    let tag = parse_type_atom_literal(&items.next()?)?;
    let mut params = Vec::new();
    let mut body_items = vec![syntax_alias_expr_leaf(SyntaxExprKind::Atom, tag)];

    for item in items {
        let (label, _ty) = split_named_tuple_type_elem(&item)?;
        if !is_lower_identifier(label) {
            return None;
        }
        params.push(label.to_string());
        body_items.push(syntax_alias_expr_leaf(
            SyntaxExprKind::Var,
            label.to_string(),
        ));
    }

    Some(SyntaxAliasConstructorTarget {
        params,
        body: syntax_alias_expr_tuple(body_items),
    })
}

fn syntax_alias_expr_leaf(kind: SyntaxExprKind, text: String) -> SyntaxExprOutput {
    SyntaxExprOutput {
        kind,
        arity: 0,
        text: Some(text),
        span: Default::default(),
        raw: None,
        operator: None,
        remote: None,
        children: Vec::new(),
        patterns: Vec::new(),
        fields: Vec::new(),
        clauses: Vec::new(),
        catch_clauses: Vec::new(),
        try_after: None,
        receive_after: None,
        html_nodes: Vec::new(),
    }
}

fn syntax_alias_expr_tuple(children: Vec<SyntaxExprOutput>) -> SyntaxExprOutput {
    SyntaxExprOutput {
        kind: SyntaxExprKind::Tuple,
        arity: children.len(),
        text: None,
        span: Default::default(),
        raw: None,
        operator: None,
        remote: None,
        children,
        patterns: Vec::new(),
        fields: Vec::new(),
        clauses: Vec::new(),
        catch_clauses: Vec::new(),
        try_after: None,
        receive_after: None,
        html_nodes: Vec::new(),
    }
}

/// Infers a conservative concrete type key for syntax-output trait dispatch.
///
/// Inputs:
/// - `expr`: syntax-output expression used as the trait method's value
///   argument.
/// - `env`: local lowering environment containing parameter annotations.
///
/// Output:
/// - `Some(type_name)` for simple literal or annotated-local expressions.
/// - `None` when the expression needs full type-checker annotation before
///   dispatch can be selected.
///
/// Transformation:
/// - Maps primitive literal shapes and annotated locals to the normalized type
///   names used by typed trait-wrapper lookup.
fn infer_syntax_trait_dispatch_type(
    expr: &SyntaxExprOutput,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    match expr.kind {
        SyntaxExprKind::Int => Some("Int".to_string()),
        SyntaxExprKind::Float => Some("Float".to_string()),
        SyntaxExprKind::Binary => Some("String".to_string()),
        SyntaxExprKind::List => Some("List".to_string()),
        SyntaxExprKind::Atom => expr.text.as_deref().and_then(|text| match text {
            "unit" => Some("Unit".to_string()),
            "lt" | "eq" | "gt" => Some("Comparison".to_string()),
            _ => None,
        }),
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref()?;
            if is_bool_literal_name(name) {
                Some("Bool".to_string())
            } else {
                env.value_types.get(name).cloned()
            }
        }
        SyntaxExprKind::RecordConstruct => expr.text.clone(),
        _ => None,
    }
}

pub(super) fn lower_syntax_module_output(
    module: &SyntaxModuleOutput,
    interfaces: &BTreeMap<String, ModuleInterface>,
    file_imports: &BTreeMap<String, Vec<u8>>,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
) -> Option<ErlModule> {
    let ctx = SyntaxLowerCtx::new(
        module,
        interfaces,
        file_imports,
        templates,
        markdown_imports,
    );
    let mut exports = BTreeSet::new();
    let mut type_exports = BTreeSet::new();
    let mut type_forms = Vec::new();
    let mut struct_forms = Vec::new();
    let mut function_forms = Vec::new();

    for decl in &module.declarations {
        match &decl.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                ..
            } => {}
            SyntaxDeclarationPayload::Import {
                import_kind:
                    SyntaxImportKind::File | SyntaxImportKind::Css | SyntaxImportKind::Markdown,
                ..
            } => {}
            SyntaxDeclarationPayload::Template { .. } => {}
            SyntaxDeclarationPayload::Trait { name, .. } => {
                function_forms.push(ErlForm::Raw(format!("%% trait {}.\n", name)));
            }
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                for_type,
                is_public,
                methods,
            } => {
                if *is_public {
                    if let Some(trait_name) = syntax_type_head_name(&trait_ref.text) {
                        let type_arg = normalize_trait_type_text(&for_type.text);
                        for method in methods {
                            exports.insert(format!(
                                "{}/{}",
                                typed_trait_method_wrapper_name(
                                    &trait_name,
                                    &method.name,
                                    &type_arg
                                ),
                                method.params.len() + 1
                            ));
                        }
                    }
                }
                function_forms.extend(lower_syntax_trait_impl_decl(
                    decl, trait_ref, for_type, methods, &ctx,
                )?);
            }
            SyntaxDeclarationPayload::Config { name, text, .. } if name == "native" => {
                for signature in extract_native_function_signatures(text) {
                    exports.insert(format!("{}/{}", signature.name, signature.arity));
                }
                function_forms.push(ErlForm::Raw(lower_raw_decl_text(name, text)));
            }
            SyntaxDeclarationPayload::Config { .. } => {}
            SyntaxDeclarationPayload::Raw { raw_kind, .. } if raw_kind == "impl" => {}
            SyntaxDeclarationPayload::Raw { raw_kind, text } => {
                if raw_kind == "native" {
                    for signature in extract_native_function_signatures(text) {
                        exports.insert(format!("{}/{}", signature.name, signature.arity));
                    }
                }
                function_forms.push(ErlForm::Raw(lower_raw_decl_text(raw_kind, text)));
            }
            SyntaxDeclarationPayload::Export { items } => {
                collect_syntax_export_payloads(module.source_kind, items, &mut exports)
            }
            SyntaxDeclarationPayload::Constructor {
                name,
                is_public,
                clauses,
                ..
            } => {
                if *is_public {
                    for clause in clauses {
                        let fixed_arity = clause
                            .params
                            .iter()
                            .filter(|param| !param.is_varargs)
                            .count();
                        let varargs = clause.params.iter().any(|param| param.is_varargs);
                        let erlang_arity = if varargs {
                            fixed_arity + 1
                        } else {
                            fixed_arity
                        };
                        exports.insert(format!(
                            "{}/{}",
                            constructor_function_name(name, fixed_arity, varargs),
                            erlang_arity
                        ));
                    }
                }
                function_forms.extend(lower_syntax_constructor_decl(name, clauses, &ctx)?);
            }
            SyntaxDeclarationPayload::Type {
                name,
                params,
                is_public,
                is_opaque,
                variants,
                ..
            } => {
                if *is_public {
                    type_exports.insert(format!("{}/{}", map_type_name(name), params.len()));
                }
                type_forms.push(ErlForm::Type(lower_syntax_type_decl(
                    decl, name, params, *is_opaque, variants,
                )));
            }
            SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                fields,
                ..
            } => {
                if *is_public {
                    type_exports.insert(format!("{}/0", map_type_name(name)));
                }
                type_forms.push(ErlForm::Type(lower_syntax_struct_type_decl(name)));
                struct_forms.push(ErlForm::Record(lower_syntax_struct_decl(
                    decl, name, fields, &ctx,
                )?));
            }
            SyntaxDeclarationPayload::Function {
                name,
                params,
                generic_bounds,
                return_type,
                is_public,
                clauses,
                ..
            } => {
                if *is_public && !clauses.is_empty() {
                    exports.insert(format!("{}/{}", name, params.len() + generic_bounds.len()));
                }
                function_forms.extend(lower_syntax_function_decl(
                    decl,
                    name,
                    params,
                    generic_bounds,
                    return_type,
                    clauses,
                    &ctx,
                )?);
            }
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                return_type,
                is_public,
                clauses,
                ..
            } => {
                let arity = params.len() + 1;
                if *is_public && !clauses.is_empty() {
                    exports.insert(format!("{}/{}", name, arity));
                }
                function_forms.extend(lower_syntax_method_decl(
                    decl,
                    receiver,
                    name,
                    params,
                    return_type,
                    clauses,
                    &ctx,
                )?);
            }
        }
    }
    let mut forms = Vec::new();
    if !exports.is_empty() {
        forms.push(ErlForm::Export(exports.into_iter().collect()));
    }
    if !type_exports.is_empty() {
        forms.push(ErlForm::ExportType(type_exports.into_iter().collect()));
    }
    forms.extend(struct_forms);
    forms.extend(type_forms);
    forms.extend(function_forms);

    Some(ErlModule {
        name: map_module_name(&module.module_name),
        docs: module.docs.clone(),
        forms,
    })
}

/// Collects syntax-output export payloads for direct Erlang lowering.
///
/// Inputs:
/// - `source_kind`: whether the syntax output came from a Terlan source module
///   or an interface summary.
/// - `items`: export entries preserved in the syntax output.
/// - `exports`: mutable Erlang export set.
///
/// Output:
/// - No direct return value; `exports` is extended only for interface outputs.
///
/// Transformation:
/// - Interface export summaries are converted into Erlang `name/arity` export
///   entries.
/// - Module-mode export payloads are ignored because canonical Terlan source
///   uses declaration-site `pub`, and HIR owns the diagnostic for impossible
///   source-kind export payloads.
fn collect_syntax_export_payloads(
    source_kind: SyntaxSourceKind,
    items: &[terlan_syntax::SyntaxExportItem],
    exports: &mut BTreeSet<String>,
) {
    if source_kind == SyntaxSourceKind::Interface {
        for item in items {
            exports.insert(format!("{}/{}", item.name, item.arity));
        }
    }
}

pub(super) fn lower_syntax_struct_headers_to_hrl(module: &SyntaxModuleOutput) -> Option<String> {
    let ctx = SyntaxLowerCtx::empty();
    module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Struct { name, fields, .. } => Some((decl, name, fields)),
            _ => None,
        })
        .map(|(decl, name, fields)| {
            Some(ErlForm::Record(lower_syntax_struct_decl(decl, name, fields, &ctx)?).render())
        })
        .collect::<Option<Vec<_>>>()
        .map(|forms| forms.join(""))
}

fn lower_syntax_type_decl(
    decl: &SyntaxDeclarationOutput,
    name: &str,
    params: &[String],
    is_opaque: bool,
    variants: &[SyntaxTypeOutput],
) -> ErlTypeDecl {
    let rhs = ErlType::Union(
        variants
            .iter()
            .map(|variant| lower_type_to_spec(&variant.text))
            .collect(),
    )
    .normalized();

    ErlTypeDecl {
        opaque: is_opaque,
        docs: decl.docs.clone(),
        name: map_type_name(name),
        params: params.to_vec(),
        rhs,
    }
}

fn lower_syntax_struct_decl(
    decl: &SyntaxDeclarationOutput,
    name: &str,
    fields: &[SyntaxStructFieldOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<ErlRecordDecl> {
    Some(ErlRecordDecl {
        docs: decl.docs.clone(),
        name: map_struct_name(name),
        fields: fields
            .iter()
            .map(|field| {
                Some(ErlRecordField {
                    name: field.name.clone(),
                    docs: field.docs.clone(),
                    default: match field.default.as_ref() {
                        Some(default) => Some(lower_syntax_expr(default, ctx)?),
                        None => None,
                    },
                })
            })
            .collect::<Option<Vec<_>>>()?,
    })
}

fn lower_syntax_struct_type_decl(name: &str) -> ErlTypeDecl {
    ErlTypeDecl {
        opaque: false,
        docs: Vec::new(),
        name: map_type_name(name),
        params: Vec::new(),
        rhs: ErlType::Raw(format!("#{}{{}}", map_struct_name(name))),
    }
}

fn lower_syntax_constructor_decl(
    name: &str,
    clauses: &[SyntaxConstructorClauseOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    clauses
        .iter()
        .map(|clause| {
            let env = lower_syntax_constructor_clause_env(&clause.params, ctx);
            let fixed_arity = clause
                .params
                .iter()
                .filter(|param| !param.is_varargs)
                .count();
            let varargs = clause.params.iter().any(|param| param.is_varargs);
            let function = constructor_function_name(name, fixed_arity, varargs);
            let args = clause
                .params
                .iter()
                .map(|param| {
                    if param.is_varargs {
                        ErlType::List(Box::new(lower_syntax_type_to_spec(
                            &param.annotation.text,
                            ctx,
                        )))
                    } else {
                        lower_syntax_type_to_spec(&param.annotation.text, ctx)
                    }
                })
                .collect();
            Some(vec![
                ErlForm::Spec(ErlSpec {
                    docs: Vec::new(),
                    name: function.clone(),
                    args,
                    ret: lower_syntax_type_to_spec(&clause.return_type.text, ctx),
                }),
                ErlForm::Function(ErlFunction {
                    docs: Vec::new(),
                    name: function,
                    clauses: vec![ErlFunctionClause {
                        patterns: clause
                            .params
                            .iter()
                            .map(|param| ErlPattern::Var(sanitize_erlang_var(&param.name)))
                            .collect(),
                        guard: None,
                        body: lower_syntax_expr_with_env(&clause.body, ctx, &env)?,
                    }],
                }),
            ])
        })
        .collect::<Option<Vec<_>>>()
        .map(|forms| forms.into_iter().flatten().collect())
}

fn lower_syntax_function_decl(
    decl: &SyntaxDeclarationOutput,
    name: &str,
    params: &[SyntaxParamOutput],
    generic_bounds: &[String],
    return_type: &SyntaxTypeOutput,
    clauses: &[SyntaxFunctionClauseOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    let mut forms = Vec::new();
    let env = lower_syntax_function_env(params, ctx, generic_bounds);
    let hidden_bound_params = generic_bound_param_names(generic_bounds);

    if !params.is_empty() {
        forms.push(ErlForm::Spec(ErlSpec {
            docs: decl.docs.clone(),
            name: name.to_string(),
            args: hidden_bound_params
                .iter()
                .map(|_| ErlType::Raw("map()".to_string()))
                .chain(
                    params
                        .iter()
                        .map(|param| lower_syntax_type_to_spec(&param.annotation.text, ctx)),
                )
                .collect(),
            ret: lower_syntax_type_to_spec(&return_type.text, ctx),
        }));
    }

    forms.push(ErlForm::Function(ErlFunction {
        docs: if params.is_empty() {
            decl.docs.clone()
        } else {
            Vec::new()
        },
        name: name.to_string(),
        clauses: clauses
            .iter()
            .map(|clause| {
                let body = lower_intrinsic_annotation_body(decl, params)
                    .or_else(|| lower_syntax_expr_with_env(&clause.body, ctx, &env))?;
                let mut patterns = hidden_bound_params
                    .iter()
                    .map(|name| ErlPattern::Var(name.clone()))
                    .collect::<Vec<_>>();
                patterns.extend(
                    clause
                        .patterns
                        .iter()
                        .map(|pattern| lower_syntax_pattern(pattern, ctx))
                        .collect::<Option<Vec<_>>>()?,
                );
                Some(ErlFunctionClause {
                    patterns,
                    guard: match clause.guard.as_ref() {
                        Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, &env)?),
                        None => None,
                    },
                    body,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    }));

    Some(forms)
}

/// Lowers an explicit trait impl declaration into typed wrapper functions.
///
/// Inputs:
/// - `decl`: source declaration metadata, docs, annotations, and span.
/// - `trait_ref`: implemented trait type expression.
/// - `for_type`: concrete type expression after `for`.
/// - `methods`: implementation methods declared inside the impl block.
/// - `ctx`: syntax lowering context for method bodies.
///
/// Output:
/// - Erlang spec/function forms for each impl method wrapper.
/// - `None` when the trait head cannot be identified or a method body cannot
///   lower through the syntax-output path.
///
/// Transformation:
/// - Emits private typed dictionary-wrapper functions with a hidden first
///   dictionary argument followed by the source method arguments. Trait call
///   lowering can then dispatch `Trait.method(value)` to the wrapper selected
///   by the inferred first argument type.
fn lower_syntax_trait_impl_decl(
    decl: &SyntaxDeclarationOutput,
    trait_ref: &SyntaxTypeOutput,
    for_type: &SyntaxTypeOutput,
    methods: &[SyntaxImplMethodOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    let trait_name = syntax_type_head_name(&trait_ref.text)?;
    let type_arg = normalize_trait_type_text(&for_type.text);

    methods
        .iter()
        .map(|method| lower_syntax_trait_impl_method(decl, &trait_name, &type_arg, method, ctx))
        .collect::<Option<Vec<_>>>()
        .map(|forms| forms.into_iter().flatten().collect())
}

/// Lowers one explicit trait impl method into a typed wrapper function.
///
/// Inputs:
/// - `decl`: parent impl declaration metadata.
/// - `trait_name`: source-visible trait head.
/// - `type_arg`: normalized concrete implementation type.
/// - `method`: implementation method signature and clauses.
/// - `ctx`: syntax lowering context for method expressions and patterns.
///
/// Output:
/// - Erlang spec and function forms for the generated wrapper.
///
/// Transformation:
/// - Prepends a hidden trait dictionary argument to the Erlang ABI while
///   preserving the source method clauses, guards, and bodies. The dictionary
///   argument is currently metadata-only for local explicit impl dispatch.
fn lower_syntax_trait_impl_method(
    decl: &SyntaxDeclarationOutput,
    trait_name: &str,
    type_arg: &str,
    method: &SyntaxImplMethodOutput,
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    let wrapper_name = typed_trait_method_wrapper_name(trait_name, &method.name, type_arg);
    let env = lower_syntax_function_env(&method.params, ctx, &[]);

    Some(vec![
        ErlForm::Spec(ErlSpec {
            docs: decl.docs.clone(),
            name: wrapper_name.clone(),
            args: std::iter::once(ErlType::Raw("map()".to_string()))
                .chain(
                    method
                        .params
                        .iter()
                        .map(|param| lower_syntax_type_to_spec(&param.annotation.text, ctx)),
                )
                .collect(),
            ret: lower_syntax_type_to_spec(&method.return_type.text, ctx),
        }),
        ErlForm::Function(ErlFunction {
            docs: Vec::new(),
            name: wrapper_name,
            clauses: method
                .clauses
                .iter()
                .map(|clause| {
                    let mut patterns = Vec::with_capacity(clause.patterns.len() + 1);
                    patterns.push(ErlPattern::Var("_TraitDict".to_string()));
                    patterns.extend(
                        clause
                            .patterns
                            .iter()
                            .map(|pattern| lower_syntax_pattern(pattern, ctx))
                            .collect::<Option<Vec<_>>>()?,
                    );
                    Some(ErlFunctionClause {
                        patterns,
                        guard: match clause.guard.as_ref() {
                            Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, &env)?),
                            None => None,
                        },
                        body: lower_syntax_expr_with_env(&clause.body, ctx, &env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        }),
    ])
}

/// Lowers a syntax-output receiver method into an Erlang function.
///
/// Inputs:
/// - `decl`: source declaration metadata, docs, annotations, and span.
/// - `receiver`: receiver parameter declared before the method name.
/// - `name`: method name to expose as a backend function.
/// - `params`: ordinary method parameters.
/// - `return_type`: method return type annotation.
/// - `clauses`: parsed method clauses; currently one clause is produced by the
///   parser.
/// - `ctx`: syntax lowering context for expressions and imports.
///
/// Output:
/// - Erlang spec/function forms, or `None` when a method body cannot lower.
///
/// Transformation:
/// - Rewrites `(receiver: Type) name(args...)` into the backend calling
///   convention `name(receiver, args...)`, using the same intrinsic annotation
///   path as ordinary functions.
fn lower_syntax_method_decl(
    decl: &SyntaxDeclarationOutput,
    receiver: &SyntaxParamOutput,
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    clauses: &[SyntaxFunctionClauseOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    let all_params = syntax_method_params(receiver, params);
    let env = lower_syntax_function_env(&all_params, ctx, &[]);
    let mut forms = Vec::new();

    forms.push(ErlForm::Spec(ErlSpec {
        docs: decl.docs.clone(),
        name: name.to_string(),
        args: all_params
            .iter()
            .map(|param| lower_syntax_type_to_spec(&param.annotation.text, ctx))
            .collect(),
        ret: lower_syntax_method_return_type_to_spec(receiver, return_type, ctx),
    }));

    forms.push(ErlForm::Function(ErlFunction {
        docs: Vec::new(),
        name: name.to_string(),
        clauses: clauses
            .iter()
            .map(|clause| {
                let body = lower_intrinsic_annotation_body_for_names(
                    decl,
                    all_params.iter().map(|param| param.name.as_str()),
                )
                .or_else(|| lower_syntax_expr_with_env(&clause.body, ctx, &env))?;
                Some(ErlFunctionClause {
                    patterns: all_params
                        .iter()
                        .map(|param| ErlPattern::Var(sanitize_erlang_var(&param.name)))
                        .collect(),
                    guard: match clause.guard.as_ref() {
                        Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, &env)?),
                        None => None,
                    },
                    body,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    }));

    Some(forms)
}

/// Lowers a receiver-method return annotation into the backend helper spec.
///
/// Inputs:
/// - `receiver`: source receiver parameter, including the contextual `mut`
///   marker.
/// - `return_type`: source-visible method return type annotation.
///
/// Output:
/// - Erlang type expression for the generated receiver-first helper function.
///
/// Transformation:
/// - Implements the first P0.2c command-style mutable receiver ABI slice:
///   a source method declared as `(mut receiver: T) method(...): Unit` exposes
///   `Unit` to Terlan callers, but the backend helper returns `T` so sequence
///   and pipe lowering can rebind the updated receiver.
fn lower_syntax_method_return_type_to_spec(
    receiver: &SyntaxParamOutput,
    return_type: &SyntaxTypeOutput,
    ctx: &SyntaxLowerCtx,
) -> ErlType {
    if receiver.is_mutable && compact_type_application(&compact_spaces(&return_type.text)) == "Unit"
    {
        lower_syntax_type_to_spec(&receiver.annotation.text, ctx)
    } else {
        lower_syntax_type_to_spec(&return_type.text, ctx)
    }
}

/// Builds the receiver-first parameter list for backend method lowering.
///
/// Inputs:
/// - `receiver`: receiver parameter from a method declaration.
/// - `params`: ordinary method parameters.
///
/// Output:
/// - Owned parameter list with `receiver` first.
///
/// Transformation:
/// - Clones syntax-output parameters into the function-like order required by
///   the current backend calling convention.
fn syntax_method_params(
    receiver: &SyntaxParamOutput,
    params: &[SyntaxParamOutput],
) -> Vec<SyntaxParamOutput> {
    std::iter::once(receiver.clone())
        .chain(params.iter().cloned())
        .collect()
}

/// Builds hidden backend parameter names for generic trait bounds.
///
/// Inputs:
/// - `generic_bounds`: source bound texts from a function declaration.
///
/// Output:
/// - Stable Erlang variable names, one per source bound.
///
/// Transformation:
/// - Parses each bound when possible and includes the trait/type shape in the
///   hidden name for readable generated code; malformed fallback names retain
///   deterministic positional identity.
fn generic_bound_param_names(generic_bounds: &[String]) -> Vec<String> {
    generic_bounds
        .iter()
        .enumerate()
        .map(|(index, bound)| {
            if let Some(parsed) = parse_syntax_generic_function_bound(bound) {
                let suffix = std::iter::once(parsed.trait_name)
                    .chain(parsed.type_args)
                    .map(|part| sanitize_erlang_fn_name(&part))
                    .collect::<Vec<_>>()
                    .join("_");
                format!("_TyperTraitDict{}", to_erlang_type_name(&suffix))
            } else {
                format!("_TyperTraitDict{}", index)
            }
        })
        .collect()
}

fn lower_syntax_function_env(
    params: &[SyntaxParamOutput],
    ctx: &SyntaxLowerCtx,
    generic_bounds: &[String],
) -> SyntaxLowerEnv {
    let value_locals = params.iter().map(|param| param.name.clone()).collect();
    let value_types = params
        .iter()
        .map(|param| {
            (
                param.name.clone(),
                qualify_imported_type_text(
                    &normalize_trait_type_text(&param.annotation.text),
                    &ctx.imported_type_refs,
                ),
            )
        })
        .collect();
    let struct_locals = params
        .iter()
        .filter_map(|param| {
            let struct_name = syntax_struct_name_from_type_annotation(&param.annotation.text, ctx)?;
            Some((param.name.clone(), struct_name))
        })
        .collect();
    let trait_bound_dicts = generic_bounds
        .iter()
        .zip(generic_bound_param_names(generic_bounds))
        .filter_map(|(bound, param_name)| {
            let bound = parse_syntax_generic_function_bound(bound)?;
            if bound.type_args.len() != 1 {
                return None;
            }
            Some(((bound.trait_name, bound.type_args[0].clone()), param_name))
        })
        .collect();
    SyntaxLowerEnv {
        struct_locals,
        value_locals,
        value_types,
        trait_bound_dicts,
        value_replacements: BTreeMap::new(),
    }
}

fn lower_syntax_constructor_clause_env(
    params: &[SyntaxConstructorParamOutput],
    ctx: &SyntaxLowerCtx,
) -> SyntaxLowerEnv {
    let value_locals = params.iter().map(|param| param.name.clone()).collect();
    let value_types = params
        .iter()
        .map(|param| {
            (
                param.name.clone(),
                qualify_imported_type_text(
                    &normalize_trait_type_text(&param.annotation.text),
                    &ctx.imported_type_refs,
                ),
            )
        })
        .collect();
    let struct_locals = params
        .iter()
        .filter_map(|param| {
            let struct_name = syntax_struct_name_from_type_annotation(&param.annotation.text, ctx)?;
            Some((param.name.clone(), struct_name))
        })
        .collect();
    SyntaxLowerEnv {
        struct_locals,
        value_locals,
        value_types,
        trait_bound_dicts: BTreeMap::new(),
        value_replacements: BTreeMap::new(),
    }
}

fn syntax_struct_name_from_type_annotation(
    annotation: &str,
    ctx: &SyntaxLowerCtx,
) -> Option<String> {
    let trimmed = annotation.trim();
    if trimmed.contains('[') || trimmed.contains('.') {
        return None;
    }
    ctx.struct_field_types
        .contains_key(trimmed)
        .then(|| trimmed.to_string())
}

fn lower_syntax_expr(expr: &SyntaxExprOutput, ctx: &SyntaxLowerCtx) -> Option<ErlExpr> {
    lower_syntax_expr_with_env(expr, ctx, &SyntaxLowerEnv::default())
}

#[derive(Debug, Clone)]
enum LoweredComprehensionSource {
    NativeList(ErlExpr),
    IterableIterator(ErlExpr),
}

/// Classifies and lowers the source side of a list comprehension.
///
/// Inputs:
/// - `source`: comprehension source expression after `<-`.
/// - `ctx`: syntax lowering context containing local receiver-method metadata.
/// - `env`: local value/type environment used to identify non-list iterable
///   sources.
///
/// Output:
/// - `NativeList` for lowered native list inputs.
/// - `IterableIterator` for lowered `iterator(Source)` calls over non-list
///   locals whose type declares a zero-argument receiver `iterator` method.
///
/// Transformation:
/// - Keeps existing list-backed Erlang comprehension lowering for list sources
///   while desugaring the first generic `Iterable` source shape into the
///   compiler-visible iterator-producing receiver method.
fn lower_syntax_list_comprehension_source(
    source: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<LoweredComprehensionSource> {
    let Some(receiver_type) = infer_syntax_trait_dispatch_type(source, env) else {
        return lower_syntax_expr_with_env(source, ctx, env)
            .map(LoweredComprehensionSource::NativeList);
    };

    if receiver_type_head(&receiver_type) == "List" {
        return lower_syntax_expr_with_env(source, ctx, env)
            .map(LoweredComprehensionSource::NativeList);
    }

    if ctx
        .receiver_method_target(&receiver_type, "iterator", 0)
        .is_some()
    {
        return Some(LoweredComprehensionSource::IterableIterator(
            ErlExpr::Call {
                module: None,
                function: "iterator".to_string(),
                args: vec![lower_syntax_expr_with_env(source, ctx, env)?],
            },
        ));
    }

    lower_syntax_expr_with_env(source, ctx, env).map(LoweredComprehensionSource::NativeList)
}

/// Lowers a list-comprehension expression.
///
/// Inputs:
/// - `expr`: syntax-output list comprehension with yield, source, pattern, and
///   optional guard.
/// - `ctx`, `env`: syntax lowering context and local type environment.
///
/// Output:
/// - Native Erlang list-comprehension expression for native list sources.
/// - Explicit state-passing loop for generic iterable sources.
///
/// Transformation:
/// - Preserves the existing backend-native lowering for list-backed
///   comprehensions and rewrites iterable sources to `iterator(Source)`,
///   repeated next-step matching, ordered guard evaluation, and reverse-order
///   accumulation.
fn lower_syntax_list_comprehension_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let value = expr.children.first()?;
    let source = expr.children.get(1)?;
    let pattern = expr.patterns.first()?;
    let guard = expr.children.get(2);
    let lowered_value = lower_syntax_expr_with_env(value, ctx, env)?;
    let lowered_pattern = lower_syntax_pattern(pattern, ctx)?;
    let lowered_guard = match guard {
        Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, env)?),
        None => None,
    };

    match lower_syntax_list_comprehension_source(source, ctx, env)? {
        LoweredComprehensionSource::NativeList(source) => Some(ErlExpr::ListComprehension {
            expr: Box::new(lowered_value),
            pattern: lowered_pattern,
            source: Box::new(source),
            guard: lowered_guard.map(Box::new),
        }),
        LoweredComprehensionSource::IterableIterator(iterator) => {
            Some(lower_syntax_iterable_comprehension_loop(
                expr,
                iterator,
                lowered_pattern,
                lowered_guard,
                lowered_value,
            ))
        }
    }
}

/// Lowers a generic iterable comprehension into an explicit BEAM loop.
///
/// Inputs:
/// - `expr`: original comprehension expression used only for deterministic
///   temporary naming.
/// - `iterator`: lowered iterator-state expression.
/// - `pattern`: lowered generator pattern.
/// - `guard`: optional lowered filter guard.
/// - `value`: lowered yielded expression.
///
/// Output:
/// - Raw Erlang expression implementing state-passing traversal.
///
/// Transformation:
/// - Binds the initial iterator, creates a recursive local fun, repeatedly
///   matches the iterator state as `None` or `Some({value, next})`, skips
///   failed patterns/filters, accumulates yielded values in reverse order, and
///   returns `lists:reverse(Acc)`.
fn lower_syntax_iterable_comprehension_loop(
    expr: &SyntaxExprOutput,
    iterator: ErlExpr,
    pattern: ErlPattern,
    guard: Option<ErlExpr>,
    value: ErlExpr,
) -> ErlExpr {
    let suffix = format!("{}_{}", expr.span.start, expr.span.end);
    let iterator_var = format!("TerlanIterator{}", suffix);
    let loop_var = format!("TerlanIterableLoop{}", suffix);
    let iter_var = format!("TerlanIter{}", suffix);
    let acc_var = format!("TerlanAcc{}", suffix);
    let raw_value_var = format!("TerlanRawValue{}", suffix);
    let raw_next_var = format!("TerlanRawNext{}", suffix);
    let next_var = format!("TerlanNext{}", suffix);
    let skipped_var = format!("_TerlanSkipped{}", suffix);

    let guard = guard
        .as_ref()
        .map(|guard| format!(" when {}", guard.render()))
        .unwrap_or_default();
    let body = format!(
        "{loop_var}({next_var}, [{value} | {acc_var}])",
        loop_var = loop_var,
        next_var = next_var,
        value = value.render(),
        acc_var = acc_var
    );

    ErlExpr::Raw(format!(
        "begin\n    {iterator_var} = {iterator},\n    {loop_var} = fun {loop_var}({iter_var}, {acc_var}) ->\n        case (case {iter_var} of\n            [{raw_value_var}|{raw_next_var}] -> {{'some', {{{raw_value_var}, {raw_next_var}}}}};\n            [] -> 'none'\n        end) of\n            'none' -> lists:reverse({acc_var});\n            {{'some', {{{pattern}, {next_var}}}}}{guard} -> {body};\n            {{'some', {{{skipped_var}, {next_var}}}}} -> {loop_var}({next_var}, {acc_var})\n        end\n    end,\n    {loop_var}({iterator_var}, [])\nend",
        iterator_var = iterator_var,
        iterator = iterator.render(),
        loop_var = loop_var,
        iter_var = iter_var,
        acc_var = acc_var,
        raw_value_var = raw_value_var,
        raw_next_var = raw_next_var,
        pattern = pattern.render(),
        next_var = next_var,
        guard = guard,
        body = body,
        skipped_var = skipped_var,
    ))
}

fn lower_syntax_expr_with_env(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    match expr.kind {
        SyntaxExprKind::Int => Some(ErlExpr::Int(expr.text.as_deref()?.parse().ok()?)),
        SyntaxExprKind::Float => Some(ErlExpr::Float(expr.text.clone()?)),
        SyntaxExprKind::Atom => Some(ErlExpr::Atom(expr.text.clone()?)),
        SyntaxExprKind::Binary => Some(ErlExpr::Binary(expr.text.clone()?)),
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref()?;
            if is_bool_literal_name(name) {
                return Some(ErlExpr::Atom(name.to_string()));
            }
            if let Some(target) = ctx.singleton_alias_value_target(name) {
                return lower_syntax_alias_constructor_expr(target, &[], ctx, env);
            }
            if let Some(replacement) = env.value_replacements.get(name) {
                return Some(replacement.clone());
            }
            if env.value_locals.contains(name) {
                return Some(ErlExpr::Var(sanitize_erlang_var(name)));
            }
            if let Some(arity) = ctx.local_function_values.get(name) {
                return Some(ErlExpr::Raw(format!(
                    "fun {}/{}",
                    sanitize_erlang_fn_name(name),
                    arity
                )));
            }
            Some(
                ctx.file_imports
                    .get(name)
                    .map(|bytes| ErlExpr::Binary(erlang_binary_bytes(bytes)))
                    .unwrap_or_else(|| ErlExpr::Var(sanitize_erlang_var(name))),
            )
        }
        SyntaxExprKind::Tuple => Some(ErlExpr::Tuple(
            expr.children
                .iter()
                .map(|child| lower_syntax_expr_with_env(child, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::List => Some(ErlExpr::List(
            expr.children
                .iter()
                .map(|child| lower_syntax_expr_with_env(child, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::ListCons => Some(ErlExpr::ListCons(
            Box::new(lower_syntax_expr_with_env(
                expr.children.first()?,
                ctx,
                env,
            )?),
            Box::new(lower_syntax_expr_with_env(expr.children.get(1)?, ctx, env)?),
        )),
        SyntaxExprKind::FixedArray => Some(ErlExpr::FixedArray(
            expr.children
                .iter()
                .map(|child| lower_syntax_expr_with_env(child, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::Index => Some(ErlExpr::Index {
            value: Box::new(lower_syntax_expr_with_env(
                expr.children.first()?,
                ctx,
                env,
            )?),
            index: Box::new(lower_syntax_expr_with_env(expr.children.get(1)?, ctx, env)?),
        }),
        SyntaxExprKind::Map => Some(ErlExpr::Map(
            expr.fields
                .iter()
                .map(|field| lower_syntax_map_expr_field(field, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::ListComprehension => lower_syntax_list_comprehension_expr(expr, ctx, env),
        SyntaxExprKind::Let => lower_syntax_let_expr(expr, ctx, env),
        SyntaxExprKind::Cast => None,
        SyntaxExprKind::Call => lower_syntax_call_expr(expr, ctx, env),
        SyntaxExprKind::FunctionCall => lower_syntax_function_value_call_expr(expr, ctx, env),
        SyntaxExprKind::Case => {
            let scrutinee = expr.children.first()?;
            Some(ErlExpr::Case {
                scrutinee: Box::new(lower_syntax_expr_with_env(scrutinee, ctx, env)?),
                clauses: expr
                    .clauses
                    .iter()
                    .map(|clause| {
                        let pattern = clause.patterns.first()?;
                        Some(ErlCaseClause {
                            pattern: lower_syntax_pattern(pattern, ctx)?,
                            guard: match clause.guard.as_deref() {
                                Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, env)?),
                                None => None,
                            },
                            body: lower_syntax_expr_with_env(&clause.body, ctx, env)?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
            })
        }
        SyntaxExprKind::Receive => Some(ErlExpr::Receive {
            clauses: expr
                .clauses
                .iter()
                .map(|clause| {
                    let pattern = clause.patterns.first()?;
                    Some(ErlCaseClause {
                        pattern: lower_syntax_pattern(pattern, ctx)?,
                        guard: match clause.guard.as_deref() {
                            Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, env)?),
                            None => None,
                        },
                        body: lower_syntax_expr_with_env(&clause.body, ctx, env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            after_clause: expr.receive_after.as_ref().and_then(|after| {
                let trigger = lower_syntax_expr_with_env(&after.trigger, ctx, env)?;
                let body = lower_syntax_expr_with_env(&after.body, ctx, env)?;
                Some(ErlTryAfterClause {
                    trigger: Box::new(trigger),
                    body: Box::new(body),
                })
            }),
        }),
        SyntaxExprKind::Try => Some(ErlExpr::Try {
            body: Box::new(lower_syntax_expr_with_env(
                expr.children.first()?,
                ctx,
                env,
            )?),
            of_clauses: expr
                .clauses
                .iter()
                .map(|clause| {
                    let pattern = clause.patterns.first()?;
                    Some(ErlCaseClause {
                        pattern: lower_syntax_pattern(pattern, ctx)?,
                        guard: match clause.guard.as_deref() {
                            Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, env)?),
                            None => None,
                        },
                        body: lower_syntax_expr_with_env(&clause.body, ctx, env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            catch_clauses: expr
                .catch_clauses
                .iter()
                .map(|clause| {
                    let pattern = clause.patterns.first()?;
                    Some(ErlCaseClause {
                        pattern: lower_syntax_pattern(pattern, ctx)?,
                        guard: match clause.guard.as_deref() {
                            Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, env)?),
                            None => None,
                        },
                        body: lower_syntax_expr_with_env(&clause.body, ctx, env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            after_clause: expr.try_after.as_ref().and_then(|after| {
                let trigger = lower_syntax_expr_with_env(&after.trigger, ctx, env)?;
                let body = lower_syntax_expr_with_env(&after.body, ctx, env)?;
                Some(ErlTryAfterClause {
                    trigger: Box::new(trigger),
                    body: Box::new(body),
                })
            }),
        }),
        SyntaxExprKind::If => Some(ErlExpr::If(
            expr.clauses
                .iter()
                .map(|clause| {
                    Some(ErlIfClause {
                        condition: lower_syntax_expr_with_env(clause.guard.as_deref()?, ctx, env)?,
                        body: lower_syntax_expr_with_env(&clause.body, ctx, env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::Fun => Some(ErlExpr::Fun(
            expr.clauses
                .iter()
                .map(|clause| {
                    Some(ErlFunctionClause {
                        patterns: clause
                            .patterns
                            .iter()
                            .map(|pattern| lower_syntax_pattern(pattern, ctx))
                            .collect::<Option<Vec<_>>>()?,
                        guard: match clause.guard.as_ref() {
                            Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, env)?),
                            None => None,
                        },
                        body: lower_syntax_expr_with_env(&clause.body, ctx, env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::RemoteFunRef => Some(ErlExpr::RemoteFunRef {
            module: expr.remote.clone()?,
            function: expr.text.clone()?,
            arity: expr.arity,
        }),
        SyntaxExprKind::Macro => Some(ErlExpr::MacroCall {
            name: expr.text.clone()?,
            args: expr
                .children
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        }),
        SyntaxExprKind::RawMacro => None,
        SyntaxExprKind::HtmlBlock => lower_syntax_html_block_with_env(expr, ctx, env),
        SyntaxExprKind::RecordAccess => {
            let (name, field) = expr.text.as_deref()?.split_once('.')?;
            Some(ErlExpr::RecordAccess {
                value: Box::new(lower_syntax_expr_with_env(
                    expr.children.first()?,
                    ctx,
                    env,
                )?),
                name: name.to_string(),
                field: field.to_string(),
            })
        }
        SyntaxExprKind::FieldAccess => {
            let field = expr.text.clone()?;
            let value = expr.children.first()?;
            if let Some(name) = syntax_expr_name(value) {
                if let Some(markdown) = ctx.markdown_imports.get(name) {
                    return match field.as_str() {
                        "raw" => Some(ErlExpr::Binary(erlang_binary_bytes(
                            markdown.raw_source.as_bytes(),
                        ))),
                        "html" => Some(ErlExpr::Binary(erlang_binary_bytes(
                            markdown.rendered_html.as_bytes(),
                        ))),
                        _ => None,
                    };
                }
            }
            let record_name =
                resolve_syntax_field_access_struct(value, env).unwrap_or_else(|| field.clone());
            Some(ErlExpr::RecordAccess {
                value: Box::new(lower_syntax_expr_with_env(value, ctx, env)?),
                name: record_name,
                field,
            })
        }
        SyntaxExprKind::RecordUpdate => Some(ErlExpr::RecordUpdate {
            value: Box::new(lower_syntax_expr_with_env(
                expr.children.first()?,
                ctx,
                env,
            )?),
            name: expr.text.clone()?,
            fields: expr
                .fields
                .iter()
                .map(|field| lower_syntax_expr_field(field, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        }),
        SyntaxExprKind::RecordConstruct => Some(ErlExpr::RecordConstruct {
            name: expr.text.clone()?,
            fields: expr
                .fields
                .iter()
                .map(|field| lower_syntax_expr_field(field, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        }),
        SyntaxExprKind::TemplateInstantiate => lower_syntax_template_instantiation(expr, ctx, env),
        SyntaxExprKind::ConstructorChain => lower_syntax_constructor_chain(expr, ctx, env),
        SyntaxExprKind::BinaryOp => {
            let left = expr.children.first()?;
            let right = expr.children.get(1)?;
            if expr.operator.as_deref() == Some("|>") {
                return lower_syntax_pipe_forward(left, right, ctx, env);
            }
            Some(ErlExpr::BinaryOp {
                op: lower_syntax_binary_op(expr.operator.as_deref()),
                left: Box::new(lower_syntax_expr_with_env(left, ctx, env)?),
                right: Box::new(lower_syntax_expr_with_env(right, ctx, env)?),
            })
        }
        SyntaxExprKind::UnaryOp => Some(ErlExpr::UnaryOp {
            op: lower_syntax_unary_op(expr.operator.as_deref()),
            expr: Box::new(lower_syntax_expr_with_env(
                expr.children.first()?,
                ctx,
                env,
            )?),
        }),
        SyntaxExprKind::Quote => Some(ErlExpr::Raw(format!(
            "quote {}",
            lower_syntax_expr_with_env(expr.children.first()?, ctx, env)?.render()
        ))),
        SyntaxExprKind::Unquote => Some(ErlExpr::Raw(format!(
            "unquote({})",
            lower_syntax_expr_with_env(expr.children.first()?, ctx, env)?.render()
        ))),
        SyntaxExprKind::Sequence => lower_syntax_sequence_expr(expr, ctx, env),
    }
}

/// Lowers a sequence while threading mutable receiver updates.
///
/// Inputs:
/// - `expr`: syntax-output sequence expression.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Lowered Erlang expression for the sequence.
/// - `None` when the sequence is empty or any child cannot lower.
///
/// Transformation:
/// - Evaluates children left-to-right. Non-final ordinary expressions are bound
///   to ignored temporaries to preserve effects. Non-final mutable receiver
///   calls bind the hidden backend-updated receiver and update the local
///   replacement environment so later source references to the receiver lower
///   to that updated binding.
fn lower_syntax_sequence_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let (last, prefix) = expr.children.split_last()?;
    let mut sequence_env = env.clone();
    let mut bindings = Vec::new();

    for (index, child) in prefix.iter().enumerate() {
        if let Some((receiver_name, binding)) =
            lower_syntax_mutable_receiver_update_binding(child, ctx, &sequence_env, index)
        {
            sequence_env
                .value_replacements
                .insert(receiver_name, ErlExpr::Var(binding.name.clone()));
            bindings.push(binding);
        } else {
            bindings.push(ErlLetBinding {
                name: format!("_TerlanSeqIgnored{index}"),
                value: lower_syntax_expr_with_env(child, ctx, &sequence_env)?,
            });
        }
    }

    let body = if let Some((receiver_name, binding)) =
        lower_syntax_mutable_receiver_update_binding(last, ctx, &sequence_env, bindings.len())
    {
        let updated_receiver = ErlExpr::Var(binding.name.clone());
        sequence_env
            .value_replacements
            .insert(receiver_name, updated_receiver.clone());
        bindings.push(binding);
        updated_receiver
    } else {
        lower_syntax_expr_with_env(last, ctx, &sequence_env)?
    };

    if bindings.is_empty() {
        Some(body)
    } else {
        Some(ErlExpr::Let {
            bindings,
            body: Box::new(body),
        })
    }
}

/// Builds one mutable receiver update binding from a direct method call.
///
/// Inputs:
/// - `expr`: syntax-output expression that may be `receiver.method(args...)`.
/// - `ctx`, `env`: lowering context and current replacement-aware environment.
/// - `index`: deterministic sequence-local temporary index.
///
/// Output:
/// - Receiver source name and Erlang binding for the backend-updated receiver.
/// - `None` for non-call expressions, non-variable receivers, immutable
///   methods, or unsupported child expressions.
///
/// Transformation:
/// - Recognizes a direct mutable receiver call, lowers the call through the
///   backend receiver-first convention, and captures its hidden updated
///   receiver result in a deterministic temporary variable.
fn lower_syntax_mutable_receiver_update_binding(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
    index: usize,
) -> Option<(String, ErlLetBinding)> {
    if !matches!(expr.kind, SyntaxExprKind::Call) || expr.remote.is_some() {
        return None;
    }

    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }

    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    if !matches!(receiver.kind, SyntaxExprKind::Var) {
        return None;
    }
    let receiver_name = receiver.text.clone()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, env)?;
    let arity = expr.children.len().checked_sub(1)?;
    let receiver_target_mutable = ctx
        .receiver_method_target(&receiver_type, method, arity)
        .is_some_and(|target| target.mutable)
        || is_mutating_primitive_receiver_method(&receiver_type, method, arity);
    if !receiver_target_mutable {
        return None;
    }

    let mut lowered_args = Vec::with_capacity(arity + 1);
    lowered_args.push(lower_syntax_expr_with_env(receiver, ctx, env)?);
    lowered_args.extend(
        expr.children
            .iter()
            .skip(1)
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    );

    let value = match primitive_receiver_method_intrinsic(&receiver_type, method, arity) {
        Some(intrinsic) => lower_core_primitive_intrinsic_to_erlang(&intrinsic, lowered_args)?,
        None => ErlExpr::Call {
            module: None,
            function: method.to_string(),
            args: lowered_args,
        },
    };

    Some((
        receiver_name,
        ErlLetBinding {
            name: format!("_TerlanMutReceiver{index}"),
            value,
        },
    ))
}

/// Returns whether a primitive receiver method updates its receiver binding.
///
/// Inputs:
/// - `receiver_type`: inferred source type of the receiver expression.
/// - `method`: receiver method name.
/// - `arg_count`: number of non-receiver call arguments.
///
/// Output:
/// - `true` for compiler-owned command-style collection mutators.
/// - `false` for observers, pure primitive methods, and unsupported calls.
///
/// Transformation:
/// - Extracts the nominal collection type head and matches only the selected
///   0.0.2 mutable receiver ABI methods so sequence lowering can rebind
///   imported std collection receivers without requiring local method bodies.
fn is_mutating_primitive_receiver_method(
    receiver_type: &str,
    method: &str,
    arg_count: usize,
) -> bool {
    matches!(
        (
            receiver_type_head(receiver_type).as_str(),
            method,
            arg_count
        ),
        ("List", "push", 1)
            | ("List", "clear", 0)
            | ("Map", "put", 2)
            | ("Map", "remove", 1)
            | ("Map", "clear", 0)
            | ("Set", "add", 1)
            | ("Set", "remove", 1)
            | ("Set", "clear", 0)
    )
}

fn resolve_syntax_field_access_struct(
    value: &SyntaxExprOutput,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    match value.kind {
        SyntaxExprKind::Var => env.struct_locals.get(value.text.as_deref()?).cloned(),
        _ => None,
    }
}

fn lower_syntax_html_block_with_env(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let mut chunks = Vec::new();
    for node in &expr.html_nodes {
        chunks.extend(lower_syntax_html_node(node, ctx, env)?);
    }
    Some(ErlExpr::List(chunks))
}

fn lower_syntax_html_node(
    node: &SyntaxHtmlNodeOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    match node {
        SyntaxHtmlNodeOutput::Text { text } => Some(vec![html_binary(text)]),
        SyntaxHtmlNodeOutput::Expr { expr } => {
            Some(vec![lower_syntax_html_child_expr(expr, ctx, env)?])
        }
        SyntaxHtmlNodeOutput::Element { element } => lower_syntax_html_element(element, ctx, env),
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            let mut chunks = Vec::new();
            for child in &slot.children {
                chunks.extend(lower_syntax_html_node(child, ctx, env)?);
            }
            Some(chunks)
        }
    }
}

fn lower_syntax_html_element(
    element: &SyntaxHtmlElementOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    let mut chunks = Vec::new();
    chunks.extend(lower_syntax_html_open_tag(element, ctx, env)?);
    for child in &element.children {
        chunks.extend(lower_syntax_html_node(child, ctx, env)?);
    }
    chunks.push(html_binary(&format!("</{}>", element.name)));
    Some(chunks)
}

fn lower_syntax_html_open_tag(
    element: &SyntaxHtmlElementOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    let mut chunks = Vec::new();
    let mut static_text = format!("<{}", element.name);

    for attr in &element.attrs {
        if let Some(rendered) = render_static_syntax_html_attr(attr) {
            static_text.push_str(&rendered);
            continue;
        }

        if !static_text.is_empty() {
            chunks.push(html_binary(&static_text));
            static_text.clear();
        }
        chunks.extend(lower_dynamic_syntax_html_attr(attr, ctx, env)?);
    }

    static_text.push('>');
    chunks.push(html_binary(&static_text));
    Some(chunks)
}

fn lower_dynamic_syntax_html_attr(
    attr: &SyntaxHtmlAttrOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    let Some(SyntaxHtmlAttrValueOutput::Expr { expr }) = &attr.value else {
        return Some(Vec::new());
    };

    Some(vec![
        html_binary(&format!(" {}=\"", attr.name)),
        ErlExpr::Call {
            module: Some("typer_html".to_string()),
            function: "escape".to_string(),
            args: vec![lower_syntax_expr_with_env(expr, ctx, env)?],
        },
        html_binary("\""),
    ])
}

fn render_static_syntax_html_attr(attr: &SyntaxHtmlAttrOutput) -> Option<String> {
    match &attr.value {
        None => Some(format!(" {}", attr.name)),
        Some(SyntaxHtmlAttrValueOutput::Text { text }) => {
            Some(format!(" {}=\"{}\"", attr.name, escape_html_attr(text)))
        }
        Some(SyntaxHtmlAttrValueOutput::Expr { expr }) => {
            render_static_syntax_html_attr_expr(&attr.name, expr)
                .map(|value| format!(" {}=\"{}\"", attr.name, escape_html_attr(&value)))
        }
    }
}

fn render_static_syntax_html_attr_expr(name: &str, expr: &SyntaxExprOutput) -> Option<String> {
    match (name, expr.kind) {
        ("class", SyntaxExprKind::List) => expr
            .children
            .iter()
            .map(|item| match item.kind {
                SyntaxExprKind::Binary => item.text.as_deref().map(static_html_attr_binary_text),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(|items| items.join(" ")),
        (_, SyntaxExprKind::Binary) => expr.text.as_deref().map(static_html_attr_binary_text),
        _ => None,
    }
}

fn lower_syntax_html_child_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if matches!(expr.kind, SyntaxExprKind::ListComprehension) {
        let value = expr.children.first()?;
        let source = expr.children.get(1)?;
        let pattern = expr.patterns.first()?;
        return Some(ErlExpr::ListComprehension {
            expr: Box::new(lower_syntax_html_child_expr(value, ctx, env)?),
            pattern: lower_syntax_pattern(pattern, ctx)?,
            source: Box::new(
                match lower_syntax_list_comprehension_source(source, ctx, env)? {
                    LoweredComprehensionSource::NativeList(source) => source,
                    LoweredComprehensionSource::IterableIterator(source) => source,
                },
            ),
            guard: match expr.children.get(2) {
                Some(guard) => Some(Box::new(lower_syntax_expr_with_env(guard, ctx, env)?)),
                None => None,
            },
        });
    }

    if matches!(expr.kind, SyntaxExprKind::Case) {
        let scrutinee = expr.children.first()?;
        return Some(ErlExpr::Case {
            scrutinee: Box::new(lower_syntax_expr_with_env(scrutinee, ctx, env)?),
            clauses: expr
                .clauses
                .iter()
                .map(|clause| {
                    let pattern = clause.patterns.first()?;
                    Some(ErlCaseClause {
                        pattern: lower_syntax_pattern(pattern, ctx)?,
                        guard: match clause.guard.as_deref() {
                            Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, env)?),
                            None => None,
                        },
                        body: lower_syntax_html_child_expr(&clause.body, ctx, env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        });
    }

    if matches!(expr.kind, SyntaxExprKind::HtmlBlock) {
        return lower_syntax_expr_with_env(expr, ctx, env);
    }

    if let Some(raw) = lower_syntax_html_raw_expr_with_env(expr, ctx, env) {
        return Some(raw);
    }

    Some(ErlExpr::Call {
        module: Some("typer_html".to_string()),
        function: "escape".to_string(),
        args: vec![lower_syntax_expr_with_env(expr, ctx, env)?],
    })
}

fn lower_syntax_template_instantiation(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let name = expr.text.as_deref()?;
    let template = ctx.templates.get(name)?;
    let values = expr
        .fields
        .iter()
        .map(|field| {
            Some((
                field.key.clone(),
                lower_syntax_html_raw_expr_with_env(&field.value, ctx, env)
                    .or_else(|| lower_syntax_expr_with_env(&field.value, ctx, env))?,
            ))
        })
        .collect::<Option<BTreeMap<_, _>>>()?;

    Some(ErlExpr::List(
        template
            .nodes
            .iter()
            .flat_map(|node| lower_syntax_template_node(node, &values, template, ctx))
            .collect(),
    ))
}

fn lower_syntax_template_node(
    node: &terlan_html::HtmlNode,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> Vec<ErlExpr> {
    match node {
        terlan_html::HtmlNode::Text(text) => vec![html_binary(text)],
        terlan_html::HtmlNode::Comment(text) => vec![html_binary(&format!("<!--{}-->", text))],
        terlan_html::HtmlNode::Doctype(text) => {
            vec![html_binary(&format!("<!DOCTYPE {}>", text))]
        }
        terlan_html::HtmlNode::Slot(slot) => {
            vec![lower_syntax_template_slot_text(slot, values, template, ctx)]
        }
        terlan_html::HtmlNode::Element(element) => {
            let mut chunks = Vec::new();
            chunks.extend(lower_syntax_template_open_tag(
                element, values, template, ctx,
            ));
            for child in &element.children {
                chunks.extend(lower_syntax_template_node(child, values, template, ctx));
            }
            chunks.push(html_binary(&format!("</{}>", element.name)));
            chunks
        }
    }
}

fn lower_syntax_template_open_tag(
    element: &terlan_html::HtmlElement,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> Vec<ErlExpr> {
    let mut chunks = Vec::new();
    let mut static_text = format!("<{}", element.name);

    for attr in &element.attrs {
        match &attr.value {
            None => static_text.push_str(&format!(" {}", attr.name)),
            Some(terlan_html::HtmlAttrValue::Text(value)) => {
                static_text.push_str(&format!(" {}=\"{}\"", attr.name, escape_html_attr(value)))
            }
            Some(terlan_html::HtmlAttrValue::Slot(slot)) => {
                if !static_text.is_empty() {
                    chunks.push(html_binary(&static_text));
                    static_text.clear();
                }
                chunks.push(html_binary(&format!(" {}=\"", attr.name)));
                chunks.push(lower_syntax_template_slot_escape(
                    slot, values, template, ctx,
                ));
                chunks.push(html_binary("\""));
            }
        }
    }

    static_text.push('>');
    chunks.push(html_binary(&static_text));
    chunks
}

fn lower_syntax_template_slot_escape(
    slot: &terlan_html::HtmlSlot,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> ErlExpr {
    let value = lower_syntax_template_slot_value(slot, values, template, ctx);
    ErlExpr::Call {
        module: Some("typer_html".to_string()),
        function: "escape".to_string(),
        args: vec![value],
    }
}

fn lower_syntax_template_slot_text(
    slot: &terlan_html::HtmlSlot,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> ErlExpr {
    if slot.path.len() == 1
        && slot
            .path
            .first()
            .and_then(|root| template.props.get(root))
            .is_some_and(|type_text| is_template_html_type(type_text))
    {
        return lower_syntax_template_slot_value(slot, values, template, ctx);
    }

    lower_syntax_template_slot_escape(slot, values, template, ctx)
}

fn lower_syntax_template_slot_value(
    slot: &terlan_html::HtmlSlot,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> ErlExpr {
    let Some(root) = slot.path.first() else {
        return html_binary("");
    };
    let mut value = values
        .get(root)
        .cloned()
        .unwrap_or_else(|| ErlExpr::Atom("undefined".to_string()));
    let mut current_type = template
        .props
        .get(root)
        .and_then(|type_text| simple_template_type_name(type_text))
        .map(str::to_string);

    for field in slot.path.iter().skip(1) {
        let Some(record_name) = current_type.clone() else {
            break;
        };
        value = ErlExpr::RecordAccess {
            value: Box::new(value),
            name: record_name.clone(),
            field: field.clone(),
        };
        current_type = ctx
            .struct_field_types
            .get(&record_name)
            .and_then(|fields| fields.get(field))
            .and_then(|type_text| simple_template_type_name(type_text))
            .map(str::to_string);
    }

    value
}

fn lower_syntax_html_raw_expr_with_env(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let SyntaxExprKind::Call = expr.kind else {
        return None;
    };
    let callee = expr.children.first()?;
    if expr.remote.as_deref() != Some("Html") || syntax_expr_name(callee)? != "raw" {
        return None;
    }
    let [trusted] = &expr.children.get(1..)? else {
        return None;
    };
    lower_syntax_expr_with_env(trusted, ctx, env)
}

fn lower_syntax_call_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let callee = expr.children.first()?;
    let args = &expr.children[1..];
    lower_syntax_call_parts(callee, args, expr.remote.as_deref(), ctx, env)
}

/// Lowers dedicated function-value invocation syntax.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` node created from `callee.(args)`.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang `Apply` expression that invokes the lowered callee value.
///
/// Transformation:
/// - Lowers the callee as an ordinary value, including local function captures
///   such as `fun increment/1`, lowers each argument, and emits callable-value
///   application. This keeps `Expr.(...)` separate from named `Name(...)`
///   calls in the backend.
fn lower_syntax_function_value_call_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let callee = expr.children.first()?;
    let args = expr.children[1..]
        .iter()
        .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
        .collect::<Option<Vec<_>>>()?;
    Some(ErlExpr::Apply {
        callee: Box::new(lower_syntax_expr_with_env(callee, ctx, env)?),
        args,
    })
}

/// Lowers a syntax-output let expression to an Erlang scoped sequence.
///
/// Inputs:
/// - `expr`: syntax-output let node with binding-name patterns and value
///   children.
/// - `ctx`, `env`: active syntax lowering context and lexical field/type
///   environment.
///
/// Output:
/// - `Some(ErlExpr::Let)` when the let shape and all values lower.
/// - `None` for malformed let output or unsupported child expressions.
///
/// Transformation:
/// - Converts each Terlan binding to an Erlang single-assignment variable
///   match, extending the local lowering environment with each simple binding
///   so later bindings and the final body can resolve receiver-method calls on
///   local values. When no explicit body child exists, the expression result is
///   the final binding variable.
fn lower_syntax_let_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if expr.patterns.is_empty()
        || expr.children.len() < expr.patterns.len()
        || expr.children.len() > expr.patterns.len() + 1
    {
        return None;
    }

    let mut let_env = env.clone();
    let bindings = expr
        .patterns
        .iter()
        .zip(expr.children.iter())
        .map(|(pattern, value)| {
            let name = pattern.text.as_deref()?;
            let lowered_value = lower_syntax_expr_with_env(value, ctx, &let_env)?;
            if let Some(value_type) = infer_syntax_trait_dispatch_type(value, &let_env) {
                let_env.value_types.insert(name.to_string(), value_type);
            }
            let_env.value_locals.insert(name.to_string());
            Some(ErlLetBinding {
                name: sanitize_erlang_var(name),
                value: lowered_value,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let body = match expr.children.get(expr.patterns.len()) {
        Some(body) => lower_syntax_expr_with_env(body, ctx, &let_env)?,
        None => ErlExpr::Var(bindings.last()?.name.clone()),
    };

    Some(ErlExpr::Let {
        bindings,
        body: Box::new(body),
    })
}

fn lower_syntax_call_parts(
    callee: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    remote: Option<&str>,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if remote.is_none() {
        if let Some(expr) = lower_syntax_list_each_receiver_method_call(callee, args, ctx, env) {
            return Some(expr);
        }
        if let Some(expr) = lower_syntax_primitive_receiver_method_call(callee, args, ctx, env) {
            return Some(expr);
        }
        if let Some(expr) = lower_syntax_receiver_method_call(callee, args, ctx, env) {
            return Some(expr);
        }
        if let Some((module, function)) = syntax_method_shaped_remote_call_parts(callee, ctx, env) {
            if let Some(expr) =
                lower_syntax_primitive_intrinsic_call(&module, &function, args, ctx, env)
            {
                return Some(expr);
            }
            return Some(ErlExpr::Call {
                module: Some(module),
                function,
                args: args
                    .iter()
                    .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                    .collect::<Option<Vec<_>>>()?,
            });
        }
    }

    let callee_name = syntax_expr_name(callee)?;

    if remote.is_none() {
        if args.len() == 1 && ctx.opaque_constructors.contains(callee_name) {
            return lower_syntax_expr_with_env(&args[0], ctx, env);
        }
        if let Some(target) = ctx.constructor_target(callee_name, args.len()) {
            return lower_syntax_explicit_constructor_call(target, args, ctx, env);
        }
        if let Some(target) = ctx.imported_constructor_target(callee_name, args.len()) {
            return lower_syntax_remote_constructor_call(target, args, ctx, env);
        }
        if let Some(target) = ctx.alias_constructor_call_target(callee_name, args.len()) {
            return lower_syntax_alias_constructor_expr(target, args, ctx, env);
        }
    } else if let Some(remote) = remote {
        if let Some(expr) =
            lower_syntax_primitive_intrinsic_call(remote, callee_name, args, ctx, env)
        {
            return Some(expr);
        }
        if let Some(expr) =
            lower_syntax_runtime_capability_call(remote, callee_name, args, ctx, env)
        {
            return Some(expr);
        }
        if let Some(target) = ctx.remote_constructor_target(remote, callee_name, args.len()) {
            return lower_syntax_remote_constructor_call(target, args, ctx, env);
        }
        if let Some(target) = ctx.remote_alias_constructor_target(remote, callee_name, args.len()) {
            return lower_syntax_alias_constructor_expr(target, args, ctx, env);
        }
        if let Some(expr) =
            lower_syntax_local_trait_receiver_method_call(remote, callee_name, args, ctx, env)
        {
            return Some(expr);
        }
        if let Some(expr) =
            lower_syntax_bound_trait_method_call(remote, callee_name, args, ctx, env)
        {
            return Some(expr);
        }
        let (trait_remote, explicit_trait_type_arg) = split_explicit_trait_call_target(remote);
        if let Some((module_name, source_trait_name)) = ctx.imported_trait_alias(&trait_remote) {
            if let Some(type_arg) = explicit_trait_type_arg
                .clone()
                .or_else(|| {
                    args.first()
                        .and_then(|arg| infer_syntax_trait_dispatch_type(arg, env))
                })
                .map(|type_arg| qualify_imported_type_text(&type_arg, &ctx.imported_type_refs))
            {
                if let Some(expr) = lower_syntax_std_trait_intrinsic_call(
                    module_name,
                    source_trait_name,
                    callee_name,
                    &type_arg,
                    args,
                    ctx,
                    env,
                ) {
                    return Some(expr);
                }
                if let Some(wrapper_type_arg) =
                    ctx.imported_trait_conformance_wrapper_type(&trait_remote, &type_arg)
                {
                    if let Some(expr) = lower_syntax_std_trait_intrinsic_call(
                        module_name,
                        source_trait_name,
                        callee_name,
                        wrapper_type_arg,
                        args,
                        ctx,
                        env,
                    ) {
                        return Some(expr);
                    }
                    let mut lowered_args = Vec::with_capacity(args.len() + 1);
                    lowered_args.push(trait_dictionary_expr(source_trait_name, callee_name));
                    lowered_args.extend(
                        args.iter()
                            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                            .collect::<Option<Vec<_>>>()?,
                    );
                    return Some(ErlExpr::Call {
                        module: Some(ctx.resolve_remote_module(module_name)),
                        function: typed_trait_method_wrapper_name(
                            source_trait_name,
                            callee_name,
                            wrapper_type_arg,
                        ),
                        args: lowered_args,
                    });
                }
            }
            let mut lowered_args = Vec::with_capacity(args.len() + 1);
            lowered_args.push(trait_dictionary_expr(source_trait_name, callee_name));
            lowered_args.extend(
                args.iter()
                    .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                    .collect::<Option<Vec<_>>>()?,
            );
            return Some(ErlExpr::Call {
                module: Some(ctx.resolve_remote_module(module_name)),
                function: trait_method_wrapper_name(source_trait_name, callee_name),
                args: lowered_args,
            });
        }
        if let Some(wrapper) = ctx.trait_method_wrapper(remote, callee_name) {
            let mut lowered_args = Vec::with_capacity(args.len() + 1);
            lowered_args.push(trait_dictionary_expr(remote, callee_name));
            lowered_args.extend(
                args.iter()
                    .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                    .collect::<Option<Vec<_>>>()?,
            );
            return Some(ErlExpr::Call {
                module: None,
                function: wrapper.clone(),
                args: lowered_args,
            });
        }
        if let Some(type_arg) = args
            .first()
            .and_then(|arg| infer_syntax_trait_dispatch_type(arg, env))
        {
            if let Some(wrapper) = ctx.typed_trait_method_wrapper(remote, callee_name, &type_arg) {
                let mut lowered_args = Vec::with_capacity(args.len() + 1);
                lowered_args.push(trait_dictionary_expr(remote, callee_name));
                lowered_args.extend(
                    args.iter()
                        .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                        .collect::<Option<Vec<_>>>()?,
                );
                return Some(ErlExpr::Call {
                    module: None,
                    function: wrapper.clone(),
                    args: lowered_args,
                });
            }
        }
    }

    if is_upper_identifier(callee_name) {
        return None;
    }

    if remote.is_none() && env.value_locals.contains(callee_name) {
        return Some(ErlExpr::Call {
            module: None,
            function: sanitize_erlang_var(callee_name),
            args: args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        });
    }

    if remote.is_none() {
        if let Some(target) = ctx.generic_function_target(callee_name, args.len()) {
            let mut lowered_args = lower_syntax_generic_bound_dictionaries(target, args, ctx, env)?;
            lowered_args.extend(
                args.iter()
                    .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                    .collect::<Option<Vec<_>>>()?,
            );
            return Some(ErlExpr::Call {
                module: None,
                function: callee_name.to_string(),
                args: lowered_args,
            });
        }
        if let Some((module, function)) = ctx.imported_function_target(callee_name, args.len()) {
            if let Some(expr) =
                lower_syntax_primitive_intrinsic_call(module, function, args, ctx, env)
            {
                return Some(expr);
            }
            if let Some(expr) =
                lower_syntax_runtime_capability_call(module, function, args, ctx, env)
            {
                return Some(expr);
            }
            return Some(ErlExpr::Call {
                module: Some(ctx.resolve_remote_module(module)),
                function: function.clone(),
                args: args
                    .iter()
                    .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                    .collect::<Option<Vec<_>>>()?,
            });
        }
    }

    Some(ErlExpr::Call {
        module: remote.map(|module| ctx.resolve_remote_module(module)),
        function: callee_name.to_string(),
        args: args
            .iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Builds hidden dictionaries for a concrete bounded generic function call.
///
/// Inputs:
/// - `target`: generic function metadata from the callee declaration.
/// - `args`: source-visible call arguments.
/// - `ctx`, `env`: syntax lowering context and caller lexical environment.
///
/// Output:
/// - One Erlang dictionary expression per declared generic bound.
/// - `None` when the concrete call cannot select a visible local impl wrapper.
///
/// Transformation:
/// - Infers simple type-variable substitutions from parameter annotations and
///   call arguments, resolves each bound to a concrete type, and creates a map
///   from trait method atoms to local typed wrapper function atoms.
fn lower_syntax_generic_bound_dictionaries(
    target: &SyntaxGenericFunctionTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    let substitutions = infer_generic_function_type_substitutions(&target.params, args, env)?;
    target
        .bounds
        .iter()
        .map(|bound| lower_syntax_generic_bound_dictionary(bound, &substitutions, ctx))
        .collect()
}

/// Infers generic type substitutions for a simple local generic call.
///
/// Inputs:
/// - `params`: callee parameter annotation texts.
/// - `args`: source-visible call arguments.
/// - `env`: caller lexical environment containing inferred value types.
///
/// Output:
/// - Map from type variable name to concrete inferred type key.
///
/// Transformation:
/// - Handles the first executable P0.5e.4 ABI shape where a parameter
///   annotation is a direct type variable such as `A`; more structural
///   matching can move into this helper later without changing the ABI.
fn infer_generic_function_type_substitutions(
    params: &[String],
    args: &[SyntaxExprOutput],
    env: &SyntaxLowerEnv,
) -> Option<BTreeMap<String, String>> {
    if params.len() != args.len() {
        return None;
    }
    let mut substitutions = BTreeMap::new();
    for (param, arg) in params.iter().zip(args.iter()) {
        if !is_generic_type_var(param) {
            continue;
        }
        let arg_type = infer_syntax_trait_dispatch_type(arg, env)?;
        substitutions.insert(param.clone(), arg_type);
    }
    Some(substitutions)
}

/// Builds one hidden dictionary for a concrete generic bound.
///
/// Inputs:
/// - `bound`: parsed source trait bound.
/// - `substitutions`: type variable substitutions inferred from call args.
/// - `ctx`: syntax lowering context containing local trait methods and impl
///   wrappers.
///
/// Output:
/// - Erlang map expression from method atom to typed impl wrapper atom.
///
/// Transformation:
/// - Resolves a one-argument trait bound such as `Eq[A]` to `Eq[Int]`, then
///   maps every known local trait method to its concrete typed wrapper.
fn lower_syntax_generic_bound_dictionary(
    bound: &SyntaxGenericFunctionBound,
    substitutions: &BTreeMap<String, String>,
    ctx: &SyntaxLowerCtx,
) -> Option<ErlExpr> {
    if bound.type_args.len() != 1 {
        return None;
    }
    let type_arg = substitutions
        .get(&bound.type_args[0])
        .cloned()
        .unwrap_or_else(|| bound.type_args[0].clone());
    let methods = ctx.local_trait_methods.get(&bound.trait_name)?;
    let entries = methods
        .iter()
        .map(|method| {
            let wrapper = ctx.typed_trait_method_wrapper(&bound.trait_name, method, &type_arg)?;
            Some(format!("{} => {}", render_atom_expr(method), wrapper))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(ErlExpr::Raw(format!("#{{{}}}", entries.join(", "))))
}

/// Lowers a trait method call satisfied by the current function's bounds.
///
/// Inputs:
/// - `trait_name`: trait qualifier from `Trait.method(...)`.
/// - `method`: method name.
/// - `args`: source call arguments.
/// - `ctx`, `env`: syntax lowering context and current function environment.
///
/// Output:
/// - Erlang expression that applies the function stored in the hidden bound
///   dictionary.
///
/// Transformation:
/// - Uses the first argument's inferred type key to find the matching hidden
///   dictionary, then emits `apply(?MODULE, maps:get(Method, Dict), [Dict|Args])`
///   so generic source code can call trait methods without knowing the concrete
///   implementation wrapper.
fn lower_syntax_bound_trait_method_call(
    trait_name: &str,
    method: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let type_arg = args
        .first()
        .and_then(|arg| infer_syntax_trait_dispatch_type(arg, env))?;
    let dict = env
        .trait_bound_dicts
        .get(&(trait_name.to_string(), type_arg))?;
    let mut rendered_args = Vec::with_capacity(args.len() + 1);
    rendered_args.push(dict.clone());
    rendered_args.extend(
        args.iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env).map(|expr| expr.render()))
            .collect::<Option<Vec<_>>>()?,
    );
    Some(ErlExpr::Raw(format!(
        "apply(?MODULE, maps:get({}, {}), [{}])",
        render_atom_expr(method),
        dict,
        rendered_args.join(", ")
    )))
}

/// Lowers selected std trait conformances through primitive intrinsics.
///
/// Inputs:
/// - `module_name`: provider module that owns the imported trait.
/// - `trait_name`: source trait name from the provider module.
/// - `method`: trait method name being called.
/// - `type_arg`: concrete conformance type selected from interface metadata.
/// - `args`: source-visible call arguments.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression for the primitive intrinsic when the imported std trait
///   conformance is compiler-owned.
/// - `None` for ordinary imported trait calls that should still use provider
///   wrappers.
///
/// Transformation:
/// - Keeps released std summary builds executable by mapping selected
///   std-owned conformances onto the same closed primitive intrinsic registry
///   used by direct std calls.
fn lower_syntax_std_trait_intrinsic_call(
    module_name: &str,
    trait_name: &str,
    method: &str,
    type_arg: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if let Some(expr) =
        lower_syntax_std_collection_trait_bridge(module_name, trait_name, method, args, ctx, env)
    {
        return Some(expr);
    }

    let intrinsic =
        std_trait_primitive_intrinsic(module_name, trait_name, method, type_arg, args.len())?;
    let lowered_args = args
        .iter()
        .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
        .collect::<Option<Vec<_>>>()?;
    lower_core_primitive_intrinsic_to_erlang(&intrinsic, lowered_args)
}

/// Lowers selected std collection trait calls through collection bridges.
///
/// Inputs:
/// - `module_name`: provider module that owns the imported trait.
/// - `trait_name`: source trait name from the provider module.
/// - `method`: trait method name being called.
/// - `args`: source-visible call arguments.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression for selected collection trait bridges.
/// - `None` when the trait call is not a closed std collection bridge.
///
/// Transformation:
/// - Reuses list-backed traversal bridges for std trait syntax so
///   `Enumerable.each(values, cb)`, `Enumerable.map(values, cb)`,
///   `Enumerable.filter(values, predicate)`, and
///   `Enumerable.fold(values, initial, reducer)` preserve the same backend
///   behavior for the selected `List[T]` conformance.
fn lower_syntax_std_collection_trait_bridge(
    module_name: &str,
    trait_name: &str,
    method: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    match (module_name, trait_name, method, args) {
        ("std.collections.Enumerable", "Enumerable", "each", [collection, callback]) => {
            let collection_type = infer_syntax_trait_dispatch_type(collection, env)?;
            if receiver_type_head(&collection_type) != "List" {
                return None;
            }
            lower_syntax_list_each_bridge(collection, callback, ctx, env)
        }
        ("std.collections.Enumerable", "Enumerable", "map", [collection, callback]) => {
            let collection_type = infer_syntax_trait_dispatch_type(collection, env)?;
            if receiver_type_head(&collection_type) != "List" {
                return None;
            }
            lower_syntax_list_map_bridge(collection, callback, ctx, env)
        }
        ("std.collections.Enumerable", "Enumerable", "filter", [collection, predicate]) => {
            let collection_type = infer_syntax_trait_dispatch_type(collection, env)?;
            if receiver_type_head(&collection_type) != "List" {
                return None;
            }
            lower_syntax_list_filter_bridge(collection, predicate, ctx, env)
        }
        ("std.collections.Enumerable", "Enumerable", "fold", [collection, initial, reducer]) => {
            let collection_type = infer_syntax_trait_dispatch_type(collection, env)?;
            if receiver_type_head(&collection_type) != "List" {
                return None;
            }
            lower_syntax_list_fold_bridge(collection, initial, reducer, ctx, env)
        }
        _ => None,
    }
}

/// Resolves a selected std trait conformance to a primitive intrinsic.
///
/// Inputs:
/// - `module_name`: canonical trait provider module.
/// - `trait_name`: trait declared by the provider module.
/// - `method`: trait method being called.
/// - `type_arg`: normalized concrete conformance type.
/// - `arity`: source-visible argument count for the call.
///
/// Output:
/// - Core primitive intrinsic for supported std trait conformances.
/// - `None` for unsupported traits, methods, types, or arities.
///
/// Transformation:
/// - Encodes executable std-facing conformance bridges. The bridge is
///   intentionally closed so user traits and non-selected std traits cannot
///   accidentally bypass provider wrapper generation.
fn std_trait_primitive_intrinsic(
    module_name: &str,
    trait_name: &str,
    method: &str,
    type_arg: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (module_name, trait_name, method, type_arg, arity) {
        ("std.core.String", "Show", "to_string", "Bool", 1) => {
            Some(CorePrimitiveIntrinsic::BoolToString)
        }
        ("std.core.String", "Show", "to_string", "Int", 1) => {
            Some(CorePrimitiveIntrinsic::IntToString)
        }
        ("std.core.String", "Show", "to_string", "Float", 1) => {
            Some(CorePrimitiveIntrinsic::FloatToString)
        }
        ("std.core.String", "Show", "to_string", "String", 1) => {
            Some(CorePrimitiveIntrinsic::StringToString)
        }
        ("std.core.String", "Parse", "from_string", "Bool", 1) => {
            Some(CorePrimitiveIntrinsic::BoolFromString)
        }
        ("std.core.String", "Parse", "from_string", "Int", 1) => {
            Some(CorePrimitiveIntrinsic::IntFromString)
        }
        ("std.core.String", "Parse", "from_string", "Float", 1) => {
            Some(CorePrimitiveIntrinsic::FloatFromString)
        }
        ("std.core.String", "Parse", "from_string", "String", 1) => {
            Some(CorePrimitiveIntrinsic::StringFromString)
        }
        ("std.core.Equal", "Equal", "equal", "Bool", 2)
        | ("std.core.Equal", "Equal", "equal", "Int", 2)
        | ("std.core.Equal", "Equal", "equal", "Float", 2)
        | ("std.core.Equal", "Equal", "equal", "Unit", 2)
        | ("std.core.Equal", "Equal", "equal", "Comparison", 2) => {
            Some(CorePrimitiveIntrinsic::BoolEqual)
        }
        ("std.core.Equal", "Equal", "equal", "String", 2) => {
            Some(CorePrimitiveIntrinsic::StringEqual)
        }
        ("std.collections.Iterable", "Iterable", "iterator", type_arg, 1)
            if receiver_type_head(type_arg) == "List" =>
        {
            Some(CorePrimitiveIntrinsic::ListIterator)
        }
        _ => None,
    }
}

/// Lowers declaration-site trait dispatch to a receiver-method call.
///
/// Inputs:
/// - `trait_name`: remote segment from a call such as `Show.to_string(value)`.
/// - `method`: trait method segment.
/// - `args`: source call arguments, where the first argument is the receiver
///   value required by the trait method.
/// - `ctx`, `env`: syntax lowering context and local type environment.
///
/// Output:
/// - `Some(ErlExpr::Call)` when a local trait declares the method and the first
///   argument's inferred type has a matching local receiver method.
/// - `None` when the call is not a supported declaration-site trait dispatch.
///
/// Transformation:
/// - Reuses the existing receiver-method backend ABI: the trait call's first
///   argument becomes the receiver argument to the generated Erlang function,
///   followed by any additional trait method arguments.
fn lower_syntax_local_trait_receiver_method_call(
    trait_name: &str,
    method: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !ctx.has_local_trait_method(trait_name, method) {
        return None;
    }

    let receiver = args.first()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, env)?;
    let method_arity = args.len().checked_sub(1)?;
    ctx.receiver_method_target(&receiver_type, method, method_arity)?;

    Some(ErlExpr::Call {
        module: None,
        function: method.to_string(),
        args: args
            .iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Lowers selected primitive std calls through compiler-owned intrinsics.
///
/// Inputs:
/// - `module`: source module path or alias owning the primitive operation.
/// - `function`: source function name.
/// - `args`: source argument expressions.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression for the primitive intrinsic when the call is selected.
/// - `None` for non-primitive calls or unsupported arity.
///
/// Transformation:
/// - Resolves module aliases, lowers arguments once through the syntax bridge,
///   maps portable `std.core.*` primitive APIs to CoreIR intrinsic identities,
///   and delegates to the shared CoreIR primitive BEAM lowering.
fn lower_syntax_primitive_intrinsic_call(
    module: &str,
    function: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let resolved_module = ctx.resolve_remote_module(module);
    let intrinsic = primitive_function_intrinsic(resolved_module.as_str(), function, args.len())?;
    let lowered_args = args
        .iter()
        .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
        .collect::<Option<Vec<_>>>()?;
    lower_core_primitive_intrinsic_to_erlang(&intrinsic, lowered_args)
}

/// Resolves a primitive std function call to its compiler-owned intrinsic.
///
/// Inputs:
/// - `module`: canonical std module path.
/// - `function`: source function name.
/// - `arity`: number of source arguments.
///
/// Output:
/// - Core primitive intrinsic id for selected primitive operations.
///
/// Transformation:
/// - Mirrors the CoreIR primitive registry at the transitional syntax bridge
///   boundary so selected imports and fully qualified primitive calls do not
///   emit calls to non-existent backend std modules.
fn primitive_function_intrinsic(
    module: &str,
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (module, function, arity) {
        ("std.core.Bool", "compare", 2) => Some(CorePrimitiveIntrinsic::BoolCompare),
        ("std.core.Bool", "to_string", 1) => Some(CorePrimitiveIntrinsic::BoolToString),
        ("std.core.Bool", "from_string", 1) => Some(CorePrimitiveIntrinsic::BoolFromString),
        ("std.core.Int", "to_string", 1) => Some(CorePrimitiveIntrinsic::IntToString),
        ("std.core.Int", "from_string", 1) => Some(CorePrimitiveIntrinsic::IntFromString),
        ("std.core.Float", "to_string", 1) => Some(CorePrimitiveIntrinsic::FloatToString),
        ("std.core.Float", "from_string", 1) => Some(CorePrimitiveIntrinsic::FloatFromString),
        ("std.core.String", "compare", 2) => Some(CorePrimitiveIntrinsic::StringCompare),
        ("std.core.String", "to_string", 1) => Some(CorePrimitiveIntrinsic::StringToString),
        ("std.core.String", "from_string", 1) => Some(CorePrimitiveIntrinsic::StringFromString),
        ("std.core.String", "is_empty", 1) => Some(CorePrimitiveIntrinsic::StringIsEmpty),
        ("std.core.String", "append", 2) => Some(CorePrimitiveIntrinsic::StringAppend),
        ("std.core.String", "concat", 1) => Some(CorePrimitiveIntrinsic::StringConcat),
        ("std.core.String", "contains", 2) => Some(CorePrimitiveIntrinsic::StringContains),
        ("std.core.String", "starts_with", 2) => Some(CorePrimitiveIntrinsic::StringStartsWith),
        ("std.core.String", "ends_with", 2) => Some(CorePrimitiveIntrinsic::StringEndsWith),
        ("std.core.String", "length", 1) => Some(CorePrimitiveIntrinsic::StringLength),
        ("std.core.String", "byte_size", 1) => Some(CorePrimitiveIntrinsic::StringByteSize),
        ("std.core.String", "lowercase", 1) => Some(CorePrimitiveIntrinsic::StringLowercase),
        ("std.core.String", "uppercase", 1) => Some(CorePrimitiveIntrinsic::StringUppercase),
        ("std.core.String", "trim", 1) => Some(CorePrimitiveIntrinsic::StringTrim),
        ("std.core.String", "trim_start", 1) => Some(CorePrimitiveIntrinsic::StringTrimStart),
        ("std.core.String", "trim_end", 1) => Some(CorePrimitiveIntrinsic::StringTrimEnd),
        ("std.core.String", "replace", 3) => Some(CorePrimitiveIntrinsic::StringReplace),
        ("std.core.String", "split", 2) => Some(CorePrimitiveIntrinsic::StringSplit),
        ("std.core.String", "split_once", 2) => Some(CorePrimitiveIntrinsic::StringSplitOnce),
        ("std.collections.List", "new", 0) => Some(CorePrimitiveIntrinsic::ListNew),
        ("std.collections.List", "is_empty", 1) => Some(CorePrimitiveIntrinsic::ListIsEmpty),
        ("std.collections.List", "length", 1) => Some(CorePrimitiveIntrinsic::ListLength),
        ("std.collections.List", "first", 1) => Some(CorePrimitiveIntrinsic::ListFirst),
        ("std.collections.List", "iterator", 1) => Some(CorePrimitiveIntrinsic::ListIterator),
        ("std.collections.List", "push", 2) => Some(CorePrimitiveIntrinsic::ListPush),
        ("std.collections.List", "clear", 1) => Some(CorePrimitiveIntrinsic::ListClear),
        ("std.collections.Iterator", "next", 1) => Some(CorePrimitiveIntrinsic::IteratorNext),
        ("std.collections.Map", "new", 0) => Some(CorePrimitiveIntrinsic::MapNew),
        ("std.collections.Map", "is_empty", 1) => Some(CorePrimitiveIntrinsic::MapIsEmpty),
        ("std.collections.Map", "size", 1) => Some(CorePrimitiveIntrinsic::MapSize),
        ("std.collections.Map", "get", 2) => Some(CorePrimitiveIntrinsic::MapGet),
        ("std.collections.Map", "contains_key", 2) => Some(CorePrimitiveIntrinsic::MapContainsKey),
        ("std.collections.Map", "put", 3) => Some(CorePrimitiveIntrinsic::MapPut),
        ("std.collections.Map", "remove", 2) => Some(CorePrimitiveIntrinsic::MapRemove),
        ("std.collections.Map", "clear", 1) => Some(CorePrimitiveIntrinsic::MapClear),
        ("std.collections.Map", "iterator", 1) => Some(CorePrimitiveIntrinsic::MapIterator),
        ("std.collections.Set", "new", 0) => Some(CorePrimitiveIntrinsic::SetNew),
        ("std.collections.Set", "is_empty", 1) => Some(CorePrimitiveIntrinsic::SetIsEmpty),
        ("std.collections.Set", "size", 1) => Some(CorePrimitiveIntrinsic::SetSize),
        ("std.collections.Set", "contains", 2) => Some(CorePrimitiveIntrinsic::SetContains),
        ("std.collections.Set", "add", 2) => Some(CorePrimitiveIntrinsic::SetAdd),
        ("std.collections.Set", "remove", 2) => Some(CorePrimitiveIntrinsic::SetRemove),
        ("std.collections.Set", "clear", 1) => Some(CorePrimitiveIntrinsic::SetClear),
        ("std.collections.Set", "iterator", 1) => Some(CorePrimitiveIntrinsic::SetIterator),
        _ => None,
    }
}

/// Lowers selected std runtime capability calls from the direct syntax emitter.
///
/// Inputs:
/// - `module`: source module path or alias at the call boundary.
/// - `function`: source function name.
/// - `args`: source argument expressions.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - `Some(ErlExpr)` for runtime capabilities supported by the direct Erlang
///   syntax bridge emitter.
/// - `None` for ordinary source calls or malformed arguments.
///
/// Transformation:
/// - Resolves source module aliases, lowers arguments through the normal syntax
///   expression path, and delegates to the same backend runtime capability
///   lowering used by CoreIR emission.
fn lower_syntax_runtime_capability_call(
    module: &str,
    function: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let resolved_module = ctx.resolve_remote_module(module);
    match (resolved_module.as_str(), function, args.len()) {
        ("std.io.Console", "println", 1) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_console_println(lowered_args)
        }
        ("std.io.File", "exists", 1) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_file_exists(lowered_args)
        }
        ("std.io.File", "read_text", 1) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_file_read_text(lowered_args)
        }
        ("std.io.File", "write_text", 2) => {
            let lowered_args = args
                .iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?;
            lower_runtime_file_write_text(lowered_args)
        }
        _ => None,
    }
}

/// Lowers the release `List.each(cb)` receiver traversal consumer.
///
/// Inputs:
/// - `callee`: field-access callee from a method call expression.
/// - `args`: expected single callback expression after the receiver.
/// - `ctx`: syntax lowering context for lowering callback and receiver
///   expressions.
/// - `env`: local type environment used to prove the receiver is a `List`.
///
/// Output:
/// - `Some(ErlExpr)` for `list.each(cb)` when the receiver type resolves to
///   `List[...]`.
/// - `None` for non-field callees, other methods, wrong arity, or non-list
///   receivers.
///
/// Transformation:
/// - Implements the source contract `Iterator.each(list.iterator(), cb)` on
///   the BEAM backend by applying the callback over the immutable list with
///   `lists:foreach/2`, then normalizing the result to Terlan `Unit`.
fn lower_syntax_list_each_receiver_method_call(
    callee: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    if callee.text.as_deref()? != "each" || args.len() != 1 {
        return None;
    }
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, env)?;
    if receiver_type_head(&receiver_type) != "List" {
        return None;
    }
    lower_syntax_list_each_bridge(receiver, &args[0], ctx, env)
}

/// Lowers a list plus callback to the selected BEAM foreach bridge.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the list being traversed.
/// - `callback`: source expression that evaluates to a function value.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression that applies the callback to each list value and returns
///   Terlan `Unit`.
/// - `None` when either expression cannot be lowered by the syntax emitter.
///
/// Transformation:
/// - Emits `lists:foreach/2` with the lowered callback and receiver, then wraps
///   the target result as `unit` so both receiver and trait-facing `each`
///   preserve Terlan's command-style return contract.
fn lower_syntax_list_each_bridge(
    receiver: &SyntaxExprOutput,
    callback: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_receiver = lower_syntax_expr_with_env(receiver, ctx, env)?;
    let lowered_callback = lower_syntax_expr_with_env(callback, ctx, env)?;
    Some(ErlExpr::Raw(format!(
        "begin lists:foreach({}, {}), unit end",
        lowered_callback.render(),
        lowered_receiver.render()
    )))
}

/// Lowers a list plus callback to the selected BEAM map bridge.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the list being transformed.
/// - `callback`: source expression that evaluates to a function value.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression that applies the callback to each list value and returns
///   the transformed list.
/// - `None` when either expression cannot be lowered by the syntax emitter.
///
/// Transformation:
/// - Emits `lists:map/2` with the lowered callback and receiver so
///   `Enumerable.map(values, cb)` has a closed backend bridge while the source
///   contract remains trait-shaped and target-neutral.
fn lower_syntax_list_map_bridge(
    receiver: &SyntaxExprOutput,
    callback: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_receiver = lower_syntax_expr_with_env(receiver, ctx, env)?;
    let lowered_callback = lower_syntax_expr_with_env(callback, ctx, env)?;
    Some(ErlExpr::Raw(format!(
        "lists:map({}, {})",
        lowered_callback.render(),
        lowered_receiver.render()
    )))
}

/// Lowers a list plus predicate callback to the selected BEAM filter bridge.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the list being filtered.
/// - `predicate`: source expression that evaluates to a boolean callback.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression that keeps each list value for which the predicate
///   returns true.
/// - `None` when either expression cannot be lowered by the syntax emitter.
///
/// Transformation:
/// - Emits `lists:filter/2` with the lowered predicate and receiver so
///   `Enumerable.filter(values, predicate)` has a closed backend bridge while
///   the source contract remains trait-shaped and target-neutral.
fn lower_syntax_list_filter_bridge(
    receiver: &SyntaxExprOutput,
    predicate: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_receiver = lower_syntax_expr_with_env(receiver, ctx, env)?;
    let lowered_predicate = lower_syntax_expr_with_env(predicate, ctx, env)?;
    Some(ErlExpr::Raw(format!(
        "lists:filter({}, {})",
        lowered_predicate.render(),
        lowered_receiver.render()
    )))
}

/// Lowers a list, initial accumulator, and reducer to the BEAM fold bridge.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the list being folded.
/// - `initial`: source expression that evaluates to the initial accumulator.
/// - `reducer`: source expression that evaluates to `(U, T) -> U`.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression that folds values from left to right and returns the
///   final accumulator.
/// - `None` when any expression cannot be lowered by the syntax emitter.
///
/// Transformation:
/// - Emits `lists:foldl/3` while adapting Erlang's `(Value, Acc)` callback
///   convention to Terlan's accumulator-first reducer shape `(Acc, Value)`.
fn lower_syntax_list_fold_bridge(
    receiver: &SyntaxExprOutput,
    initial: &SyntaxExprOutput,
    reducer: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_receiver = lower_syntax_expr_with_env(receiver, ctx, env)?;
    let lowered_initial = lower_syntax_expr_with_env(initial, ctx, env)?;
    let lowered_reducer = lower_syntax_expr_with_env(reducer, ctx, env)?;
    Some(ErlExpr::Raw(format!(
        "lists:foldl(fun(TerlanFoldValue, TerlanFoldAcc) -> ({reducer})(TerlanFoldAcc, TerlanFoldValue) end, {initial}, {receiver})",
        reducer = lowered_reducer.render(),
        initial = lowered_initial.render(),
        receiver = lowered_receiver.render(),
    )))
}

/// Lowers a local receiver-method call.
///
/// Inputs:
/// - `callee`: field-access callee from a method call expression.
/// - `args`: ordinary method arguments after the receiver.
/// - `ctx`: syntax lowering context containing local receiver-method identity.
/// - `env`: lexical environment used for conservative receiver type inference.
///
/// Output:
/// - `Some(ErlExpr::Call)` when the current module declares the selected
///   receiver method for the inferred receiver type.
/// - `None` when the callee is not a local receiver-method call.
///
/// Transformation:
/// - Rewrites `receiver.method(args...)` to the backend receiver-first calling
///   convention `method(receiver, args...)`, matching how method declarations
///   are lowered.
fn lower_syntax_receiver_method_call(
    callee: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, env)?;
    let receiver_target = ctx.receiver_method_target(&receiver_type, method, args.len())?;
    let _receiver_is_mutable = receiver_target.mutable;

    let mut lowered_args = Vec::with_capacity(args.len() + 1);
    lowered_args.push(lower_syntax_expr_with_env(receiver, ctx, env)?);
    lowered_args.extend(
        args.iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    );

    Some(ErlExpr::Call {
        module: None,
        function: method.to_string(),
        args: lowered_args,
    })
}

/// Lowers compiler-known primitive receiver method calls.
///
/// Inputs:
/// - `callee`: field-access callee from a method call expression.
/// - `args`: ordinary call arguments after the receiver.
/// - `ctx`: syntax lowering context for module aliases and expression lowering.
/// - `env`: local type environment used to infer receiver primitive type.
///
/// Output:
/// - `Some(ErlExpr::Call)` for known primitive receiver methods.
/// - `None` when the callee is not a primitive method call.
///
/// Transformation:
/// - Rewrites primitive receiver calls such as `"abc".trim()` or
///   `1.to_string()` into CoreIR primitive intrinsic calls and delegates to the
///   shared CoreIR intrinsic Erlang lowerer.
fn lower_syntax_primitive_receiver_method_call(
    callee: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, env)?;
    let intrinsic = primitive_receiver_method_intrinsic(&receiver_type, method, args.len())?;
    let mut lowered_args = Vec::with_capacity(args.len() + 1);
    lowered_args.push(lower_syntax_expr_with_env(receiver, ctx, env)?);
    lowered_args.extend(
        args.iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    );

    lower_core_primitive_intrinsic_to_erlang(&intrinsic, lowered_args)
}

/// Resolves a primitive receiver method to its compiler-owned intrinsic.
///
/// Inputs:
/// - `receiver_type`: normalized source type inferred for the receiver.
/// - `method`: method name from the field-access callee.
/// - `arg_count`: number of non-receiver arguments.
///
/// Output:
/// - Core primitive intrinsic id for supported primitive receiver calls.
/// - `None` for unsupported receiver types, methods, or arities.
///
/// Transformation:
/// - Keeps primitive receiver dispatch closed and explicit so source method
///   syntax cannot accidentally call arbitrary backend modules.
fn primitive_receiver_method_intrinsic(
    receiver_type: &str,
    method: &str,
    arg_count: usize,
) -> Option<CorePrimitiveIntrinsic> {
    if let Some(intrinsic) = collection_receiver_method_intrinsic(receiver_type, method, arg_count)
    {
        return Some(intrinsic);
    }

    match (receiver_type, method, arg_count) {
        ("Int", "to_string", 0) => Some(CorePrimitiveIntrinsic::IntToString),
        ("Float", "to_string", 0) => Some(CorePrimitiveIntrinsic::FloatToString),
        ("String", "compare", 1) => Some(CorePrimitiveIntrinsic::StringCompare),
        ("String", "to_string", 0) => Some(CorePrimitiveIntrinsic::StringToString),
        ("String", "from_string", 0) => Some(CorePrimitiveIntrinsic::StringFromString),
        ("String", "is_empty", 0) => Some(CorePrimitiveIntrinsic::StringIsEmpty),
        ("String", "append", 1) => Some(CorePrimitiveIntrinsic::StringAppend),
        ("String", "contains", 1) => Some(CorePrimitiveIntrinsic::StringContains),
        ("String", "starts_with", 1) => Some(CorePrimitiveIntrinsic::StringStartsWith),
        ("String", "ends_with", 1) => Some(CorePrimitiveIntrinsic::StringEndsWith),
        ("String", "replace", 2) => Some(CorePrimitiveIntrinsic::StringReplace),
        ("String", "split", 1) => Some(CorePrimitiveIntrinsic::StringSplit),
        ("String", "split_once", 1) => Some(CorePrimitiveIntrinsic::StringSplitOnce),
        ("String", "length", 0) => Some(CorePrimitiveIntrinsic::StringLength),
        ("String", "byte_size", 0) => Some(CorePrimitiveIntrinsic::StringByteSize),
        ("String", "lowercase", 0) => Some(CorePrimitiveIntrinsic::StringLowercase),
        ("String", "uppercase", 0) => Some(CorePrimitiveIntrinsic::StringUppercase),
        ("String", "trim", 0) => Some(CorePrimitiveIntrinsic::StringTrim),
        ("String", "trim_start", 0) => Some(CorePrimitiveIntrinsic::StringTrimStart),
        ("String", "trim_end", 0) => Some(CorePrimitiveIntrinsic::StringTrimEnd),
        _ => None,
    }
}

/// Resolves collection receiver methods to compiler-owned intrinsics.
///
/// Inputs:
/// - `receiver_type`: normalized source type inferred for the receiver.
/// - `method`: source receiver method name.
/// - `arg_count`: number of non-receiver call arguments.
///
/// Output:
/// - Core primitive intrinsic id for supported collection receiver calls.
/// - `None` for unsupported collection types, methods, or arities.
///
/// Transformation:
/// - Extracts the nominal type head from generic or qualified collection type
///   text and maps portable receiver methods to backend-neutral collection
///   intrinsic IDs.
fn collection_receiver_method_intrinsic(
    receiver_type: &str,
    method: &str,
    arg_count: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (
        receiver_type_head(receiver_type).as_str(),
        method,
        arg_count,
    ) {
        ("List", "is_empty", 0) => Some(CorePrimitiveIntrinsic::ListIsEmpty),
        ("List", "length", 0) => Some(CorePrimitiveIntrinsic::ListLength),
        ("List", "first", 0) => Some(CorePrimitiveIntrinsic::ListFirst),
        ("List", "iterator", 0) => Some(CorePrimitiveIntrinsic::ListIterator),
        ("List", "push", 1) => Some(CorePrimitiveIntrinsic::ListPush),
        ("List", "clear", 0) => Some(CorePrimitiveIntrinsic::ListClear),
        ("Map", "is_empty", 0) => Some(CorePrimitiveIntrinsic::MapIsEmpty),
        ("Map", "size", 0) => Some(CorePrimitiveIntrinsic::MapSize),
        ("Map", "get", 1) => Some(CorePrimitiveIntrinsic::MapGet),
        ("Map", "contains_key", 1) => Some(CorePrimitiveIntrinsic::MapContainsKey),
        ("Map", "put", 2) => Some(CorePrimitiveIntrinsic::MapPut),
        ("Map", "remove", 1) => Some(CorePrimitiveIntrinsic::MapRemove),
        ("Map", "clear", 0) => Some(CorePrimitiveIntrinsic::MapClear),
        ("Map", "iterator", 0) => Some(CorePrimitiveIntrinsic::MapIterator),
        ("Set", "is_empty", 0) => Some(CorePrimitiveIntrinsic::SetIsEmpty),
        ("Set", "size", 0) => Some(CorePrimitiveIntrinsic::SetSize),
        ("Set", "contains", 1) => Some(CorePrimitiveIntrinsic::SetContains),
        ("Set", "add", 1) => Some(CorePrimitiveIntrinsic::SetAdd),
        ("Set", "remove", 1) => Some(CorePrimitiveIntrinsic::SetRemove),
        ("Set", "clear", 0) => Some(CorePrimitiveIntrinsic::SetClear),
        ("Set", "iterator", 0) => Some(CorePrimitiveIntrinsic::SetIterator),
        _ => None,
    }
}

/// Extracts the nominal type head from receiver type text.
///
/// Inputs:
/// - `receiver_type`: normalized source type text, optionally generic or
///   qualified.
///
/// Output:
/// - Final nominal type segment without generic arguments.
///
/// Transformation:
/// - Compacts type-application spacing, strips generic arguments after `[`,
///   and keeps the segment after the final module qualifier.
fn receiver_type_head(receiver_type: &str) -> String {
    let compact = compact_type_application(&compact_spaces(receiver_type));
    let head = compact
        .split_once('[')
        .map_or(compact.as_str(), |(head, _)| head);
    head.rsplit('.').next().unwrap_or(head).to_string()
}

/// Extracts module/function names from method-shaped remote-call syntax.
///
/// Inputs:
/// - `callee`: the syntax-output callee of a parsed call expression.
/// - `ctx`: syntax lowering context used to resolve imported module aliases.
/// - `env`: local value environment used to distinguish receiver methods from
///   module calls.
///
/// Output:
/// - `Some((module, function))` for two-part calls whose receiver is not a
///   local value binding, otherwise `None`.
///
/// Transformation:
/// - Recognizes the field-access tree produced by the canonical
///   `MethodCallSuffix` parser and reclassifies non-local receiver names as
///   Erlang remote calls for the syntax bridge. Local receiver names
///   stay outside this path and must be handled by later semantic method
///   resolution.
fn syntax_method_shaped_remote_call_parts(
    callee: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<(String, String)> {
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let function = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let module = syntax_expr_name(receiver)?;
    if env.value_locals.contains(module) {
        None
    } else {
        Some((ctx.resolve_remote_module(module), function.to_string()))
    }
}

fn lower_syntax_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(right.kind, SyntaxExprKind::Call) {
        return Some(ErlExpr::Raw(format!(
            "{} |> {}",
            lower_syntax_expr_with_env(left, ctx, env)?.render(),
            lower_syntax_expr_with_env(right, ctx, env)?.render()
        )));
    }

    if let Some(expr) = lower_syntax_mutable_receiver_pipe_forward(left, right, ctx, env) {
        return Some(expr);
    }

    let callee = right.children.first()?;
    let mut args = Vec::with_capacity(right.children.len());
    args.push(left.clone());
    args.extend(right.children.iter().skip(1).cloned());
    lower_syntax_call_parts(callee, &args, right.remote.as_deref(), ctx, env)
}

/// Lowers mutable receiver-method pipe forwarding.
///
/// Inputs:
/// - `left`: source pipe receiver expression.
/// - `right`: source call expression on the right side of `|>`.
/// - `ctx`, `env`: syntax lowering context and lexical type environment.
///
/// Output:
/// - `Some(ErlExpr::Let)` when `right` names a declared mutable receiver method
///   for the inferred receiver type.
/// - `None` for non-call right sides, remote calls, non-method calls,
///   immutable receiver methods, or expressions whose receiver type cannot be
///   inferred from the syntax lowering environment.
///
/// Transformation:
/// - Rewrites `receiver |> mut_method(args...)` into a backend-local binding
///   whose value is the hidden updated receiver returned by the lowered mutable
///   method function. The binding result becomes the pipe expression value so
///   later pipe steps receive the updated receiver.
fn lower_syntax_mutable_receiver_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(right.kind, SyntaxExprKind::Call) || right.remote.is_some() {
        return None;
    }

    let callee = right.children.first()?;
    let method = syntax_expr_name(callee)?;
    let arity = right.children.len().checked_sub(1)?;
    let receiver_type = infer_syntax_pipe_receiver_type(left, env)?;
    let receiver_target = ctx.receiver_method_target(&receiver_type, method, arity)?;
    if !receiver_target.mutable {
        return None;
    }

    let mut lowered_args = Vec::with_capacity(arity + 1);
    lowered_args.push(lower_syntax_expr_with_env(left, ctx, env)?);
    lowered_args.extend(
        right
            .children
            .iter()
            .skip(1)
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    );

    let updated_receiver = "_TerlanMutReceiver".to_string();
    Some(ErlExpr::Let {
        bindings: vec![ErlLetBinding {
            name: updated_receiver.clone(),
            value: ErlExpr::Call {
                module: None,
                function: method.to_string(),
                args: lowered_args,
            },
        }],
        body: Box::new(ErlExpr::Var(updated_receiver)),
    })
}

/// Infers the receiver type that should flow through a pipe chain.
///
/// Inputs:
/// - `expr`: source expression used as a pipe receiver.
/// - `env`: lexical lowering environment containing known value types.
///
/// Output:
/// - Normalized receiver type text when the receiver can be inferred.
/// - `None` when the expression shape has no receiver-type evidence.
///
/// Transformation:
/// - Reads ordinary expression receiver types through the existing trait
///   dispatch inference helper. For nested pipe expressions, follows the left
///   side of the pipe because mutable receiver pipe lowering preserves the
///   original receiver type across each mutating step.
fn infer_syntax_pipe_receiver_type(
    expr: &SyntaxExprOutput,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    if matches!(expr.kind, SyntaxExprKind::BinaryOp) && expr.operator.as_deref() == Some("|>") {
        return infer_syntax_pipe_receiver_type(expr.children.first()?, env);
    }

    infer_syntax_trait_dispatch_type(expr, env)
}

fn lower_syntax_constructor_chain(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let base = expr.children.first()?;
    let record = expr.children.get(1)?;

    let lowered_base = lower_syntax_expr_with_env(base, ctx, env)?;
    let lowered_record = lower_syntax_constructor_extension_record(record, ctx, env)?;

    Some(ErlExpr::Raw(format!(
        "begin\n    {},\n    {}\nend",
        lowered_base.render(),
        lowered_record.render()
    )))
}

/// Lowers the derived side of constructor extension into an Erlang tuple.
///
/// Inputs:
/// - `record`: syntax-output record-construction node used after `with`.
/// - `ctx`: syntax lowering context with imports, templates, and constructor
///   metadata.
/// - `env`: local lowering environment for parameter/field-sensitive rewrites.
///
/// Output:
/// - `Some(ErlExpr::Tuple)` containing the derived shape tag followed by field
///   values in source order.
/// - `None` when the right side is not record-construction syntax or a field
///   value cannot lower.
///
/// Transformation:
/// - Treats `Base(args) with Derived { field = value }` as constructor-style
///   shape composition for the current formal Erlang path by emitting
///   `{Derived, Value}` instead of an Erlang record literal. This avoids
///   generating undeclared `#derived{}` references for constructor-extension
///   shapes that are not source-level structs.
fn lower_syntax_constructor_extension_record(
    record: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if record.kind != SyntaxExprKind::RecordConstruct {
        return None;
    }

    let mut items = Vec::with_capacity(record.fields.len() + 1);
    items.push(ErlExpr::Atom(record.text.clone()?));
    items.extend(
        record
            .fields
            .iter()
            .map(|field| lower_syntax_expr_with_env(&field.value, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    );
    Some(ErlExpr::Tuple(items))
}

fn lower_syntax_expr_field(
    field: &SyntaxExprFieldOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlMapField> {
    Some(ErlMapField {
        key: field.key.clone(),
        value: lower_syntax_expr_with_env(&field.value, ctx, env)?,
        required: field.required,
    })
}

/// Lowers one Terlan map-construction field to an Erlang map field.
///
/// Inputs are the formal syntax field, lowering context, and current
/// expression environment. Output is a lowered Erlang map field when the value
/// expression lowers successfully. The transformation intentionally emits
/// associative construction (`=>`) because Erlang reserves `:=` for matching
/// or updating existing keys, while Terlan permits `:=` in source map literals
/// as a required-key notation.
fn lower_syntax_map_expr_field(
    field: &SyntaxExprFieldOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlMapField> {
    Some(ErlMapField {
        key: field.key.clone(),
        value: lower_syntax_expr_with_env(&field.value, ctx, env)?,
        required: false,
    })
}

fn lower_syntax_explicit_constructor_call(
    target: &SyntaxConstructorTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_args = if target.varargs {
        let mut lowered = args
            .iter()
            .take(target.fixed_arity)
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?;
        lowered.push(ErlExpr::List(
            args.iter()
                .skip(target.fixed_arity)
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        ));
        lowered
    } else {
        let mut lowered = args
            .iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?;
        for default in target.defaults.iter().skip(args.len()).flatten() {
            lowered.push(lower_syntax_expr_with_env(default, ctx, env)?);
        }
        lowered
    };

    Some(ErlExpr::Call {
        module: None,
        function: target.function.clone(),
        args: lowered_args,
    })
}

fn lower_syntax_remote_constructor_call(
    target: &SyntaxRemoteConstructorTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_args = if target.varargs {
        let mut lowered = args
            .iter()
            .take(target.fixed_arity)
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?;
        lowered.push(ErlExpr::List(
            args.iter()
                .skip(target.fixed_arity)
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        ));
        lowered
    } else {
        args.iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?
    };

    Some(ErlExpr::Call {
        module: Some(target.module.clone()),
        function: target.function.clone(),
        args: lowered_args,
    })
}

fn lower_syntax_alias_constructor_expr(
    target: &SyntaxAliasConstructorTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let bindings = target
        .params
        .iter()
        .cloned()
        .zip(args.iter())
        .collect::<BTreeMap<_, _>>();
    syntax_expr_to_alias_constructor_expr(&target.body, &bindings, ctx, env)
}

fn syntax_expr_to_alias_constructor_expr(
    expr: &SyntaxExprOutput,
    bindings: &BTreeMap<String, &SyntaxExprOutput>,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    match expr.kind {
        SyntaxExprKind::Atom => Some(ErlExpr::Atom(expr.text.clone()?)),
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref()?;
            bindings
                .get(name)
                .and_then(|expr| lower_syntax_expr_with_env(expr, ctx, env))
                .or_else(|| Some(ErlExpr::Var(sanitize_erlang_var(name))))
        }
        SyntaxExprKind::Int => Some(ErlExpr::Int(expr.text.as_deref()?.parse().ok()?)),
        SyntaxExprKind::Float => Some(ErlExpr::Float(expr.text.clone()?)),
        SyntaxExprKind::Binary => Some(ErlExpr::Binary(expr.text.clone()?)),
        SyntaxExprKind::Tuple => Some(ErlExpr::Tuple(
            expr.children
                .iter()
                .map(|item| syntax_expr_to_alias_constructor_expr(item, bindings, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::List => Some(ErlExpr::List(
            expr.children
                .iter()
                .map(|item| syntax_expr_to_alias_constructor_expr(item, bindings, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::ListCons => Some(ErlExpr::ListCons(
            Box::new(syntax_expr_to_alias_constructor_expr(
                expr.children.first()?,
                bindings,
                ctx,
                env,
            )?),
            Box::new(syntax_expr_to_alias_constructor_expr(
                expr.children.get(1)?,
                bindings,
                ctx,
                env,
            )?),
        )),
        _ => None,
    }
}

fn lower_syntax_pattern(pattern: &SyntaxPatternOutput, ctx: &SyntaxLowerCtx) -> Option<ErlPattern> {
    match pattern.kind {
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => Some(ErlPattern::Wildcard),
        SyntaxPatternKind::Var => {
            let name = pattern.text.as_deref()?;
            if is_bool_literal_name(name) {
                Some(ErlPattern::Atom(name.to_string()))
            } else {
                Some(ErlPattern::Var(sanitize_erlang_var(name)))
            }
        }
        SyntaxPatternKind::Int => Some(ErlPattern::Int(pattern.text.as_deref()?.parse().ok()?)),
        SyntaxPatternKind::Float => Some(ErlPattern::Float(pattern.text.clone()?)),
        SyntaxPatternKind::Atom => Some(ErlPattern::Atom(pattern.text.clone()?)),
        SyntaxPatternKind::Tuple => Some(ErlPattern::Tuple(
            pattern
                .children
                .iter()
                .map(|child| lower_syntax_pattern(child, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxPatternKind::List => Some(ErlPattern::List(
            pattern
                .children
                .iter()
                .map(|child| lower_syntax_pattern(child, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxPatternKind::ListCons => Some(ErlPattern::ListCons(
            Box::new(lower_syntax_pattern(pattern.children.first()?, ctx)?),
            Box::new(lower_syntax_pattern(pattern.children.get(1)?, ctx)?),
        )),
        SyntaxPatternKind::MapField => Some(ErlPattern::Map(vec![ErlPatternMapField {
            key: pattern.text.clone()?,
            value: lower_syntax_pattern(pattern.children.first()?, ctx)?,
            required: pattern.fields.first().is_none_or(|field| field.required),
        }])),
        SyntaxPatternKind::Map => Some(ErlPattern::Map(
            pattern
                .fields
                .iter()
                .map(|field| lower_syntax_pattern_field(field, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxPatternKind::Record => Some(ErlPattern::Record {
            name: pattern.text.clone()?,
            fields: pattern
                .fields
                .iter()
                .map(|field| lower_syntax_pattern_field(field, ctx))
                .collect::<Option<Vec<_>>>()?,
        }),
        SyntaxPatternKind::Constructor => {
            let name = pattern.text.as_deref()?;
            if let Some(target) = ctx.constructor_pattern_target(name, pattern.children.len()) {
                return lower_syntax_explicit_constructor_pattern(target, &pattern.children, ctx);
            }
            let target = ctx.alias_constructor_target(name, pattern.children.len())?;
            lower_syntax_constructor_pattern(target, &pattern.children, ctx)
        }
    }
}

fn lower_syntax_pattern_field(
    field: &SyntaxPatternFieldOutput,
    ctx: &SyntaxLowerCtx,
) -> Option<ErlPatternMapField> {
    Some(ErlPatternMapField {
        key: field.key.clone(),
        value: lower_syntax_pattern(&field.value, ctx)?,
        required: field.required,
    })
}

fn lower_syntax_explicit_constructor_pattern(
    target: &SyntaxConstructorPatternTarget,
    args: &[SyntaxPatternOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<ErlPattern> {
    let bindings = target
        .params
        .iter()
        .cloned()
        .zip(args.iter())
        .collect::<BTreeMap<_, _>>();
    syntax_expr_to_constructor_pattern(&target.body, &bindings, ctx)
}

fn lower_syntax_constructor_pattern(
    target: &SyntaxAliasConstructorTarget,
    args: &[SyntaxPatternOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<ErlPattern> {
    let bindings = target
        .params
        .iter()
        .cloned()
        .zip(args.iter())
        .collect::<BTreeMap<_, _>>();
    syntax_expr_to_constructor_pattern(&target.body, &bindings, ctx)
}

fn syntax_expr_to_constructor_pattern(
    expr: &SyntaxExprOutput,
    bindings: &BTreeMap<String, &SyntaxPatternOutput>,
    ctx: &SyntaxLowerCtx,
) -> Option<ErlPattern> {
    match expr.kind {
        SyntaxExprKind::Atom => Some(ErlPattern::Atom(expr.text.clone()?)),
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref()?;
            bindings
                .get(name)
                .and_then(|pattern| lower_syntax_pattern(pattern, ctx))
                .or_else(|| Some(ErlPattern::Var(sanitize_erlang_var(name))))
        }
        SyntaxExprKind::Int => Some(ErlPattern::Int(expr.text.as_deref()?.parse().ok()?)),
        SyntaxExprKind::Float => Some(ErlPattern::Float(expr.text.clone()?)),
        SyntaxExprKind::Tuple => Some(ErlPattern::Tuple(
            expr.children
                .iter()
                .map(|item| syntax_expr_to_constructor_pattern(item, bindings, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::List => Some(ErlPattern::List(
            expr.children
                .iter()
                .map(|item| syntax_expr_to_constructor_pattern(item, bindings, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::ListCons => Some(ErlPattern::ListCons(
            Box::new(syntax_expr_to_constructor_pattern(
                expr.children.first()?,
                bindings,
                ctx,
            )?),
            Box::new(syntax_expr_to_constructor_pattern(
                expr.children.get(1)?,
                bindings,
                ctx,
            )?),
        )),
        _ => None,
    }
}

fn syntax_expr_name(expr: &SyntaxExprOutput) -> Option<&str> {
    match expr.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => expr.text.as_deref(),
        _ => None,
    }
}
