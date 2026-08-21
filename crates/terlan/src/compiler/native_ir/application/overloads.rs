//! Typed overload identity normalization before NativeIR admission.

use super::super::QualifiedFunctionIdentity as OverloadKey;
use super::*;
use crate::terlan_typeck::{core_type_contract_text, CoreTupleTypeElem};

#[derive(Clone)]
/// One source overload and its deterministic NativeIR-facing identity.
struct OverloadCandidate {
    module: String,
    source_name: String,
    arity: usize,
    internal_name: String,
    parameters: Vec<CoreType>,
    result: CoreType,
}

type AliasBodies = HashMap<String, CoreType>;

/// Reports whether a public standard-library facade owns overload lowering.
fn has_target_owned_overload_lowering(module: &str) -> bool {
    matches!(module, "std.http.Response" | "std.template.Template")
}

/// Assigns unique internal identities to typed overloads and rewrites calls.
///
/// Source interfaces remain keyed by their public Terlan name. The native
/// application closure instead receives a distinct name for every parameter
/// vector so its compact `(name, arity)` resolver cannot merge overloads.
pub(super) fn resolve_typed_overloads(cores: &mut [CoreModule]) -> Result<(), String> {
    let groups = collect_overload_groups(cores)?;
    if groups.is_empty() {
        return Ok(());
    }
    let aliases = collect_alias_bodies(cores);
    rename_overload_declarations(cores, &groups)?;
    let returns = collect_return_types(cores);
    for core in cores {
        for function in &mut core.functions {
            let mut environment = function
                .params
                .iter()
                .filter_map(|parameter| {
                    parameter
                        .core_ty
                        .clone()
                        .map(|ty| (parameter.name.clone(), ty))
                })
                .collect::<HashMap<_, _>>();
            for clause in &mut function.clauses {
                let mut clause_environment = environment.clone();
                for (pattern, parameter) in
                    clause.core_patterns.iter().flatten().zip(&function.params)
                {
                    if let Some(ty) = &parameter.core_ty {
                        bind_pattern_type(pattern, ty, &mut clause_environment);
                    }
                }
                if let Some(guard) = &mut clause.guard {
                    if let Some(expr) = &mut guard.core_expr {
                        rewrite_expr(
                            expr,
                            &core.module,
                            &mut clause_environment,
                            &groups,
                            &returns,
                            &aliases,
                        )?;
                    }
                }
                if let Some(expr) = &mut clause.body.core_expr {
                    rewrite_expr(
                        expr,
                        &core.module,
                        &mut clause_environment,
                        &groups,
                        &returns,
                        &aliases,
                    )?;
                }
            }
            environment.clear();
        }
    }
    Ok(())
}

/// Collects transparent, zero-parameter type aliases used during overload matching.
fn collect_alias_bodies(cores: &[CoreModule]) -> AliasBodies {
    let mut aliases = HashMap::new();
    for core in cores {
        for declaration in &core.types {
            if declaration.params.is_empty() {
                if let Some(body) = &declaration.core_body {
                    aliases.insert(declaration.name.clone(), body.clone());
                    aliases.insert(
                        format!("{}.{}", core.module, declaration.name),
                        body.clone(),
                    );
                }
            }
        }
    }
    aliases
}

/// Collects only callable groups that contain multiple distinct type vectors.
fn collect_overload_groups(
    cores: &[CoreModule],
) -> Result<HashMap<OverloadKey, Vec<OverloadCandidate>>, String> {
    let mut declarations = HashMap::<OverloadKey, Vec<(Vec<CoreType>, CoreType)>>::new();
    for core in cores {
        if has_target_owned_overload_lowering(&core.module) {
            continue;
        }
        for function in &core.functions {
            let Some(parameters) = function
                .params
                .iter()
                .map(|parameter| parameter.core_ty.clone())
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let Some(result) = function.core_return_type.clone() else {
                continue;
            };
            declarations
                .entry((core.module.clone(), function.name.clone(), function.arity))
                .or_default()
                .push((parameters, result));
        }
    }

    let mut groups = HashMap::new();
    for ((module, name, arity), mut signatures) in declarations {
        signatures.sort_by_key(|signature| parameter_contract(&signature.0));
        if signatures.len() < 2 {
            continue;
        }
        if signatures.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            // Exact duplicates are not overloads. Leave their source identities
            // intact so application admission reports its established diagnostic.
            continue;
        }
        let candidates = signatures
            .into_iter()
            .enumerate()
            .map(|(index, (parameters, result))| OverloadCandidate {
                module: module.clone(),
                source_name: name.clone(),
                arity,
                internal_name: format!("{name}__terlan_overload_{index}"),
                parameters,
                result,
            })
            .collect();
        groups.insert((module, name, arity), candidates);
    }
    Ok(groups)
}

