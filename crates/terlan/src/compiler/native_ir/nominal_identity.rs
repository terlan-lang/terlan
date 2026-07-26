//! Application-local qualification of nominal CoreIR identities.
//!
//! The source typechecker deliberately retains short names for declarations
//! owned by the current module. Native image metadata cannot do that: managed
//! semantic IDs live application-wide and must not alias two modules that both
//! declare (for example) `State`.

use std::collections::HashSet;

use crate::terlan_typeck::{CoreExpr, CoreIntrinsicId, CoreModule, CoreType};

/// Qualifies every checked reference to a struct owned by this module.
pub(super) fn qualify_local_nominal_types(core: &mut CoreModule) {
    let local = core
        .types
        .iter()
        .filter_map(|declaration| {
            (matches!(declaration.core_body, Some(CoreType::Struct { .. }))
                || matches!(
                    declaration.visibility,
                    crate::terlan_typeck::CoreVisibility::Opaque
                ))
            .then(|| declaration.name.clone())
        })
        .collect::<HashSet<_>>();
    if local.is_empty() {
        return;
    }
    let module = core.module.clone();
    for declaration in &mut core.types {
        if let Some(body) = &mut declaration.core_body {
            qualify_type(body, &module, &local);
        }
    }
    for declaration in &mut core.constructors {
        declaration
            .params
            .iter_mut()
            .filter_map(|parameter| parameter.core_ty.as_mut())
            .for_each(|ty| qualify_type(ty, &module, &local));
        if let Some(parameter) = &mut declaration.vararg {
            if let Some(ty) = &mut parameter.core_ty {
                qualify_type(ty, &module, &local);
            }
        }
        if let Some(ty) = &mut declaration.core_return_type {
            qualify_type(ty, &module, &local);
        }
    }
    for function in &mut core.functions {
        function
            .params
            .iter_mut()
            .filter_map(|parameter| parameter.core_ty.as_mut())
            .for_each(|ty| qualify_type(ty, &module, &local));
        if let Some(ty) = &mut function.core_return_type {
            qualify_type(ty, &module, &local);
        }
        for clause in &mut function.clauses {
            if let Some(guard) = clause
                .guard
                .as_mut()
                .and_then(|guard| guard.core_expr.as_mut())
            {
                qualify_expr(guard, &module, &local);
            }
            if let Some(body) = &mut clause.body.core_expr {
                qualify_expr(body, &module, &local);
            }
        }
    }
}

fn qualify_name(name: &mut String, module: &str, local: &HashSet<String>) {
    if !name.contains('.') && local.contains(name) {
        *name = format!("{module}.{name}");
    }
}

