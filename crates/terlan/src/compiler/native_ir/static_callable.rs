//! Elimination of statically known non-escaping function values.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern};

use super::expression::free_variables;

#[path = "static_callable/preflight.rs"]
mod preflight;
use preflight::reject_deep_immediate_callable_chain;

/// Maximum number of function-value applications expanded in one expression.
const MAX_STATIC_CALL_EXPANSIONS: usize = 128;

/// Maximum number of lexical values captured by one statically erased closure.
const MAX_STATIC_CLOSURE_CAPTURES: usize = 64;

/// One function value whose runtime allocation can be erased safely.
#[derive(Clone, Debug, Eq, PartialEq)]
enum StaticCallable {
    /// Lambda with variable parameters and a lexical body.
    Lambda {
        /// Ordered lambda parameter names.
        params: Vec<String>,
        /// Body after outer static callables have been normalized.
        body: Box<CoreExpr>,
    },
    /// Qualified function identity produced by backend CoreIR.
    Remote {
        /// Qualified native application symbol.
        function: String,
        /// Required call arity.
        arity: usize,
    },
}

/// Stateful bounded static-callable normalizer.
struct StaticCallableNormalizer<'a> {
    /// Monotonic identity for deterministic compiler-generated capture locals.
    capture_ordinal: u64,
    /// Number of static calls expanded in the current function body.
    expansions: usize,
    /// Canonical module that owns the current function body.
    module: &'a str,
    /// Application-wide expansion budget shared with other specialization passes.
    budget: &'a mut super::specialization_budget::SpecializationBudget,
}

/// Erases non-escaping statically known function values from one CoreIR body.
///
/// Lambda bindings snapshot lexical captures into compiler-generated locals.
/// Invocation then becomes ordinary CoreIR `Let` control, preserving argument
/// order and parameter shadowing without adding a runtime closure ABI.
#[cfg(test)]
pub(super) fn normalize_static_callables(expr: &CoreExpr) -> Result<CoreExpr, String> {
    let mut budget = super::specialization_budget::SpecializationBudget::default();
    normalize_static_callables_with_budget(expr, "<expression>", &mut budget)
}

