use super::core_expr_lowering::core_expr_from_syntax;
use super::core_expr_proof::core_expr_proof_coverage;
use super::core_intrinsic_lowering::core_mutable_receiver_call_expr_from_syntax;
use super::core_pattern_lowering::{
    core_pattern_from_syntax, core_pattern_proof_coverage, core_pattern_summary_text,
};
use super::*;

mod evidence;
mod module_facts;

use evidence::{
    core_expr_checked_preservation_evidence, core_pattern_checked_preservation_evidence,
};
pub(crate) use module_facts::{
    core_resolved_imported_modules, core_syntax_imports, core_syntax_trait_conformances,
    merge_core_imports,
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
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_prop_order: &HashMap<String, Vec<String>>,
) -> HashMap<(String, usize), Vec<CoreFunctionClause>> {
    let mut clauses = HashMap::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Function {
            name,
            params,
            clauses: function_clauses,
            ..
        } = &declaration.payload
        {
            clauses.insert(
                (name.clone(), params.len()),
                function_clauses
                    .iter()
                    .map(|clause| {
                        core_function_clause_summary(clause, receiver_methods, template_prop_order)
                    })
                    .collect(),
            );
        }
    }
    clauses
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
    identities.extend(local_aliases.iter().filter_map(|(name, _)| {
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
    if let Some(core_expr) = &mut summary.core_expr {
        resolve_constructor_identities_in_core_expr(core_expr, constructor_identities);
    }
    for child in &mut summary.children {
        resolve_constructor_identities_in_expr_summary(child, constructor_identities);
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
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::SqlQuery { .. } => {}
        CoreExpr::Tuple(items)
        | CoreExpr::List(items)
        | CoreExpr::FixedArray(items)
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
            pattern,
            source,
            guard,
        } => {
            resolve_constructor_identities_in_core_expr(expr, constructor_identities);
            resolve_constructor_identities_in_core_pattern(pattern, constructor_identities);
            resolve_constructor_identities_in_core_expr(source, constructor_identities);
            if let Some(guard) = guard {
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
        | CorePattern::Atom(_) => {}
        CorePattern::Tuple(elements) | CorePattern::List(elements) => {
            for element in elements {
                resolve_constructor_identities_in_core_pattern(element, constructor_identities);
            }
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
fn core_function_clause_summary(
    clause: &terlan_syntax::SyntaxFunctionClauseOutput,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_prop_order: &HashMap<String, Vec<String>>,
) -> CoreFunctionClause {
    let patterns = clause
        .patterns
        .iter()
        .map(core_pattern_summary_text)
        .collect();
    let core_patterns: Vec<Option<CorePattern>> = clause
        .patterns
        .iter()
        .map(core_pattern_from_syntax)
        .collect();
    let pattern_proof_coverage = clause
        .patterns
        .iter()
        .zip(core_patterns.iter())
        .map(|(pattern, core_pattern)| core_pattern_proof_coverage(pattern, core_pattern.as_ref()))
        .collect();
    let pattern_checked_preservation_evidence = clause
        .patterns
        .iter()
        .zip(core_patterns.iter())
        .map(|(_, core_pattern)| {
            core_pattern
                .as_ref()
                .and_then(core_pattern_checked_preservation_evidence)
        })
        .collect();
    CoreFunctionClause {
        patterns,
        core_patterns,
        pattern_proof_coverage,
        pattern_checked_preservation_evidence,
        guard: clause
            .guard
            .as_ref()
            .map(|guard| core_expr_summary(guard, receiver_methods, template_prop_order)),
        body: core_expr_summary(&clause.body, receiver_methods, template_prop_order),
    }
}

/// Converts a syntax expression into a recursive CoreIR expression summary.
///
/// Inputs:
/// - `expr`: syntax-output expression.
///
/// Output:
/// - Core expression summary.
///
/// Transformation:
/// - Preserves semantic expression kind, arity, text, remote target, operator,
///   and recursively summarized child expressions while intentionally omitting
///   backend rendering details.
fn core_expr_summary(
    expr: &SyntaxExprOutput,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_prop_order: &HashMap<String, Vec<String>>,
) -> CoreExprSummary {
    let mut children = expr
        .children
        .iter()
        .map(|child| core_expr_summary(child, receiver_methods, template_prop_order))
        .collect::<Vec<_>>();
    children.extend(
        expr.fields
            .iter()
            .map(|field| core_expr_summary(&field.value, receiver_methods, template_prop_order)),
    );
    children.extend(expr.clauses.iter().flat_map(|clause| {
        let mut clause_children = Vec::new();
        if let Some(guard) = &clause.guard {
            clause_children.push(core_expr_summary(
                guard,
                receiver_methods,
                template_prop_order,
            ));
        }
        clause_children.push(core_expr_summary(
            &clause.body,
            receiver_methods,
            template_prop_order,
        ));
        clause_children
    }));
    children.extend(expr.catch_clauses.iter().flat_map(|clause| {
        let mut clause_children = Vec::new();
        if let Some(guard) = &clause.guard {
            clause_children.push(core_expr_summary(
                guard,
                receiver_methods,
                template_prop_order,
            ));
        }
        clause_children.push(core_expr_summary(
            &clause.body,
            receiver_methods,
            template_prop_order,
        ));
        clause_children
    }));
    if let Some(after) = &expr.try_after {
        children.push(core_expr_summary(
            &after.trigger,
            receiver_methods,
            template_prop_order,
        ));
        children.push(core_expr_summary(
            &after.body,
            receiver_methods,
            template_prop_order,
        ));
    }
    let core_expr = core_mutable_receiver_call_expr_from_syntax(expr, receiver_methods)
        .or_else(|| core_template_call_expr_from_syntax(expr, template_prop_order))
        .or_else(|| core_expr_from_syntax(expr));
    let checked_preservation_evidence = core_expr
        .as_ref()
        .and_then(core_expr_checked_preservation_evidence);
    let proof_coverage = core_expr_proof_coverage(expr, core_expr.as_ref());

    CoreExprSummary {
        kind: format!("{:?}", expr.kind),
        core_expr,
        checked_preservation_evidence,
        proof_coverage,
        text: expr.text.clone(),
        remote: expr.remote.clone(),
        operator: expr.operator.clone(),
        arity: expr.arity,
        children,
    }
}

/// Converts a direct generated template function call into CoreIR.
///
/// Inputs:
/// - `expr`: syntax-output expression that may be a direct template call.
/// - `template_prop_order`: template names mapped to declaration-order props.
///
/// Output:
/// - `Some(CoreExpr::TemplateInstantiate)` when `expr` is a local direct call
///   to a declared template and all provided argument values lower to Core.
/// - `None` for non-template calls or unsupported argument expressions.
///
/// Transformation:
/// - Maps positional call arguments to declaration-order props and named
///   arguments to exact prop keys, preserving the same backend-neutral shape as
///   `Page{...}` template instantiation.
fn core_template_call_expr_from_syntax(
    expr: &SyntaxExprOutput,
    template_prop_order: &HashMap<String, Vec<String>>,
) -> Option<CoreExpr> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return None;
    }
    let (callee, args) = expr.children.split_first()?;
    let name = match callee.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => callee.text.as_deref()?,
        _ => return None,
    };
    let prop_order = template_prop_order.get(name)?;
    let mut fields = Vec::with_capacity(args.len());
    let mut next_positional_index = 0;
    for (index, arg) in args.iter().enumerate() {
        let key = if let Some(arg_name) = expr.arg_names.get(index).and_then(Option::as_ref) {
            arg_name.clone()
        } else {
            let prop_name = prop_order.get(next_positional_index)?;
            next_positional_index += 1;
            prop_name.clone()
        };
        fields.push(CoreRecordExprField {
            key,
            required: true,
            value: core_expr_from_syntax(arg)?,
        });
    }

    Some(CoreExpr::TemplateInstantiate {
        name: name.to_string(),
        fields,
    })
}
