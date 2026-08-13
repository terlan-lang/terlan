use std::collections::{HashMap, HashSet};

use super::{core_function_clause_summary, function_value_parameter_names};
use crate::terlan_syntax::{SyntaxDeclarationPayload, SyntaxImplMethodOutput, SyntaxModuleOutput};
use crate::terlan_typeck::{
    core_type_from_text, CoreExpr, CoreExprSummary, CoreFunction, CoreParam,
    ReceiverMethodDispatchSignature, ResolvedModule,
};

type StructuralImplDispatch = HashMap<(String, String, usize), String>;

/// Lowers the single-candidate structural generic trait implementations that
/// can be selected entirely at compile time.
///
/// The typechecker has already rejected calls whose concrete argument does not
/// satisfy the implication. When one local implication-constrained impl owns a
/// trait method, the runtime therefore needs neither a trait dictionary nor a
/// dynamic lookup: the method body becomes a private CoreIR function and calls
/// to the trait surface are rewritten to that function.
pub(crate) fn core_syntax_structural_impl_dispatch(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_prop_order: &HashMap<String, Vec<String>>,
) -> (Vec<CoreFunction>, StructuralImplDispatch) {
    let mut candidates: HashMap<(String, String, usize), Vec<&SyntaxImplMethodOutput>> =
        HashMap::new();
    let mut local_impl_counts = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            generic_params,
            is_negative: false,
            methods,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let is_structural = generic_params
            .iter()
            .any(|parameter| parameter.contains("=>"));
        let trait_name = trait_ref
            .text
            .split_once('[')
            .map(|(name, _)| name)
            .unwrap_or(&trait_ref.text)
            .trim()
            .to_string();
        for method in methods {
            let key = (trait_name.clone(), method.name.clone(), method.params.len());
            *local_impl_counts.entry(key.clone()).or_insert(0usize) += 1;
            if is_structural {
                candidates.entry(key).or_default().push(method);
            }
        }
    }

    let imported_conformance_traits = resolved
        .interface_map
        .values()
        .flat_map(|interface| &interface.trait_conformances)
        .filter(|conformance| !conformance.is_negative)
        .map(|conformance| trait_head(&conformance.trait_ref))
        .collect::<HashSet<_>>();

    let mut functions = Vec::new();
    let mut dispatch = HashMap::new();
    let mut keys = candidates.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        let (trait_name, method_name, arity) = &key;
        let methods = &candidates[&key];
        if methods.len() != 1
            || local_impl_counts.get(&key) != Some(&1)
            || imported_conformance_traits.contains(trait_name)
        {
            continue;
        }
        let method = methods[0];
        let function_name = format!(
            "__terlan_structural_impl_{}_{}_{}",
            trait_name.replace('.', "_"),
            method_name,
            arity
        );
        let function_value_locals = function_value_parameter_names(&method.params);
        let clauses = method
            .clauses
            .iter()
            .map(|clause| {
                core_function_clause_summary(
                    clause,
                    receiver_methods,
                    template_prop_order,
                    &function_value_locals,
                )
            })
            .collect();
        functions.push(CoreFunction {
            name: function_name.clone(),
            arity: *arity,
            public: false,
            generic_params: Vec::new(),
            native_operation: None,
            params: method
                .params
                .iter()
                .map(|param| CoreParam {
                    name: param.name.clone(),
                    ty: param.annotation.text.clone(),
                    core_ty: core_type_from_text(&param.annotation.text),
                })
                .collect(),
            return_type: method.return_type.text.clone(),
            core_return_type: core_type_from_text(&method.return_type.text),
            clauses,
        });
        dispatch.insert(key, function_name);
    }
    (functions, dispatch)
}

fn trait_head(reference: &str) -> String {
    reference
        .split_once('[')
        .map(|(name, _)| name)
        .unwrap_or(reference)
        .trim()
        .to_string()
}

/// Rewrites statically selected structural trait calls to their private CoreIR
/// implementation functions.
pub(crate) fn rewrite_structural_impl_calls(
    functions: &mut [CoreFunction],
    dispatch: &HashMap<(String, String, usize), String>,
) {
    for function in functions {
        for clause in &mut function.clauses {
            if let Some(guard) = &mut clause.guard {
                rewrite_structural_impl_summary(guard, dispatch);
            }
            rewrite_structural_impl_summary(&mut clause.body, dispatch);
        }
    }
}

