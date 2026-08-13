use super::core_expr_lowering::core_expr_from_syntax;
use super::core_expr_proof::core_expr_proof_coverage;
use super::core_intrinsic_lowering::core_mutable_receiver_call_expr_from_syntax;
use super::core_pattern_lowering::{
    core_pattern_from_syntax, core_pattern_proof_coverage, core_pattern_summary_text,
};
use super::*;

use std::collections::HashSet;

mod evidence;
mod module_facts;
mod structural_impl;

use evidence::{
    core_expr_checked_preservation_evidence, core_pattern_checked_preservation_evidence,
};
pub(crate) use module_facts::{
    core_resolved_executable_modules, core_resolved_imported_modules, core_syntax_imports,
    core_syntax_trait_conformances, merge_core_imports,
};
pub(crate) use structural_impl::{
    core_syntax_structural_impl_dispatch, rewrite_structural_impl_calls,
};

pub(crate) mod metadata;
pub(crate) use metadata::core_module_metadata;

/// Collects CoreIR function clause summaries from syntax output.
///
/// Inputs:
/// - `module`: compiler-facing syntax output.
///
/// Output:
/// - Map keyed by function name and arity.
///
/// Transformation:
/// - Converts syntax-output clause patterns, guards, and bodies into stable
///   backend-neutral summaries for the initial CoreIR lowering slice.
pub(crate) fn core_syntax_function_clauses(
    module: &SyntaxModuleOutput,
    functions: &[CoreFunction],
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_prop_order: &HashMap<String, Vec<String>>,
) -> HashMap<(String, usize), Vec<CoreFunctionClause>> {
    let mut clauses = HashMap::new();
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                clauses: function_clauses,
                ..
            } => {
                if !core_signature_matches(functions, name, params) {
                    continue;
                }
                let function_value_locals = function_value_parameter_names(params);
                clauses.insert(
                    (name.clone(), params.len()),
                    function_clauses
                        .iter()
                        .map(|clause| {
                            core_function_clause_summary(
                                clause,
                                receiver_methods,
                                template_prop_order,
                                &function_value_locals,
                            )
                        })
                        .collect(),
                );
            }
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                clauses: method_clauses,
                ..
            } => {
                let mut receiver_first_params = Vec::with_capacity(params.len() + 1);
                receiver_first_params.push(receiver.as_ref().clone());
                receiver_first_params.extend(params.iter().cloned());
                if !core_signature_matches(functions, name, &receiver_first_params) {
                    continue;
                }
                let function_value_locals = function_value_parameter_names(&receiver_first_params);
                clauses.insert(
                    (name.clone(), receiver_first_params.len()),
                    receiver_method_clauses_with_bindings(receiver, params, method_clauses)
                        .iter()
                        .map(|clause| {
                            core_function_clause_summary(
                                clause,
                                receiver_methods,
                                template_prop_order,
                                &function_value_locals,
                            )
                        })
                        .collect(),
                );
            }
            _ => {}
        }
    }
    clauses
}

/// Selects the source overload represented by the compatibility Core
/// signature. CoreIR does not yet encode overload identity independently of
/// `(name, arity)`, so attaching a different overload's clauses would create a
/// malformed typed function and let backend behavior depend on declaration
/// order.
fn core_signature_matches(
    functions: &[CoreFunction],
    name: &str,
    params: &[crate::terlan_syntax::syntax_output::SyntaxParamOutput],
) -> bool {
    let Some(function) = functions
        .iter()
        .find(|function| function.name == name && function.arity == params.len())
    else {
        return false;
    };
    function.params.iter().zip(params).all(|(core, syntax)| {
        core_type_from_text(&core.ty) == core_type_from_text(&syntax.annotation.text)
    })
}

