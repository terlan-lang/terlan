//! Physical lowering of atom aliases and checked forwarding constructor patterns.

use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreExpr, CoreImportKind, CoreModule, CorePattern, CoreType, CoreVisibility,
};

#[cfg(test)]
#[path = "atom_alias_values_test.rs"]
#[cfg(test)]
mod atom_alias_values_test;

#[derive(Clone)]
struct AliasValue {
    atom: String,
    tuple_arity: Option<usize>,
}

pub(super) fn lower_atom_alias_values(cores: &mut [CoreModule]) {
    let providers = cores
        .iter()
        .map(|core| {
            let values = core
                .types
                .iter()
                .filter_map(|declaration| match declaration.core_body.as_ref() {
                    Some(CoreType::AtomLiteral(value)) => Some((
                        declaration.name.clone(),
                        AliasValue {
                            atom: value.clone(),
                            tuple_arity: None,
                        },
                    )),
                    None if declaration.visibility != CoreVisibility::Opaque
                        && declaration.params.is_empty() =>
                    {
                        Some((
                            declaration.name.clone(),
                            AliasValue {
                                atom: declaration.name.to_lowercase(),
                                tuple_arity: None,
                            },
                        ))
                    }
                    _ => None,
                })
                .collect::<HashMap<_, _>>();
            (core.module.clone(), values)
        })
        .collect::<HashMap<_, _>>();

    // Preserve actual source bodies, including tuple aliases whose type name
    // differs from the constructor. Never derive a tag from an import spelling.
    let mut qualified = providers
        .iter()
        .flat_map(|(module, values)| {
            values
                .iter()
                .map(move |(name, value)| (format!("{module}.{name}"), value.clone()))
        })
        .collect::<HashMap<_, _>>();
    for core in cores.iter() {
        for constructor in &core.constructors {
            if constructor.vararg.is_some() {
                continue;
            }
            let parameter_names = constructor
                .params
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>();
            let source_body = constructor.physical_body.as_ref().or_else(|| {
                let implementation = constructor.implementation.as_ref()?;
                let function = core
                    .functions
                    .iter()
                    .find(|function| function.name == implementation.function)?;
                let [clause] = function.clauses.as_slice() else {
                    return None;
                };
                clause
                    .guard
                    .is_none()
                    .then_some(clause.body.core_expr.as_ref()?)
            });
            let value = source_body.and_then(|body| {
                forwarding_alias_value(body, &parameter_names, &core.module, &providers)
            });
            if let Some(value) = value {
                qualified.insert(format!("{}.{}", core.module, constructor.name), value);
            }
        }
    }

    for core in cores {
        let mut visible = providers.get(&core.module).cloned().unwrap_or_default();
        for constructor in &core.constructors {
            if let Some(value) = qualified.get(&format!("{}.{}", core.module, constructor.name)) {
                visible.insert(constructor.name.clone(), value.clone());
            }
        }
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
                let prefix = format!("{}.", import.module);
                for (identity, value) in &qualified {
                    if let Some(name) = identity
                        .strip_prefix(&prefix)
                        .filter(|name| !name.contains('.'))
                    {
                        visible
                            .entry(name.to_string())
                            .or_insert_with(|| value.clone());
                    }
                }
            }
        }
        if visible.is_empty() && qualified.is_empty() {
            continue;
        }
        for function in &mut core.functions {
            for clause in &mut function.clauses {
                clause
                    .core_patterns
                    .iter_mut()
                    .flatten()
                    .for_each(|pattern| rewrite_pattern(pattern, &visible, &qualified));
                if let Some(guard) = clause
                    .guard
                    .as_mut()
                    .and_then(|guard| guard.core_expr.as_mut())
                {
                    rewrite(guard, &visible, &qualified);
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    rewrite(body, &visible, &qualified);
                }
            }
        }
    }
}

fn forwarding_alias_value(
    body: &CoreExpr,
    parameters: &[&str],
    module: &str,
    providers: &HashMap<String, HashMap<String, AliasValue>>,
) -> Option<AliasValue> {
    if let CoreExpr::Tuple(items) = body {
        if items.len() != parameters.len() + 1
            || matches!(items.first(), Some(CoreExpr::Var(name))
                if parameters.contains(&name.as_str()))
            || !items
                .iter()
                .skip(1)
                .zip(parameters)
                .all(|(item, parameter)| matches!(item, CoreExpr::Var(name) if name == parameter))
        {
            return None;
        }
        Some(AliasValue {
            atom: atom_value(items.first()?, module, providers)?,
            tuple_arity: Some(parameters.len()),
        })
    } else if parameters.is_empty() {
        Some(AliasValue {
            atom: atom_value(body, module, providers)?,
            tuple_arity: None,
        })
    } else {
        None
    }
}

fn atom_value(
    expr: &CoreExpr,
    module: &str,
    providers: &HashMap<String, HashMap<String, AliasValue>>,
) -> Option<String> {
    match expr {
        CoreExpr::Atom(atom) => Some(atom.clone()),
        CoreExpr::Var(name) => providers
            .get(module)
            .and_then(|values| values.get(name))
            .or_else(|| {
                let (provider, alias) = name.rsplit_once('.')?;
                providers.get(provider)?.get(alias)
            })
            .map(|value| value.atom.clone()),
        _ => None,
    }
}

