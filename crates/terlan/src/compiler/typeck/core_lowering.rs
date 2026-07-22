use super::*;
use crate::terlan_syntax::syntax_output::SyntaxAnnotationValueOutput;

/// Lowers resolved formal compiler state to the current core boundary.
///
/// Inputs:
/// - `resolved` compiler module produced by resolution and typechecking.
///
/// Output:
/// - Deterministic backend-neutral `CoreModule` payload.
///
/// Transformation:
/// - Copies the resolver interface into the core artifact and retains the
///   canonical module name.
/// - This function intentionally does not include backend-specific calls,
///   Vm syntax, or JVM/JS encoding assumptions.
pub fn lower_resolved_module_to_core(resolved: &ResolvedModule) -> CoreModule {
    let imports = lower_core_imports(resolved);
    let exports = lower_core_exports(&resolved.interface);
    let types = lower_core_types(&resolved.interface);
    let functions = lower_core_functions(&resolved.interface);
    let constructors = lower_core_constructors(&resolved.interface);
    let metadata = core_module_metadata(&functions, &types, &constructors);

    CoreModule {
        schema: CORE_IR_SCHEMA.to_string(),
        module: resolved.name.clone(),
        source: CoreSourceIdentity {
            source_kind: "resolved_module".to_string(),
            source_path: None,
            syntax_contract_fingerprint: None,
        },
        imports,
        exports,
        types,
        functions,
        constructors,
        templates: Vec::new(),
        trait_conformances: Vec::new(),
        metadata,
        interface: resolved.interface.clone(),
    }
}

/// Lowers syntax-output plus resolved formal compiler state to CoreIR.
///
/// Inputs:
/// - `module`: compiler-facing syntax output produced from the canonical syntax
///   contract.
/// - `resolved`: resolver artifact after formal typechecking.
///
/// Output:
/// - Deterministic backend-neutral `CoreModule` payload with function clause
///   and expression summaries.
///
/// Transformation:
/// - Starts from the resolver/interface Core boundary, attaches syntax contract
///   identity, and overlays syntax-output function clauses as Core summaries
///   without encoding backend syntax or emitted Vm forms.
pub fn lower_syntax_module_output_to_core(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> CoreModule {
    let (macro_expanded_module, _) = super::expand_syntax_raw_macros(module.clone());
    let (mut prepared_module, _) = super::prepare_syntax_constants_with_interfaces(
        &macro_expanded_module,
        &resolved.interface_map,
    );
    let module_aliases =
        super::collect_syntax_import_maps(&prepared_module, &resolved.interface_map).module_aliases;
    canonicalize_core_module_aliases(&mut prepared_module, &module_aliases);
    annotate_syntax_comprehension_lifts(&mut prepared_module, resolved);
    let module = &prepared_module;
    let mut core = lower_resolved_module_to_core(resolved);
    let macro_functions = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                is_macro: true,
                ..
            } => Some((name.clone(), params.len())),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    core.functions
        .retain(|function| !macro_functions.contains(&(function.name.clone(), function.arity)));
    core.exports.retain(|export| match &export.kind {
        CoreExportKind::Function { arity } => {
            !macro_functions.contains(&(export.name.clone(), *arity))
        }
        CoreExportKind::Type | CoreExportKind::Constructor { .. } => true,
    });
    let expanded_interface = syntax_module_output_to_interface(module);
    core.interface.struct_fields = expanded_interface.struct_fields;
    core.source = CoreSourceIdentity {
        source_kind: format!("{:?}", module.source_kind),
        source_path: None,
        syntax_contract_fingerprint: Some(module.syntax_contract.fingerprint.clone()),
    };
    core.imports = core_syntax_imports(module);
    merge_core_imports(&mut core.imports, core_resolved_imported_modules(resolved));
    core.trait_conformances = core_syntax_trait_conformances(module);
    let syntax_struct_bodies = core_syntax_struct_type_bodies(module);
    for type_decl in &mut core.types {
        if let Some(core_body) = syntax_struct_bodies.get(&type_decl.name) {
            type_decl.core_body = Some(core_body.clone());
        }
    }

    let receiver_methods = core_receiver_method_dispatch_signatures(module, resolved);
    let template_prop_order = core_syntax_template_prop_order(module);
    let mut function_clauses =
        core_syntax_function_clauses(module, &receiver_methods, &template_prop_order);
    let (mut structural_impl_functions, structural_impl_dispatch) =
        core_syntax_structural_impl_dispatch(
            module,
            resolved,
            &receiver_methods,
            &template_prop_order,
        );
    for function in &mut structural_impl_functions {
        function_clauses.insert(
            (function.name.clone(), function.arity),
            std::mem::take(&mut function.clauses),
        );
    }
    let native_operations = core_syntax_native_operations(module);
    let constructor_identities = core_constructor_identities(module, resolved, &core.constructors);
    resolve_constructor_identities_in_function_clauses(
        &mut function_clauses,
        &constructor_identities,
    );
    refresh_core_evidence_in_function_clauses(&mut function_clauses);
    core.functions.extend(structural_impl_functions);
    for function in &mut core.functions {
        if let Some(clauses) = function_clauses.get(&(function.name.clone(), function.arity)) {
            function.clauses = clauses.clone();
        }
        function.native_operation = native_operations
            .get(&(function.name.clone(), function.arity))
            .cloned();
    }
    rewrite_structural_impl_calls(&mut core.functions, &structural_impl_dispatch);
    core.functions.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.arity.cmp(&right.arity))
    });
    core.metadata = core_module_metadata(&core.functions, &core.types, &core.constructors);
    core
}