/// Renames declarations by matching their complete parameter contracts.
fn rename_overload_declarations(
    cores: &mut [CoreModule],
    groups: &HashMap<OverloadKey, Vec<OverloadCandidate>>,
) -> Result<(), String> {
    for core in cores {
        let occupied = core
            .functions
            .iter()
            .map(|function| (function.name.clone(), function.arity))
            .collect::<HashSet<_>>();
        for function in &mut core.functions {
            let Some(candidates) =
                groups.get(&(core.module.clone(), function.name.clone(), function.arity))
            else {
                continue;
            };
            let parameters = function
                .params
                .iter()
                .map(|parameter| parameter.core_ty.clone())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.overload_type]: `{}` has an untyped overload parameter",
                        function.name
                    )
                })?;
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.parameters == parameters)
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.overload_identity]: `{}` has no retained typed overload identity",
                        function.name
                    )
                })?;
            if occupied.contains(&(candidate.internal_name.clone(), candidate.arity)) {
                return Err(format!(
                    "error[native_ir.overload_name_collision]: generated identity `{}` collides with a source function",
                    candidate.internal_name
                ));
            }
            function.name.clone_from(&candidate.internal_name);
        }
    }
    Ok(())
}

/// Collects return types after overload declarations have unique identities.
fn collect_return_types(cores: &[CoreModule]) -> HashMap<OverloadKey, CoreType> {
    cores
        .iter()
        .flat_map(|core| {
            core.functions.iter().filter_map(move |function| {
                function.core_return_type.clone().map(|result| {
                    (
                        (core.module.clone(), function.name.clone(), function.arity),
                        result,
                    )
                })
            })
        })
        .collect()
}