fn rewrite(
    expr: &mut CoreExpr,
    aliases: &HashMap<String, AliasValue>,
    qualified: &HashMap<String, AliasValue>,
) {
    if let CoreExpr::Var(name) = expr {
        if let Some(value) = aliases.get(name) {
            if value.tuple_arity.is_some() {
                return;
            }
            let atom = if name.rsplit('.').next() == Some("Unit") && value.atom == "unit" {
                // NativeIR carries Unit as its own zero-width value kind. Keep
                // the sentinel spelling recognized by native lowering instead
                // of erasing it into a broad runtime atom.
                "Unit".to_string()
            } else {
                value.atom.clone()
            };
            *expr = CoreExpr::Atom(atom);
        }
        return;
    }
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::Array(items) => {
            items
                .iter_mut()
                .for_each(|item| rewrite(item, aliases, qualified));
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
            rewrite(head, aliases, qualified);
            rewrite(tail, aliases, qualified);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            rewrite(expr, aliases, qualified);
            for generator in generators.iter_mut() {
                rewrite_pattern(&mut generator.pattern, aliases, qualified);
            }
            generators
                .iter_mut()
                .for_each(|generator| rewrite(&mut generator.source, aliases, qualified));
            guards
                .iter_mut()
                .for_each(|guard| rewrite(guard, aliases, qualified));
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings.iter_mut() {
                rewrite_pattern(&mut binding.pattern, aliases, qualified);
            }
            bindings
                .iter_mut()
                .for_each(|binding| rewrite(&mut binding.value, aliases, qualified));
            rewrite(body, aliases, qualified);
        }
        CoreExpr::Map(fields) => fields
            .iter_mut()
            .for_each(|field| rewrite(&mut field.value, aliases, qualified)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter_mut()
                .for_each(|field| rewrite(&mut field.value, aliases, qualified))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            rewrite(base, aliases, qualified);
            fields
                .iter_mut()
                .for_each(|field| rewrite(&mut field.value, aliases, qualified));
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => rewrite(base, aliases, qualified),
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter_mut()
                .for_each(|arg| rewrite(arg, aliases, qualified));
            rewrite(record, aliases, qualified);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => {
            args.iter_mut()
                .for_each(|arg| rewrite(arg, aliases, qualified));
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            rewrite(receiver, aliases, qualified);
            args.iter_mut()
                .for_each(|arg| rewrite(arg, aliases, qualified));
        }
        CoreExpr::FunctionCall { callee, args } => {
            rewrite(callee, aliases, qualified);
            args.iter_mut()
                .for_each(|arg| rewrite(arg, aliases, qualified));
        }
        CoreExpr::Intrinsic(call) => {
            call.args
                .iter_mut()
                .for_each(|arg| rewrite(arg, aliases, qualified));
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            parameters
                .iter_mut()
                .for_each(|parameter| rewrite(parameter, aliases, qualified));
        }
        CoreExpr::Case { scrutinee, clauses } => {
            rewrite(scrutinee, aliases, qualified);
            for clause in clauses {
                rewrite_pattern(&mut clause.pattern, aliases, qualified);
                if let Some(guard) = &mut clause.guard {
                    rewrite(guard, aliases, qualified);
                }
                rewrite(&mut clause.body, aliases, qualified);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            rewrite(body, aliases, qualified);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                rewrite_pattern(&mut clause.pattern, aliases, qualified);
                if let Some(guard) = &mut clause.guard {
                    rewrite(guard, aliases, qualified);
                }
                rewrite(&mut clause.body, aliases, qualified);
            }
            if let Some(after) = after_clause {
                rewrite(&mut after.trigger, aliases, qualified);
                rewrite(&mut after.body, aliases, qualified);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                rewrite(&mut clause.condition, aliases, qualified);
                rewrite(&mut clause.body, aliases, qualified);
            }
        }
        CoreExpr::Lam { params, body, .. } => {
            params
                .iter_mut()
                .for_each(|pattern| rewrite_pattern(pattern, aliases, qualified));
            rewrite(body, aliases, qualified);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

fn rewrite_pattern(
    pattern: &mut CorePattern,
    aliases: &HashMap<String, AliasValue>,
    qualified: &HashMap<String, AliasValue>,
) {
    if let CorePattern::Constructor {
        name,
        constructor_identity,
        args,
    } = pattern
    {
        let identity = constructor_identity.as_deref().unwrap_or(name);
        if let Some(value) = qualified.get(identity).or_else(|| aliases.get(identity)) {
            if value.tuple_arity == Some(args.len()) {
                let mut fields = Vec::with_capacity(args.len() + 1);
                fields.push(CorePattern::Atom(value.atom.clone()));
                fields.append(args);
                *pattern = CorePattern::Tuple(fields);
            } else if value.tuple_arity.is_none() && args.is_empty() {
                // Preserve the alias's semantic atom here. Physical matching
                // depends on the scrutinee, not on other unions that happen to
                // contain this alias elsewhere in the application.
                *pattern = CorePattern::Atom(value.atom.clone());
                return;
            }
        }
    }
    match pattern {
        CorePattern::Tuple(items) | CorePattern::List(items) => {
            items
                .iter_mut()
                .for_each(|item| rewrite_pattern(item, aliases, qualified));
        }
        CorePattern::Alias { pattern, .. } => rewrite_pattern(pattern, aliases, qualified),
        CorePattern::ListCons { head, tail } => {
            rewrite_pattern(head, aliases, qualified);
            rewrite_pattern(tail, aliases, qualified);
        }
        CorePattern::Map(fields) => {
            fields
                .iter_mut()
                .for_each(|field| rewrite_pattern(&mut field.value, aliases, qualified));
        }
        CorePattern::Record { fields, .. } => {
            fields
                .iter_mut()
                .for_each(|field| rewrite_pattern(&mut field.value, aliases, qualified));
        }
        CorePattern::Constructor { args, .. } => {
            args.iter_mut()
                .for_each(|arg| rewrite_pattern(arg, aliases, qualified));
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