/// Erases static callables under one application-wide specialization budget.
pub(super) fn normalize_static_callables_with_budget(
    expr: &CoreExpr,
    module: &str,
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<CoreExpr, String> {
    let coverage = super::lowering_coverage::expression_coverage(expr);
    debug_assert!(!coverage.node.is_empty());
    reject_deep_immediate_callable_chain(expr)?;
    StaticCallableNormalizer {
        capture_ordinal: 0,
        expansions: 0,
        module,
        budget,
    }
    .rewrite(expr, &HashMap::new())
}

impl StaticCallableNormalizer<'_> {
    /// Rewrites one expression under the statically known callable scope.
    fn rewrite(
        &mut self,
        expr: &CoreExpr,
        callables: &HashMap<String, StaticCallable>,
    ) -> Result<CoreExpr, String> {
        match expr {
            CoreExpr::Tuple(items) => Ok(CoreExpr::Tuple(self.rewrite_many(items, callables)?)),
            CoreExpr::List(items) => Ok(CoreExpr::List(self.rewrite_many(items, callables)?)),
            CoreExpr::FixedArray(items) => {
                Ok(CoreExpr::FixedArray(self.rewrite_many(items, callables)?))
            }
            CoreExpr::ListCons { head, tail } => Ok(CoreExpr::ListCons {
                head: Box::new(self.rewrite(head, callables)?),
                tail: Box::new(self.rewrite(tail, callables)?),
            }),
            CoreExpr::Index { base, index } => Ok(CoreExpr::Index {
                base: Box::new(self.rewrite(base, callables)?),
                index: Box::new(self.rewrite(index, callables)?),
            }),
            CoreExpr::ListComprehension {
                expr,
                generators,
                guards,
                lift,
            } => {
                let mut scope = callables.clone();
                let mut lowered_generators = Vec::with_capacity(generators.len());
                for generator in generators {
                    let mut generator = generator.clone();
                    generator.source = self.rewrite(&generator.source, &scope)?;
                    remove_pattern_callables(&generator.pattern, &mut scope);
                    lowered_generators.push(generator);
                }
                Ok(CoreExpr::ListComprehension {
                    expr: Box::new(self.rewrite(expr, &scope)?),
                    generators: lowered_generators,
                    guards: self.rewrite_many(guards, &scope)?,
                    lift: lift.clone(),
                })
            }
            CoreExpr::Map(fields) => Ok(CoreExpr::Map(self.rewrite_map_fields(fields, callables)?)),
            CoreExpr::Call { function, args } if callables.contains_key(function) => {
                self.reserve_static_expansion()?;
                self.rewrite_function_call(&CoreExpr::Var(function.clone()), args, callables)
            }
            CoreExpr::Call { function, args } => Ok(CoreExpr::Call {
                function: function.clone(),
                args: self.rewrite_many(args, callables)?,
            }),
            CoreExpr::RemoteCall {
                module,
                function,
                args,
            } => Ok(CoreExpr::RemoteCall {
                module: module.clone(),
                function: function.clone(),
                args: self.rewrite_many(args, callables)?,
            }),
            CoreExpr::ConstructorCall {
                constructor,
                constructor_identity,
                args,
            } => Ok(CoreExpr::ConstructorCall {
                constructor: constructor.clone(),
                constructor_identity: constructor_identity.clone(),
                args: self.rewrite_many(args, callables)?,
            }),
            CoreExpr::Intrinsic(call) => {
                let mut call = call.clone();
                call.args = self.rewrite_many(&call.args, callables)?;
                Ok(CoreExpr::Intrinsic(call))
            }
            CoreExpr::MutableReceiverCall {
                receiver,
                method,
                args,
                effects,
            } => Ok(CoreExpr::MutableReceiverCall {
                receiver: Box::new(self.rewrite(receiver, callables)?),
                method: method.clone(),
                args: self.rewrite_many(args, callables)?,
                effects: effects.clone(),
            }),
            CoreExpr::RecordConstruct { name, fields } => Ok(CoreExpr::RecordConstruct {
                name: name.clone(),
                fields: self.rewrite_record_fields(fields, callables)?,
            }),
            CoreExpr::TemplateInstantiate { name, fields } => Ok(CoreExpr::TemplateInstantiate {
                name: name.clone(),
                fields: self.rewrite_record_fields(fields, callables)?,
            }),
            CoreExpr::RecordUpdate { base, name, fields } => Ok(CoreExpr::RecordUpdate {
                base: Box::new(self.rewrite(base, callables)?),
                name: name.clone(),
                fields: self.rewrite_record_fields(fields, callables)?,
            }),
            CoreExpr::FieldAccess { base, field } => Ok(CoreExpr::FieldAccess {
                base: Box::new(self.rewrite(base, callables)?),
                field: field.clone(),
            }),
            CoreExpr::RecordAccess { base, name, field } => Ok(CoreExpr::RecordAccess {
                base: Box::new(self.rewrite(base, callables)?),
                name: name.clone(),
                field: field.clone(),
            }),
            CoreExpr::ConstructorChain {
                base,
                base_constructor_identity,
                args,
                record,
            } => Ok(CoreExpr::ConstructorChain {
                base: base.clone(),
                base_constructor_identity: base_constructor_identity.clone(),
                args: self.rewrite_many(args, callables)?,
                record: Box::new(self.rewrite(record, callables)?),
            }),
            CoreExpr::UnaryOp { operator, operand } => Ok(CoreExpr::UnaryOp {
                operator: operator.clone(),
                operand: Box::new(self.rewrite(operand, callables)?),
            }),
            CoreExpr::BinaryOp {
                operator,
                left,
                right,
            } => Ok(CoreExpr::BinaryOp {
                operator: operator.clone(),
                left: Box::new(self.rewrite(left, callables)?),
                right: Box::new(self.rewrite(right, callables)?),
            }),
            CoreExpr::Let { bindings, body } => self.rewrite_let(bindings, body, callables),
            CoreExpr::If { clauses } => {
                let mut lowered = Vec::with_capacity(clauses.len());
                for clause in clauses {
                    let mut clause = clause.clone();
                    clause.condition = self.rewrite(&clause.condition, callables)?;
                    clause.body = self.rewrite(&clause.body, callables)?;
                    lowered.push(clause);
                }
                Ok(CoreExpr::If { clauses: lowered })
            }
            CoreExpr::Case { scrutinee, clauses } => Ok(CoreExpr::Case {
                scrutinee: Box::new(self.rewrite(scrutinee, callables)?),
                clauses: self.rewrite_case_clauses(clauses, callables)?,
            }),
            CoreExpr::Try {
                body,
                of_clauses,
                catch_clauses,
                after_clause,
            } => {
                let after_clause = after_clause
                    .as_ref()
                    .map(|after| {
                        let mut after = after.clone();
                        after.trigger = Box::new(self.rewrite(&after.trigger, callables)?);
                        after.body = Box::new(self.rewrite(&after.body, callables)?);
                        Ok::<_, String>(after)
                    })
                    .transpose()?;
                Ok(CoreExpr::Try {
                    body: Box::new(self.rewrite(body, callables)?),
                    of_clauses: self.rewrite_case_clauses(of_clauses, callables)?,
                    catch_clauses: self.rewrite_case_clauses(catch_clauses, callables)?,
                    after_clause,
                })
            }
            CoreExpr::FunctionCall { callee, args } => {
                if self.static_callable(callee, callables)?.is_some() {
                    self.reserve_static_expansion()?;
                    self.rewrite_function_call(callee, args, callables)
                } else {
                    Ok(CoreExpr::FunctionCall {
                        callee: Box::new(self.rewrite(callee, callables)?),
                        args: self.rewrite_many(args, callables)?,
                    })
                }
            }
            CoreExpr::Cast { expr, target_type } => Ok(CoreExpr::Cast {
                expr: Box::new(self.rewrite(expr, callables)?),
                target_type: target_type.clone(),
            }),
            CoreExpr::SqlQuery { parameters, .. } => {
                let mut query = expr.clone();
                let CoreExpr::SqlQuery {
                    parameters: lowered,
                    ..
                } = &mut query
                else {
                    unreachable!()
                };
                *lowered = self.rewrite_many(parameters, callables)?;
                Ok(query)
            }
            // Preserve the escaping lambda while normalizing statically known
            // calls in its body under parameter shadowing.
            CoreExpr::Lam { params, body } => {
                let mut scope = callables.clone();
                for pattern in params {
                    remove_pattern_callables(pattern, &mut scope);
                }
                Ok(CoreExpr::Lam {
                    params: params.clone(),
                    body: Box::new(self.rewrite(body, &scope)?),
                })
            }
            // Whole-result named references are closure-lowered after closed-world
            // application admission; immediate invocation was handled above.
            CoreExpr::RemoteFunRef { .. } => Ok(expr.clone()),
            CoreExpr::Var(name) if callables.contains_key(name) => Err(format!(
                "error[native_ir.function_value_escape]: `{name}` escapes static native lowering"
            )),
            _ => Ok(expr.clone()),
        }
    }

    /// Rewrites a sequence of expressions without changing evaluation order.
    fn rewrite_many(
        &mut self,
        expressions: &[CoreExpr],
        callables: &HashMap<String, StaticCallable>,
    ) -> Result<Vec<CoreExpr>, String> {
        expressions
            .iter()
            .map(|expr| self.rewrite(expr, callables))
            .collect()
    }

    fn rewrite_map_fields(
        &mut self,
        fields: &[crate::terlan_typeck::CoreMapExprField],
        callables: &HashMap<String, StaticCallable>,
    ) -> Result<Vec<crate::terlan_typeck::CoreMapExprField>, String> {
        fields
            .iter()
            .map(|field| {
                let mut field = field.clone();
                field.value = self.rewrite(&field.value, callables)?;
                Ok(field)
            })
            .collect()
    }

    fn rewrite_record_fields(
        &mut self,
        fields: &[crate::terlan_typeck::CoreRecordExprField],
        callables: &HashMap<String, StaticCallable>,
    ) -> Result<Vec<crate::terlan_typeck::CoreRecordExprField>, String> {
        fields
            .iter()
            .map(|field| {
                let mut field = field.clone();
                field.value = self.rewrite(&field.value, callables)?;
                Ok(field)
            })
            .collect()
    }

    fn rewrite_case_clauses(
        &mut self,
        clauses: &[crate::terlan_typeck::CoreCaseClause],
        callables: &HashMap<String, StaticCallable>,
    ) -> Result<Vec<crate::terlan_typeck::CoreCaseClause>, String> {
        clauses
            .iter()
            .map(|clause| {
                let mut scope = callables.clone();
                remove_pattern_callables(&clause.pattern, &mut scope);
                let mut clause = clause.clone();
                clause.guard = clause
                    .guard
                    .as_ref()
                    .map(|guard| self.rewrite(guard, &scope))
                    .transpose()?;
                clause.body = self.rewrite(&clause.body, &scope)?;
                Ok(clause)
            })
            .collect()
    }

    /// Rewrites sequential bindings and erases callable-only bindings.
    fn rewrite_let(
        &mut self,
        bindings: &[CoreLetBinding],
        body: &CoreExpr,
        callables: &HashMap<String, StaticCallable>,
    ) -> Result<CoreExpr, String> {
        let mut scope = callables.clone();
        let mut lowered = Vec::new();
        for (index, binding) in bindings.iter().enumerate() {
            let CorePattern::Var(name) = &binding.pattern else {
                let value = self.rewrite(&binding.value, &scope)?;
                lowered.push(CoreLetBinding {
                    pattern: binding.pattern.clone(),
                    value,
                });
                continue;
            };
            if let Some(callable) = self.static_callable(&binding.value, &scope)? {
                let terminal_escape = index + 1 == bindings.len()
                    && matches!(body, CoreExpr::Var(body_name) if body_name == name);
                if terminal_escape {
                    return retain_terminal_callable(callable, lowered);
                }
                let callable = self.snapshot_captures(callable, &mut lowered)?;
                scope.insert(name.clone(), callable);
                continue;
            }
            scope.remove(name);
            lowered.push(CoreLetBinding {
                pattern: binding.pattern.clone(),
                value: self.rewrite(&binding.value, &scope)?,
            });
        }
        let body = self.rewrite(body, &scope)?;
        if lowered.is_empty() {
            Ok(body)
        } else {
            Ok(CoreExpr::Let {
                bindings: lowered,
                body: Box::new(body),
            })
        }
    }

    /// Resolves and expands one function-value invocation.
    fn rewrite_function_call(
        &mut self,
        callee: &CoreExpr,
        args: &[CoreExpr],
        callables: &HashMap<String, StaticCallable>,
    ) -> Result<CoreExpr, String> {
        let callable = self.static_callable(callee, callables)?.ok_or_else(|| {
            "error[native_ir.dynamic_call]: unresolved function value cannot enter the native image"
                .to_string()
        })?;
        let args = self.rewrite_many(args, callables)?;
        match callable {
            StaticCallable::Remote { function, arity } => {
                if arity != args.len() {
                    return Err(static_arity_error(arity, args.len()));
                }
                Ok(CoreExpr::Call { function, args })
            }
            StaticCallable::Lambda { params, body } => {
                if params.len() != args.len() {
                    return Err(static_arity_error(params.len(), args.len()));
                }
                let bindings = params
                    .into_iter()
                    .zip(args)
                    .map(|(name, value)| CoreLetBinding {
                        pattern: CorePattern::Var(name),
                        value,
                    })
                    .collect();
                self.rewrite(&CoreExpr::Let { bindings, body }, callables)
            }
        }
    }

    /// Reserves one bounded static expansion before adding another recursive
    /// call frame.
    fn reserve_static_expansion(&mut self) -> Result<(), String> {
        self.expansions = self.expansions.saturating_add(1);
        if self.expansions > MAX_STATIC_CALL_EXPANSIONS {
            return Err(format!(
                "error[native_ir.specialization_limit]: static function-value expansion exceeds {MAX_STATIC_CALL_EXPANSIONS} calls"
            ));
        }
        self.budget.reserve(
            super::specialization_budget::SpecializationKind::StaticCallable,
            self.module,
            1,
        )?;
        Ok(())
    }

    /// Resolves one expression into a statically erasable callable.
    fn static_callable(
        &mut self,
        expr: &CoreExpr,
        callables: &HashMap<String, StaticCallable>,
    ) -> Result<Option<StaticCallable>, String> {
        match expr {
            CoreExpr::Var(name) => Ok(callables.get(name).cloned()),
            CoreExpr::RemoteFunRef {
                module,
                function,
                arity,
            } => Ok(Some(StaticCallable::Remote {
                function: format!("{module}.{function}"),
                arity: *arity,
            })),
            CoreExpr::Lam { params, body } => {
                let params = params
                    .iter()
                    .map(|pattern| match pattern {
                        CorePattern::Var(name) => {
                            let coverage = super::lowering_coverage::pattern_coverage(pattern);
                            debug_assert!(!coverage.node.is_empty());
                            Ok(name.clone())
                        }
                        _ => Err(
                            "error[native_ir.closure_pattern]: static lambda parameters must be variable patterns"
                                .to_string(),
                        ),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut lexical_callables = callables.clone();
                for param in &params {
                    lexical_callables.remove(param);
                }
                Ok(Some(StaticCallable::Lambda {
                    params,
                    body: Box::new(self.rewrite(body, &lexical_callables)?),
                }))
            }
            _ => Ok(None),
        }
    }

    /// Snapshots a lambda's lexical variables at closure creation time.
    fn snapshot_captures(
        &mut self,
        callable: StaticCallable,
        bindings: &mut Vec<CoreLetBinding>,
    ) -> Result<StaticCallable, String> {
        let StaticCallable::Lambda { params, body } = callable else {
            return Ok(callable);
        };
        let parameter_names = params.iter().cloned().collect::<HashSet<_>>();
        let mut captures = free_variables(&body)
            .into_iter()
            .filter(|name| !parameter_names.contains(name))
            .collect::<Vec<_>>();
        captures.sort();
        if captures.len() > MAX_STATIC_CLOSURE_CAPTURES {
            return Err(format!(
                "error[native_ir.closure_capture_limit]: static closure captures {} values; maximum is {MAX_STATIC_CLOSURE_CAPTURES}",
                captures.len()
            ));
        }
        let mut renames = HashMap::new();
        for capture in captures {
            let fresh = format!(
                "$native_closure_capture_{}_{}",
                self.capture_ordinal, capture
            );
            self.capture_ordinal = self.capture_ordinal.saturating_add(1);
            bindings.push(CoreLetBinding {
                pattern: CorePattern::Var(fresh.clone()),
                value: CoreExpr::Var(capture.clone()),
            });
            renames.insert(capture, fresh);
        }
        Ok(StaticCallable::Lambda {
            params,
            body: Box::new(rename_free_variables(&body, &renames, &mut HashSet::new())),
        })
    }
}

fn remove_pattern_callables(
    pattern: &CorePattern,
    callables: &mut HashMap<String, StaticCallable>,
) {
    let mut names = HashSet::new();
    bind_static_pattern(pattern, &mut names);
    for name in names {
        callables.remove(&name);
    }
}

/// Reifies one terminal static binding for later owned-closure conversion.
///
/// Keeping this construction outside the recursive normalizer avoids placing a
/// full `CoreExpr` temporary in every static-expansion stack frame.
#[cold]
fn retain_terminal_callable(
    callable: StaticCallable,
    bindings: Vec<CoreLetBinding>,
) -> Result<CoreExpr, String> {
    let escaped = match callable {
        StaticCallable::Lambda { params, body } => CoreExpr::Lam {
            params: params.into_iter().map(CorePattern::Var).collect(),
            body,
        },
        StaticCallable::Remote { function, arity } => {
            let (module, function) = function.rsplit_once('.').ok_or_else(|| {
                "error[native_ir.function_value_target]: terminal named function value is not qualified"
                    .to_string()
            })?;
            CoreExpr::RemoteFunRef {
                module: module.to_string(),
                function: function.to_string(),
                arity,
            }
        }
    };
    Ok(if bindings.is_empty() {
        escaped
    } else {
        CoreExpr::Let {
            bindings,
            body: Box::new(escaped),
        }
    })
}

/// Produces the stable static-call arity diagnostic.
fn static_arity_error(expected: usize, actual: usize) -> String {
    format!(
        "error[native_ir.function_value_arity]: expected {expected} arguments but received {actual}"
    )
}

/// Renames free variables while preserving sequential lexical shadowing.
pub(super) fn rename_free_variables(
    expr: &CoreExpr,
    renames: &HashMap<String, String>,
    bound: &mut HashSet<String>,
) -> CoreExpr {
    match expr {
        CoreExpr::Var(name) if !bound.contains(name) => renames
            .get(name)
            .cloned()
            .map(CoreExpr::Var)
            .unwrap_or_else(|| expr.clone()),
        CoreExpr::Tuple(items) => CoreExpr::Tuple(rename_many(items, renames, bound)),
        CoreExpr::List(items) => CoreExpr::List(rename_many(items, renames, bound)),
        CoreExpr::FixedArray(items) => CoreExpr::FixedArray(rename_many(items, renames, bound)),
        CoreExpr::ListCons { head, tail } => CoreExpr::ListCons {
            head: Box::new(rename_free_variables(head, renames, bound)),
            tail: Box::new(rename_free_variables(tail, renames, bound)),
        },
        CoreExpr::Index { base, index } => CoreExpr::Index {
            base: Box::new(rename_free_variables(base, renames, bound)),
            index: Box::new(rename_free_variables(index, renames, bound)),
        },
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            lift,
        } => {
            let original = bound.clone();
            let generators = generators
                .iter()
                .map(|generator| {
                    let lowered = crate::terlan_typeck::CoreListComprehensionGenerator {
                        pattern: generator.pattern.clone(),
                        source: rename_free_variables(&generator.source, renames, bound),
                    };
                    bind_static_pattern(&generator.pattern, bound);
                    lowered
                })
                .collect();
            let guards = rename_many(guards, renames, bound);
            let expr = Box::new(rename_free_variables(expr, renames, bound));
            *bound = original;
            CoreExpr::ListComprehension {
                expr,
                generators,
                guards,
                lift: lift.clone(),
            }
        }
        CoreExpr::Map(fields) => CoreExpr::Map(
            fields
                .iter()
                .map(|field| {
                    let mut field = field.clone();
                    field.value = rename_free_variables(&field.value, renames, bound);
                    field
                })
                .collect(),
        ),
        CoreExpr::Call { function, args } => CoreExpr::Call {
            function: function.clone(),
            args: rename_many(args, renames, bound),
        },
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => CoreExpr::RemoteCall {
            module: module.clone(),
            function: function.clone(),
            args: rename_many(args, renames, bound),
        },
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => CoreExpr::ConstructorCall {
            constructor: constructor.clone(),
            constructor_identity: constructor_identity.clone(),
            args: rename_many(args, renames, bound),
        },
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            effects,
        } => CoreExpr::MutableReceiverCall {
            receiver: Box::new(rename_free_variables(receiver, renames, bound)),
            method: method.clone(),
            args: rename_many(args, renames, bound),
            effects: effects.clone(),
        },
        CoreExpr::Intrinsic(call) => {
            let mut call = call.clone();
            call.args = rename_many(&call.args, renames, bound);
            CoreExpr::Intrinsic(call)
        }
        CoreExpr::FunctionCall { callee, args } => CoreExpr::FunctionCall {
            callee: Box::new(rename_free_variables(callee, renames, bound)),
            args: rename_many(args, renames, bound),
        },
        CoreExpr::RecordConstruct { name, fields } => CoreExpr::RecordConstruct {
            name: name.clone(),
            fields: rename_record_fields(fields, renames, bound),
        },
        CoreExpr::TemplateInstantiate { name, fields } => CoreExpr::TemplateInstantiate {
            name: name.clone(),
            fields: rename_record_fields(fields, renames, bound),
        },
        CoreExpr::RecordUpdate { base, name, fields } => CoreExpr::RecordUpdate {
            base: Box::new(rename_free_variables(base, renames, bound)),
            name: name.clone(),
            fields: rename_record_fields(fields, renames, bound),
        },
        CoreExpr::FieldAccess { base, field } => CoreExpr::FieldAccess {
            base: Box::new(rename_free_variables(base, renames, bound)),
            field: field.clone(),
        },
        CoreExpr::RecordAccess { base, name, field } => CoreExpr::RecordAccess {
            base: Box::new(rename_free_variables(base, renames, bound)),
            name: name.clone(),
            field: field.clone(),
        },
        CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            args,
            record,
        } => CoreExpr::ConstructorChain {
            base: base.clone(),
            base_constructor_identity: base_constructor_identity.clone(),
            args: rename_many(args, renames, bound),
            record: Box::new(rename_free_variables(record, renames, bound)),
        },
        CoreExpr::UnaryOp { operator, operand } => CoreExpr::UnaryOp {
            operator: operator.clone(),
            operand: Box::new(rename_free_variables(operand, renames, bound)),
        },
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => CoreExpr::BinaryOp {
            operator: operator.clone(),
            left: Box::new(rename_free_variables(left, renames, bound)),
            right: Box::new(rename_free_variables(right, renames, bound)),
        },
        CoreExpr::Let { bindings, body } => {
            let original = bound.clone();
            let mut lowered = Vec::with_capacity(bindings.len());
            for binding in bindings {
                lowered.push(CoreLetBinding {
                    pattern: binding.pattern.clone(),
                    value: rename_free_variables(&binding.value, renames, bound),
                });
                bind_static_pattern(&binding.pattern, bound);
            }
            let body = rename_free_variables(body, renames, bound);
            *bound = original;
            CoreExpr::Let {
                bindings: lowered,
                body: Box::new(body),
            }
        }
        CoreExpr::Cast { expr, target_type } => CoreExpr::Cast {
            expr: Box::new(rename_free_variables(expr, renames, bound)),
            target_type: target_type.clone(),
        },
        CoreExpr::If { clauses } => CoreExpr::If {
            clauses: clauses
                .iter()
                .map(|clause| {
                    let mut clause = clause.clone();
                    clause.condition = rename_free_variables(&clause.condition, renames, bound);
                    clause.body = rename_free_variables(&clause.body, renames, bound);
                    clause
                })
                .collect(),
        },
        CoreExpr::Case { scrutinee, clauses } => CoreExpr::Case {
            scrutinee: Box::new(rename_free_variables(scrutinee, renames, bound)),
            clauses: rename_case_clauses(clauses, renames, bound),
        },
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => CoreExpr::Try {
            body: Box::new(rename_free_variables(body, renames, bound)),
            of_clauses: rename_case_clauses(of_clauses, renames, bound),
            catch_clauses: rename_case_clauses(catch_clauses, renames, bound),
            after_clause: after_clause.as_ref().map(|after| {
                let mut after = after.clone();
                after.trigger = Box::new(rename_free_variables(&after.trigger, renames, bound));
                after.body = Box::new(rename_free_variables(&after.body, renames, bound));
                after
            }),
        },
        CoreExpr::Lam { params, body } => {
            let original = bound.clone();
            for pattern in params {
                bind_static_pattern(pattern, bound);
            }
            let body = Box::new(rename_free_variables(body, renames, bound));
            *bound = original;
            CoreExpr::Lam {
                params: params.clone(),
                body,
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            let mut query = expr.clone();
            let CoreExpr::SqlQuery {
                parameters: lowered,
                ..
            } = &mut query
            else {
                unreachable!()
            };
            *lowered = rename_many(parameters, renames, bound);
            query
        }
        _ => expr.clone(),
    }
}

