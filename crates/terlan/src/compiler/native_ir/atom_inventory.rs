//! Compiler-owned inventory of finite atom identities admitted to one image.

use std::collections::BTreeSet;

use crate::terlan_typeck::{CoreExpr, CoreModule, CorePattern, CoreTupleTypeElem, CoreType};

/// Collects every non-scalar atom identity visible in checked application CoreIR.
pub(super) fn application_atom_identities(cores: &[&CoreModule]) -> Vec<String> {
    let mut atoms = BTreeSet::new();
    for core in cores {
        if core
            .imports
            .iter()
            .any(|import| import.module == "std.http.Router")
        {
            atoms.insert("router_execution_failed".to_string());
        }
        if core.imports.iter().any(|import| {
            matches!(
                import.module.as_str(),
                "std.http.Request" | "std.http.Router"
            )
        }) {
            atoms.insert("json.parse".to_string());
        }
        for declaration in &core.types {
            if let Some(body) = &declaration.core_body {
                collect_type(body, &mut atoms);
            }
        }
        for constructor in &core.constructors {
            for parameter in constructor.params.iter().chain(constructor.vararg.iter()) {
                if let Some(ty) = &parameter.core_ty {
                    collect_type(ty, &mut atoms);
                }
            }
            if let Some(ty) = &constructor.core_return_type {
                collect_type(ty, &mut atoms);
            }
        }
        for function in &core.functions {
            for parameter in &function.params {
                if let Some(ty) = &parameter.core_ty {
                    collect_type(ty, &mut atoms);
                }
            }
            if let Some(ty) = &function.core_return_type {
                collect_type(ty, &mut atoms);
            }
            for clause in &function.clauses {
                for pattern in clause.core_patterns.iter().flatten() {
                    collect_pattern(pattern, &mut atoms);
                }
                if let Some(expr) = clause
                    .guard
                    .as_ref()
                    .and_then(|guard| guard.core_expr.as_ref())
                {
                    collect_expr(expr, &mut atoms);
                }
                if let Some(expr) = &clause.body.core_expr {
                    collect_expr(expr, &mut atoms);
                }
            }
        }
    }
    atoms.into_iter().collect()
}

/// Adds one semantic atom while excluding compact scalar singleton values.
fn collect_atom(identity: &str, atoms: &mut BTreeSet<String>) {
    if !matches!(identity, "Unit" | "true" | "false") {
        atoms.insert(identity.to_owned());
    }
}

/// Recursively inventories atom literal identities embedded in one Core type.
pub(super) fn collect_type(ty: &CoreType, atoms: &mut BTreeSet<String>) {
    match ty {
        CoreType::AtomLiteral(identity) => collect_atom(identity, atoms),
        CoreType::Apply { args, .. } | CoreType::Union(args) => {
            args.iter().for_each(|ty| collect_type(ty, atoms));
        }
        CoreType::List(element) => collect_type(element, atoms),
        CoreType::Tuple(elements) => elements.iter().for_each(|element| match element {
            CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => {
                collect_type(ty, atoms);
            }
        }),
        CoreType::Struct { fields, .. } => fields
            .iter()
            .for_each(|field| collect_type(&field.ty, atoms)),
        CoreType::Map(fields) => fields
            .iter()
            .for_each(|field| collect_type(&field.value, atoms)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params.iter().for_each(|ty| collect_type(ty, atoms));
            collect_type(return_type, atoms);
        }
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
        | CoreType::Named(_) => {}
    }
}

/// Recursively inventories atom identities embedded in one checked pattern.
pub(super) fn collect_pattern(pattern: &CorePattern, atoms: &mut BTreeSet<String>) {
    match pattern {
        CorePattern::Atom(identity) => collect_atom(identity, atoms),
        CorePattern::Tuple(elements) | CorePattern::List(elements) => {
            elements
                .iter()
                .for_each(|pattern| collect_pattern(pattern, atoms));
        }
        CorePattern::Alias { pattern, .. } => collect_pattern(pattern, atoms),
        CorePattern::ListCons { head, tail } => {
            collect_pattern(head, atoms);
            collect_pattern(tail, atoms);
        }
        CorePattern::Map(fields) => fields
            .iter()
            .for_each(|field| collect_pattern(&field.value, atoms)),
        CorePattern::Record { fields, .. } => fields
            .iter()
            .for_each(|field| collect_pattern(&field.value, atoms)),
        CorePattern::Constructor { args, .. } => args
            .iter()
            .for_each(|pattern| collect_pattern(pattern, atoms)),
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::String(_)
        | CorePattern::StringPattern(_)
        | CorePattern::BinaryLayout { .. } => {}
    }
}

