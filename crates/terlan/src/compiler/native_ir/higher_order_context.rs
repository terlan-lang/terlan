//! Call-site contexts for private higher-order application helpers.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreModule, CoreType};
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use super::LocalFunctionIdentity as FunctionIdentity;

fn specialization_error(rendered: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeIrEmission,
        "specialize higher-order contexts",
        rendered,
    )
}

/// Clones private higher-order call chains by source call site.
///
/// Type monomorphization alone merges every callback with the same signature
/// into one recursive worker. That is too imprecise for suspension profiling:
/// an unrelated nested callback can make an otherwise finite worker appear
/// recursively re-entrant. A call-site clone preserves ordinary closure
/// values and ownership while giving closed-world target analysis a precise
/// function identity. Direct recursion within a clone closes back to the same
/// clone instead of generating an unbounded chain.
pub(super) fn specialize_higher_order_contexts(
    core: &mut CoreModule,
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<(), BoundaryError> {
    let helpers = core
        .functions
        .iter()
        .filter(|function| !function.public && has_callable_parameter(function))
        .map(|function| ((function.name.clone(), function.arity), function.clone()))
        .collect::<HashMap<_, _>>();
    if helpers.is_empty() {
        return Ok(());
    }
    let mut specializer = ContextSpecializer {
        module: core.module.clone(),
        helpers,
        active: Vec::new(),
        generated: Vec::new(),
        ordinal: 0,
        budget,
    };
    for function in &mut core.functions {
        if specializer
            .helpers
            .contains_key(&(function.name.clone(), function.arity))
        {
            continue;
        }
        specializer.rewrite_function(function)?;
    }
    core.functions.extend(specializer.generated);
    Ok(())
}

struct ContextSpecializer<'a> {
    module: String,
    helpers: HashMap<FunctionIdentity, CoreFunction>,
    active: Vec<(FunctionIdentity, String)>,
    generated: Vec<CoreFunction>,
    ordinal: usize,
    budget: &'a mut super::specialization_budget::SpecializationBudget,
}

impl ContextSpecializer<'_> {
    fn rewrite_function(&mut self, function: &mut CoreFunction) -> Result<(), BoundaryError> {
        for clause in &mut function.clauses {
            if let Some(body) = clause.body.core_expr.as_mut() {
                self.rewrite(body)?;
            }
        }
        Ok(())
    }

    fn rewrite(&mut self, expr: &mut CoreExpr) -> Result<(), BoundaryError> {
        match expr {
            CoreExpr::Call { function, args } => {
                for argument in args.iter_mut() {
                    self.rewrite(argument)?;
                }
                let identity = (function.clone(), args.len());
                if self.helpers.contains_key(&identity) {
                    *function = self.context_clone(identity)?;
                }
            }
            CoreExpr::RemoteCall { args, .. }
            | CoreExpr::ConstructorCall { args, .. }
            | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
                for argument in args {
                    self.rewrite(argument)?;
                }
            }
            CoreExpr::MutableReceiverCall { receiver, args, .. }
            | CoreExpr::FunctionCall {
                callee: receiver,
                args,
            } => {
                self.rewrite(receiver)?;
                for argument in args {
                    self.rewrite(argument)?;
                }
            }
            CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
                for item in items {
                    self.rewrite(item)?;
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
                self.rewrite(head)?;
                self.rewrite(tail)?;
            }
            CoreExpr::Map(fields) => {
                for field in fields {
                    self.rewrite(&mut field.value)?;
                }
            }
            CoreExpr::RecordConstruct { fields, .. }
            | CoreExpr::TemplateInstantiate { fields, .. } => {
                for field in fields {
                    self.rewrite(&mut field.value)?;
                }
            }
            CoreExpr::RecordUpdate { base, fields, .. } => {
                self.rewrite(base)?;
                for field in fields {
                    self.rewrite(&mut field.value)?;
                }
            }
            CoreExpr::FieldAccess { base, .. }
            | CoreExpr::RecordAccess { base, .. }
            | CoreExpr::Cast { expr: base, .. }
            | CoreExpr::UnaryOp { operand: base, .. } => self.rewrite(base)?,
            CoreExpr::Let { bindings, body } => {
                for binding in bindings {
                    self.rewrite(&mut binding.value)?;
                }
                self.rewrite(body)?;
            }
            CoreExpr::If { clauses } => {
                for clause in clauses {
                    self.rewrite(&mut clause.condition)?;
                    self.rewrite(&mut clause.body)?;
                }
            }
            CoreExpr::Case { scrutinee, clauses } => {
                self.rewrite(scrutinee)?;
                for clause in clauses {
                    if let Some(guard) = &mut clause.guard {
                        self.rewrite(guard)?;
                    }
                    self.rewrite(&mut clause.body)?;
                }
            }
            CoreExpr::Lam { body, .. } => self.rewrite(body)?,
            CoreExpr::ConstructorChain { args, record, .. } => {
                for argument in args {
                    self.rewrite(argument)?;
                }
                self.rewrite(record)?;
            }
            CoreExpr::ListComprehension {
                expr,
                generators,
                guards,
                ..
            } => {
                self.rewrite(expr)?;
                for generator in generators {
                    self.rewrite(&mut generator.source)?;
                }
                for guard in guards {
                    self.rewrite(guard)?;
                }
            }
            CoreExpr::Try { .. }
            | CoreExpr::SqlQuery { .. }
            | CoreExpr::Int(_)
            | CoreExpr::Float(_)
            | CoreExpr::Binary(_)
            | CoreExpr::Atom(_)
            | CoreExpr::Var(_)
            | CoreExpr::RemoteFunRef { .. } => {}
        }
        Ok(())
    }

    fn context_clone(&mut self, identity: FunctionIdentity) -> Result<String, BoundaryError> {
        if let Some((_, active)) = self
            .active
            .iter()
            .rev()
            .find(|(active, _)| active == &identity)
        {
            return Ok(active.clone());
        }
        self.budget
            .reserve(
                super::specialization_budget::SpecializationKind::HigherOrder,
                &self.module,
                1,
            )
            .map_err(specialization_error)?;
        let ordinal = self.ordinal;
        self.ordinal = ordinal.saturating_add(1);
        let name = format!(
            "$aot_hof_context_{}_{}_{}",
            sanitize(&identity.0),
            identity.1,
            ordinal,
        );
        let mut function = self.helpers.get(&identity).cloned().ok_or_else(|| {
            specialization_error(format!(
                "error[native_ir.higher_order_context]: helper `{}/{}` is absent",
                identity.0, identity.1
            ))
        })?;
        function.name = name.clone();
        function.public = false;
        self.active.push((identity, name.clone()));
        self.rewrite_function(&mut function)?;
        self.active.pop();
        self.generated.push(function);
        Ok(name)
    }
}

fn has_callable_parameter(function: &CoreFunction) -> bool {
    function
        .params
        .iter()
        .any(|parameter| matches!(parameter.core_ty, Some(CoreType::Arrow { .. })))
}

fn sanitize(identity: &str) -> String {
    identity
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}
