//! Application-wide qualification of nominal CoreIR identities.
//!
//! The source typechecker deliberately retains short names for declarations
//! owned by the current module. Native image metadata cannot do that: managed
//! semantic IDs live application-wide and must not alias two modules that both
//! declare (for example) `State`.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    CoreExpr, CoreImportKind, CoreIntrinsicId, CoreModule, CoreType, CoreVisibility,
};

struct NominalScope<'a> {
    module: &'a str,
    local: &'a HashSet<String>,
    imported: &'a HashMap<String, String>,
}

fn nominal_declarations(core: &CoreModule, public_only: bool) -> HashSet<String> {
    core.types
        .iter()
        .filter(|declaration| {
            (!public_only || declaration.visibility != CoreVisibility::Private)
                && (matches!(declaration.core_body, Some(CoreType::Struct { .. }))
                    || matches!(declaration.visibility, CoreVisibility::Opaque))
        })
        .map(|declaration| declaration.name.clone())
        .collect()
}

/// Qualifies local and uniquely imported nominal types before application AOT.
///
/// CoreIR intentionally stores only module-level import facts, so qualification
/// admits an imported short name only when exactly one visible provider exports
/// that nominal declaration. Ambiguous names remain unresolved and are rejected
/// by the ordinary native type admission path instead of acquiring an arbitrary
/// application-wide semantic identity.
pub(super) fn qualify_application_nominal_types(cores: &mut [CoreModule]) {
    let providers = cores
        .iter()
        .map(|core| (core.module.clone(), nominal_declarations(core, true)))
        .collect::<Vec<_>>();
    let imported = cores
        .iter()
        .map(|core| {
            let mut candidates = HashMap::<String, HashSet<String>>::new();
            for import in core.imports.iter().filter(|import| {
                matches!(
                    import.kind,
                    CoreImportKind::Module | CoreImportKind::TypeModule
                )
            }) {
                for (provider, declarations) in &providers {
                    for declaration in declarations {
                        if import.module == *provider
                            || import.module == format!("{provider}.{declaration}")
                        {
                            candidates
                                .entry(declaration.clone())
                                .or_default()
                                .insert(format!("{provider}.{declaration}"));
                        }
                    }
                }
            }
            candidates
                .into_iter()
                .filter_map(|(name, candidates)| {
                    (candidates.len() == 1)
                        .then(|| (name, candidates.into_iter().next().expect("one candidate")))
                })
                .collect::<HashMap<_, _>>()
        })
        .collect::<Vec<_>>();
    for (core, imported) in cores.iter_mut().zip(&imported) {
        qualify_nominal_types(core, imported);
    }
}

/// Qualifies every checked reference to a struct owned by this module.
#[cfg(test)]
pub(super) fn qualify_local_nominal_types(core: &mut CoreModule) {
    qualify_nominal_types(core, &HashMap::new());
}

fn qualify_nominal_types(core: &mut CoreModule, imported: &HashMap<String, String>) {
    let local = nominal_declarations(core, false);
    if local.is_empty() && imported.is_empty() {
        return;
    }
    let module = core.module.clone();
    let scope = NominalScope {
        module: &module,
        local: &local,
        imported,
    };
    for declaration in &mut core.types {
        if let Some(body) = &mut declaration.core_body {
            qualify_type(body, &scope);
        }
    }
    for declaration in &mut core.constructors {
        declaration
            .params
            .iter_mut()
            .filter_map(|parameter| parameter.core_ty.as_mut())
            .for_each(|ty| qualify_type(ty, &scope));
        if let Some(parameter) = &mut declaration.vararg {
            if let Some(ty) = &mut parameter.core_ty {
                qualify_type(ty, &scope);
            }
        }
        if let Some(ty) = &mut declaration.core_return_type {
            qualify_type(ty, &scope);
        }
    }
    for function in &mut core.functions {
        function
            .params
            .iter_mut()
            .filter_map(|parameter| parameter.core_ty.as_mut())
            .for_each(|ty| qualify_type(ty, &scope));
        if let Some(ty) = &mut function.core_return_type {
            qualify_type(ty, &scope);
        }
        for clause in &mut function.clauses {
            if let Some(guard) = clause
                .guard
                .as_mut()
                .and_then(|guard| guard.core_expr.as_mut())
            {
                qualify_expr(guard, &scope);
            }
            if let Some(body) = &mut clause.body.core_expr {
                qualify_expr(body, &scope);
            }
        }
    }
}

fn qualify_name(name: &mut String, scope: &NominalScope<'_>) {
    if name.contains('.') {
        return;
    }
    if scope.local.contains(name) {
        *name = format!("{}.{}", scope.module, name);
    } else if let Some(canonical) = scope.imported.get(name) {
        *name = canonical.clone();
    }
}

fn qualify_type(ty: &mut CoreType, scope: &NominalScope<'_>) {
    match ty {
        CoreType::Named(name) => qualify_name(name, scope),
        CoreType::Apply { constructor, args } => {
            qualify_name(constructor, scope);
            args.iter_mut().for_each(|ty| qualify_type(ty, scope));
        }
        CoreType::List(item) => qualify_type(item, scope),
        CoreType::Tuple(items) => items.iter_mut().for_each(|item| match item {
            crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
            | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => {
                qualify_type(ty, scope);
            }
        }),
        CoreType::Struct { name, fields } => {
            qualify_name(name, scope);
            fields
                .iter_mut()
                .for_each(|field| qualify_type(&mut field.ty, scope));
        }
        CoreType::Map(fields) => fields
            .iter_mut()
            .for_each(|field| qualify_type(&mut field.value, scope)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params.iter_mut().for_each(|ty| qualify_type(ty, scope));
            qualify_type(return_type, scope);
        }
        CoreType::Union(types) => types.iter_mut().for_each(|ty| qualify_type(ty, scope)),
        CoreType::Int
        | CoreType::Float
        | CoreType::Number
        | CoreType::String
        | CoreType::Binary
        | CoreType::Atom
        | CoreType::Bool
        | CoreType::Term
        | CoreType::Dynamic
        | CoreType::Never
        | CoreType::AtomLiteral(_) => {}
    }
}

