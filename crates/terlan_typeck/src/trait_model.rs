use std::collections::{HashMap, HashSet};

use terlan_hir::{ModuleInterface, ResolvedModule};
use terlan_syntax::{span::Span, SyntaxDeclarationPayload, SyntaxModuleOutput, Token, TokenKind};

use super::{
    interface_qualified_type_names, interface_type_names, normalize_type_param_name,
    parse_generic_bounds, parse_type_expr, pretty_type, qualify_type_names, substitute_type_vars,
    syntax_declared_implements, syntax_trait_impl_to_parsed, FunctionBound, FunctionScheme, Type,
};

/// Parsed trait declaration surface visible to type checking.
///
/// Inputs:
/// - Local trait declarations and imported trait interface metadata.
///
/// Output:
/// - Normalized trait parameter, method, and inheritance shape.
///
/// Transformation:
/// - Converts syntax/interface text into a lookup-ready model while preserving
///   method defaults and super trait references.
#[derive(Debug, Clone)]
pub(super) struct ParsedTraitSignature {
    pub(super) type_params: Vec<String>,
    pub(super) methods: HashMap<String, TraitMethodSignature>,
    pub(super) super_traits: Vec<String>,
}

/// Resolved trait method implementation candidate.
///
/// Inputs:
/// - Declared or explicit trait implementation method plus resolved type args.
///
/// Output:
/// - Callable function scheme paired with implementation type arguments.
///
/// Transformation:
/// - Binds generic implementation context to the method shape used by
///   expression dispatch.
#[derive(Debug, Clone)]
pub(super) struct ResolvedTraitMethod {
    pub(super) scheme: FunctionScheme,
    pub(super) impl_type_args: Vec<Type>,
}

/// Method requirement declared by a trait.
///
/// Inputs:
/// - Trait method syntax or imported interface method metadata.
///
/// Output:
/// - Parameter, return, generic-bound, and default-method metadata.
///
/// Transformation:
/// - Normalizes source-level type text into the comparable trait signature
///   format used for conformance checks.
#[derive(Debug, Clone)]
pub(super) struct TraitMethodSignature {
    pub(super) params: Vec<TraitMethodParamSignature>,
    pub(super) return_type: String,
    pub(super) generic_bounds: Vec<String>,
    pub(super) has_default: bool,
}

/// Trait method parameter requirement.
///
/// Inputs:
/// - One trait method parameter annotation and mutability marker.
///
/// Output:
/// - Normalized parameter type text and mutable receiver/argument flag.
///
/// Transformation:
/// - Keeps mutability explicit so trait conformance can validate mutable
///   receiver methods.
#[derive(Debug, Clone)]
pub(super) struct TraitMethodParamSignature {
    pub(super) ty: String,
    pub(super) is_mutable: bool,
}

/// Parsed explicit trait implementation declaration.
///
/// Inputs:
/// - `impl Trait[...] for Type` syntax-output declaration.
///
/// Output:
/// - Target trait instance, optional `for` type, and implemented methods.
///
/// Transformation:
/// - Converts implementation syntax into a compact semantic shape before
///   coherence and signature validation.
#[derive(Debug, Clone)]
pub(super) struct ParsedTraitImpl {
    pub(super) target: ParsedTraitInstance,
    pub(super) for_type: Option<String>,
    pub(super) methods: Vec<ParsedMethodSignature>,
}

/// Parsed trait reference with type arguments.
///
/// Inputs:
/// - Trait name text such as `Show[User]`.
///
/// Output:
/// - Trait name and raw type-argument text.
///
/// Transformation:
/// - Splits the trait reference enough for later type parsing and generic
///   substitution.
#[derive(Debug, Clone)]
pub(super) struct ParsedTraitInstance {
    pub(super) name: String,
    pub(super) type_args: Vec<String>,
}

/// Parsed method signature inside an explicit trait implementation.
///
/// Inputs:
/// - Implementation method syntax-output declaration.
///
/// Output:
/// - Method name, parameter type text, mutability markers, return type, and
///   diagnostic span.
///
/// Transformation:
/// - Captures the implementation method surface used for trait signature
///   matching without retaining the method body.
#[derive(Debug, Clone)]
pub(super) struct ParsedMethodSignature {
    pub(super) name: String,
    pub(super) params: Vec<String>,
    pub(super) mutable_params: Vec<bool>,
    pub(super) return_type: String,
    pub(super) span: Span,
}