/// Annotates syntax-lowered Core clauses with resolved constructor identities.
///
/// Inputs:
/// - `clauses`: mutable syntax-output Core function-clause summaries.
/// - `constructor_identities`: local constructor names mapped to stable
///   semantic constructor identities.
///
/// Output:
/// - None; constructor-call candidate payloads are updated in place.
///
/// Transformation:
/// - Recursively annotates `CoreExpr::ConstructorCall`,
///   `CoreExpr::ConstructorChain`, and `CorePattern::Constructor` nodes whose
///   candidate name resolves in the current module, an eligible single-shape
///   type alias, or imported public constructor/type-alias surface. Unknown
///   uppercase calls and patterns remain candidate-only.
pub(crate) fn resolve_constructor_identities_in_function_clauses(
    clauses: &mut HashMap<(String, usize), Vec<CoreFunctionClause>>,
    constructor_identities: &HashMap<String, String>,
) {
    if constructor_identities.is_empty() {
        return;
    }

    for function_clauses in clauses.values_mut() {
        for clause in function_clauses {
            for pattern in clause.core_patterns.iter_mut().flatten() {
                resolve_constructor_identities_in_core_pattern(pattern, constructor_identities);
            }
            if let Some(guard) = &mut clause.guard {
                resolve_constructor_identities_in_expr_summary(guard, constructor_identities);
            }
            resolve_constructor_identities_in_expr_summary(
                &mut clause.body,
                constructor_identities,
            );
        }
    }
}

/// Refreshes proof evidence after Core payload annotation.
///
/// Inputs:
/// - `clauses`: mutable syntax-output Core function-clause summaries.
///
/// Output:
/// - None; evidence payloads and annotation-dependent proof labels are updated
///   in place.
///
/// Transformation:
/// - Recomputes expression-summary and top-level pattern preservation evidence
///   from final typed Core payloads after semantic annotation passes have
///   changed Core contract text, such as constructor identity resolution.
/// - Recomputes proof coverage for forms whose coverage depends on final
///   semantic annotation, such as resolved constructor calls.
pub(crate) fn refresh_core_evidence_in_function_clauses(
    clauses: &mut HashMap<(String, usize), Vec<CoreFunctionClause>>,
) {
    for function_clauses in clauses.values_mut() {
        for clause in function_clauses {
            for (evidence, pattern) in clause
                .pattern_checked_preservation_evidence
                .iter_mut()
                .zip(&clause.core_patterns)
            {
                if let Some(pattern) = pattern {
                    *evidence = core_pattern_checked_preservation_evidence(pattern);
                }
            }
            if let Some(guard) = &mut clause.guard {
                refresh_core_evidence_in_expr_summary(guard);
            }
            refresh_core_evidence_in_expr_summary(&mut clause.body);
        }
    }
}

/// Refreshes proof evidence in one expression-summary tree.
///
/// Inputs:
/// - `summary`: mutable Core expression summary.
///
/// Output:
/// - None; expression evidence payloads and annotation-dependent proof labels
///   are updated in place.
///
/// Transformation:
/// - Recomputes the current summary's evidence from its final typed Core
///   payload.
/// - Promotes resolved constructor calls to Lean-covered proof coverage while
///   leaving unresolved constructor-call candidates partial.
/// - Recursively refreshes all child summaries.
fn refresh_core_evidence_in_expr_summary(summary: &mut CoreExprSummary) {
    summary.checked_preservation_evidence = summary
        .core_expr
        .as_ref()
        .and_then(core_expr_checked_preservation_evidence);
    if let Some(CoreExpr::ConstructorCall {
        constructor_identity,
        ..
    }) = &summary.core_expr
    {
        summary.proof_coverage = if constructor_identity.is_some() {
            CoreProofCoverage::LeanCovered
        } else {
            CoreProofCoverage::Partial
        };
    }
    for child in &mut summary.children {
        refresh_core_evidence_in_expr_summary(child);
    }
}