fn qualify_intrinsic_id(id: &mut CoreIntrinsicId, scope: &NominalScope<'_>) {
    match id {
        CoreIntrinsicId::VmProcessSendMessage(ty)
        | CoreIntrinsicId::VmProcessReceiveMessage(ty)
        | CoreIntrinsicId::VmProcessSpawn(ty)
        | CoreIntrinsicId::VmProcessEntry(ty)
        | CoreIntrinsicId::VmProcessCurrent(ty)
        | CoreIntrinsicId::VmProcessLink(ty)
        | CoreIntrinsicId::VmProcessMonitor(ty)
        | CoreIntrinsicId::VmProcessAcquireResource(ty)
        | CoreIntrinsicId::VmProcessCancel(ty)
        | CoreIntrinsicId::MemoryLayoutOf(ty)
        | CoreIntrinsicId::MemoryShallowSize(ty)
        | CoreIntrinsicId::MemoryRetainedSize(ty) => qualify_type(ty, scope),
        CoreIntrinsicId::NativeOperation {
            parameter_types, ..
        } => parameter_types
            .iter_mut()
            .for_each(|ty| qualify_type(ty, scope)),
        CoreIntrinsicId::Primitive(_) | CoreIntrinsicId::Runtime(_) => {}
    }
}

fn qualify_expr(expr: &mut CoreExpr, scope: &NominalScope<'_>) {
    match expr {
        CoreExpr::Tuple(values) | CoreExpr::List(values) | CoreExpr::FixedArray(values) => values
            .iter_mut()
            .for_each(|value| qualify_expr(value, scope)),
        CoreExpr::ListCons { head, tail } => {
            qualify_expr(head, scope);
            qualify_expr(tail, scope);
        }
        CoreExpr::Index { base, index } => {
            qualify_expr(base, scope);
            qualify_expr(index, scope);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            qualify_expr(expr, scope);
            generators
                .iter_mut()
                .for_each(|item| qualify_expr(&mut item.source, scope));
            guards
                .iter_mut()
                .for_each(|guard| qualify_expr(guard, scope));
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter_mut()
                .for_each(|item| qualify_expr(&mut item.value, scope));
            qualify_expr(body, scope);
        }
        CoreExpr::Map(fields) => fields
            .iter_mut()
            .for_each(|field| qualify_expr(&mut field.value, scope)),
        CoreExpr::RecordConstruct { name, fields }
        | CoreExpr::TemplateInstantiate { name, fields } => {
            qualify_name(name, scope);
            fields
                .iter_mut()
                .for_each(|field| qualify_expr(&mut field.value, scope))
        }
        CoreExpr::FieldAccess { base, .. } => qualify_expr(base, scope),
        CoreExpr::RecordAccess { base, name, .. } => {
            qualify_name(name, scope);
            qualify_expr(base, scope)
        }
        CoreExpr::RecordUpdate { base, name, fields } => {
            qualify_name(name, scope);
            qualify_expr(base, scope);
            fields
                .iter_mut()
                .for_each(|field| qualify_expr(&mut field.value, scope));
        }
        CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            args,
            record,
        } => {
            qualify_name(base, scope);
            if let Some(identity) = base_constructor_identity {
                qualify_name(identity, scope);
            }
            args.iter_mut()
                .for_each(|argument| qualify_expr(argument, scope));
            qualify_expr(record, scope);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => args
            .iter_mut()
            .for_each(|argument| qualify_expr(argument, scope)),
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            qualify_expr(receiver, scope);
            args.iter_mut()
                .for_each(|argument| qualify_expr(argument, scope));
        }
        CoreExpr::FunctionCall { callee, args } => {
            qualify_expr(callee, scope);
            args.iter_mut()
                .for_each(|argument| qualify_expr(argument, scope));
        }
        CoreExpr::Cast { expr, target_type } => {
            qualify_expr(expr, scope);
            qualify_type(target_type, scope);
        }
        CoreExpr::Intrinsic(call) => {
            qualify_intrinsic_id(&mut call.id, scope);
            qualify_type(&mut call.return_type, scope);
            call.args
                .iter_mut()
                .for_each(|argument| qualify_expr(argument, scope));
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter_mut()
            .for_each(|parameter| qualify_expr(parameter, scope)),
        CoreExpr::Case { scrutinee, clauses } => {
            qualify_expr(scrutinee, scope);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    qualify_expr(guard, scope);
                }
                qualify_expr(&mut clause.body, scope);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            qualify_expr(body, scope);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    qualify_expr(guard, scope);
                }
                qualify_expr(&mut clause.body, scope);
            }
            if let Some(after) = after_clause {
                qualify_expr(&mut after.trigger, scope);
                qualify_expr(&mut after.body, scope);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                qualify_expr(&mut clause.condition, scope);
                qualify_expr(&mut clause.body, scope);
            }
        }
        CoreExpr::Lam { body, .. } => qualify_expr(body, scope),
        CoreExpr::UnaryOp { operand, .. } => qualify_expr(operand, scope),
        CoreExpr::BinaryOp { left, right, .. } => {
            qualify_expr(left, scope);
            qualify_expr(right, scope);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

#[cfg(test)]
#[path = "nominal_identity_test.rs"]
mod tests;