fn qualify_type(ty: &mut CoreType, module: &str, local: &HashSet<String>) {
    match ty {
        CoreType::Named(name) => qualify_name(name, module, local),
        CoreType::Apply { constructor, args } => {
            qualify_name(constructor, module, local);
            args.iter_mut()
                .for_each(|ty| qualify_type(ty, module, local));
        }
        CoreType::List(item) => qualify_type(item, module, local),
        CoreType::Tuple(items) => items.iter_mut().for_each(|item| match item {
            crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
            | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => {
                qualify_type(ty, module, local);
            }
        }),
        CoreType::Struct { name, fields } => {
            qualify_name(name, module, local);
            fields
                .iter_mut()
                .for_each(|field| qualify_type(&mut field.ty, module, local));
        }
        CoreType::Map(fields) => fields
            .iter_mut()
            .for_each(|field| qualify_type(&mut field.value, module, local)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params
                .iter_mut()
                .for_each(|ty| qualify_type(ty, module, local));
            qualify_type(return_type, module, local);
        }
        CoreType::Union(types) => types
            .iter_mut()
            .for_each(|ty| qualify_type(ty, module, local)),
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

fn qualify_intrinsic_id(id: &mut CoreIntrinsicId, module: &str, local: &HashSet<String>) {
    match id {
        CoreIntrinsicId::VmProcessSendMessage(ty)
        | CoreIntrinsicId::VmProcessReceiveMessage(ty)
        | CoreIntrinsicId::VmProcessSpawn(ty)
        | CoreIntrinsicId::VmProcessLink(ty)
        | CoreIntrinsicId::VmProcessMonitor(ty)
        | CoreIntrinsicId::VmProcessAcquireResource(ty)
        | CoreIntrinsicId::VmProcessCancel(ty) => qualify_type(ty, module, local),
        CoreIntrinsicId::NativeOperation {
            parameter_types, ..
        } => parameter_types
            .iter_mut()
            .for_each(|ty| qualify_type(ty, module, local)),
        CoreIntrinsicId::Primitive(_) | CoreIntrinsicId::Runtime(_) => {}
    }
}

fn qualify_expr(expr: &mut CoreExpr, module: &str, local: &HashSet<String>) {
    match expr {
        CoreExpr::Tuple(values) | CoreExpr::List(values) | CoreExpr::FixedArray(values) => values
            .iter_mut()
            .for_each(|value| qualify_expr(value, module, local)),
        CoreExpr::ListCons { head, tail } => {
            qualify_expr(head, module, local);
            qualify_expr(tail, module, local);
        }
        CoreExpr::Index { base, index } => {
            qualify_expr(base, module, local);
            qualify_expr(index, module, local);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            qualify_expr(expr, module, local);
            generators
                .iter_mut()
                .for_each(|item| qualify_expr(&mut item.source, module, local));
            guards
                .iter_mut()
                .for_each(|guard| qualify_expr(guard, module, local));
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter_mut()
                .for_each(|item| qualify_expr(&mut item.value, module, local));
            qualify_expr(body, module, local);
        }
        CoreExpr::Map(fields) => fields
            .iter_mut()
            .for_each(|field| qualify_expr(&mut field.value, module, local)),
        CoreExpr::RecordConstruct { name, fields }
        | CoreExpr::TemplateInstantiate { name, fields } => {
            qualify_name(name, module, local);
            fields
                .iter_mut()
                .for_each(|field| qualify_expr(&mut field.value, module, local))
        }
        CoreExpr::FieldAccess { base, .. } => qualify_expr(base, module, local),
        CoreExpr::RecordAccess { base, name, .. } => {
            qualify_name(name, module, local);
            qualify_expr(base, module, local)
        }
        CoreExpr::RecordUpdate { base, name, fields } => {
            qualify_name(name, module, local);
            qualify_expr(base, module, local);
            fields
                .iter_mut()
                .for_each(|field| qualify_expr(&mut field.value, module, local));
        }
        CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            args,
            record,
        } => {
            qualify_name(base, module, local);
            if let Some(identity) = base_constructor_identity {
                qualify_name(identity, module, local);
            }
            args.iter_mut()
                .for_each(|argument| qualify_expr(argument, module, local));
            qualify_expr(record, module, local);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => args
            .iter_mut()
            .for_each(|argument| qualify_expr(argument, module, local)),
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            qualify_expr(receiver, module, local);
            args.iter_mut()
                .for_each(|argument| qualify_expr(argument, module, local));
        }
        CoreExpr::FunctionCall { callee, args } => {
            qualify_expr(callee, module, local);
            args.iter_mut()
                .for_each(|argument| qualify_expr(argument, module, local));
        }
        CoreExpr::Cast { expr, target_type } => {
            qualify_expr(expr, module, local);
            qualify_type(target_type, module, local);
        }
        CoreExpr::Intrinsic(call) => {
            qualify_intrinsic_id(&mut call.id, module, local);
            qualify_type(&mut call.return_type, module, local);
            call.args
                .iter_mut()
                .for_each(|argument| qualify_expr(argument, module, local));
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter_mut()
            .for_each(|parameter| qualify_expr(parameter, module, local)),
        CoreExpr::Case { scrutinee, clauses } => {
            qualify_expr(scrutinee, module, local);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    qualify_expr(guard, module, local);
                }
                qualify_expr(&mut clause.body, module, local);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            qualify_expr(body, module, local);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    qualify_expr(guard, module, local);
                }
                qualify_expr(&mut clause.body, module, local);
            }
            if let Some(after) = after_clause {
                qualify_expr(&mut after.trigger, module, local);
                qualify_expr(&mut after.body, module, local);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                qualify_expr(&mut clause.condition, module, local);
                qualify_expr(&mut clause.body, module, local);
            }
        }
        CoreExpr::Lam { body, .. } => qualify_expr(body, module, local),
        CoreExpr::UnaryOp { operand, .. } => qualify_expr(operand, module, local),
        CoreExpr::BinaryOp { left, right, .. } => {
            qualify_expr(left, module, local);
            qualify_expr(right, module, local);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}
