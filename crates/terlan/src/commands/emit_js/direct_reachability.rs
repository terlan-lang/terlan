use std::collections::{BTreeMap, BTreeSet};

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreModule};

/// Computes the function set that direct JavaScript emission must include.
///
/// Inputs:
/// - `module`: CoreIR module whose public functions define the emitted module
///   surface.
///
/// Output:
/// - Set of function names reachable from public functions through local CoreIR
///   calls.
///
/// Transformation:
/// - Builds a name index, seeds traversal from public functions, recursively
///   follows `CoreExpr::Call` edges that target functions in the same module,
///   and ignores unused private functions so unsupported dead helpers do not
///   force the direct backend to fall back.
pub(super) fn reachable_direct_function_names(module: &CoreModule) -> BTreeSet<String> {
    let functions_by_name = module
        .functions
        .iter()
        .map(|function| (function.name.as_str(), function))
        .collect::<BTreeMap<_, _>>();
    let mut reachable = BTreeSet::new();
    let mut pending = module
        .functions
        .iter()
        .filter(|function| function.public)
        .map(|function| function.name.clone())
        .collect::<Vec<_>>();

    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        let Some(function) = functions_by_name.get(name.as_str()) else {
            continue;
        };
        for clause in &function.clauses {
            if let Some(expr) = clause.body.core_expr.as_ref() {
                collect_core_expr_local_calls(expr, &functions_by_name, &mut pending);
            }
        }
    }

    reachable
}

/// Collects local function calls contained in a CoreIR expression.
///
/// Inputs:
/// - `expr`: CoreIR expression to inspect.
/// - `functions_by_name`: module-local function name index.
/// - `pending`: traversal stack receiving newly discovered local callees.
///
/// Output:
/// - Mutates `pending` with called local function names.
///
/// Transformation:
/// - Recursively walks expression children and adds local `CoreExpr::Call`
///   targets to the pending reachability stack. Remote calls and constructor
///   calls still have their argument expressions traversed but do not add local
///   function dependencies.
fn collect_core_expr_local_calls<'a>(
    expr: &CoreExpr,
    functions_by_name: &BTreeMap<&'a str, &'a CoreFunction>,
    pending: &mut Vec<String>,
) {
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                collect_core_expr_local_calls(item, functions_by_name, pending);
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            collect_core_expr_local_calls(head, functions_by_name, pending);
            collect_core_expr_local_calls(tail, functions_by_name, pending);
        }
        CoreExpr::ListComprehension {
            expr,
            source,
            guard,
            ..
        } => {
            collect_core_expr_local_calls(expr, functions_by_name, pending);
            collect_core_expr_local_calls(source, functions_by_name, pending);
            if let Some(guard) = guard.as_ref() {
                collect_core_expr_local_calls(guard, functions_by_name, pending);
            }
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                collect_core_expr_local_calls(&field.value, functions_by_name, pending);
            }
        }
        CoreExpr::RecordConstruct { fields, .. }
        | CoreExpr::RecordUpdate { fields, .. }
        | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                collect_core_expr_local_calls(&field.value, functions_by_name, pending);
            }
        }
        CoreExpr::FieldAccess { base: expr, .. }
        | CoreExpr::RecordAccess { base: expr, .. }
        | CoreExpr::Cast { expr, .. } => {
            collect_core_expr_local_calls(expr, functions_by_name, pending)
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
            collect_core_expr_local_calls(record, functions_by_name, pending);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
        }
        CoreExpr::Call { function, args } => {
            if functions_by_name.contains_key(function.as_str()) {
                pending.push(function.clone());
            }
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            collect_core_expr_local_calls(receiver, functions_by_name, pending);
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            collect_core_expr_local_calls(callee, functions_by_name, pending);
            for arg in args {
                collect_core_expr_local_calls(arg, functions_by_name, pending);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            collect_core_expr_local_calls(scrutinee, functions_by_name, pending);
            for clause in clauses {
                if let Some(guard) = clause.guard.as_ref() {
                    collect_core_expr_local_calls(guard, functions_by_name, pending);
                }
                collect_core_expr_local_calls(&clause.body, functions_by_name, pending);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            collect_core_expr_local_calls(body, functions_by_name, pending);
            for clause in of_clauses.iter().chain(catch_clauses.iter()) {
                if let Some(guard) = clause.guard.as_ref() {
                    collect_core_expr_local_calls(guard, functions_by_name, pending);
                }
                collect_core_expr_local_calls(&clause.body, functions_by_name, pending);
            }
            if let Some(after_clause) = after_clause.as_ref() {
                collect_core_expr_local_calls(&after_clause.trigger, functions_by_name, pending);
                collect_core_expr_local_calls(&after_clause.body, functions_by_name, pending);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                collect_core_expr_local_calls(&clause.condition, functions_by_name, pending);
                collect_core_expr_local_calls(&clause.body, functions_by_name, pending);
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                collect_core_expr_local_calls(&binding.value, functions_by_name, pending);
            }
            collect_core_expr_local_calls(body, functions_by_name, pending);
        }
        CoreExpr::Lam { body, .. } | CoreExpr::UnaryOp { operand: body, .. } => {
            collect_core_expr_local_calls(body, functions_by_name, pending);
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            collect_core_expr_local_calls(left, functions_by_name, pending);
            collect_core_expr_local_calls(right, functions_by_name, pending);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::SqlQuery { .. } => {}
    }
}