/// Attaches typechecked `GuardResult` lift containers before CoreIR lowering.
fn annotate_syntax_comprehension_lifts(module: &mut SyntaxModuleOutput, resolved: &ResolvedModule) {
    let mut lifts = Vec::new();
    for interface in std::iter::once(&resolved.interface).chain(resolved.interface_map.values()) {
        for conformance in &interface.trait_conformances {
            if conformance.is_negative {
                continue;
            }
            let Some(instance) = parse_trait_instance_from_text(&conformance.trait_ref) else {
                continue;
            };
            if instance.name.rsplit('.').next() != Some("GuardResult") {
                continue;
            }
            let [result, container] = instance.type_args.as_slice() else {
                continue;
            };
            lifts.push((type_constructor_head(result), container.clone()));
        }
    }

    for declaration in &mut module.declarations {
        let clauses = match &mut declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. }
            | SyntaxDeclarationPayload::Method { clauses, .. } => clauses,
            _ => continue,
        };
        for clause in clauses {
            annotate_comprehension_lifts_in_expr(&mut clause.body, resolved, &lifts);
            if let Some(guard) = &mut clause.guard {
                annotate_comprehension_lifts_in_expr(guard, resolved, &lifts);
            }
        }
    }
}

fn annotate_comprehension_lifts_in_expr(
    expr: &mut SyntaxExprOutput,
    resolved: &ResolvedModule,
    lifts: &[(String, String)],
) {
    if expr.kind == SyntaxExprKind::ListComprehension {
        let guard_start = expr.patterns.len() + 1;
        let mut selected: Option<String> = None;
        for guard in expr.children.iter().skip(guard_start) {
            let Some(result) = syntax_call_return_type(guard, resolved) else {
                continue;
            };
            let head = type_constructor_head(result);
            for (result_head, container) in lifts {
                if type_heads_match(&head, result_head) {
                    selected.get_or_insert_with(|| container.clone());
                }
            }
        }
        expr.comprehension_lift = selected;
    }

    for child in &mut expr.children {
        annotate_comprehension_lifts_in_expr(child, resolved, lifts);
    }
    for guard in &mut expr.let_guards {
        if let Some(guard) = guard {
            annotate_comprehension_lifts_in_expr(guard, resolved, lifts);
        }
    }
    for field in &mut expr.fields {
        annotate_comprehension_lifts_in_expr(&mut field.value, resolved, lifts);
    }
    for clause in expr.clauses.iter_mut().chain(&mut expr.catch_clauses) {
        if let Some(guard) = &mut clause.guard {
            annotate_comprehension_lifts_in_expr(guard, resolved, lifts);
        }
        annotate_comprehension_lifts_in_expr(&mut clause.body, resolved, lifts);
    }
    if let Some(after) = &mut expr.try_after {
        annotate_comprehension_lifts_in_expr(&mut after.trigger, resolved, lifts);
        annotate_comprehension_lifts_in_expr(&mut after.body, resolved, lifts);
    }
}

fn syntax_call_return_type<'a>(
    expr: &SyntaxExprOutput,
    resolved: &'a ResolvedModule,
) -> Option<&'a str> {
    if expr.kind != SyntaxExprKind::Call {
        return None;
    }
    let name = expr.children.first()?.text.as_deref()?;
    if let Some(module) = expr.remote.as_deref() {
        return resolved
            .interface_map
            .get(module)
            .and_then(|interface| interface.functions.get(&(name.to_string(), expr.arity)))
            .map(|signature| signature.return_type.as_str());
    }
    if let Some(symbol) = resolved
        .function_symbols
        .get(&(name.to_string(), expr.arity))
    {
        return Some(symbol.return_type.as_str());
    }
    let mut matches = resolved.interface_map.values().filter_map(|interface| {
        interface
            .functions
            .get(&(name.to_string(), expr.arity))
            .map(|signature| signature.return_type.as_str())
    });
    let first = matches.next()?;
    matches.all(|candidate| candidate == first).then_some(first)
}