/// Collects receiver-method dispatch metadata for syntax-to-Core lowering.
///
/// Inputs:
/// - `module`: syntax-output module whose local receiver methods should be
///   available to Core expression summarization.
/// - `resolved`: resolved module state containing imported type names and
///   imported type-alias interfaces.
///
/// Output:
/// - Receiver-method dispatch signatures keyed by `(method name, non-receiver
///   arity)`.
///
/// Transformation:
/// - Rebuilds the same alias/type-name context used by typechecking, then
///   delegates to the receiver-method dispatch collector so CoreIR lowering can
///   preserve the declared mutability marker without reading backend syntax.
pub(crate) fn core_receiver_method_dispatch_signatures(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>> {
    let local_aliases = collect_syntax_type_aliases(module);
    let imported_aliases = imported_type_aliases(resolved);
    let imported_names = imported_type_names(resolved);
    let mut alias_names = collect_syntax_type_names(module);
    alias_names.extend(imported_aliases.keys().cloned());
    alias_names.extend(resolved.imported_types.keys().cloned());
    alias_names.extend(collect_syntax_alias_extra_names(module));
    alias_names.extend(primitive_type_names());

    collect_syntax_receiver_method_dispatch_signatures_with_imports(
        module,
        resolved,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &local_aliases,
    )
}

/// Collects constructor identities eligible for CoreIR identity annotation.
///
/// Inputs:
/// - `module`: syntax-output module whose declarations may include local
///   constructors and eligible single-shape type aliases.
/// - `resolved`: resolved module context containing imported item metadata and
///   interface snapshots.
/// - `constructors`: Core constructor declarations from the resolved interface.
///
/// Output:
/// - Map from source-visible constructor name to stable CoreIR constructor
///   identity.
///
/// Transformation:
/// - Preserves local constructor identities as their source-visible name.
/// - Preserves local default struct-constructor identities for structs that do
///   not declare explicit constructors.
/// - Preserves eligible local single-shape type aliases as their source-visible
///   name.
/// - Adds imported public constructors as `module.name` identities so aliased
///   imports can be distinguished from local constructor declarations.
/// - Adds imported public eligible single-shape type aliases as `module.name`
///   identities for the same reason.
/// - Uses both syntax-output declarations and resolved Core constructor
///   declarations so identity annotation can proceed while the Core constructor
///   declaration migration is still catching up.
pub(crate) fn core_constructor_identities(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    constructors: &[CoreConstructorDecl],
) -> HashMap<String, String> {
    let mut identities = constructors
        .iter()
        .map(|constructor| (constructor.name.clone(), constructor.name.clone()))
        .collect::<HashMap<_, _>>();
    identities.extend(module.declarations.iter().filter_map(
        |declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Constructor { name, .. } => {
                Some((name.clone(), name.clone()))
            }
            _ => None,
        },
    ));
    let explicit_constructor_names = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Constructor { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    identities.extend(module.declarations.iter().filter_map(
        |declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, .. }
                if !explicit_constructor_names.contains(name.as_str()) =>
            {
                Some((name.clone(), name.clone()))
            }
            _ => None,
        },
    ));
    let local_aliases = collect_syntax_type_aliases(module);
    identities.extend(local_aliases.keys().filter_map(|name| {
        alias_constructor_schemes(name, &local_aliases).map(|_| (name.clone(), name.clone()))
    }));
    identities.extend(
        resolved
            .imported_types
            .iter()
            .filter_map(|(local_name, imported)| {
                let interface = resolved.interface_map.get(&imported.source_module)?;
                let signatures = interface.constructors.get(&imported.source_name)?;
                signatures
                    .iter()
                    .any(|signature| signature.public)
                    .then(|| {
                        (
                            local_name.clone(),
                            format!("{}.{}", imported.source_module, imported.source_name),
                        )
                    })
            }),
    );
    identities.extend(
        resolved
            .imported_types
            .iter()
            .filter_map(|(local_name, imported)| {
                let interface = resolved.interface_map.get(&imported.source_module)?;
                let interface_aliases = interface_type_aliases(interface);
                alias_constructor_schemes(&imported.source_name, &interface_aliases).map(|_| {
                    (
                        local_name.clone(),
                        format!("{}.{}", imported.source_module, imported.source_name),
                    )
                })
            }),
    );
    identities
}

