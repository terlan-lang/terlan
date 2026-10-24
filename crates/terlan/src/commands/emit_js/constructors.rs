//! Tagged tuple aliases retain their checked representation in JavaScript.

use std::collections::BTreeMap;

use crate::terlan_typeck::{
    visit_core_expr_mut, CoreExpr, CoreModule, CoreTupleTypeElem, CoreType,
};

/// Expands only checked tagged tuple constructors whose complete shape is in
/// CoreIR. Executable constructors and other nominal layouts remain untouched.
pub(super) fn lower_tagged_tuples(modules: &mut [CoreModule]) {
    let mut layouts = BTreeMap::new();
    for module in modules.iter() {
        for declaration in &module.types {
            if module.constructors.iter().any(|constructor| {
                constructor.name == declaration.name && constructor.implementation.is_some()
            }) {
                continue;
            }
            let Some(CoreType::Tuple(elements)) = &declaration.core_body else {
                continue;
            };
            let Some((CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag)), fields)) =
                elements.split_first()
            else {
                continue;
            };
            if fields
                .iter()
                .all(|field| matches!(field, CoreTupleTypeElem::Field { .. }))
            {
                layouts.insert(
                    format!("{}.{}", module.module, declaration.name),
                    (tag.clone(), fields.len()),
                );
            }
        }
    }
    for module in modules {
        for function in &mut module.functions {
            for clause in &mut function.clauses {
                for summary in clause
                    .guard
                    .iter_mut()
                    .chain(std::iter::once(&mut clause.body))
                {
                    let Some(expression) = &mut summary.core_expr else {
                        continue;
                    };
                    visit_core_expr_mut(expression, &mut |expression| {
                        let CoreExpr::ConstructorCall {
                            constructor,
                            constructor_identity,
                            args,
                            ..
                        } = expression
                        else {
                            return;
                        };
                        let local = format!("{}.{}", module.module, constructor);
                        let identity = constructor_identity.as_deref().unwrap_or(&local);
                        let Some((tag, arity)) = layouts.get(identity) else {
                            return;
                        };
                        if args.len() != *arity {
                            return;
                        }
                        let mut values = vec![CoreExpr::Atom(tag.clone())];
                        values.append(args);
                        *expression = CoreExpr::Tuple(values);
                    });
                }
            }
        }
    }
}