/// Collects visible trait signatures from imports and local declarations.
///
/// Inputs:
/// - `module`: syntax-output module containing local trait declarations.
/// - `resolved`: resolved module with imported trait interfaces.
///
/// Output:
/// - Trait signatures keyed by the local trait name visible in this module.
///
/// Transformation:
/// - Starts with selected imported trait signatures, then overlays local trait
///   declarations using normalized type text and default-method metadata.
pub(super) fn collect_syntax_trait_signatures(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> HashMap<String, ParsedTraitSignature> {
    let mut traits = collect_imported_trait_signatures(resolved);

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Trait {
            name,
            params,
            super_traits,
            methods,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let mut method_signatures = HashMap::new();
        for method in methods {
            method_signatures.insert(
                method.name.clone(),
                TraitMethodSignature {
                    params: method
                        .params
                        .iter()
                        .map(|param| TraitMethodParamSignature {
                            ty: normalize_trait_type_text(&param.annotation.text),
                            is_mutable: param.is_mutable,
                        })
                        .collect(),
                    return_type: normalize_trait_type_text(&method.return_type.text),
                    generic_bounds: method.generic_bounds.clone(),
                    has_default: method.default_body.is_some(),
                },
            );
        }

        traits.insert(
            name.clone(),
            ParsedTraitSignature {
                type_params: params.clone(),
                methods: method_signatures,
                super_traits: super_traits.clone(),
            },
        );
    }

    traits
}

/// Collects trait signatures selected from imported interfaces.
///
/// Inputs:
/// - `resolved`: resolved module with selected trait imports and provider
///   interfaces.
///
/// Output:
/// - Imported trait signatures keyed by their local import names.
///
/// Transformation:
/// - Reads only explicitly imported trait metadata from provider interfaces and
///   converts the interface signature shape into the local parsed trait model.
fn collect_imported_trait_signatures(
    resolved: &ResolvedModule,
) -> HashMap<String, ParsedTraitSignature> {
    let mut traits = HashMap::new();

    for imported in resolved.imported_traits.values() {
        let Some(interface) = resolved.interface_map.get(&imported.source_module) else {
            continue;
        };

        let Some(imported_signature) = interface.traits.get(&imported.source_name) else {
            continue;
        };

        let mut methods = HashMap::new();
        for (method_name, method_signature) in &imported_signature.methods {
            methods.insert(
                method_name.clone(),
                TraitMethodSignature {
                    params: method_signature
                        .params
                        .iter()
                        .map(|param| TraitMethodParamSignature {
                            ty: normalize_trait_type_text(&param.annotation),
                            is_mutable: param.is_mutable,
                        })
                        .collect(),
                    return_type: normalize_trait_type_text(&method_signature.return_type),
                    generic_bounds: method_signature.generic_bounds.clone(),
                    has_default: method_signature.has_default,
                },
            );
        }

        traits.insert(
            imported.local_name.clone(),
            ParsedTraitSignature {
                type_params: imported_signature.type_params.clone(),
                methods,
                super_traits: imported_signature.super_traits.clone(),
            },
        );
    }

    traits
}

/// Collects all methods required by a trait and its super traits.
///
/// Inputs:
/// - `signatures`: visible trait signatures.
/// - `trait_name`: trait whose inherited method surface should be collected.
/// - `cache`: shared inheritance cache.
/// - `visiting`: active recursion stack for cycle detection.
///
/// Output:
/// - Method map for the trait including inherited defaults and requirements.
/// - `None` when inheritance cycles are detected or the trait is unknown.
///
/// Transformation:
/// - Recursively walks super trait references, merges parent methods first, and
///   overlays direct methods from the requested trait.
pub(super) fn collect_trait_methods_with_inheritance(
    signatures: &HashMap<String, ParsedTraitSignature>,
    trait_name: &str,
    cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
    visiting: &mut HashSet<String>,
) -> Option<HashMap<String, TraitMethodSignature>> {
    if let Some(cached) = cache.get(trait_name) {
        return cached.clone();
    }

    if !visiting.insert(trait_name.to_string()) {
        cache.insert(trait_name.to_string(), None);
        return None;
    }

    let signature = signatures.get(trait_name)?;
    let mut methods = HashMap::new();

    for super_trait_text in &signature.super_traits {
        let Some(super_trait) = parse_trait_instance_from_text(super_trait_text) else {
            continue;
        };

        if let Some(parent_methods) =
            collect_trait_methods_with_inheritance(signatures, &super_trait.name, cache, visiting)
        {
            methods.extend(parent_methods);
        }
    }

    for (name, method) in &signature.methods {
        methods.insert(name.clone(), method.clone());
    }

    visiting.remove(trait_name);
    let methods = Some(methods);
    cache.insert(trait_name.to_string(), methods.clone());
    methods
}

/// Collects visible trait method dispatch candidates.
///
/// Inputs:
/// - `module`: syntax-output module containing local conformance declarations.
/// - `alias_names`: visible type names for type-expression parsing.
/// - `trait_signatures`: visible trait signatures.
/// - `resolved`: resolved module with imported interface conformances.
///
/// Output:
/// - Dispatch candidates keyed by `(trait_name, method_name)`.
///
/// Transformation:
/// - Seeds every visible trait method key, then adds declaration-site,
///   explicit-impl, and imported interface conformance candidates.
pub(super) fn collect_syntax_trait_method_calls(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    resolved: &ResolvedModule,
) -> HashMap<(String, String), Vec<ResolvedTraitMethod>> {
    let mut methods: HashMap<(String, String), Vec<ResolvedTraitMethod>> = HashMap::new();
    let mut inheritance_cache: HashMap<String, Option<HashMap<String, TraitMethodSignature>>> =
        HashMap::new();

    seed_syntax_trait_method_call_keys(trait_signatures, &mut methods, &mut inheritance_cache);
    collect_syntax_declared_implements_trait_method_calls(
        module,
        &mut methods,
        trait_signatures,
        alias_names,
        &mut inheritance_cache,
    );
    collect_syntax_explicit_trait_method_calls(
        module,
        &mut methods,
        trait_signatures,
        alias_names,
        &mut inheritance_cache,
    );
    collect_imported_interface_trait_method_calls(
        resolved,
        &mut methods,
        trait_signatures,
        alias_names,
        &mut inheritance_cache,
    );

    methods
}

/// Registers imported interface conformances as trait dispatch candidates.
///
/// Inputs:
/// - `resolved`: resolved module carrying selected trait imports and provider
///   interfaces.
/// - `methods`: dispatch candidate map to extend.
/// - `trait_signatures`: known local/imported trait signatures, keyed by local
///   trait name.
/// - `alias_names`: type names visible to type-expression parsing.
/// - `inheritance_cache`: inherited trait method cache shared with other
///   conformance candidate collectors.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Reads public provider `ModuleInterface::trait_conformances`, keeps only
///   facts for selected imported traits, rewrites the trait head to the local
///   import name, and reuses the existing candidate specialization machinery.
fn collect_imported_interface_trait_method_calls(
    resolved: &ResolvedModule,
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    alias_names: &HashSet<String>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    for imported in resolved.imported_traits.values() {
        let Some(interface) = resolved.interface_map.get(&imported.source_module) else {
            continue;
        };

        for conformance in &interface.trait_conformances {
            if !conformance.public {
                continue;
            }
            let Some(mut implemented_trait) =
                parse_trait_instance_from_text(&conformance.trait_ref)
            else {
                continue;
            };
            if implemented_trait.name != imported.source_name {
                continue;
            }
            implemented_trait.name = imported.local_name.clone();
            implemented_trait.type_args =
                qualify_interface_trait_type_args(&implemented_trait.type_args, interface);
            let for_type = qualify_interface_trait_type_text(&conformance.for_type, interface)
                .unwrap_or_else(|| conformance.for_type.clone());
            collect_trait_method_candidates(
                methods,
                &ParsedTraitImpl {
                    target: implemented_trait,
                    for_type: Some(for_type),
                    methods: Vec::new(),
                },
                &imported.local_name,
                trait_signatures,
                alias_names,
                inheritance_cache,
            );
        }
    }
}

/// Qualifies provider-local trait instance arguments from an interface.
///
/// Inputs:
/// - `type_args`: concrete trait arguments from a provider interface
///   conformance, such as `ExternalUser`.
/// - `interface`: provider module interface that defines public type names.
///
/// Output:
/// - Type argument text rendered with provider module qualification where the
///   argument names refer to provider public types.
///
/// Transformation:
/// - Parses each type argument using the provider interface type namespace,
///   qualifies public type heads, then renders the internal type back to stable
///   text for ordinary trait candidate specialization.
fn qualify_interface_trait_type_args(
    type_args: &[String],
    interface: &ModuleInterface,
) -> Vec<String> {
    type_args
        .iter()
        .map(|arg| qualify_interface_trait_type_text(arg, interface).unwrap_or_else(|| arg.clone()))
        .collect()
}

/// Qualifies one provider-local type expression from an interface.
///
/// Inputs:
/// - `text`: type expression text from provider interface metadata.
/// - `interface`: provider module interface that owns public type names.
///
/// Output:
/// - Qualified type text, or `None` when the expression cannot be parsed.
///
/// Transformation:
/// - Parses text in the provider interface namespace, qualifies public
///   unqualified type names to `module.Type`, and renders the result through
///   `pretty_type` so imported conformance dispatch matches consumer-side
///   imported type inference.
fn qualify_interface_trait_type_text(text: &str, interface: &ModuleInterface) -> Option<String> {
    let mut vars = HashMap::new();
    let mut next_var = 0usize;
    let alias_names = interface_type_names(interface);
    let qualified_names = interface_qualified_type_names(interface);
    let parsed = parse_type_expr(text, &alias_names, &mut vars, &mut next_var)?;
    Some(pretty_type(&qualify_type_names(&parsed, &qualified_names)))
}

/// Seeds trait method lookup keys before concrete impl candidates are added.
///
/// Inputs:
/// - `trait_signatures`: known local/imported trait signatures.
/// - `methods`: dispatch candidate map to initialize.
/// - `inheritance_cache`: inherited trait method cache shared with candidate
///   collection.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Inserts an empty candidate list for every visible trait method. This lets
///   trait-call inference distinguish “known trait method with no impl” from
///   ordinary remote calls and emit a conformance diagnostic instead of falling
///   through as dynamic.
fn seed_syntax_trait_method_call_keys(
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    for trait_name in trait_signatures.keys() {
        let inherited_methods = collect_trait_methods_with_inheritance(
            trait_signatures,
            trait_name,
            inheritance_cache,
            &mut HashSet::new(),
        )
        .unwrap_or_default();

        for method_name in inherited_methods.keys() {
            methods
                .entry((trait_name.clone(), method_name.clone()))
                .or_default();
        }
    }
}

/// Registers declaration-site `implements` entries as trait dispatch candidates.
///
/// Inputs:
/// - `module`: syntax-output module containing type or struct declarations
///   with `implements` clauses.
/// - `methods`: dispatch candidate map to extend.
/// - `trait_signatures`: known local/imported trait signatures.
/// - `alias_names`: type names visible to type-expression parsing.
/// - `inheritance_cache`: inherited trait method cache shared with candidate
///   collection.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Converts each declaration-site conformance into the same specialized
///   trait method candidates used by trait-call inference. Receiver-method
///   conformance is validated separately; this function only exposes the
///   declared conformance to type inference.
fn collect_syntax_declared_implements_trait_method_calls(
    module: &SyntaxModuleOutput,
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    alias_names: &HashSet<String>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    for declaration in &module.declarations {
        let Some((_type_name, implements)) = syntax_declared_implements(declaration) else {
            continue;
        };

        for trait_ref in implements {
            let Some(implemented_trait) = parse_trait_instance_from_text(&trait_ref.text) else {
                continue;
            };
            let trait_name = implemented_trait.name.clone();
            let mut synthesized = HashMap::new();
            collect_trait_method_candidates(
                &mut synthesized,
                &ParsedTraitImpl {
                    target: implemented_trait,
                    for_type: None,
                    methods: Vec::new(),
                },
                &trait_name,
                trait_signatures,
                alias_names,
                inheritance_cache,
            );

            for (key, candidates) in synthesized {
                let existing = methods.entry(key).or_default();
                for method in candidates {
                    if !existing
                        .iter()
                        .any(|existing| existing.impl_type_args == method.impl_type_args)
                    {
                        existing.push(method);
                    }
                }
            }
        }
    }
}

/// Registers explicit adapter impls as trait method dispatch candidates.
///
/// Inputs:
/// - `module`: syntax-output module containing structured trait impl
///   declarations.
/// - `methods`: dispatch candidate map to extend.
/// - `trait_signatures`: known local/imported trait signatures.
/// - `alias_names`: type names visible to type-expression parsing.
/// - `inheritance_cache`: inherited trait method cache shared with other
///   conformance candidate collectors.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Converts each `impl TraitRef for Type` declaration into the same
///   `ResolvedTraitMethod` candidates used by trait-call inference. The
///   structured path reads syntax output directly and does not reparse raw
///   source blocks.
fn collect_syntax_explicit_trait_method_calls(
    module: &SyntaxModuleOutput,
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    alias_names: &HashSet<String>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl { .. } = &declaration.payload else {
            continue;
        };
        let Some(impl_decl) = syntax_trait_impl_to_parsed(declaration) else {
            continue;
        };
        let trait_name = impl_decl.target.name.clone();
        let mut synthesized = HashMap::new();
        collect_trait_method_candidates(
            &mut synthesized,
            &impl_decl,
            &trait_name,
            trait_signatures,
            alias_names,
            inheritance_cache,
        );

        for (key, candidates) in synthesized {
            let existing = methods.entry(key).or_default();
            for method in candidates {
                if !existing
                    .iter()
                    .any(|existing| existing.impl_type_args == method.impl_type_args)
                {
                    existing.push(method);
                }
            }
        }
    }
}

