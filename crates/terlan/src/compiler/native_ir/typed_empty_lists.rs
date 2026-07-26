//! Restores checked call-site types for empty collection literals.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreImportKind, CoreModule, CoreType};

type Signature = Vec<CoreType>;

pub(super) fn annotate_empty_list_arguments(cores: &mut [CoreModule]) {
    let providers = cores
        .iter()
        .flat_map(|core| {
            core.functions.iter().filter_map(move |function| {
                function
                    .params
                    .iter()
                    .map(|parameter| parameter.core_ty.clone())
                    .collect::<Option<Vec<_>>>()
                    .map(|params| {
                        (
                            core.module.clone(),
                            function.name.clone(),
                            function.arity,
                            function.public,
                            params,
                        )
                    })
            })
        })
        .collect::<Vec<_>>();

    for core in cores {
        let mut resolver = HashMap::<(String, usize), Option<Signature>>::new();
        for (module, name, arity, public, params) in &providers {
            if module == &core.module {
                resolver.insert((name.clone(), *arity), Some(params.clone()));
                continue;
            }
            if !public
                || !core.imports.iter().any(|import| {
                    import.kind == CoreImportKind::Module
                        && (import.module == *module || import.module == format!("{module}.{name}"))
                })
            {
                continue;
            }
            for identity in [(name.clone(), *arity), (format!("{module}.{name}"), *arity)] {
                match resolver.entry(identity) {
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(Some(params.clone()));
                    }
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        if entry.get().as_ref() != Some(params) {
                            entry.insert(None);
                        }
                    }
                }
            }
        }
        let resolver = resolver
            .into_iter()
            .filter_map(|(identity, signature)| signature.map(|signature| (identity, signature)))
            .collect::<HashMap<_, _>>();
        for function in &mut core.functions {
            for clause in &mut function.clauses {
                if let Some(guard) = clause
                    .guard
                    .as_mut()
                    .and_then(|guard| guard.core_expr.as_mut())
                {
                    annotate(guard, &resolver);
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    annotate(body, &resolver);
                }
            }
        }
    }
}

fn annotate(expr: &mut CoreExpr, resolver: &HashMap<(String, usize), Signature>) {
    match expr {
        CoreExpr::Call { function, args } => {
            args.iter_mut().for_each(|arg| annotate(arg, resolver));
            if let Some(parameters) = resolver.get(&(function.clone(), args.len())) {
                for (argument, expected) in args.iter_mut().zip(parameters) {
                    if matches!(argument, CoreExpr::List(items) if items.is_empty())
                        && is_list_type(expected)
                    {
                        *argument = CoreExpr::Cast {
                            expr: Box::new(CoreExpr::List(Vec::new())),
                            target_type: expected.clone(),
                        };
                    }
                }
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter_mut().for_each(|item| annotate(item, resolver));
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
            annotate(head, resolver);
            annotate(tail, resolver);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            annotate(expr, resolver);
            generators
                .iter_mut()
                .for_each(|generator| annotate(&mut generator.source, resolver));
            guards
                .iter_mut()
                .for_each(|guard| annotate(guard, resolver));
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter_mut()
                .for_each(|binding| annotate(&mut binding.value, resolver));
            annotate(body, resolver);
        }
        CoreExpr::Map(fields) => fields
            .iter_mut()
            .for_each(|field| annotate(&mut field.value, resolver)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter_mut()
                .for_each(|field| annotate(&mut field.value, resolver))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            annotate(base, resolver);
            fields
                .iter_mut()
                .for_each(|field| annotate(&mut field.value, resolver));
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => annotate(base, resolver),
        CoreExpr::Cast { expr, .. } => annotate(expr, resolver),
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter_mut().for_each(|arg| annotate(arg, resolver));
            annotate(record, resolver);
        }
        CoreExpr::RemoteCall { args, .. } | CoreExpr::ConstructorCall { args, .. } => {
            args.iter_mut().for_each(|arg| annotate(arg, resolver));
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            annotate(receiver, resolver);
            args.iter_mut().for_each(|arg| annotate(arg, resolver));
        }
        CoreExpr::FunctionCall { callee, args } => {
            annotate(callee, resolver);
            args.iter_mut().for_each(|arg| annotate(arg, resolver));
        }
        CoreExpr::Intrinsic(call) => {
            call.args.iter_mut().for_each(|arg| annotate(arg, resolver));
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            parameters
                .iter_mut()
                .for_each(|parameter| annotate(parameter, resolver));
        }
        CoreExpr::Case { scrutinee, clauses } => {
            annotate(scrutinee, resolver);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    annotate(guard, resolver);
                }
                annotate(&mut clause.body, resolver);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            annotate(body, resolver);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    annotate(guard, resolver);
                }
                annotate(&mut clause.body, resolver);
            }
            if let Some(after) = after_clause {
                annotate(&mut after.trigger, resolver);
                annotate(&mut after.body, resolver);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                annotate(&mut clause.condition, resolver);
                annotate(&mut clause.body, resolver);
            }
        }
        CoreExpr::Lam { body, .. } => annotate(body, resolver),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

fn is_list_type(ty: &CoreType) -> bool {
    matches!(ty, CoreType::List(_))
        || matches!(
            ty,
            CoreType::Apply { constructor, args }
                if constructor.rsplit('.').next() == Some("List") && args.len() == 1
        )
}
