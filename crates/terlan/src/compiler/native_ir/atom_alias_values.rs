//! Physical lowering of singleton atom aliases used as value expressions.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    CoreExpr, CoreImportKind, CoreModule, CorePattern, CoreType, CoreVisibility,
};

#[cfg(test)]
#[path = "atom_alias_values_test.rs"]
mod atom_alias_values_test;

#[derive(Clone)]
struct AliasValue {
    atom: String,
    managed_variant: bool,
}

pub(super) fn lower_atom_alias_values(cores: &mut [CoreModule]) {
    let providers = cores
        .iter()
        .map(|core| {
            let managed_variants = managed_union_variants(core);
            let values = core
                .types
                .iter()
                .filter_map(|declaration| match declaration.core_body.as_ref() {
                    Some(CoreType::AtomLiteral(value)) => Some((
                        declaration.name.clone(),
                        AliasValue {
                            atom: value.clone(),
                            managed_variant: managed_variants.contains(&declaration.name),
                        },
                    )),
                    None if declaration.visibility != CoreVisibility::Opaque
                        && declaration.params.is_empty() =>
                    {
                        Some((
                            declaration.name.clone(),
                            AliasValue {
                                atom: declaration.name.to_lowercase(),
                                managed_variant: managed_variants.contains(&declaration.name),
                            },
                        ))
                    }
                    _ => None,
                })
                .collect::<HashMap<_, _>>();
            (core.module.clone(), values)
        })
        .collect::<HashMap<_, _>>();

    for core in cores {
        let mut visible = providers.get(&core.module).cloned().unwrap_or_default();
        for import in &core.imports {
            if matches!(
                import.kind,
                CoreImportKind::Module | CoreImportKind::TypeModule
            ) {
                if let Some(values) = providers.get(&import.module) {
                    for (name, value) in values {
                        visible.entry(name.clone()).or_insert_with(|| value.clone());
                    }
                }
            }
        }
        if visible.is_empty() {
            continue;
        }
        for function in &mut core.functions {
            for clause in &mut function.clauses {
                clause
                    .core_patterns
                    .iter_mut()
                    .flatten()
                    .for_each(|pattern| rewrite_pattern(pattern, &visible));
                if let Some(guard) = clause
                    .guard
                    .as_mut()
                    .and_then(|guard| guard.core_expr.as_mut())
                {
                    rewrite(guard, &visible);
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    rewrite(body, &visible);
                }
            }
        }
    }
}

fn managed_union_variants(core: &CoreModule) -> HashSet<String> {
    let declarations = core
        .types
        .iter()
        .filter_map(|declaration| {
            declaration
                .core_body
                .as_ref()
                .map(|body| (declaration.name.as_str(), body))
        })
        .collect::<HashMap<_, _>>();
    core.types
        .iter()
        .filter_map(|declaration| match declaration.core_body.as_ref() {
            Some(CoreType::Union(variants))
                if variants
                    .iter()
                    .any(|variant| !is_atom_alias_variant(variant, &declarations)) =>
            {
                Some(variants)
            }
            _ => None,
        })
        .flatten()
        .filter_map(|variant| match variant {
            CoreType::Named(name) => Some(name.rsplit('.').next().unwrap_or(name).to_string()),
            CoreType::Apply { constructor, .. } => Some(
                constructor
                    .rsplit('.')
                    .next()
                    .unwrap_or(constructor)
                    .to_string(),
            ),
            _ => None,
        })
        .collect()
}

fn is_atom_alias_variant(variant: &CoreType, declarations: &HashMap<&str, &CoreType>) -> bool {
    match variant {
        CoreType::AtomLiteral(_) => true,
        CoreType::Named(name)
        | CoreType::Apply {
            constructor: name, ..
        } => declarations
            .get(name.rsplit('.').next().unwrap_or(name))
            .is_some_and(|body| matches!(body, CoreType::AtomLiteral(_))),
        _ => false,
    }
}

