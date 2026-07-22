use super::super::expression::{expression_has_effects_with_call_facts, EffectfulCallFacts};
use super::*;
use crate::terlan_hir::FunctionSignature;

/// Imported calls that lack an explicit purity proof.
pub(super) struct ImportedEffectFacts {
    pub(super) selected: HashSet<(String, usize)>,
    pub(super) qualified: HashSet<(String, String, usize)>,
    pub(super) receiver_methods: HashSet<(String, usize)>,
    pub(super) trait_calls: HashSet<(String, String, usize)>,
    pub(super) trait_receiver_calls: HashSet<(String, usize)>,
    pub(super) module_aliases: HashSet<String>,
}

/// Collects local functions whose bodies contain structural effects.
///
/// Inputs:
/// - `module`: syntax-output module being typechecked.
/// - `function_signatures`: parsed local callable parameter types.
/// - `aliases`: visible aliases used to recognize named function types.
/// - `templates`: local template declarations visible to function bodies.
/// - `effectful_imported_calls`: selected imported calls without purity proof.
///
/// Output:
/// - Function name/arity pairs for ordinary functions with at least one guard
///   or body that directly performs an effect or reaches an effectful call.
///
/// Transformation:
/// - Computes a fixed point over same-module calls so declaration order and
///   recursive call components cannot hide imported or structural effects.
pub(super) fn collect_effectful_local_calls(
    module: &SyntaxModuleOutput,
    function_signatures: &HashMap<(String, usize), Vec<FunctionScheme>>,
    aliases: &HashMap<String, TypeAlias>,
    templates: &HashMap<String, TemplateScheme>,
    imported_effects: &ImportedEffectFacts,
) -> HashSet<(String, usize)> {
    let no_effectful_locals = HashSet::new();
    let mut effectful = module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let SyntaxDeclarationPayload::Function {
                name,
                params,
                clauses,
                ..
            } = &declaration.payload
            else {
                return None;
            };
            let identity = (name.clone(), params.len());
            let function_values =
                function_value_param_names(params, function_signatures.get(&identity), aliases);
            clauses
                .iter()
                .any(|clause| {
                    clause_has_effects(
                        clause,
                        templates,
                        &no_effectful_locals,
                        &function_values,
                        imported_effects,
                    )
                })
                .then_some(identity)
        })
        .collect::<HashSet<_>>();

    loop {
        let mut changed = false;
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Function {
                name,
                params,
                clauses,
                ..
            } = &declaration.payload
            else {
                continue;
            };
            let identity = (name.clone(), params.len());
            if effectful.contains(&identity) {
                continue;
            }
            let function_values =
                function_value_param_names(params, function_signatures.get(&identity), aliases);
            if clauses.iter().any(|clause| {
                clause_has_effects(
                    clause,
                    templates,
                    &effectful,
                    &function_values,
                    imported_effects,
                )
            }) {
                changed |= effectful.insert(identity);
            }
        }
        if !changed {
            return effectful;
        }
    }
}

fn clause_has_effects(
    clause: &SyntaxFunctionClauseOutput,
    templates: &HashMap<String, TemplateScheme>,
    effectful_local_calls: &HashSet<(String, usize)>,
    function_values: &HashSet<String>,
    imported_effects: &ImportedEffectFacts,
) -> bool {
    let mut facts = effectful_call_facts(effectful_local_calls, imported_effects);
    facts.function_values = Some(function_values);
    clause
        .guard
        .as_ref()
        .is_some_and(|guard| expression_has_effects_with_call_facts(guard, templates, &facts))
        || expression_has_effects_with_call_facts(&clause.body, templates, &facts)
}

/// Collects parameters whose declared type is an invokable function value.
fn function_value_param_names(
    params: &[SyntaxParamOutput],
    schemes: Option<&Vec<FunctionScheme>>,
    aliases: &HashMap<String, TypeAlias>,
) -> HashSet<String> {
    params
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            schemes.is_some_and(|schemes| {
                schemes.iter().any(|scheme| {
                    scheme.params.get(*index).is_some_and(|param| {
                        matches!(expand_type_aliases(param, aliases), Type::Function { .. })
                    })
                })
            })
        })
        .map(|(_, param)| param.name.clone())
        .collect()
}

pub(super) fn effectful_call_facts<'a>(
    local: &'a HashSet<(String, usize)>,
    imported: &'a ImportedEffectFacts,
) -> EffectfulCallFacts<'a> {
    EffectfulCallFacts {
        local: Some(local),
        function_values: None,
        imported: Some(&imported.selected),
        qualified: Some(&imported.qualified),
        imported_receiver: Some(&imported.receiver_methods),
        trait_qualified: Some(&imported.trait_calls),
        trait_receiver: Some(&imported.trait_receiver_calls),
        module_aliases: Some(&imported.module_aliases),
        proven_pure_receiver_calls: None,
    }
}