/// Renames free variables in an ordered expression list.
fn rename_many(
    expressions: &[CoreExpr],
    renames: &HashMap<String, String>,
    bound: &mut HashSet<String>,
) -> Vec<CoreExpr> {
    expressions
        .iter()
        .map(|expr| rename_free_variables(expr, renames, bound))
        .collect()
}

fn rename_record_fields(
    fields: &[crate::terlan_typeck::CoreRecordExprField],
    renames: &HashMap<String, String>,
    bound: &mut HashSet<String>,
) -> Vec<crate::terlan_typeck::CoreRecordExprField> {
    fields
        .iter()
        .map(|field| {
            let mut field = field.clone();
            field.value = rename_free_variables(&field.value, renames, bound);
            field
        })
        .collect()
}

fn rename_case_clauses(
    clauses: &[crate::terlan_typeck::CoreCaseClause],
    renames: &HashMap<String, String>,
    bound: &mut HashSet<String>,
) -> Vec<crate::terlan_typeck::CoreCaseClause> {
    clauses
        .iter()
        .map(|clause| {
            let original = bound.clone();
            bind_static_pattern(&clause.pattern, bound);
            let mut clause = clause.clone();
            clause.guard = clause
                .guard
                .as_ref()
                .map(|guard| rename_free_variables(guard, renames, bound));
            clause.body = rename_free_variables(&clause.body, renames, bound);
            *bound = original;
            clause
        })
        .collect()
}