fn rewrite(expr: &mut CoreExpr, aliases: &HashMap<String, AliasValue>) {
    if let CoreExpr::Var(name) = expr {
        if let Some(value) = aliases.get(name) {
            *expr = CoreExpr::Atom(value.atom.clone());
        }
        return;
    }
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter_mut().for_each(|item| rewrite(item, aliases));
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => {
            rewrite(head, aliases);
            rewrite(tail, aliases);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            rewrite(expr, aliases);
            generators
                .iter_mut()
                .for_each(|generator| rewrite(&mut generator.source, aliases));
            guards.iter_mut().for_each(|guard| rewrite(guard, aliases));
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter_mut()
                .for_each(|binding| rewrite(&mut binding.value, aliases));
            rewrite(body, aliases);
        }
        CoreExpr::Map(fields) => fields
            .iter_mut()
            .for_each(|field| rewrite(&mut field.value, aliases)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter_mut()
                .for_each(|field| rewrite(&mut field.value, aliases))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            rewrite(base, aliases);
            fields
                .iter_mut()
                .for_each(|field| rewrite(&mut field.value, aliases));
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => rewrite(base, aliases),
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter_mut().for_each(|arg| rewrite(arg, aliases));
            rewrite(record, aliases);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => {
            args.iter_mut().for_each(|arg| rewrite(arg, aliases));
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            rewrite(receiver, aliases);
            args.iter_mut().for_each(|arg| rewrite(arg, aliases));
        }
        CoreExpr::FunctionCall { callee, args } => {
            rewrite(callee, aliases);
            args.iter_mut().for_each(|arg| rewrite(arg, aliases));
        }
        CoreExpr::Intrinsic(call) => {
            call.args.iter_mut().for_each(|arg| rewrite(arg, aliases));
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            parameters
                .iter_mut()
                .for_each(|parameter| rewrite(parameter, aliases));
        }
        CoreExpr::Case { scrutinee, clauses } => {
            rewrite(scrutinee, aliases);
            for clause in clauses {
                rewrite_pattern(&mut clause.pattern, aliases);
                if let Some(guard) = &mut clause.guard {
                    rewrite(guard, aliases);
                }
                rewrite(&mut clause.body, aliases);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            rewrite(body, aliases);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                rewrite_pattern(&mut clause.pattern, aliases);
                if let Some(guard) = &mut clause.guard {
                    rewrite(guard, aliases);
                }
                rewrite(&mut clause.body, aliases);
            }
            if let Some(after) = after_clause {
                rewrite(&mut after.trigger, aliases);
                rewrite(&mut after.body, aliases);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                rewrite(&mut clause.condition, aliases);
                rewrite(&mut clause.body, aliases);
            }
        }
        CoreExpr::Lam { params, body } => {
            params
                .iter_mut()
                .for_each(|pattern| rewrite_pattern(pattern, aliases));
            rewrite(body, aliases);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

fn rewrite_pattern(pattern: &mut CorePattern, aliases: &HashMap<String, AliasValue>) {
    if let CorePattern::Constructor { name, args, .. } = pattern {
        if args.is_empty() {
            if let Some(value) = aliases.get(name.rsplit('.').next().unwrap_or(name)) {
                if value.managed_variant {
                    return;
                }
                *pattern = CorePattern::Atom(value.atom.clone());
                return;
            }
        }
    }
    match pattern {
        CorePattern::Tuple(items) | CorePattern::List(items) => {
            items
                .iter_mut()
                .for_each(|item| rewrite_pattern(item, aliases));
        }
        CorePattern::Alias { pattern, .. } => rewrite_pattern(pattern, aliases),
        CorePattern::ListCons { head, tail } => {
            rewrite_pattern(head, aliases);
            rewrite_pattern(tail, aliases);
        }
        CorePattern::Map(fields) => {
            fields
                .iter_mut()
                .for_each(|field| rewrite_pattern(&mut field.value, aliases));
        }
        CorePattern::Record { fields, .. } => {
            fields
                .iter_mut()
                .for_each(|field| rewrite_pattern(&mut field.value, aliases));
        }
        CorePattern::Constructor { args, .. } => {
            args.iter_mut()
                .for_each(|arg| rewrite_pattern(arg, aliases));
        }
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::String(_)
        | CorePattern::StringPattern(_)
        | CorePattern::Atom(_)
        | CorePattern::BinaryLayout { .. } => {}
    }
}
