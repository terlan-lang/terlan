//! Checked adapters for named callbacks whose work belongs to the VM scheduler.

use super::*;

/// Gives a suspending named reference the same owned-entry lowering as a lambda.
/// Pure references keep their direct callable identity and synchronous fast path.
pub(super) fn reference(
    body: &CoreExpr,
    expected: Option<&CoreType>,
    available: &HashMap<String, usize>,
    suspending: &HashSet<(String, usize)>,
) -> Option<CoreExpr> {
    let CoreType::Arrow { params, .. } = expected? else {
        return None;
    };
    let function = match body {
        CoreExpr::Var(name) if !available.contains_key(name) => name.clone(),
        CoreExpr::RemoteFunRef {
            module,
            function,
            arity,
        } if *arity == params.len() => format!("{module}.{function}"),
        _ => return None,
    };
    if !suspending.contains(&(function.clone(), params.len())) {
        return None;
    }
    let names = (0..params.len())
        .map(|index| format!("$callback_argument_{index}"))
        .collect::<Vec<_>>();
    Some(CoreExpr::Lam {
        params: names.iter().cloned().map(CorePattern::Var).collect(),
        parameter_types: params.iter().cloned().map(Some).collect(),
        body: Box::new(CoreExpr::Call {
            function,
            type_args: Vec::new(),
            args: names.into_iter().map(CoreExpr::Var).collect(),
        }),
    })
}