fn rewrite_structural_impl_summary(
    summary: &mut CoreExprSummary,
    dispatch: &HashMap<(String, String, usize), String>,
) {
    if let Some(expr) = &mut summary.core_expr {
        if rewrite_structural_impl_expr(expr, dispatch) {
            summary.remote = None;
        }
    }
    for child in &mut summary.children {
        rewrite_structural_impl_summary(child, dispatch);
    }
}

fn rewrite_structural_impl_expr(
    expr: &mut CoreExpr,
    dispatch: &HashMap<(String, String, usize), String>,
) -> bool {
    let mut replaced = false;
    match expr {
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            for arg in args.iter_mut() {
                replaced |= rewrite_structural_impl_expr(arg, dispatch);
            }
            let key = (module.clone(), function.clone(), args.len());
            if let Some(local_function) = dispatch.get(&key) {
                let args = std::mem::take(args);
                *expr = CoreExpr::Call {
                    function: local_function.clone(),
                    args,
                };
                return true;
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                replaced |= rewrite_structural_impl_expr(item, dispatch);
            }
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
            replaced |= rewrite_structural_impl_expr(head, dispatch);
            replaced |= rewrite_structural_impl_expr(tail, dispatch);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            replaced |= rewrite_structural_impl_expr(expr, dispatch);
            for generator in generators {
                replaced |= rewrite_structural_impl_expr(&mut generator.source, dispatch);
            }
            for guard in guards {
                replaced |= rewrite_structural_impl_expr(guard, dispatch);
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                replaced |= rewrite_structural_impl_expr(&mut binding.value, dispatch);
            }
            replaced |= rewrite_structural_impl_expr(body, dispatch);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                replaced |= rewrite_structural_impl_expr(&mut field.value, dispatch);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                replaced |= rewrite_structural_impl_expr(&mut field.value, dispatch);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            replaced |= rewrite_structural_impl_expr(base, dispatch);
            for field in fields {
                replaced |= rewrite_structural_impl_expr(&mut field.value, dispatch);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => {
            replaced |= rewrite_structural_impl_expr(base, dispatch);
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                replaced |= rewrite_structural_impl_expr(arg, dispatch);
            }
            replaced |= rewrite_structural_impl_expr(record, dispatch);
        }
        CoreExpr::ConstructorCall { args, .. } | CoreExpr::Call { args, .. } => {
            for arg in args {
                replaced |= rewrite_structural_impl_expr(arg, dispatch);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            replaced |= rewrite_structural_impl_expr(receiver, dispatch);
            for arg in args {
                replaced |= rewrite_structural_impl_expr(arg, dispatch);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            replaced |= rewrite_structural_impl_expr(callee, dispatch);
            for arg in args {
                replaced |= rewrite_structural_impl_expr(arg, dispatch);
            }
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &mut call.args {
                replaced |= rewrite_structural_impl_expr(arg, dispatch);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                replaced |= rewrite_structural_impl_expr(parameter, dispatch);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            replaced |= rewrite_structural_impl_expr(scrutinee, dispatch);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    replaced |= rewrite_structural_impl_expr(guard, dispatch);
                }
                replaced |= rewrite_structural_impl_expr(&mut clause.body, dispatch);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            replaced |= rewrite_structural_impl_expr(body, dispatch);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    replaced |= rewrite_structural_impl_expr(guard, dispatch);
                }
                replaced |= rewrite_structural_impl_expr(&mut clause.body, dispatch);
            }
            if let Some(after) = after_clause {
                replaced |= rewrite_structural_impl_expr(&mut after.trigger, dispatch);
                replaced |= rewrite_structural_impl_expr(&mut after.body, dispatch);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                replaced |= rewrite_structural_impl_expr(&mut clause.condition, dispatch);
                replaced |= rewrite_structural_impl_expr(&mut clause.body, dispatch);
            }
        }
        CoreExpr::Lam { body, .. } => {
            replaced |= rewrite_structural_impl_expr(body, dispatch);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
    replaced
}
