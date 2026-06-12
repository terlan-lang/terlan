//! Formal syntax-output to Erlang lowering.
//!
//! This module owns the direct `SyntaxModuleOutput` bridge emitter used
//! by the CoreIR-gated Erlang backend while CoreIR executable payload coverage
//! is still being expanded. It lowers compiler-facing syntax output into the
//! internal Erlang render model without routing through the source AST adapter.

use super::*;
use terlan_typeck::CorePrimitiveIntrinsic;

struct SyntaxLowerCtx {
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
    imported_trait_aliases: BTreeMap<String, (String, String)>,
    receiver_methods: BTreeMap<(String, usize), BTreeSet<String>>,
    module_aliases: BTreeMap<String, String>,
    file_imports: BTreeMap<String, Vec<u8>>,
    markdown_imports: BTreeMap<String, terlan_html::MarkdownDocument>,
    templates: BTreeMap<String, LowerTemplate>,
    struct_field_types: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Default)]
struct SyntaxLowerEnv {
    struct_locals: BTreeMap<String, String>,
    value_locals: BTreeSet<String>,
    value_types: BTreeMap<String, String>,
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
            imported_trait_aliases: BTreeMap::new(),
            receiver_methods: BTreeMap::new(),
            module_aliases: BTreeMap::new(),
            file_imports: BTreeMap::new(),
            markdown_imports: BTreeMap::new(),
            templates: BTreeMap::new(),
            struct_field_types: BTreeMap::new(),
        }
    }

    fn new(
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
                ))
            })
            .fold(
                BTreeMap::<(String, usize), BTreeSet<String>>::new(),
                |mut methods, (key, receiver_type)| {
                    methods.entry(key).or_default().insert(receiver_type);
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
            typed_trait_method_wrappers: BTreeMap::new(),
            imported_trait_aliases,
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

    fn alias_constructor_target(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<&SyntaxAliasConstructorTarget> {
        self.alias_constructor_targets
            .get(name)
            .filter(|target| target.params.len() == arity)
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

    fn imported_trait_alias(&self, name: &str) -> Option<(&str, &str)> {
        self.imported_trait_aliases
            .get(name)
            .map(|(module, source_name)| (module.as_str(), source_name.as_str()))
    }

    /// Tests whether a local receiver-method declaration can handle a call.
    ///
    /// Inputs:
    /// - `receiver_type`: normalized type key inferred for the call receiver.
    /// - `method`: source method name.
    /// - `arity`: number of non-receiver call arguments.
    ///
    /// Output:
    /// - `true` when the current module declares a receiver method with the
    ///   same receiver type, method name, and non-receiver arity.
    ///
    /// Transformation:
    /// - Performs an exact lookup against the receiver-method inventory built
    ///   from formal syntax output. This emitter-side check mirrors the
    ///   typechecker contract without doing full type inference.
    fn has_receiver_method(&self, receiver_type: &str, method: &str, arity: usize) -> bool {
        self.receiver_methods
            .get(&(method.to_string(), arity))
            .map(|receivers| receivers.contains(receiver_type))
            .unwrap_or(false)
    }
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
            SyntaxDeclarationPayload::TraitImpl { .. } => {}
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
                return_type,
                is_public,
                clauses,
                ..
            } => {
                if *is_public && !clauses.is_empty() {
                    exports.insert(format!("{}/{}", name, params.len()));
                }
                function_forms.extend(lower_syntax_function_decl(
                    decl,
                    name,
                    params,
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
                        ErlType::List(Box::new(lower_type_to_spec(&param.annotation.text)))
                    } else {
                        lower_type_to_spec(&param.annotation.text)
                    }
                })
                .collect();
            Some(vec![
                ErlForm::Spec(ErlSpec {
                    docs: Vec::new(),
                    name: function.clone(),
                    args,
                    ret: lower_type_to_spec(&clause.return_type.text),
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
    return_type: &SyntaxTypeOutput,
    clauses: &[SyntaxFunctionClauseOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    let mut forms = Vec::new();
    let env = lower_syntax_function_env(params, ctx);

    if !params.is_empty() {
        forms.push(ErlForm::Spec(ErlSpec {
            docs: decl.docs.clone(),
            name: name.to_string(),
            args: params
                .iter()
                .map(|param| lower_type_to_spec(&param.annotation.text))
                .collect(),
            ret: lower_type_to_spec(&return_type.text),
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
                Some(ErlFunctionClause {
                    patterns: clause
                        .patterns
                        .iter()
                        .map(|pattern| lower_syntax_pattern(pattern, ctx))
                        .collect::<Option<Vec<_>>>()?,
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
    let env = lower_syntax_function_env(&all_params, ctx);
    let mut forms = Vec::new();

    forms.push(ErlForm::Spec(ErlSpec {
        docs: decl.docs.clone(),
        name: name.to_string(),
        args: all_params
            .iter()
            .map(|param| lower_type_to_spec(&param.annotation.text))
            .collect(),
        ret: lower_type_to_spec(&return_type.text),
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

fn lower_syntax_function_env(params: &[SyntaxParamOutput], ctx: &SyntaxLowerCtx) -> SyntaxLowerEnv {
    let value_locals = params.iter().map(|param| param.name.clone()).collect();
    let value_types = params
        .iter()
        .map(|param| {
            (
                param.name.clone(),
                normalize_trait_type_text(&param.annotation.text),
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
                normalize_trait_type_text(&param.annotation.text),
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
            if is_bool_literal_name(name) {
                return Some(ErlExpr::Atom(name.to_string()));
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
                .map(|field| lower_syntax_expr_field(field, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::ListComprehension => {
            let value = expr.children.first()?;
            let source = expr.children.get(1)?;
            let pattern = expr.patterns.first()?;
            Some(ErlExpr::ListComprehension {
                expr: Box::new(lower_syntax_expr_with_env(value, ctx, env)?),
                pattern: lower_syntax_pattern(pattern, ctx)?,
                source: Box::new(lower_syntax_expr_with_env(source, ctx, env)?),
                guard: match expr.children.get(2) {
                    Some(guard) => Some(Box::new(lower_syntax_expr_with_env(guard, ctx, env)?)),
                    None => None,
                },
            })
        }
        SyntaxExprKind::Let => lower_syntax_let_expr(expr, ctx, env),
        SyntaxExprKind::Cast => None,
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall => {
            lower_syntax_call_expr(expr, ctx, env)
        }
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
            source: Box::new(lower_syntax_expr_with_env(source, ctx, env)?),
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
///   match. When no explicit body child exists, the expression result is the
///   final binding variable.
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

    let bindings = expr
        .patterns
        .iter()
        .zip(expr.children.iter())
        .map(|(pattern, value)| {
            Some(ErlLetBinding {
                name: sanitize_erlang_var(pattern.text.as_deref()?),
                value: lower_syntax_expr_with_env(value, ctx, env)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let body = match expr.children.get(expr.patterns.len()) {
        Some(body) => lower_syntax_expr_with_env(body, ctx, env)?,
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
        if let Some(target) = ctx.alias_constructor_target(callee_name, args.len()) {
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
        if let Some((module_name, source_trait_name)) = ctx.imported_trait_alias(remote) {
            if callee_name == "to_string" {
                if let Some(type_arg) = args
                    .first()
                    .and_then(|arg| infer_syntax_trait_dispatch_type(arg, env))
                {
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
                            &type_arg,
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
        ("std.core.Bool", "equal", 2) => Some(CorePrimitiveIntrinsic::BoolEqual),
        ("std.core.Bool", "compare", 2) => Some(CorePrimitiveIntrinsic::BoolCompare),
        ("std.core.Bool", "to_string", 1) => Some(CorePrimitiveIntrinsic::BoolToString),
        ("std.core.Bool", "from_string", 1) => Some(CorePrimitiveIntrinsic::BoolFromString),
        ("std.core.Int", "to_string", 1) => Some(CorePrimitiveIntrinsic::IntToString),
        ("std.core.Int", "from_string", 1) => Some(CorePrimitiveIntrinsic::IntFromString),
        ("std.core.Float", "to_string", 1) => Some(CorePrimitiveIntrinsic::FloatToString),
        ("std.core.Float", "from_string", 1) => Some(CorePrimitiveIntrinsic::FloatFromString),
        ("std.core.String", "equal", 2) => Some(CorePrimitiveIntrinsic::StringEqual),
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
        _ => None,
    }
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
    if !ctx.has_receiver_method(&receiver_type, method, args.len()) {
        return None;
    }

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
    match (receiver_type, method, arg_count) {
        ("Int", "to_string", 0) => Some(CorePrimitiveIntrinsic::IntToString),
        ("Float", "to_string", 0) => Some(CorePrimitiveIntrinsic::FloatToString),
        ("String", "equal", 1) => Some(CorePrimitiveIntrinsic::StringEqual),
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

    let callee = right.children.first()?;
    let mut args = Vec::with_capacity(right.children.len());
    args.push(left.clone());
    args.extend(right.children.iter().skip(1).cloned());
    lower_syntax_call_parts(callee, &args, right.remote.as_deref(), ctx, env)
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