/// Adds trait method candidates for one conformance declaration.
///
/// Inputs:
/// - `methods`: dispatch candidate map to extend.
/// - `impl_decl`: parsed conformance declaration.
/// - `trait_name`: local trait name being implemented.
/// - `trait_signatures`: visible trait signatures.
/// - `alias_names`: visible type names for parsing type expressions.
/// - `inheritance_cache`: inherited method cache.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Parses concrete impl type arguments, specializes inherited method
///   signatures and generic bounds, then stores resolved function schemes as
///   trait dispatch candidates.
fn collect_trait_method_candidates(
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    impl_decl: &ParsedTraitImpl,
    trait_name: &str,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    alias_names: &HashSet<String>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    let Some(trait_signature) = trait_signatures.get(trait_name) else {
        return;
    };

    if impl_decl.target.type_args.len() != trait_signature.type_params.len() {
        return;
    }

    let mut arg_vars = HashMap::new();
    let mut next_arg_var = 0usize;
    let mut impl_type_args = Vec::new();
    let mut parse_ok = true;

    for raw_arg in &impl_decl.target.type_args {
        let parsed = parse_type_expr(raw_arg, alias_names, &mut arg_vars, &mut next_arg_var);
        match parsed {
            Some(parsed) => impl_type_args.push(parsed),
            None => {
                parse_ok = false;
                break;
            }
        }
    }
    if !parse_ok {
        return;
    }

    let inherited_methods = collect_trait_methods_with_inheritance(
        trait_signatures,
        trait_name,
        inheritance_cache,
        &mut HashSet::new(),
    )
    .unwrap_or_default();

    for (method_name, method_sig) in &inherited_methods {
        let mut method_vars = HashMap::new();
        let mut next_method_var = 0usize;
        for name in &trait_signature.type_params {
            method_vars.insert(name.clone(), next_method_var);
            next_method_var += 1;
        }

        let parsed_params = method_sig
            .params
            .iter()
            .map(|param| {
                parse_type_expr(
                    &param.ty,
                    alias_names,
                    &mut method_vars,
                    &mut next_method_var,
                )
            })
            .collect::<Option<Vec<_>>>();

        let parsed_return = parse_type_expr(
            &method_sig.return_type,
            alias_names,
            &mut method_vars,
            &mut next_method_var,
        );
        let Some(parsed_return) = parsed_return else {
            continue;
        };
        let Some(parsed_params) = parsed_params else {
            continue;
        };

        let mut substitution = HashMap::new();
        let mut valid = true;
        for (param_name, param_type) in trait_signature
            .type_params
            .iter()
            .zip(impl_type_args.iter())
        {
            if let Some(var_id) = method_vars.get(param_name) {
                substitution.insert(*var_id, param_type.clone());
            } else {
                valid = false;
                break;
            }
        }
        if !valid {
            continue;
        }

        let specialized_bounds =
            parse_generic_bounds(&method_sig.generic_bounds, &method_vars, alias_names)
                .into_iter()
                .map(|bound| FunctionBound {
                    trait_name: bound.trait_name,
                    trait_args: bound
                        .trait_args
                        .into_iter()
                        .map(|arg| substitute_type_vars(&arg, &substitution))
                        .collect(),
                })
                .collect();

        let specialized = FunctionScheme {
            params: parsed_params
                .into_iter()
                .map(|param| substitute_type_vars(&param, &substitution))
                .collect(),
            ret: substitute_type_vars(&parsed_return, &substitution),
            bounds: specialized_bounds,
        };

        methods
            .entry((trait_name.to_string(), method_name.clone()))
            .or_default()
            .push(ResolvedTraitMethod {
                scheme: specialized,
                impl_type_args: impl_type_args.clone(),
            });
    }
}