/// Recursively inventories atom identities embedded in one checked expression.
pub(super) fn collect_expr(expr: &CoreExpr, atoms: &mut BTreeSet<String>) {
    match expr {
        CoreExpr::Atom(identity) => collect_atom(identity, atoms),
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter().for_each(|item| collect_expr(item, atoms));
        }
        CoreExpr::ListCons { head, tail } => {
            collect_expr(head, atoms);
            collect_expr(tail, atoms);
        }
        CoreExpr::Index { base, index } => {
            collect_expr(base, atoms);
            collect_expr(index, atoms);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            collect_expr(expr, atoms);
            for generator in generators {
                collect_pattern(&generator.pattern, atoms);
                collect_expr(&generator.source, atoms);
            }
            guards.iter().for_each(|guard| collect_expr(guard, atoms));
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                collect_pattern(&binding.pattern, atoms);
                collect_expr(&binding.value, atoms);
            }
            collect_expr(body, atoms);
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .for_each(|field| collect_expr(&field.value, atoms)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .for_each(|field| collect_expr(&field.value, atoms))
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            collect_expr(base, atoms);
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            collect_expr(base, atoms);
            fields
                .iter()
                .for_each(|field| collect_expr(&field.value, atoms));
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter().for_each(|arg| collect_expr(arg, atoms));
            collect_expr(record, atoms);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => {
            args.iter().for_each(|arg| collect_expr(arg, atoms));
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            collect_expr(receiver, atoms);
            args.iter().for_each(|arg| collect_expr(arg, atoms));
        }
        CoreExpr::FunctionCall { callee, args } => {
            collect_expr(callee, atoms);
            args.iter().for_each(|arg| collect_expr(arg, atoms));
        }
        CoreExpr::Cast { expr, target_type } => {
            collect_expr(expr, atoms);
            collect_type(target_type, atoms);
        }
        CoreExpr::Intrinsic(intrinsic) => {
            intrinsic
                .args
                .iter()
                .for_each(|arg| collect_expr(arg, atoms));
            collect_type(&intrinsic.return_type, atoms);
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .for_each(|parameter| collect_expr(parameter, atoms)),
        CoreExpr::Case { scrutinee, clauses } => {
            collect_expr(scrutinee, atoms);
            collect_clauses(clauses, atoms);
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            collect_expr(body, atoms);
            collect_clauses(of_clauses, atoms);
            collect_clauses(catch_clauses, atoms);
            if let Some(after) = after_clause {
                collect_expr(&after.trigger, atoms);
                collect_expr(&after.body, atoms);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                collect_expr(&clause.condition, atoms);
                collect_expr(&clause.body, atoms);
            }
        }
        CoreExpr::Lam { params, body } => {
            params
                .iter()
                .for_each(|pattern| collect_pattern(pattern, atoms));
            collect_expr(body, atoms);
        }
        CoreExpr::UnaryOp { operand, .. } => collect_expr(operand, atoms),
        CoreExpr::BinaryOp { left, right, .. } => {
            collect_expr(left, atoms);
            collect_expr(right, atoms);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

/// Inventories atoms in a shared case-like branch sequence.
fn collect_clauses(clauses: &[crate::terlan_typeck::CoreCaseClause], atoms: &mut BTreeSet<String>) {
    for clause in clauses {
        collect_pattern(&clause.pattern, atoms);
        if let Some(guard) = &clause.guard {
            collect_expr(guard, atoms);
        }
        collect_expr(&clause.body, atoms);
    }
}
