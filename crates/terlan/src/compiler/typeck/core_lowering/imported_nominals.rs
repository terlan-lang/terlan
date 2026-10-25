//! Preserve checked imported type identities before dependency discovery.

use super::*;

pub(super) fn qualify(core: &mut CoreModule, resolved: &ResolvedModule) {
    let singletons = singleton_types(resolved);
    let mut names = resolved
        .imported_types
        .iter()
        .filter_map(|(local, imported)| {
            let provider = resolved.interface_map.get(&imported.source_module)?;
            (provider.struct_fields.contains_key(&imported.source_name)
                || provider.opaque_types.contains(&imported.source_name)
                || (local != &imported.source_name
                    && provider.public_types.contains(&imported.source_name)))
            .then(|| {
                (
                    local.clone(),
                    format!("{}.{}", imported.source_module, imported.source_name),
                )
            })
        })
        .collect::<HashMap<_, _>>();
    for interface in resolved
        .interface_map
        .values()
        .chain(std::iter::once(&resolved.interface))
    {
        let Some(name) = interface.module.rsplit('.').next() else {
            continue;
        };
        if interface.public_types.contains(name) || interface.opaque_types.contains(name) {
            names.insert(
                interface.module.clone(),
                format!("{}.{name}", interface.module),
            );
        }
    }
    if names.is_empty() && singletons.is_empty() {
        return;
    }
    for declaration in &mut core.types {
        if let Some(body) = &mut declaration.core_body {
            qualify_type(body, &names, &singletons, &declaration.params);
        }
    }
    for function in &mut core.functions {
        for ty in function
            .params
            .iter_mut()
            .filter_map(|param| param.core_ty.as_mut())
            .chain(function.core_return_type.as_mut())
        {
            qualify_type(ty, &names, &singletons, &function.generic_params);
        }
        for clause in &mut function.clauses {
            for expr in clause
                .guard
                .as_mut()
                .and_then(|guard| guard.core_expr.as_mut())
                .into_iter()
                .chain(clause.body.core_expr.as_mut())
            {
                crate::terlan_typeck::visit_core_expr_mut(expr, &mut |expr| {
                    let qualify = |ty: &mut CoreType| {
                        qualify_type(ty, &names, &singletons, &function.generic_params)
                    };
                    match expr {
                        CoreExpr::Lam {
                            parameter_types, ..
                        } => parameter_types.iter_mut().flatten().for_each(qualify),
                        CoreExpr::Call { type_args, .. }
                        | CoreExpr::RemoteCall { type_args, .. }
                        | CoreExpr::ConstructorCall { type_args, .. }
                        | CoreExpr::ConstructorChain { type_args, .. } => {
                            type_args.iter_mut().for_each(qualify)
                        }
                        CoreExpr::Cast { target_type, .. } => qualify(target_type),
                        CoreExpr::ListComprehension {
                            lift: Some(lift), ..
                        } => {
                            if let Some(qualified) = names.get(lift) {
                                *lift = qualified.clone();
                            }
                        }
                        CoreExpr::Intrinsic(call) => {
                            expand_singletons(
                                &mut call.return_type,
                                &singletons,
                                &function.generic_params,
                            );
                            call.return_type.visit_names_mut(&mut |name| {
                                if name.contains('.') {
                                    if let Some(qualified) = names.get(name) {
                                        *name = qualified.clone();
                                    }
                                }
                            })
                        }
                        _ => {}
                    }
                });
            }
        }
    }
}

fn qualify_type(
    ty: &mut CoreType,
    names: &HashMap<String, String>,
    singletons: &HashMap<String, CoreType>,
    parameters: &[String],
) {
    ty.visit_names_mut(&mut |name| {
        if !parameters
            .iter()
            .any(|parameter| normalize_type_param_name(parameter) == *name)
        {
            if let Some(qualified) = names.get(name) {
                *name = qualified.clone();
            }
        }
    });
    expand_singletons(ty, singletons, parameters);
}

/// Retain the checked physical shape of imported singleton aliases. Their
/// declarations need not be linked when the application only uses their values.
fn singleton_types(resolved: &ResolvedModule) -> HashMap<String, CoreType> {
    let aliases = imported_type_aliases(resolved);
    let mut singletons = HashMap::new();
    for (name, alias) in &aliases {
        if alias.is_opaque || !alias.params.is_empty() {
            continue;
        }
        if let Type::LiteralAtom(atom) = expand_type_aliases(&alias.body, &aliases) {
            let ty = CoreType::AtomLiteral(atom);
            singletons.insert(name.clone(), ty.clone());
            if let Some((module, leaf)) = name.rsplit_once('.') {
                singletons.insert(crate::terlan_hir::public_type_path(module, leaf), ty);
            }
        }
    }
    singletons
}

fn expand_singletons(
    ty: &mut CoreType,
    singletons: &HashMap<String, CoreType>,
    parameters: &[String],
) {
    match ty {
        CoreType::Named(name) => {
            if !parameters
                .iter()
                .any(|parameter| normalize_type_param_name(parameter) == *name)
            {
                if let Some(singleton) = singletons.get(name) {
                    *ty = singleton.clone();
                }
            }
        }
        CoreType::Apply { args, .. } | CoreType::Union(args) => {
            for arg in args {
                expand_singletons(arg, singletons, parameters);
            }
        }
        CoreType::List(item) => expand_singletons(item, singletons, parameters),
        CoreType::Tuple(items) => {
            for item in items {
                let (CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. }) = item;
                expand_singletons(ty, singletons, parameters);
            }
        }
        CoreType::Struct { fields, .. } => {
            for field in fields {
                expand_singletons(&mut field.ty, singletons, parameters);
            }
        }
        CoreType::Map(fields) => {
            for field in fields {
                expand_singletons(&mut field.value, singletons, parameters);
            }
        }
        CoreType::Arrow {
            params,
            return_type,
        } => {
            for param in params {
                expand_singletons(param, singletons, parameters);
            }
            expand_singletons(return_type, singletons, parameters);
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "imported_nominals_test.rs"]
mod tests;
