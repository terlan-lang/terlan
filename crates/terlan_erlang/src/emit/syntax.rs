//! Formal syntax-output to Erlang lowering.
//!
//! This module owns the direct `SyntaxModuleOutput` bridge emitter used
//! by the CoreIR-gated Erlang backend while CoreIR executable payload coverage
//! is still being expanded. It lowers compiler-facing syntax output into the
//! internal Erlang render model without routing through the source AST adapter.

use super::*;
use terlan_typeck::CorePrimitiveIntrinsic;

mod html;
use html::*;

mod indexing;
use indexing::*;

mod lets;
use lets::*;

mod imports;
use imports::*;

mod intrinsics;
use intrinsics::*;

mod collections;
use collections::*;

mod comprehensions;
use comprehensions::*;

mod construction;
use construction::*;

mod constructors;
use constructors::*;

mod declarations;
pub(super) use declarations::*;

mod patterns;
use patterns::*;

mod receiver_types;
use receiver_types::*;

mod sequences;
use sequences::*;

mod type_values;
use type_values::*;

mod generic_dispatch;
use generic_dispatch::*;

mod calls;
use calls::*;

pub(super) struct SyntaxLowerCtx {
    module_name: String,
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
            module_name: String::new(),
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

        let mut receiver_methods = module
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
        extend_receiver_methods_with_local_struct_derives(module, &mut receiver_methods);

        Self {
            module_name: module.module_name.clone(),
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
        let normalized_type_arg = normalize_syntax_trait_dispatch_type_key(type_arg);
        if let Some(wrapper) = self.imported_trait_conformances.get(key).and_then(|types| {
            types
                .get(type_arg)
                .or_else(|| types.get(normalized_type_arg.as_str()))
        }) {
            return Some(wrapper.as_str());
        }
        let key = key.rsplit('.').next().unwrap_or(key);
        self.imported_trait_conformances
            .get(key)
            .and_then(|types| {
                types
                    .get(type_arg)
                    .or_else(|| types.get(normalized_type_arg.as_str()))
            })
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

/// Adds inherited local receiver-method targets for derived structs.
///
/// Inputs:
/// - `module`: syntax-output module containing struct and receiver-method
///   declarations.
/// - `receiver_methods`: backend receiver dispatch map keyed by method/arity
///   and receiver type.
///
/// Output:
/// - None; `receiver_methods` is updated in place.
///
/// Transformation:
/// - For each local `struct Child derives Parent`, copies parent receiver
///   method targets to the child receiver type unless the child already has an
///   explicit method target. The inherited target still lowers to the original
///   receiver-first function body.
fn extend_receiver_methods_with_local_struct_derives(
    module: &SyntaxModuleOutput,
    receiver_methods: &mut BTreeMap<(String, usize), BTreeMap<String, SyntaxReceiverMethodTarget>>,
) {
    let local_structs = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let derive_edges = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Struct { name, derives, .. } => Some((name, derives)),
            _ => None,
        })
        .flat_map(|(child, derives)| {
            let local_structs = &local_structs;
            derives
                .iter()
                .filter(move |parent| local_structs.contains(*parent))
                .map(move |parent| (child.clone(), parent.clone()))
        })
        .collect::<Vec<_>>();

    let inherited = derive_edges
        .iter()
        .flat_map(|(child, parent)| {
            receiver_methods.iter().filter_map(move |(key, receivers)| {
                receivers
                    .get(parent)
                    .cloned()
                    .map(|target| (key.clone(), child.clone(), target))
            })
        })
        .collect::<Vec<_>>();

    for (key, child, target) in inherited {
        receiver_methods
            .entry(key)
            .or_default()
            .entry(child)
            .or_insert(target);
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
                env.value_types
                    .get(name)
                    .map(|type_text| normalize_syntax_trait_dispatch_type_key(type_text))
            }
        }
        SyntaxExprKind::RecordConstruct => expr.text.clone(),
        _ => None,
    }
}

/// Normalizes an annotated local type into the wrapper-map key used for trait dispatch.
///
/// Inputs:
/// - `type_text`: source or imported-qualified type annotation from the
///   lowering environment.
///
/// Output:
/// - Dispatch key used by typed trait wrapper lookup.
///
/// Transformation:
/// - Preserves ordinary user types exactly.
/// - Collapses compiler-known core aliases imported from their owning modules,
///   such as `std.core.Unit.Unit`, back to the source-level key used by
///   `impl Ordering[Unit]`.
fn normalize_syntax_trait_dispatch_type_key(type_text: &str) -> String {
    match normalize_trait_type_text(type_text).as_str() {
        "std.core.Unit.Unit" => "Unit".to_string(),
        "std.core.Ordering.Comparison" => "Comparison".to_string(),
        other => other.to_string(),
    }
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
            if name == "Unit" {
                return Some(ErlExpr::Atom("unit".to_string()));
            }
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
        SyntaxExprKind::Index => lower_syntax_index_expr(expr, ctx, env),
        SyntaxExprKind::IndexAssign => lower_syntax_index_assign_expr(expr, ctx, env),
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

fn resolve_syntax_field_access_struct(
    value: &SyntaxExprOutput,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    match value.kind {
        SyntaxExprKind::Var => env.struct_locals.get(value.text.as_deref()?).cloned(),
        _ => None,
    }
}