/// Annotates one Core expression summary tree with constructor identities.
///
/// Inputs:
/// - `summary`: mutable Core expression summary.
/// - `constructor_identities`: source-visible constructor names mapped to
///   stable semantic identities.
///
/// Output:
/// - None; nested Core expression payloads are updated in place.
///
/// Transformation:
/// - Recursively walks both the typed Core payload and summary children so the
///   current node and all nested expression summaries agree on constructor
///   identity annotations.
fn resolve_constructor_identities_in_expr_summary(
    summary: &mut CoreExprSummary,
    constructor_identities: &HashMap<String, String>,
) {
    for child in &mut summary.children {
        resolve_constructor_identities_in_expr_summary(child, constructor_identities);
    }
    if let Some(core_expr) = &mut summary.core_expr {
        resolve_constructor_identities_in_core_expr(core_expr, constructor_identities);
        if let CoreExpr::ConstructorChain {
            base_constructor_identity,
            ..
        } = core_expr
        {
            if base_constructor_identity.is_none() {
                *base_constructor_identity = summary
                    .children
                    .first()
                    .and_then(|child| child.core_expr.as_ref())
                    .and_then(|child| match child {
                        CoreExpr::ConstructorCall {
                            constructor_identity,
                            ..
                        } => constructor_identity.clone(),
                        _ => None,
                    });
            }
        }
    }
}