fn bind_static_pattern(pattern: &CorePattern, bound: &mut HashSet<String>) {
    match pattern {
        CorePattern::Var(name) => {
            bound.insert(name.clone());
        }
        CorePattern::Alias { alias, pattern } => {
            bound.insert(alias.clone());
            bind_static_pattern(pattern, bound);
        }
        CorePattern::Tuple(patterns) | CorePattern::List(patterns) => {
            for pattern in patterns {
                bind_static_pattern(pattern, bound);
            }
        }
        CorePattern::ListCons { head, tail } => {
            bind_static_pattern(head, bound);
            bind_static_pattern(tail, bound);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                bind_static_pattern(&field.value, bound);
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                bind_static_pattern(&field.value, bound);
            }
        }
        CorePattern::Constructor { args, .. } => {
            for pattern in args {
                bind_static_pattern(pattern, bound);
            }
        }
        CorePattern::BinaryLayout { fields, .. } => {
            for field in fields {
                if field.name != "_" {
                    bound.insert(field.name.clone());
                }
            }
        }
        CorePattern::StringPattern(segments) => {
            for segment in segments {
                if let crate::terlan_typeck::CoreStringPatternSegment::Capture(capture) = segment {
                    bound.insert(capture.name.clone());
                }
            }
        }
        CorePattern::Wildcard
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::String(_)
        | CorePattern::Atom(_) => {}
    }
}