/// Collects selected imported calls lacking an explicit purity proof.
///
/// Inputs:
/// - `expr_ctx`: expression context containing selected import targets and
///   provider interfaces.
///
/// Output:
/// - Local import alias/arity pairs whose matching public provider overloads
///   are not all marked `@pure`.
///
/// Transformation:
/// - Resolves selected import aliases through provider overload metadata.
/// - Treats mixed pure/impure overload sets conservatively as effectful while
///   leaving missing signatures to ordinary import/type diagnostics.
pub(super) fn collect_effectful_imported_calls(expr_ctx: &ExprInferContext) -> ImportedEffectFacts {
    let mut selected = HashSet::new();
    for (local_name, targets) in expr_ctx.function_imports {
        for target in targets {
            let Some(interface) = expr_ctx.interface_map.get(&target.module) else {
                continue;
            };
            for ((function_name, arity), signatures) in &interface.function_overloads {
                if function_name == &target.function && signatures_have_unproven_purity(signatures)
                {
                    selected.insert((local_name.clone(), *arity));
                }
            }
        }
    }

    let mut qualified = HashSet::new();
    for (module_alias, resolved_module) in expr_ctx.module_aliases {
        let Some(interface) = expr_ctx.interface_map.get(resolved_module) else {
            continue;
        };
        for ((function_name, arity), signatures) in &interface.function_overloads {
            if signatures_have_unproven_purity(signatures) {
                qualified.insert((module_alias.clone(), function_name.clone(), *arity));
            }
        }
    }

    let receiver_methods = expr_ctx
        .interface_map
        .values()
        .flat_map(|interface| interface.function_overloads.values())
        .flat_map(|signatures| signatures.iter())
        .filter(|signature| signature.public && signature.receiver_method && !signature.pure)
        .filter_map(|signature| {
            signature
                .params
                .len()
                .checked_sub(1)
                .map(|arity| (signature.name.clone(), arity))
        })
        .collect();
    let mut trait_calls = HashSet::new();
    let mut trait_receiver_calls = HashSet::new();
    let mut inheritance_cache = HashMap::new();
    for trait_name in expr_ctx.trait_signatures.keys() {
        let methods = collect_trait_methods_with_inheritance(
            expr_ctx.trait_signatures,
            trait_name,
            &mut inheritance_cache,
            &mut HashSet::new(),
        )
        .unwrap_or_default();
        for (method_name, method) in methods {
            if method.pure {
                continue;
            }
            trait_calls.insert((trait_name.clone(), method_name.clone(), method.params.len()));
            if let Some(arity) = method.params.len().checked_sub(1) {
                if !expr_ctx
                    .receiver_methods
                    .contains_key(&(method_name.clone(), arity))
                {
                    trait_receiver_calls.insert((method_name, arity));
                }
            }
        }
    }

    ImportedEffectFacts {
        selected,
        qualified,
        receiver_methods,
        trait_calls,
        trait_receiver_calls,
        module_aliases: expr_ctx.module_aliases.keys().cloned().collect(),
    }
}

fn signatures_have_unproven_purity(signatures: &[FunctionSignature]) -> bool {
    signatures
        .iter()
        .any(|signature| signature.public && !signature.receiver_method && !signature.pure)
}

/// Reports whether the implemented trait method requires a proven-pure body.
pub(super) fn trait_method_requires_pure_body(
    expr_ctx: &ExprInferContext,
    trait_ref: &str,
    method_name: &str,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) -> bool {
    let Some(target) = parse_trait_instance_from_text(trait_ref) else {
        return false;
    };
    collect_trait_methods_with_inheritance(
        expr_ctx.trait_signatures,
        &target.name,
        inheritance_cache,
        &mut HashSet::new(),
    )
    .and_then(|methods| methods.get(method_name).cloned())
    .is_some_and(|method| method.pure)
}

/// Reports whether any declared receiver trait requires this method to be pure.
pub(super) fn receiver_method_requires_pure_body(
    module: &SyntaxModuleOutput,
    expr_ctx: &ExprInferContext,
    receiver_type: &str,
    method_name: &str,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) -> bool {
    let receiver_name = parse_trait_instance_from_text(receiver_type)
        .map(|instance| instance.name)
        .unwrap_or_else(|| receiver_type.to_string());
    module.declarations.iter().any(|declaration| {
        let Some((type_name, implements)) = syntax_declared_implements(declaration) else {
            return false;
        };
        type_name == receiver_name
            && implements.iter().any(|trait_ref| {
                trait_method_requires_pure_body(
                    expr_ctx,
                    &trait_ref.text,
                    method_name,
                    inheritance_cache,
                )
            })
    })
}