/// Rewrites nested calls and returns the best retained expression type.
fn rewrite_expr(
    expr: &mut CoreExpr,
    current_module: &str,
    environment: &mut HashMap<String, CoreType>,
    groups: &HashMap<OverloadKey, Vec<OverloadCandidate>>,
    returns: &HashMap<OverloadKey, CoreType>,
    aliases: &AliasBodies,
) -> Result<Option<CoreType>, String> {
    let inferred = match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::Binary),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(value) => Some(CoreType::AtomLiteral(value.clone())),
        CoreExpr::Var(name) if matches!(name.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Var(name) => environment.get(name).cloned(),
        CoreExpr::Cast { expr, target_type } => {
            rewrite_expr(expr, current_module, environment, groups, returns, aliases)?;
            Some(target_type.clone())
        }
        CoreExpr::List(items) => {
            let item_types =
                rewrite_items(items, current_module, environment, groups, returns, aliases)?;
            Some(CoreType::List(Box::new(common_type(&item_types))))
        }
        CoreExpr::ListCons { head, tail } => {
            let head = rewrite_expr(head, current_module, environment, groups, returns, aliases)?;
            let tail = rewrite_expr(tail, current_module, environment, groups, returns, aliases)?;
            match tail {
                Some(CoreType::List(item)) => Some(CoreType::List(item)),
                _ => head.map(|item| CoreType::List(Box::new(item))),
            }
        }
        CoreExpr::Tuple(items) => {
            let item_types =
                rewrite_items(items, current_module, environment, groups, returns, aliases)?;
            Some(CoreType::Tuple(
                item_types
                    .into_iter()
                    .map(|item| CoreTupleTypeElem::Type(item.unwrap_or(CoreType::Dynamic)))
                    .collect(),
            ))
        }
        CoreExpr::FixedArray(items) => {
            let item_types =
                rewrite_items(items, current_module, environment, groups, returns, aliases)?;
            Some(CoreType::Apply {
                constructor: "FixedArray".to_string(),
                args: vec![common_type(&item_types)],
            })
        }
        CoreExpr::Call { function, args } => {
            let argument_types =
                rewrite_items(args, current_module, environment, groups, returns, aliases)?;
            if let Some(candidates) =
                local_overload_candidates(current_module, function, args.len(), groups)
            {
                let selected = select_candidate(candidates, &argument_types, function, aliases)?;
                function.clone_from(&selected.internal_name);
                Some(selected.result.clone())
            } else {
                lookup_call_return(current_module, function, args.len(), returns)
            }
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            let argument_types =
                rewrite_items(args, current_module, environment, groups, returns, aliases)?;
            if let Some(candidates) = groups.get(&(module.clone(), function.clone(), args.len())) {
                let selected = select_candidate(candidates, &argument_types, function, aliases)?;
                function.clone_from(&selected.internal_name);
                Some(selected.result.clone())
            } else {
                returns
                    .get(&(module.clone(), function.clone(), args.len()))
                    .cloned()
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut nested = environment.clone();
            for binding in bindings {
                let value_type = rewrite_expr(
                    &mut binding.value,
                    current_module,
                    &mut nested,
                    groups,
                    returns,
                    aliases,
                )?;
                if let Some(value_type) = value_type {
                    bind_pattern_type(&binding.pattern, &value_type, &mut nested);
                }
            }
            rewrite_expr(body, current_module, &mut nested, groups, returns, aliases)?
        }
        CoreExpr::UnaryOp { operator, operand } => {
            let operand = rewrite_expr(
                operand,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?;
            if operator == "not" {
                Some(CoreType::Bool)
            } else {
                operand
            }
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            let left = rewrite_expr(left, current_module, environment, groups, returns, aliases)?;
            let right = rewrite_expr(right, current_module, environment, groups, returns, aliases)?;
            if matches!(
                operator.as_str(),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "or"
            ) {
                Some(CoreType::Bool)
            } else {
                left.or(right)
            }
        }
        CoreExpr::If { clauses } => {
            let mut branch_types = Vec::new();
            for clause in clauses {
                rewrite_expr(
                    &mut clause.condition,
                    current_module,
                    environment,
                    groups,
                    returns,
                    aliases,
                )?;
                branch_types.push(rewrite_expr(
                    &mut clause.body,
                    current_module,
                    environment,
                    groups,
                    returns,
                    aliases,
                )?);
            }
            Some(common_type(&branch_types))
        }
        CoreExpr::Case { scrutinee, clauses } => {
            let scrutinee_type = rewrite_expr(
                scrutinee,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?;
            let mut branch_types = Vec::new();
            for clause in clauses {
                let mut nested = environment.clone();
                if let Some(scrutinee_type) = &scrutinee_type {
                    bind_pattern_type(&clause.pattern, scrutinee_type, &mut nested);
                }
                if let Some(guard) = &mut clause.guard {
                    rewrite_expr(guard, current_module, &mut nested, groups, returns, aliases)?;
                }
                branch_types.push(rewrite_expr(
                    &mut clause.body,
                    current_module,
                    &mut nested,
                    groups,
                    returns,
                    aliases,
                )?);
            }
            Some(common_type(&branch_types))
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            let mut branch_types = vec![rewrite_expr(
                body,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?];
            for clause in of_clauses.iter_mut().chain(catch_clauses.iter_mut()) {
                if let Some(guard) = &mut clause.guard {
                    rewrite_expr(guard, current_module, environment, groups, returns, aliases)?;
                }
                branch_types.push(rewrite_expr(
                    &mut clause.body,
                    current_module,
                    environment,
                    groups,
                    returns,
                    aliases,
                )?);
            }
            if let Some(after) = after_clause {
                rewrite_expr(
                    &mut after.trigger,
                    current_module,
                    environment,
                    groups,
                    returns,
                    aliases,
                )?;
                rewrite_expr(
                    &mut after.body,
                    current_module,
                    environment,
                    groups,
                    returns,
                    aliases,
                )?;
            }
            Some(common_type(&branch_types))
        }
        CoreExpr::Index { base, index } => {
            let base = rewrite_expr(base, current_module, environment, groups, returns, aliases)?;
            rewrite_expr(index, current_module, environment, groups, returns, aliases)?;
            match base {
                Some(CoreType::List(item)) => Some(*item),
                _ => None,
            }
        }
        CoreExpr::ConstructorCall {
            constructor, args, ..
        } => {
            rewrite_items(args, current_module, environment, groups, returns, aliases)?;
            Some(CoreType::Named(constructor.clone()))
        }
        CoreExpr::RecordConstruct { name, fields }
        | CoreExpr::TemplateInstantiate { name, fields } => {
            rewrite_fields(
                fields,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?;
            Some(CoreType::Named(name.clone()))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            let base = rewrite_expr(base, current_module, environment, groups, returns, aliases)?;
            rewrite_fields(
                fields,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?;
            base
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            rewrite_expr(base, current_module, environment, groups, returns, aliases)?;
            None
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                rewrite_expr(
                    &mut field.value,
                    current_module,
                    environment,
                    groups,
                    returns,
                    aliases,
                )?;
            }
            Some(CoreType::Map(Vec::new()))
        }
        CoreExpr::Intrinsic(call) => {
            rewrite_items(
                &mut call.args,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?;
            None
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            rewrite_expr(
                receiver,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?;
            rewrite_items(args, current_module, environment, groups, returns, aliases)?;
            None
        }
        CoreExpr::FunctionCall { callee, args } => {
            rewrite_expr(
                callee,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?;
            rewrite_items(args, current_module, environment, groups, returns, aliases)?;
            None
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            for generator in generators {
                rewrite_expr(
                    &mut generator.source,
                    current_module,
                    environment,
                    groups,
                    returns,
                    aliases,
                )?;
            }
            rewrite_items(
                guards,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?;
            rewrite_expr(expr, current_module, environment, groups, returns, aliases)?
                .map(|item| CoreType::List(Box::new(item)))
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            rewrite_items(args, current_module, environment, groups, returns, aliases)?;
            rewrite_expr(
                record,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?
        }
        CoreExpr::SqlQuery {
            parameters,
            result_core_type,
            ..
        } => {
            rewrite_items(
                parameters,
                current_module,
                environment,
                groups,
                returns,
                aliases,
            )?;
            Some(result_core_type.clone())
        }
        CoreExpr::Lam { body, .. } => {
            rewrite_expr(body, current_module, environment, groups, returns, aliases)?;
            None
        }
        CoreExpr::RemoteFunRef { .. } => None,
    };
    Ok(inferred)
}

/// Rewrites a homogeneous expression list and returns parallel inferred types.
fn rewrite_items(
    items: &mut [CoreExpr],
    current_module: &str,
    environment: &mut HashMap<String, CoreType>,
    groups: &HashMap<OverloadKey, Vec<OverloadCandidate>>,
    returns: &HashMap<OverloadKey, CoreType>,
    aliases: &AliasBodies,
) -> Result<Vec<Option<CoreType>>, String> {
    items
        .iter_mut()
        .map(|item| rewrite_expr(item, current_module, environment, groups, returns, aliases))
        .collect()
}

/// Rewrites values stored in record-shaped expression fields.
fn rewrite_fields(
    fields: &mut [crate::terlan_typeck::CoreRecordExprField],
    current_module: &str,
    environment: &mut HashMap<String, CoreType>,
    groups: &HashMap<OverloadKey, Vec<OverloadCandidate>>,
    returns: &HashMap<OverloadKey, CoreType>,
    aliases: &AliasBodies,
) -> Result<(), String> {
    for field in fields {
        rewrite_expr(
            &mut field.value,
            current_module,
            environment,
            groups,
            returns,
            aliases,
        )?;
    }
    Ok(())
}

/// Finds a local or already-qualified overload group.
fn local_overload_candidates<'a>(
    current_module: &str,
    function: &str,
    arity: usize,
    groups: &'a HashMap<OverloadKey, Vec<OverloadCandidate>>,
) -> Option<&'a [OverloadCandidate]> {
    if let Some(candidates) = groups.get(&(current_module.to_string(), function.to_string(), arity))
    {
        return Some(candidates);
    }
    groups.values().find_map(|candidates| {
        let candidate = candidates.first()?;
        (candidate.arity == arity
            && format!("{}.{}", candidate.module, candidate.source_name) == function)
            .then_some(candidates.as_slice())
    })
}

/// Selects the unique most-specific candidate accepted by inferred arguments.
fn select_candidate<'a>(
    candidates: &'a [OverloadCandidate],
    arguments: &[Option<CoreType>],
    source_name: &str,
    aliases: &AliasBodies,
) -> Result<&'a OverloadCandidate, String> {
    let mut matches = candidates
        .iter()
        .filter_map(|candidate| {
            let mut score = 0usize;
            for (expected, actual) in candidate.parameters.iter().zip(arguments) {
                let Some(actual) = actual else {
                    continue;
                };
                score += type_match_score(expected, actual, aliases)?;
            }
            Some((score, candidate))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    let Some((best_score, best)) = matches.first().copied() else {
        return Err(format!(
            "error[native_ir.overload_no_match]: `{source_name}/{}` has no overload matching `{}`",
            arguments.len(),
            argument_contract(arguments)
        ));
    };
    if matches.get(1).is_some_and(|next| next.0 == best_score) {
        return Err(format!(
            "error[native_ir.overload_ambiguous]: `{source_name}/{}` remains ambiguous for `{}`",
            arguments.len(),
            argument_contract(arguments)
        ));
    }
    Ok(best)
}

/// Scores structural compatibility, preferring exact nested type matches.
fn type_match_score(
    expected: &CoreType,
    actual: &CoreType,
    aliases: &AliasBodies,
) -> Option<usize> {
    type_match_score_at(expected, actual, aliases, 0)
}

/// Scores structural compatibility while resolving bounded transparent aliases.
fn type_match_score_at(
    expected: &CoreType,
    actual: &CoreType,
    aliases: &AliasBodies,
    depth: usize,
) -> Option<usize> {
    if expected == actual {
        return Some(8);
    }
    if depth < 16 {
        if let CoreType::Named(name) = expected {
            if let Some(body) = aliases
                .get(name)
                .or_else(|| aliases.get(name.rsplit('.').next().unwrap_or(name)))
            {
                return type_match_score_at(body, actual, aliases, depth + 1)
                    .map(|score| score.saturating_sub(1));
            }
        }
        if let CoreType::Named(name) = actual {
            if let Some(body) = aliases
                .get(name)
                .or_else(|| aliases.get(name.rsplit('.').next().unwrap_or(name)))
            {
                return type_match_score_at(expected, body, aliases, depth + 1)
                    .map(|score| score.saturating_sub(1));
            }
        }
    }
    match (expected, actual) {
        (CoreType::Dynamic | CoreType::Term, _) | (_, CoreType::Dynamic) => Some(1),
        (CoreType::Number, CoreType::Int | CoreType::Float | CoreType::Number) => Some(2),
        (CoreType::Atom, CoreType::AtomLiteral(_)) => Some(4),
        (CoreType::List(expected), CoreType::List(actual)) => {
            type_match_score_at(expected, actual, aliases, depth).map(|score| score + 4)
        }
        (
            CoreType::Apply {
                constructor: expected_constructor,
                args: expected_args,
            },
            CoreType::Apply {
                constructor: actual_constructor,
                args: actual_args,
            },
        ) if expected_constructor == actual_constructor
            && expected_args.len() == actual_args.len() =>
        {
            let mut score = 4;
            for (expected, actual) in expected_args.iter().zip(actual_args) {
                score += type_match_score_at(expected, actual, aliases, depth)?;
            }
            Some(score)
        }
        _ => None,
    }
}

/// Looks up a non-overloaded local or qualified call result.
fn lookup_call_return(
    current_module: &str,
    function: &str,
    arity: usize,
    returns: &HashMap<OverloadKey, CoreType>,
) -> Option<CoreType> {
    returns
        .get(&(current_module.to_string(), function.to_string(), arity))
        .cloned()
        .or_else(|| {
            returns
                .iter()
                .find_map(|((module, name, candidate_arity), result)| {
                    (*candidate_arity == arity && format!("{module}.{name}") == function)
                        .then(|| result.clone())
                })
        })
}

/// Binds variables introduced by a pattern to the retained structural type.
fn bind_pattern_type(
    pattern: &CorePattern,
    ty: &CoreType,
    environment: &mut HashMap<String, CoreType>,
) {
    match pattern {
        CorePattern::Var(name) => {
            environment.insert(name.clone(), ty.clone());
        }
        CorePattern::Alias { alias, pattern } => {
            environment.insert(alias.clone(), ty.clone());
            bind_pattern_type(pattern, ty, environment);
        }
        CorePattern::Tuple(patterns) => {
            if let CoreType::Tuple(elements) = ty {
                for (pattern, element) in patterns.iter().zip(elements) {
                    let element = match element {
                        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
                    };
                    bind_pattern_type(pattern, element, environment);
                }
            }
        }
        CorePattern::List(patterns) => {
            if let CoreType::List(item) = ty {
                for pattern in patterns {
                    bind_pattern_type(pattern, item, environment);
                }
            }
        }
        CorePattern::ListCons { head, tail } => {
            if let CoreType::List(item) = ty {
                bind_pattern_type(head, item, environment);
                bind_pattern_type(tail, ty, environment);
            }
        }
        _ => {}
    }
}

/// Collapses equal inferred branches and otherwise returns `Dynamic`.
fn common_type(types: &[Option<CoreType>]) -> CoreType {
    let mut known = types.iter().flatten();
    let Some(first) = known.next() else {
        return CoreType::Dynamic;
    };
    if known.all(|candidate| candidate == first) {
        first.clone()
    } else {
        CoreType::Dynamic
    }
}

/// Renders one deterministic parameter-vector contract for sorting/errors.
fn parameter_contract(parameters: &[CoreType]) -> String {
    parameters
        .iter()
        .map(|parameter| core_type_contract_text(Some(parameter)))
        .collect::<Vec<_>>()
        .join(",")
}

/// Renders inferred call arguments while preserving unknown positions.
fn argument_contract(arguments: &[Option<CoreType>]) -> String {
    arguments
        .iter()
        .map(|argument| core_type_contract_text(argument.as_ref()))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
#[path = "overloads_test.rs"]
mod tests;
