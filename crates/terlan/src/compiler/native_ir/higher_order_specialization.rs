//! Bounded specialization of private higher-order application helpers.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreLetBinding, CoreModule, CoreParam, CorePattern, CoreType,
};

use super::LocalFunctionIdentity as FunctionIdentity;

/// Maximum private higher-order calls expanded in one module.
const MAX_HIGHER_ORDER_SPECIALIZATIONS: usize = 128;

/// One private higher-order function used as an AOT specialization template.
#[derive(Clone)]
struct HigherOrderHelper {
    /// Source function identity.
    identity: FunctionIdentity,
    /// Ordered source parameters.
    params: Vec<CoreParam>,
    /// Single direct-binding function body.
    body: CoreExpr,
}

/// Stateful bounded module specialization pass.
struct HigherOrderSpecializer<'a> {
    /// Module name used by generated direct function references.
    module: String,
    /// Every local function available as a named callback value.
    functions: HashMap<FunctionIdentity, CoreFunction>,
    /// Private higher-order specialization templates.
    helpers: HashMap<FunctionIdentity, HigherOrderHelper>,
    /// Active helper stack used to reject recursive expansion.
    active: Vec<FunctionIdentity>,
    /// Number of helper calls expanded in this module.
    expansions: usize,
    /// Monotonic identity for compiler-generated argument temporaries.
    temporary_ordinal: u64,
    /// Application-wide expansion budget shared with other specialization passes.
    budget: &'a mut super::specialization_budget::SpecializationBudget,
}

/// Specializes every reachable call to a private higher-order helper.
///
/// Public higher-order functions retain their owned-closure ABI. Private
/// helpers are removed after every direct call has been expanded.
#[cfg(test)]
pub(super) fn specialize_higher_order_helpers(core: &mut CoreModule) -> Result<(), String> {
    let mut budget = super::specialization_budget::SpecializationBudget::default();
    specialize_higher_order_helpers_with_budget(core, &mut budget)
}

/// Specializes private higher-order helpers under one application-wide budget.
pub(super) fn specialize_higher_order_helpers_with_budget(
    core: &mut CoreModule,
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<(), String> {
    let functions = core
        .functions
        .iter()
        .map(|function| ((function.name.clone(), function.arity), function.clone()))
        .collect::<HashMap<_, _>>();
    let helpers = core
        .functions
        .iter()
        .filter(|function| {
            !function.public
                && !function.name.starts_with("$aot_generic_")
                && !function.name.starts_with("$aot_comprehension_")
                && has_function_parameter(function)
        })
        .map(higher_order_helper)
        .collect::<Result<HashMap<_, _>, _>>()?;
    if helpers.is_empty() {
        return Ok(());
    }
    let helper_identities = helpers.keys().cloned().collect::<HashSet<_>>();
    let mut specializer = HigherOrderSpecializer {
        module: core.module.clone(),
        functions,
        helpers,
        active: Vec::new(),
        expansions: 0,
        temporary_ordinal: 0,
        budget,
    };
    for function in &mut core.functions {
        if helper_identities.contains(&(function.name.clone(), function.arity)) {
            continue;
        }
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                *body = specializer.rewrite(body)?;
            }
        }
    }
    for function in &core.functions {
        if helper_identities.contains(&(function.name.clone(), function.arity)) {
            continue;
        }
        for clause in &function.clauses {
            if clause
                .body
                .core_expr
                .as_ref()
                .is_some_and(|body| mentions_helper(body, &helper_identities))
            {
                return Err(
                    "error[native_ir.higher_order_escape]: a private higher-order helper remains reachable after specialization"
                        .to_string(),
                );
            }
        }
    }
    core.functions
        .retain(|function| !helper_identities.contains(&(function.name.clone(), function.arity)));
    Ok(())
}