fn type_constructor_head(ty: &str) -> String {
    ty.split_once('[')
        .map(|(head, _)| head)
        .unwrap_or(ty)
        .trim()
        .to_string()
}

fn type_heads_match(left: &str, right: &str) -> bool {
    left == right || left.rsplit('.').next() == right.rsplit('.').next()
}

/// Canonicalizes source-visible module aliases before syntax-to-Core lowering.
///
/// Inputs:
/// - `module`: the prepared syntax clone used only for CoreIR construction.
/// - `aliases`: resolver/typechecker import aliases mapped to provider modules.
///
/// Output:
/// - Function and method expressions carry canonical provider module names.
///
/// Transformation:
/// - Rewrites only the structured `remote` field, recursively through every
///   expression-owned child. Source text and the caller's syntax tree remain
///   untouched. This lets intrinsic lookup use semantic module identity while
///   preserving ordinary imported source such as `Process.yield_now()`.
fn canonicalize_core_module_aliases(
    module: &mut SyntaxModuleOutput,
    aliases: &HashMap<String, String>,
) {
    if aliases.is_empty() {
        return;
    }

    for declaration in &mut module.declarations {
        let clauses = match &mut declaration.payload {
            SyntaxDeclarationPayload::Function {
                params, clauses, ..
            }
            | SyntaxDeclarationPayload::Method {
                params, clauses, ..
            } => {
                for param in params {
                    if let Some(default) = &mut param.default {
                        canonicalize_core_expr_module_aliases(default, aliases);
                    }
                }
                clauses
            }
            _ => continue,
        };
        for clause in clauses {
            if let Some(guard) = &mut clause.guard {
                canonicalize_core_expr_module_aliases(guard, aliases);
            }
            canonicalize_core_expr_module_aliases(&mut clause.body, aliases);
        }
    }
}

fn canonicalize_core_expr_module_aliases(
    expr: &mut SyntaxExprOutput,
    aliases: &HashMap<String, String>,
) {
    if let Some(remote) = &mut expr.remote {
        if let Some(canonical) = aliases.get(remote) {
            *remote = canonical.clone();
        }
    }
    for child in &mut expr.children {
        canonicalize_core_expr_module_aliases(child, aliases);
    }
    for guard in &mut expr.let_guards {
        if let Some(guard) = guard {
            canonicalize_core_expr_module_aliases(guard, aliases);
        }
    }
    for field in &mut expr.fields {
        canonicalize_core_expr_module_aliases(&mut field.value, aliases);
    }
    for clause in expr.clauses.iter_mut().chain(&mut expr.catch_clauses) {
        if let Some(guard) = &mut clause.guard {
            canonicalize_core_expr_module_aliases(guard, aliases);
        }
        canonicalize_core_expr_module_aliases(&mut clause.body, aliases);
    }
    if let Some(after) = &mut expr.try_after {
        canonicalize_core_expr_module_aliases(&mut after.trigger, aliases);
        canonicalize_core_expr_module_aliases(&mut after.body, aliases);
    }
}

/// Collects explicit compiler-native operation identities by callable ABI.
fn core_syntax_native_operations(module: &SyntaxModuleOutput) -> HashMap<(String, usize), String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let operation = declaration.annotations.iter().find_map(|annotation| {
                (annotation.path == ["compiler", "native"])
                    .then(|| annotation.values.first())
                    .flatten()
                    .and_then(|value| match value {
                        SyntaxAnnotationValueOutput::Name { segments } if !segments.is_empty() => {
                            Some(segments.join("."))
                        }
                        _ => None,
                    })
            })?;
            match &declaration.payload {
                SyntaxDeclarationPayload::Function { name, params, .. } => {
                    Some(((name.clone(), params.len()), operation))
                }
                SyntaxDeclarationPayload::Method { name, params, .. } => {
                    Some(((name.clone(), params.len() + 1), operation))
                }
                _ => None,
            }
        })
        .collect()
}

/// Collects declaration-order template props for CoreIR template-call lowering.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
///
/// Output:
/// - Template name to declaration-order prop-name list.
///
/// Transformation:
/// - Preserves only the metadata needed to map `Page(...)` positional calls
///   into backend-neutral template-instantiation fields.
fn core_syntax_template_prop_order(module: &SyntaxModuleOutput) -> HashMap<String, Vec<String>> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Template { name, props, .. } => Some((
                name.clone(),
                props
                    .iter()
                    .map(|prop| prop.name.clone())
                    .collect::<Vec<_>>(),
            )),
            _ => None,
        })
        .collect()
}

use super::core_interface::*;
use super::core_proof::*;