/// Annotates one typed Core expression with constructor identities.
///
/// Inputs:
/// - `expr`: mutable typed Core expression.
/// - `constructor_identities`: source-visible constructor names mapped to
///   stable semantic identities.
///
/// Output:
/// - None; matching constructor-call and constructor-pattern payloads are
///   updated in place.
///
/// Transformation:
/// - Traverses every recursive expression and embedded-pattern position and
///   sets constructor identity fields when a candidate name is declared by the
///   resolved module interface.
fn resolve_constructor_identities_in_core_expr(
    expr: &mut CoreExpr,
    constructor_identities: &HashMap<String, String>,
) {
    match expr {
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
        CoreExpr::Tuple(items)
        | CoreExpr::List(items)
        | CoreExpr::FixedArray(items)
        | CoreExpr::SqlQuery {
            parameters: items, ..
        }
        | CoreExpr::RemoteCall { args: items, .. }
        | CoreExpr::Call { args: items, .. }
        | CoreExpr::Intrinsic(CoreIntrinsicCall { args: items, .. }) => {
            for item in items {
                resolve_constructor_identities_in_core_expr(item, constructor_identities);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            resolve_constructor_identities_in_core_expr(callee, constructor_identities);
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            resolve_constructor_identities_in_core_expr(receiver, constructor_identities);
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
        }
        CoreExpr::Cast { expr, .. } => {
            resolve_constructor_identities_in_core_expr(expr, constructor_identities);
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            resolve_constructor_identities_in_core_expr(head, constructor_identities);
            resolve_constructor_identities_in_core_expr(tail, constructor_identities);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            resolve_constructor_identities_in_core_expr(expr, constructor_identities);
            for generator in generators {
                resolve_constructor_identities_in_core_pattern(
                    &mut generator.pattern,
                    constructor_identities,
                );
                resolve_constructor_identities_in_core_expr(
                    &mut generator.source,
                    constructor_identities,
                );
            }
            for guard in guards {
                resolve_constructor_identities_in_core_expr(guard, constructor_identities);
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                resolve_constructor_identities_in_core_expr(
                    &mut binding.value,
                    constructor_identities,
                );
            }
            resolve_constructor_identities_in_core_expr(body, constructor_identities);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                resolve_constructor_identities_in_core_expr(
                    &mut field.value,
                    constructor_identities,
                );
            }
        }
        CoreExpr::RecordConstruct { fields, .. }
        | CoreExpr::RecordUpdate { fields, .. }
        | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                resolve_constructor_identities_in_core_expr(
                    &mut field.value,
                    constructor_identities,
                );
            }
            if let CoreExpr::RecordUpdate { base, .. } = expr {
                resolve_constructor_identities_in_core_expr(base, constructor_identities);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => {
            resolve_constructor_identities_in_core_expr(base, constructor_identities);
        }
        CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            args,
            record,
        } => {
            if let Some(identity) = constructor_identities.get(base) {
                *base_constructor_identity = Some(identity.clone());
            }
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
            resolve_constructor_identities_in_core_expr(record, constructor_identities);
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => {
            if let Some(identity) = constructor_identities.get(constructor) {
                *constructor_identity = Some(identity.clone());
            }
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            resolve_constructor_identities_in_core_expr(scrutinee, constructor_identities);
            for clause in clauses {
                resolve_constructor_identities_in_core_pattern(
                    &mut clause.pattern,
                    constructor_identities,
                );
                if let Some(guard) = &mut clause.guard {
                    resolve_constructor_identities_in_core_expr(guard, constructor_identities);
                }
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            resolve_constructor_identities_in_core_expr(body, constructor_identities);
            for clause in of_clauses.iter_mut().chain(catch_clauses.iter_mut()) {
                resolve_constructor_identities_in_core_pattern(
                    &mut clause.pattern,
                    constructor_identities,
                );
                if let Some(guard) = &mut clause.guard {
                    resolve_constructor_identities_in_core_expr(guard, constructor_identities);
                }
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
            if let Some(after_clause) = after_clause {
                resolve_constructor_identities_in_core_expr(
                    &mut after_clause.trigger,
                    constructor_identities,
                );
                resolve_constructor_identities_in_core_expr(
                    &mut after_clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                resolve_constructor_identities_in_core_expr(
                    &mut clause.condition,
                    constructor_identities,
                );
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::Lam { params, body } => {
            for param in params {
                resolve_constructor_identities_in_core_pattern(param, constructor_identities);
            }
            resolve_constructor_identities_in_core_expr(body, constructor_identities);
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            resolve_constructor_identities_in_core_expr(left, constructor_identities);
            resolve_constructor_identities_in_core_expr(right, constructor_identities);
        }
    }
}

/// Annotates one typed Core pattern with constructor identities.
///
/// Inputs:
/// - `pattern`: mutable typed Core pattern.
/// - `constructor_identities`: source-visible constructor names mapped to
///   stable semantic identities.
///
/// Output:
/// - None; matching constructor-pattern payloads are updated in place.
///
/// Transformation:
/// - Recursively traverses compound pattern positions and sets
///   `constructor_identity` when a constructor-pattern candidate name is
///   declared locally or imported from a public constructor interface.
fn resolve_constructor_identities_in_core_pattern(
    pattern: &mut CorePattern,
    constructor_identities: &HashMap<String, String>,
) {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::String(_)
        | CorePattern::StringPattern(_)
        | CorePattern::BinaryLayout { .. }
        | CorePattern::Atom(_) => {}
        CorePattern::Tuple(elements) | CorePattern::List(elements) => {
            for element in elements {
                resolve_constructor_identities_in_core_pattern(element, constructor_identities);
            }
        }
        CorePattern::Alias { pattern, .. } => {
            resolve_constructor_identities_in_core_pattern(pattern, constructor_identities);
        }
        CorePattern::ListCons { head, tail } => {
            resolve_constructor_identities_in_core_pattern(head, constructor_identities);
            resolve_constructor_identities_in_core_pattern(tail, constructor_identities);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                resolve_constructor_identities_in_core_pattern(
                    &mut field.value,
                    constructor_identities,
                );
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                resolve_constructor_identities_in_core_pattern(
                    &mut field.value,
                    constructor_identities,
                );
            }
        }
        CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } => {
            if let Some(identity) = constructor_identities.get(name) {
                *constructor_identity = Some(identity.clone());
            }
            for arg in args {
                resolve_constructor_identities_in_core_pattern(arg, constructor_identities);
            }
        }
    }
}

/// Converts one syntax function clause into a CoreIR clause summary.
///
/// Inputs:
/// - `clause`: syntax-output function clause.
///
/// Output:
/// - Core function clause summary.
///
/// Transformation:
/// - Renders patterns into stable syntax summaries and recursively summarizes
///   guard/body expressions without backend lowering. Pattern proof labels are
///   retained in the same order as the rendered pattern summaries.
mod summary;

pub(crate) use summary::{core_function_clause_summary, function_value_parameter_names};