impl HigherOrderSpecializer<'_> {
    /// Rewrites one expression and recursively specializes helper calls.
    fn rewrite(&mut self, expr: &CoreExpr) -> Result<CoreExpr, String> {
        match expr {
            CoreExpr::Call { function, args } => {
                let args = self.rewrite_many(args)?;
                let identity = (function.clone(), args.len());
                if let Some(helper) = self.helpers.get(&identity).cloned() {
                    self.inline_helper(helper, args)
                } else {
                    Ok(CoreExpr::Call {
                        function: function.clone(),
                        args,
                    })
                }
            }
            CoreExpr::RemoteCall {
                module,
                function,
                args,
            } => Ok(CoreExpr::RemoteCall {
                module: module.clone(),
                function: function.clone(),
                args: self.rewrite_many(args)?,
            }),
            CoreExpr::ConstructorCall {
                constructor,
                constructor_identity,
                args,
            } => Ok(CoreExpr::ConstructorCall {
                constructor: constructor.clone(),
                constructor_identity: constructor_identity.clone(),
                args: self.rewrite_many(args)?,
            }),
            CoreExpr::Intrinsic(call) => {
                let mut call = call.clone();
                call.args = self.rewrite_many(&call.args)?;
                Ok(CoreExpr::Intrinsic(call))
            }
            CoreExpr::FunctionCall { callee, args } => Ok(CoreExpr::FunctionCall {
                callee: Box::new(self.rewrite(callee)?),
                args: self.rewrite_many(args)?,
            }),
            CoreExpr::Lam { params, body } => Ok(CoreExpr::Lam {
                params: params.clone(),
                body: Box::new(self.rewrite(body)?),
            }),
            CoreExpr::Cast { expr, target_type } => Ok(CoreExpr::Cast {
                expr: Box::new(self.rewrite(expr)?),
                target_type: target_type.clone(),
            }),
            CoreExpr::UnaryOp { operator, operand } => Ok(CoreExpr::UnaryOp {
                operator: operator.clone(),
                operand: Box::new(self.rewrite(operand)?),
            }),
            CoreExpr::BinaryOp {
                operator,
                left,
                right,
            } => Ok(CoreExpr::BinaryOp {
                operator: operator.clone(),
                left: Box::new(self.rewrite(left)?),
                right: Box::new(self.rewrite(right)?),
            }),
            CoreExpr::Let { bindings, body } => Ok(CoreExpr::Let {
                bindings: bindings
                    .iter()
                    .map(|binding| {
                        Ok(CoreLetBinding {
                            pattern: binding.pattern.clone(),
                            value: self.rewrite(&binding.value)?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                body: Box::new(self.rewrite(body)?),
            }),
            CoreExpr::If { clauses } => {
                let mut lowered = Vec::with_capacity(clauses.len());
                for clause in clauses {
                    let mut clause = clause.clone();
                    clause.condition = self.rewrite(&clause.condition)?;
                    clause.body = self.rewrite(&clause.body)?;
                    lowered.push(clause);
                }
                Ok(CoreExpr::If { clauses: lowered })
            }
            CoreExpr::Case { scrutinee, clauses } => {
                let mut lowered = Vec::with_capacity(clauses.len());
                for clause in clauses {
                    let mut clause = clause.clone();
                    if let Some(guard) = &mut clause.guard {
                        *guard = self.rewrite(guard)?;
                    }
                    clause.body = self.rewrite(&clause.body)?;
                    lowered.push(clause);
                }
                Ok(CoreExpr::Case {
                    scrutinee: Box::new(self.rewrite(scrutinee)?),
                    clauses: lowered,
                })
            }
            _ => Ok(expr.clone()),
        }
    }

    /// Rewrites an ordered argument sequence.
    fn rewrite_many(&mut self, expressions: &[CoreExpr]) -> Result<Vec<CoreExpr>, String> {
        expressions
            .iter()
            .map(|expression| self.rewrite(expression))
            .collect()
    }

    /// Expands one helper call under the specialization budget.
    fn inline_helper(
        &mut self,
        helper: HigherOrderHelper,
        args: Vec<CoreExpr>,
    ) -> Result<CoreExpr, String> {
        if self.active.contains(&helper.identity) {
            return Err(format!(
                "error[native_ir.higher_order_recursion]: `{}/{}` recursively requires higher-order specialization",
                helper.identity.0, helper.identity.1
            ));
        }
        self.expansions = self.expansions.saturating_add(1);
        if self.expansions > MAX_HIGHER_ORDER_SPECIALIZATIONS {
            return Err(format!(
                "error[native_ir.specialization_limit]: higher-order specialization exceeds {MAX_HIGHER_ORDER_SPECIALIZATIONS} calls"
            ));
        }
        self.budget.reserve(
            super::specialization_budget::SpecializationKind::HigherOrder,
            &self.module,
            1,
        )?;
        if helper.params.len() != args.len() {
            return Err(format!(
                "error[native_ir.higher_order_arity]: `{}/{}` expected {} arguments but received {}",
                helper.identity.0,
                helper.identity.1,
                helper.params.len(),
                args.len()
            ));
        }
        let specialization = self.temporary_ordinal;
        self.temporary_ordinal = self.temporary_ordinal.saturating_add(1);
        let mut bindings = Vec::with_capacity(args.len() * 2);
        let mut temporaries = Vec::with_capacity(args.len());
        for (index, (parameter, argument)) in helper.params.iter().zip(args).enumerate() {
            let argument = if let Some(arity) = function_parameter_arity(parameter) {
                self.resolve_callable_argument(argument, arity, specialization, index)?
            } else {
                argument
            };
            let temporary = format!("$native_hofn_{specialization}_arg_{index}");
            bindings.push(CoreLetBinding {
                pattern: CorePattern::Var(temporary.clone()),
                value: argument,
            });
            temporaries.push(temporary);
        }
        for (parameter, temporary) in helper.params.iter().zip(temporaries) {
            bindings.push(CoreLetBinding {
                pattern: CorePattern::Var(parameter.name.clone()),
                value: CoreExpr::Var(temporary),
            });
        }
        self.active.push(helper.identity.clone());
        let body = self.rewrite(&helper.body);
        self.active.pop();
        Ok(CoreExpr::Let {
            bindings,
            body: Box::new(body?),
        })
    }

    /// Resolves one callback argument into static callable CoreIR.
    fn resolve_callable_argument(
        &self,
        argument: CoreExpr,
        expected_arity: usize,
        specialization: u64,
        argument_index: usize,
    ) -> Result<CoreExpr, String> {
        match argument {
            CoreExpr::Lam { params, body } if params.len() == expected_arity => {
                Ok(CoreExpr::Lam { params, body })
            }
            CoreExpr::RemoteFunRef {
                module,
                function,
                arity,
            } if arity == expected_arity => Ok(CoreExpr::RemoteFunRef {
                module,
                function,
                arity,
            }),
            CoreExpr::Var(function)
                if self
                    .functions
                    .contains_key(&(function.clone(), expected_arity)) =>
            {
                let params = (0..expected_arity)
                    .map(|index| {
                        format!(
                            "$native_hofn_{specialization}_callback_{argument_index}_{index}"
                        )
                    })
                    .collect::<Vec<_>>();
                Ok(CoreExpr::Lam {
                    params: params
                        .iter()
                        .cloned()
                        .map(CorePattern::Var)
                        .collect(),
                    body: Box::new(CoreExpr::Call {
                        function,
                        args: params.into_iter().map(CoreExpr::Var).collect(),
                    }),
                })
            }
            CoreExpr::Lam { params, .. } => Err(format!(
                "error[native_ir.higher_order_callback_arity]: callback expected {expected_arity} parameters but received {}",
                params.len()
            )),
            CoreExpr::RemoteFunRef { arity, .. } => Err(format!(
                "error[native_ir.higher_order_callback_arity]: function reference expected arity {expected_arity} but declared {arity}"
            )),
            _ => Err(format!(
                "error[native_ir.higher_order_argument]: argument {argument_index} to a function parameter is not statically callable in module `{}`",
                self.module
            )),
        }
    }
}

/// Builds one validated private helper specialization template.
fn higher_order_helper(
    function: &CoreFunction,
) -> Result<(FunctionIdentity, HigherOrderHelper), String> {
    let [clause] = function.clauses.as_slice() else {
        return Err(helper_shape_error(function));
    };
    if clause.guard.is_some()
        || clause.core_patterns.len() != function.params.len()
        || !clause
            .core_patterns
            .iter()
            .zip(&function.params)
            .all(|(pattern, parameter)| {
                matches!(pattern, Some(CorePattern::Var(name)) if name == &parameter.name)
            })
    {
        return Err(helper_shape_error(function));
    }
    let body = clause
        .body
        .core_expr
        .clone()
        .ok_or_else(|| helper_shape_error(function))?;
    let identity = (function.name.clone(), function.arity);
    Ok((
        identity.clone(),
        HigherOrderHelper {
            identity,
            params: function.params.clone(),
            body,
        },
    ))
}

/// Produces the stable unsupported-helper-shape diagnostic.
fn helper_shape_error(function: &CoreFunction) -> String {
    format!(
        "error[native_ir.higher_order_helper_shape]: private helper `{}/{}` must have one unguarded variable-binding clause",
        function.name, function.arity
    )
}

/// Reports whether one function accepts at least one function value.
fn has_function_parameter(function: &CoreFunction) -> bool {
    function.params.iter().any(function_parameter_arity_is_some)
}

/// Adapter used by iterator predicates over function parameters.
fn function_parameter_arity_is_some(parameter: &CoreParam) -> bool {
    function_parameter_arity(parameter).is_some()
}

/// Returns the callback arity carried by one function parameter.
fn function_parameter_arity(parameter: &CoreParam) -> Option<usize> {
    match parameter.core_ty.as_ref() {
        Some(CoreType::Arrow { params, .. }) => Some(params.len()),
        _ => None,
    }
}

/// Reports whether an expression still refers to a removed helper identity.
fn mentions_helper(expr: &CoreExpr, helpers: &HashSet<FunctionIdentity>) -> bool {
    match expr {
        CoreExpr::Call { function, args } => {
            helpers.contains(&(function.clone(), args.len()))
                || args.iter().any(|arg| mentions_helper(arg, helpers))
        }
        CoreExpr::Var(name) => helpers.iter().any(|(helper, _)| helper == name),
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().any(|arg| mentions_helper(arg, helpers))
        }
        CoreExpr::FunctionCall { callee, args } => {
            mentions_helper(callee, helpers) || args.iter().any(|arg| mentions_helper(arg, helpers))
        }
        CoreExpr::Lam { body, .. }
        | CoreExpr::UnaryOp { operand: body, .. }
        | CoreExpr::Cast { expr: body, .. } => mentions_helper(body, helpers),
        CoreExpr::BinaryOp { left, right, .. } => {
            mentions_helper(left, helpers) || mentions_helper(right, helpers)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| mentions_helper(&binding.value, helpers))
                || mentions_helper(body, helpers)
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            mentions_helper(&clause.condition, helpers) || mentions_helper(&clause.body, helpers)
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            mentions_helper(scrutinee, helpers)
                || clauses.iter().any(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_some_and(|guard| mentions_helper(guard, helpers))
                        || mentions_helper(&clause.body, helpers)
                })
        }
        _ => false,
    }
}