/// Parses one trait instance from source text.
///
/// Inputs:
/// - `text`: trait reference text, such as `Show[User]`.
///
/// Output:
/// - Parsed trait instance when lexing and structural parsing succeed.
/// - `None` for malformed or empty references.
///
/// Transformation:
/// - Lexes the source text with the canonical syntax lexer and delegates token
///   grouping to the bracket-aware trait instance parser.
pub(super) fn parse_trait_instance_from_text(text: &str) -> Option<ParsedTraitInstance> {
    let tokens = terlan_syntax::lexer::lex(text).ok()?;
    parse_trait_instance(&tokens)
}

/// Builds a stable display key for a trait instance.
///
/// Inputs:
/// - `target`: parsed trait instance.
///
/// Output:
/// - Normalized trait key, or `None` when the trait name is empty.
///
/// Transformation:
/// - Normalizes type argument whitespace and renders `Trait[Arg, ...]` only
///   when type arguments are present.
pub(super) fn trait_instance_key(target: &ParsedTraitInstance) -> Option<String> {
    if target.name.is_empty() {
        return None;
    }

    if target.type_args.is_empty() {
        Some(target.name.clone())
    } else {
        Some(format!(
            "{}[{}]",
            target.name,
            target
                .type_args
                .iter()
                .map(|arg| normalize_trait_type_text(arg))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// Normalizes trait type-expression text for stable comparison.
///
/// Inputs:
/// - `text`: source type-expression text.
///
/// Output:
/// - Text with whitespace runs collapsed to one space.
///
/// Transformation:
/// - Performs a syntax-light normalization suitable for diagnostics and
///   signature comparison before full type lowering.
pub(super) fn normalize_trait_type_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Substitutes trait type parameters inside a type-expression string.
///
/// Inputs:
/// - `text`: type-expression text from a trait method signature.
/// - `params`: trait type-parameter names.
/// - `args`: concrete type arguments from an implemented trait reference.
///
/// Output:
/// - Normalized type-expression text after replacing matching type variables.
///
/// Transformation:
/// - Lexes the type text and replaces upper-case identifier tokens whose text
///   matches a trait type parameter. Punctuation and non-matching tokens are
///   preserved, then normalized for stable diagnostics and comparisons.
pub(super) fn specialize_trait_type_text(text: &str, params: &[String], args: &[String]) -> String {
    if params.is_empty() || args.is_empty() {
        return normalize_trait_type_text(text);
    }

    let substitutions = params
        .iter()
        .zip(args.iter())
        .map(|(param, arg)| (normalize_type_param_name(param), arg.as_str()))
        .collect::<HashMap<_, _>>();

    let Ok(tokens) = terlan_syntax::lexer::lex(text) else {
        return normalize_trait_type_text(text);
    };

    let mut parts = Vec::new();
    for token in tokens {
        if token.kind == TokenKind::EOF {
            break;
        }
        if matches!(
            token.kind,
            TokenKind::Comment | TokenKind::DocComment | TokenKind::ModuleDocComment
        ) {
            continue;
        }
        if token.kind == TokenKind::Var {
            if let Some(replacement) = substitutions.get(&normalize_type_param_name(&token.text)) {
                parts.push((*replacement).to_string());
                continue;
            }
        }
        parts.push(token.text);
    }

    normalize_trait_type_text(&join_token_texts_from_strings(&parts))
}

/// Compares two type-expression texts using compact whitespace-insensitive form.
///
/// Inputs:
/// - `left`: first type text.
/// - `right`: second type text.
///
/// Output:
/// - `true` when both texts are equal after removing whitespace.
///
/// Transformation:
/// - Applies the same compacting strategy used by syntax diagnostics that only
///   need source-stable shape comparison before full type identity lowering.
pub(super) fn trait_type_text_equal(left: &str, right: &str) -> bool {
    compact_trait_type_text(left) == compact_trait_type_text(right)
}

/// Parses one tokenized trait instance.
///
/// Inputs:
/// - `tokens`: lexer tokens for a trait reference.
///
/// Output:
/// - Parsed trait instance, including nested type argument text.
/// - `None` when the trait name is empty.
///
/// Transformation:
/// - Splits the leading trait name from bracketed type arguments, preserving
///   nested bracket/brace/paren groups inside each argument.
fn parse_trait_instance(tokens: &[Token]) -> Option<ParsedTraitInstance> {
    if tokens.is_empty() {
        return None;
    }

    let mut name_end = tokens.len();
    for (idx, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::LBracket {
            name_end = idx;
            break;
        }
    }

    let name = tokens[..name_end]
        .iter()
        .filter_map(|token| match token.kind {
            TokenKind::Comment | TokenKind::DocComment | TokenKind::ModuleDocComment => None,
            TokenKind::Dot => Some(".".to_string()),
            _ => Some(token.text.clone()),
        })
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .trim_matches('.')
        .to_string();
    if name.is_empty() {
        return None;
    }

    let mut type_args = Vec::new();
    if name_end >= tokens.len() {
        return Some(ParsedTraitInstance { name, type_args });
    }

    let mut pos = name_end + 1;
    let mut depth = 0i32;
    let mut current = Vec::new();

    while pos < tokens.len() {
        let token = &tokens[pos];

        if token.kind == TokenKind::RBracket && depth == 0 {
            if !current.is_empty() {
                type_args.push(
                    join_token_texts(&current)
                        .split_whitespace()
                        .collect::<String>(),
                );
            }
            break;
        }

        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                depth += 1;
                current.push(token.clone());
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
                if depth >= 0 {
                    current.push(token.clone());
                }
            }
            TokenKind::Comma if depth == 0 => {
                type_args.push(
                    join_token_texts(&current)
                        .split_whitespace()
                        .collect::<String>(),
                );
                current.clear();
            }
            _ => current.push(token.clone()),
        }

        pos += 1;
    }

    Some(ParsedTraitInstance { name, type_args })
}

/// Joins lexer token text with spaces.
///
/// Inputs:
/// - `tokens`: token slice to render.
///
/// Output:
/// - Space-separated token text.
///
/// Transformation:
/// - Preserves token text while reintroducing one space between adjacent
///   tokens, allowing caller-specific whitespace compaction afterward.
fn join_token_texts(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Joins token-text strings for type normalization.
///
/// Inputs:
/// - `parts`: token text fragments.
///
/// Output:
/// - A space-separated string.
///
/// Transformation:
/// - Mirrors `join_token_texts` for callers that already own string fragments.
fn join_token_texts_from_strings(parts: &[String]) -> String {
    parts
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compacts all whitespace out of trait type text.
///
/// Inputs:
/// - `input`: type-expression text.
///
/// Output:
/// - Text without whitespace.
///
/// Transformation:
/// - Uses the compact comparison style expected by trait signature diagnostics.
fn compact_trait_type_text(input: &str) -> String {
    input.split_whitespace().collect::<String>()
}
