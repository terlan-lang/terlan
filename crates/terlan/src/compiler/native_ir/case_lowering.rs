//! Scalar `Case` elimination before NativeIR admission.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreFunction, CoreIfClause, CoreLetBinding, CoreModule, CorePattern,
    CoreType,
};

/// Maximum clauses admitted by one scalar case expression.
const MAX_SCALAR_CASE_CLAUSES: usize = 256;

/// Maximum nested scalar case expressions admitted in one body.
const MAX_SCALAR_CASE_DEPTH: usize = 64;

/// Match predicate and lexical bindings produced from one scalar pattern.
struct ScalarPatternPlan {
    /// Predicate evaluated before the optional source guard.
    predicate: Option<CoreExpr>,
    /// Names bound to the complete scalar scrutinee for this clause.
    bindings: Vec<String>,
}

/// Stateful scalar-case normalizer with deterministic temporary identities.
struct ScalarCaseLowerer {
    /// Next compiler-generated scrutinee identity.
    scrutinee_ordinal: u64,
    /// Checked local return types used to preserve structured case joins.
    function_core_types: HashMap<(String, usize), CoreType>,
}

/// Eliminates supported scalar `Case` expressions from one CoreIR module.
///
/// Each scrutinee is evaluated exactly once into a compiler local. Ordered
/// clauses become existing `If` control flow, while pattern captures are
/// introduced independently around guards and selected bodies.
pub(super) fn lower_scalar_cases(core: &mut CoreModule) -> Result<(), String> {
    let mut function_core_types = HashMap::new();
    for function in &core.functions {
        let Some(return_type) = &function.core_return_type else {
            continue;
        };
        function_core_types.insert((function.name.clone(), function.arity), return_type.clone());
        function_core_types.insert(
            (format!("{}.{}", core.module, function.name), function.arity),
            return_type.clone(),
        );
    }
    let mut lowerer = ScalarCaseLowerer {
        scrutinee_ordinal: 0,
        function_core_types,
    };
    for function in &mut core.functions {
        lowerer.normalize_function_head(function)?;
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                *body = lowerer.rewrite(body, 0)?;
            }
        }
    }
    Ok(())
}

impl ScalarCaseLowerer {
    /// Moves a single function clause's structural head patterns into its body.
    ///
    /// Native functions use ordinary named ABI parameters. Source-level head
    /// destructuring therefore becomes the same bounded `Case` representation
    /// used by expression and lambda patterns. Binary-layout heads also recover
    /// their concrete VM value type when the source annotation was a
    /// shape-expanded layout rather than ordinary type text.
    fn normalize_function_head(&mut self, function: &mut CoreFunction) -> Result<(), String> {
        let [clause] = function.clauses.as_mut_slice() else {
            return Ok(());
        };
        if clause.core_patterns.len() != function.params.len() {
            return Ok(());
        }
        let has_structured_head = clause
            .core_patterns
            .iter()
            .zip(&function.params)
            .any(|(pattern, parameter)| {
                !matches!(pattern, Some(CorePattern::Var(name)) if name == &parameter.name)
            });
        if !has_structured_head {
            return Ok(());
        }

        let mut body = clause.body.core_expr.take().ok_or_else(|| {
            format!(
                "error[native_ir.function_head_body]: `{}/{}` has no typed CoreIR body",
                function.name, function.arity
            )
        })?;
        let mut source_guard = clause
            .guard
            .take()
            .map(|guard| {
                guard.core_expr.ok_or_else(|| {
                    format!(
                        "error[native_ir.function_head_guard]: `{}/{}` has no typed CoreIR guard",
                        function.name, function.arity
                    )
                })
            })
            .transpose()?;

        for (parameter, pattern) in function
            .params
            .iter_mut()
            .zip(clause.core_patterns.iter_mut())
            .rev()
        {
            let original = pattern.take().ok_or_else(|| {
                format!(
                    "error[native_ir.function_head_pattern]: `{}/{}` parameter `{}` has no typed CoreIR pattern",
                    function.name, function.arity, parameter.name
                )
            })?;
            if matches!(&original, CorePattern::BinaryLayout { .. }) {
                parameter.ty = "BitString".to_string();
                parameter.core_ty = Some(CoreType::Named("BitString".to_string()));
            }
            if matches!(&original, CorePattern::Var(name) if name == &parameter.name) {
                *pattern = Some(original);
                continue;
            }
            body = CoreExpr::Case {
                scrutinee: Box::new(CoreExpr::Var(parameter.name.clone())),
                clauses: vec![CoreCaseClause {
                    pattern: original,
                    guard: source_guard.take(),
                    body,
                }],
            };
            *pattern = Some(CorePattern::Var(parameter.name.clone()));
        }
        if source_guard.is_some() {
            return Err(format!(
                "error[native_ir.function_head_guard_scope]: `{}/{}` guard has no structural head pattern owner",
                function.name, function.arity
            ));
        }
        clause.body.core_expr = Some(body);
        Ok(())
    }

    /// Rewrites one expression and every executable child expression.
    fn rewrite(&mut self, expr: &CoreExpr, case_depth: usize) -> Result<CoreExpr, String> {
        if let CoreExpr::Let { bindings, body } = expr {
            if bindings
                .iter()
                .any(|binding| !matches!(binding.pattern, CorePattern::Var(_)))
            {
                let normalized = self.normalize_let_bindings(bindings, body, case_depth)?;
                return self.rewrite(&normalized, case_depth);
            }

            // A retained structured case is control flow, not an eager scalar
            // value. Carry the lexical continuation into every selected arm
            // so a suspending arm can compose with transitions that follow
            // the binding. This is the CoreIR form of bind associativity:
            //
            //     let value = case subject { pattern -> selected };
            //     continuation(value)
            //
            // becomes:
            //
            //     case subject {
            //         pattern -> let value = selected; continuation(value)
            //     }
            //
            // Only the selected arm executes, so source evaluation order and
            // cleanup behavior are preserved. Scalar cases have already been
            // eliminated into `If`; this route therefore owns the structured
            // constructor/record/list cases retained for NativeIR lowering.
            let rewritten_bindings = bindings
                .iter()
                .map(|binding| {
                    Ok(CoreLetBinding {
                        pattern: binding.pattern.clone(),
                        value: self.rewrite(&binding.value, case_depth)?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let rewritten_body = self.rewrite(body, case_depth)?;
            if let Some(binding_index) = rewritten_bindings
                .iter()
                .position(|binding| matches!(binding.value, CoreExpr::Case { .. }))
            {
                let selected_binding = &rewritten_bindings[binding_index];
                let identity_continuation = binding_index + 1 == rewritten_bindings.len()
                    && matches!(
                        (&selected_binding.pattern, &rewritten_body),
                        (CorePattern::Var(binding), CoreExpr::Var(result)) if binding == result
                    );
                if !identity_continuation {
                    if case_depth >= MAX_SCALAR_CASE_DEPTH {
                        return Err(format!(
                            "error[native_ir.case_continuation_depth]: structured case continuation nesting exceeds {MAX_SCALAR_CASE_DEPTH} expressions"
                        ));
                    }
                    let CoreExpr::Case {
                        scrutinee,
                        mut clauses,
                    } = selected_binding.value.clone()
                    else {
                        unreachable!("structured case binding selected by match predicate")
                    };
                    let result_core_type = super::structured_case::core_expr_type(
                        &CoreExpr::Case {
                            scrutinee: scrutinee.clone(),
                            clauses: clauses.clone(),
                        },
                        &HashMap::new(),
                        &self.function_core_types,
                    );
                    let remaining = rewritten_bindings[binding_index + 1..].to_vec();
                    for clause in &mut clauses {
                        let mut continuation_bindings = Vec::with_capacity(1 + remaining.len());
                        continuation_bindings.push(CoreLetBinding {
                            pattern: selected_binding.pattern.clone(),
                            value: result_core_type.as_ref().map_or_else(
                                || clause.body.clone(),
                                |target_type| {
                                    contextualize_control_result(clause.body.clone(), target_type)
                                },
                            ),
                        });
                        continuation_bindings.extend(remaining.clone());
                        clause.body = self.rewrite(
                            &CoreExpr::Let {
                                bindings: continuation_bindings,
                                body: Box::new(rewritten_body.clone()),
                            },
                            case_depth + 1,
                        )?;
                    }
                    let distributed = CoreExpr::Case { scrutinee, clauses };
                    let prefix = rewritten_bindings[..binding_index].to_vec();
                    return Ok(if prefix.is_empty() {
                        distributed
                    } else {
                        CoreExpr::Let {
                            bindings: prefix,
                            body: Box::new(distributed),
                        }
                    });
                }
            }
            return Ok(CoreExpr::Let {
                bindings: rewritten_bindings,
                body: Box::new(rewritten_body),
            });
        }
        if let CoreExpr::Lam { params, body } = expr {
            if params
                .iter()
                .any(|pattern| !matches!(pattern, CorePattern::Var(_)))
            {
                let mut lowered_body = body.as_ref().clone();
                let mut lowered_params = Vec::with_capacity(params.len());
                for pattern in params.iter().rev() {
                    let ordinal = self.scrutinee_ordinal;
                    self.scrutinee_ordinal = self.scrutinee_ordinal.saturating_add(1);
                    let parameter = format!("$native_lambda_{ordinal}_parameter");
                    lowered_body = CoreExpr::Case {
                        scrutinee: Box::new(CoreExpr::Var(parameter.clone())),
                        clauses: vec![CoreCaseClause {
                            pattern: pattern.clone(),
                            guard: None,
                            body: lowered_body,
                        }],
                    };
                    lowered_params.push(CorePattern::Var(parameter));
                }
                lowered_params.reverse();
                let mut lowered = CoreExpr::Lam {
                    params: lowered_params,
                    body: Box::new(lowered_body),
                };
                self.rewrite_children(&mut lowered, case_depth)?;
                return Ok(lowered);
            }
        }
        if let CoreExpr::Case { scrutinee, clauses } = expr {
            if clauses.iter().all(|clause| scalar_pattern(&clause.pattern)) {
                return self.lower_case(scrutinee, clauses, case_depth);
            }
            let mut retained = expr.clone();
            self.rewrite_children(&mut retained, case_depth)?;
            return Ok(retained);
        }
        if let CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } = expr
        {
            let left = self.rewrite(left, case_depth)?;
            let right = self.rewrite(right, case_depth)?;
            if expression_contains_case(&left) || expression_contains_case(&right) {
                if operator == "and" {
                    return Ok(CoreExpr::If {
                        clauses: vec![
                            CoreIfClause {
                                condition: left,
                                body: right,
                            },
                            CoreIfClause {
                                condition: CoreExpr::Atom("true".to_string()),
                                body: CoreExpr::Atom("false".to_string()),
                            },
                        ],
                    });
                }
                if operator == "or" {
                    return Ok(CoreExpr::If {
                        clauses: vec![
                            CoreIfClause {
                                condition: left,
                                body: CoreExpr::Atom("true".to_string()),
                            },
                            CoreIfClause {
                                condition: CoreExpr::Atom("true".to_string()),
                                body: right,
                            },
                        ],
                    });
                }
                return Ok(self.hoist_eager_binary_cases(operator, left, right));
            }
            return Ok(CoreExpr::BinaryOp {
                operator: operator.clone(),
                left: Box::new(left),
                right: Box::new(right),
            });
        }

        let mut lowered = expr.clone();
        self.rewrite_children(&mut lowered, case_depth)?;
        Ok(lowered)
    }

    /// Hoists control-valued eager operands without changing left-to-right order.
    fn hoist_eager_binary_cases(
        &mut self,
        operator: &str,
        left: CoreExpr,
        right: CoreExpr,
    ) -> CoreExpr {
        let mut bindings = Vec::new();
        let left = self.hoist_case_operand(left, &mut bindings);
        let right = self.hoist_case_operand(right, &mut bindings);
        let body = CoreExpr::BinaryOp {
            operator: operator.to_string(),
            left: Box::new(left),
            right: Box::new(right),
        };
        CoreExpr::Let {
            bindings,
            body: Box::new(body),
        }
    }

    /// Replaces one control-valued operand with a compiler temporary.
    fn hoist_case_operand(
        &mut self,
        operand: CoreExpr,
        bindings: &mut Vec<CoreLetBinding>,
    ) -> CoreExpr {
        if !expression_contains_case(&operand) {
            return operand;
        }
        let ordinal = self.scrutinee_ordinal;
        self.scrutinee_ordinal = self.scrutinee_ordinal.saturating_add(1);
        let temporary = format!("$native_case_{ordinal}_result");
        bindings.push(CoreLetBinding {
            pattern: CorePattern::Var(temporary.clone()),
            value: operand,
        });
        CoreExpr::Var(temporary)
    }

    /// Rewrites ordered destructuring bindings into single-evaluation cases.
    fn normalize_let_bindings(
        &mut self,
        bindings: &[CoreLetBinding],
        body: &CoreExpr,
        case_depth: usize,
    ) -> Result<CoreExpr, String> {
        let Some((binding, remaining)) = bindings.split_first() else {
            return self.rewrite(body, case_depth);
        };
        let value = self.rewrite(&binding.value, case_depth)?;
        let continuation = self.normalize_let_bindings(remaining, body, case_depth)?;
        if matches!(&binding.pattern, CorePattern::Var(_)) {
            return Ok(CoreExpr::Let {
                bindings: vec![CoreLetBinding {
                    pattern: binding.pattern.clone(),
                    value,
                }],
                body: Box::new(continuation),
            });
        }

        let ordinal = self.scrutinee_ordinal;
        self.scrutinee_ordinal = self.scrutinee_ordinal.saturating_add(1);
        let temporary = format!("$native_let_{ordinal}_scrutinee");
        Ok(CoreExpr::Let {
            bindings: vec![CoreLetBinding {
                pattern: CorePattern::Var(temporary.clone()),
                value,
            }],
            body: Box::new(CoreExpr::Case {
                scrutinee: Box::new(CoreExpr::Var(temporary)),
                clauses: vec![CoreCaseClause {
                    pattern: binding.pattern.clone(),
                    guard: None,
                    body: continuation,
                }],
            }),
        })
    }

    /// Rewrites all child expressions while preserving the parent node.
    fn rewrite_children(&mut self, expr: &mut CoreExpr, case_depth: usize) -> Result<(), String> {
        match expr {
            CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
                self.rewrite_many(items, case_depth)?;
            }
            CoreExpr::ListCons { head, tail } => {
                **head = self.rewrite(head, case_depth)?;
                **tail = self.rewrite(tail, case_depth)?;
            }
            CoreExpr::Index { base, index } => {
                **base = self.rewrite(base, case_depth)?;
                **index = self.rewrite(index, case_depth)?;
            }
            CoreExpr::ListComprehension {
                expr,
                generators,
                guards,
                ..
            } => {
                **expr = self.rewrite(expr, case_depth)?;
                for generator in generators {
                    generator.source = self.rewrite(&generator.source, case_depth)?;
                }
                self.rewrite_many(guards, case_depth)?;
            }
            CoreExpr::Let { bindings, body } => {
                for binding in bindings {
                    binding.value = self.rewrite(&binding.value, case_depth)?;
                }
                **body = self.rewrite(body, case_depth)?;
            }
            CoreExpr::Map(fields) => {
                for field in fields {
                    field.value = self.rewrite(&field.value, case_depth)?;
                }
            }
            CoreExpr::RecordConstruct { fields, .. }
            | CoreExpr::TemplateInstantiate { fields, .. } => {
                for field in fields {
                    field.value = self.rewrite(&field.value, case_depth)?;
                }
            }
            CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
                **base = self.rewrite(base, case_depth)?;
            }
            CoreExpr::RecordUpdate { base, fields, .. } => {
                **base = self.rewrite(base, case_depth)?;
                for field in fields {
                    field.value = self.rewrite(&field.value, case_depth)?;
                }
            }
            CoreExpr::ConstructorChain { args, record, .. } => {
                self.rewrite_many(args, case_depth)?;
                **record = self.rewrite(record, case_depth)?;
            }
            CoreExpr::RemoteCall { args, .. }
            | CoreExpr::ConstructorCall { args, .. }
            | CoreExpr::Call { args, .. } => self.rewrite_many(args, case_depth)?,
            CoreExpr::MutableReceiverCall { receiver, args, .. } => {
                **receiver = self.rewrite(receiver, case_depth)?;
                self.rewrite_many(args, case_depth)?;
            }
            CoreExpr::FunctionCall { callee, args } => {
                **callee = self.rewrite(callee, case_depth)?;
                self.rewrite_many(args, case_depth)?;
            }
            CoreExpr::Cast { expr, .. } => **expr = self.rewrite(expr, case_depth)?,
            CoreExpr::Intrinsic(call) => self.rewrite_many(&mut call.args, case_depth)?,
            CoreExpr::SqlQuery { parameters, .. } => {
                self.rewrite_many(parameters, case_depth)?;
            }
            CoreExpr::Try {
                body,
                of_clauses,
                catch_clauses,
                after_clause,
            } => {
                **body = self.rewrite(body, case_depth)?;
                self.rewrite_case_clauses(of_clauses, case_depth)?;
                self.rewrite_case_clauses(catch_clauses, case_depth)?;
                if let Some(after) = after_clause {
                    *after.trigger = self.rewrite(&after.trigger, case_depth)?;
                    *after.body = self.rewrite(&after.body, case_depth)?;
                }
            }
            CoreExpr::If { clauses } => {
                for clause in clauses {
                    clause.condition = self.rewrite(&clause.condition, case_depth)?;
                    clause.body = self.rewrite(&clause.body, case_depth)?;
                }
            }
            CoreExpr::Lam { body, .. } => **body = self.rewrite(body, case_depth)?,
            CoreExpr::UnaryOp { operand, .. } => {
                **operand = self.rewrite(operand, case_depth)?;
            }
            CoreExpr::BinaryOp { left, right, .. } => {
                **left = self.rewrite(left, case_depth)?;
                **right = self.rewrite(right, case_depth)?;
            }
            CoreExpr::Case { scrutinee, clauses } => {
                **scrutinee = self.rewrite(scrutinee, case_depth + 1)?;
                self.rewrite_case_clauses(clauses, case_depth + 1)?;
            }
            CoreExpr::Int(_)
            | CoreExpr::Float(_)
            | CoreExpr::Binary(_)
            | CoreExpr::Atom(_)
            | CoreExpr::Var(_)
            | CoreExpr::RemoteFunRef { .. } => {}
        }
        Ok(())
    }

    /// Rewrites a mutable ordered expression sequence in place.
    fn rewrite_many(
        &mut self,
        expressions: &mut [CoreExpr],
        case_depth: usize,
    ) -> Result<(), String> {
        for expression in expressions {
            *expression = self.rewrite(expression, case_depth)?;
        }
        Ok(())
    }

    /// Rewrites nested expressions retained by a non-`Case` clause owner.
    fn rewrite_case_clauses(
        &mut self,
        clauses: &mut [CoreCaseClause],
        case_depth: usize,
    ) -> Result<(), String> {
        for clause in clauses {
            if let Some(guard) = &mut clause.guard {
                *guard = self.rewrite(guard, case_depth)?;
            }
            clause.body = self.rewrite(&clause.body, case_depth)?;
        }
        Ok(())
    }

    /// Lowers one scalar case into a single-evaluation `Let` and ordered `If`.
    fn lower_case(
        &mut self,
        scrutinee: &CoreExpr,
        clauses: &[CoreCaseClause],
        case_depth: usize,
    ) -> Result<CoreExpr, String> {
        if clauses.is_empty() {
            return Err("error[native_ir.case_empty]: scalar case has no clauses".to_string());
        }
        if clauses.len() > MAX_SCALAR_CASE_CLAUSES {
            return Err(format!(
                "error[native_ir.case_clause_limit]: scalar case has {} clauses; maximum is {MAX_SCALAR_CASE_CLAUSES}",
                clauses.len()
            ));
        }
        if case_depth >= MAX_SCALAR_CASE_DEPTH {
            return Err(format!(
                "error[native_ir.case_depth_limit]: scalar case nesting exceeds {MAX_SCALAR_CASE_DEPTH} expressions"
            ));
        }

        let ordinal = self.scrutinee_ordinal;
        self.scrutinee_ordinal = self.scrutinee_ordinal.saturating_add(1);
        let temporary = format!("$native_case_{ordinal}_scrutinee");
        let scrutinee = self.rewrite(scrutinee, case_depth + 1)?;
        let mut lowered_clauses = Vec::with_capacity(clauses.len());
        for (clause_index, clause) in clauses.iter().enumerate() {
            let plan = scalar_pattern_plan(&clause.pattern, &temporary)?;
            let exhaustive_terminal = clause_index + 1 == clauses.len() && clause.guard.is_none();
            let guard = clause
                .guard
                .as_ref()
                .map(|guard| self.rewrite(guard, case_depth + 1))
                .transpose()?
                .unwrap_or_else(|| CoreExpr::Atom("true".to_string()));
            let guarded = bind_pattern_names(&plan.bindings, &temporary, guard);
            let condition = match (exhaustive_terminal, plan.predicate) {
                (true, _) => CoreExpr::Atom("true".to_string()),
                (false, Some(predicate)) => CoreExpr::BinaryOp {
                    operator: "and".to_string(),
                    left: Box::new(predicate),
                    right: Box::new(guarded),
                },
                (false, None) => guarded,
            };
            let body = self.rewrite(&clause.body, case_depth + 1)?;
            lowered_clauses.push(CoreIfClause {
                condition,
                body: bind_pattern_names(&plan.bindings, &temporary, body),
            });
        }

        Ok(CoreExpr::Let {
            bindings: vec![CoreLetBinding {
                pattern: CorePattern::Var(temporary),
                value: scrutinee,
            }],
            body: Box::new(CoreExpr::If {
                clauses: lowered_clauses,
            }),
        })
    }
}

/// Pushes a checked result type to terminal control values.
///
/// Keeping `If`, `Case`, and `Let` as the outer node lets suspension-region
/// discovery see calls inside their selected branches. It also preserves
/// effectful prefixes such as diagnostic logging before a nullary `None`
/// result instead of replacing the entire wrapper during typed allocation.
fn contextualize_control_result(expr: CoreExpr, target_type: &CoreType) -> CoreExpr {
    match expr {
        CoreExpr::If { clauses } => CoreExpr::If {
            clauses: clauses
                .into_iter()
                .map(|clause| crate::terlan_typeck::CoreIfClause {
                    condition: clause.condition,
                    body: contextualize_control_result(clause.body, target_type),
                })
                .collect(),
        },
        CoreExpr::Case { scrutinee, clauses } => CoreExpr::Case {
            scrutinee,
            clauses: clauses
                .into_iter()
                .map(|clause| crate::terlan_typeck::CoreCaseClause {
                    pattern: clause.pattern,
                    guard: clause.guard,
                    body: contextualize_control_result(clause.body, target_type),
                })
                .collect(),
        },
        CoreExpr::Let { bindings, body } => CoreExpr::Let {
            bindings,
            body: Box::new(contextualize_control_result(*body, target_type)),
        },
        CoreExpr::Cast {
            expr,
            target_type: existing,
        } if existing == *target_type => CoreExpr::Cast {
            expr,
            target_type: existing,
        },
        other => CoreExpr::Cast {
            expr: Box::new(other),
            target_type: target_type.clone(),
        },
    }
}

fn scalar_pattern(pattern: &CorePattern) -> bool {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::String(_)
        | CorePattern::Atom(_) => true,
        CorePattern::Alias { pattern, .. } => scalar_pattern(pattern),
        CorePattern::Constructor { name, args, .. } => name == "Unit" && args.is_empty(),
        _ => false,
    }
}

/// Reports whether an expression contains a structured or retained case.
fn expression_contains_case(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Case { .. } => true,
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| expression_contains_case(&binding.value))
                || expression_contains_case(body)
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            expression_contains_case(&clause.condition) || expression_contains_case(&clause.body)
        }),
        CoreExpr::BinaryOp { left, right, .. } => {
            expression_contains_case(left) || expression_contains_case(right)
        }
        _ => false,
    }
}

/// Produces matching and binding operations for the scalar pattern subset.
fn scalar_pattern_plan(
    pattern: &CorePattern,
    temporary: &str,
) -> Result<ScalarPatternPlan, String> {
    match pattern {
        CorePattern::Wildcard => Ok(ScalarPatternPlan {
            predicate: None,
            bindings: Vec::new(),
        }),
        CorePattern::Var(value) if matches!(value.as_str(), "true" | "false" | "Unit" | "unit") => {
            Ok(ScalarPatternPlan {
                predicate: Some(scalar_equality(
                    temporary,
                    CoreExpr::Var(if value == "unit" {
                        "Unit".to_string()
                    } else {
                        value.clone()
                    }),
                )),
                bindings: Vec::new(),
            })
        }
        CorePattern::Var(name) => Ok(ScalarPatternPlan {
            predicate: None,
            bindings: vec![name.clone()],
        }),
        CorePattern::Int(value) => Ok(ScalarPatternPlan {
            predicate: Some(scalar_equality(temporary, CoreExpr::Int(*value))),
            bindings: Vec::new(),
        }),
        CorePattern::Float(value) => {
            let parsed = value.parse::<f64>().map_err(|error| {
                format!("error[native_ir.case_float]: invalid Float pattern `{value}`: {error}")
            })?;
            if !parsed.is_finite() {
                return Err(format!(
                    "error[native_ir.case_float]: invalid Float pattern `{value}`: value must be finite"
                ));
            }
            Ok(ScalarPatternPlan {
                predicate: Some(scalar_equality(temporary, CoreExpr::Float(value.clone()))),
                bindings: Vec::new(),
            })
        }
        CorePattern::String(value) => Ok(ScalarPatternPlan {
            predicate: Some(CoreExpr::RemoteCall {
                module: "$terlan.managed.http".to_string(),
                function: "string_equal".to_string(),
                args: vec![
                    CoreExpr::Var(temporary.to_string()),
                    CoreExpr::Binary(
                        serde_json::to_string(value)
                            .map_err(|error| format!("error[native_ir.case_string]: {error}"))?,
                    ),
                ],
            }),
            bindings: Vec::new(),
        }),
        CorePattern::Atom(value)
            if matches!(value.as_str(), "true" | "false" | "Unit" | "unit") =>
        {
            Ok(ScalarPatternPlan {
                predicate: Some(scalar_equality(
                    temporary,
                    CoreExpr::Atom(if value == "unit" {
                        "Unit".to_string()
                    } else {
                        value.clone()
                    }),
                )),
                bindings: Vec::new(),
            })
        }
        CorePattern::Atom(value) => Ok(ScalarPatternPlan {
            predicate: Some(scalar_equality(temporary, CoreExpr::Atom(value.clone()))),
            bindings: Vec::new(),
        }),
        CorePattern::Constructor { name, args, .. } if name == "Unit" && args.is_empty() => {
            Ok(ScalarPatternPlan {
                predicate: Some(scalar_equality(
                    temporary,
                    CoreExpr::Var("Unit".to_string()),
                )),
                bindings: Vec::new(),
            })
        }
        CorePattern::Alias { alias, pattern } => {
            let mut plan = scalar_pattern_plan(pattern, temporary)?;
            plan.bindings.insert(0, alias.clone());
            let mut unique = HashSet::new();
            plan.bindings.retain(|name| unique.insert(name.clone()));
            Ok(plan)
        }
        unsupported => Err(format!(
            "error[native_ir.case_pattern]: `{}` is not in the scalar case pattern profile",
            pattern_name(unsupported)
        )),
    }
}

/// Creates one word-equality predicate against the case scrutinee local.
fn scalar_equality(temporary: &str, literal: CoreExpr) -> CoreExpr {
    CoreExpr::BinaryOp {
        operator: "==".to_string(),
        left: Box::new(CoreExpr::Var(temporary.to_string())),
        right: Box::new(literal),
    }
}

/// Wraps an expression in ordered scalar pattern bindings.
fn bind_pattern_names(names: &[String], temporary: &str, body: CoreExpr) -> CoreExpr {
    if names.is_empty() {
        return body;
    }
    CoreExpr::Let {
        bindings: names
            .iter()
            .map(|name| CoreLetBinding {
                pattern: CorePattern::Var(name.clone()),
                value: CoreExpr::Var(temporary.to_string()),
            })
            .collect(),
        body: Box::new(body),
    }
}

/// Returns a stable diagnostic name for one unsupported pattern family.
fn pattern_name(pattern: &CorePattern) -> &'static str {
    match pattern {
        CorePattern::Wildcard => "Wildcard",
        CorePattern::Var(_) => "Var",
        CorePattern::Int(_) => "Int",
        CorePattern::Float(_) => "Float",
        CorePattern::String(_) => "String",
        CorePattern::StringPattern(_) => "StringPattern",
        CorePattern::Atom(_) => "Atom",
        CorePattern::Tuple(_) => "Tuple",
        CorePattern::Alias { .. } => "Alias",
        CorePattern::List(_) => "List",
        CorePattern::ListCons { .. } => "ListCons",
        CorePattern::Map(_) => "Map",
        CorePattern::Record { .. } => "Record",
        CorePattern::BinaryLayout { .. } => "BinaryLayout",
        CorePattern::Constructor { .. } => "Constructor",
    }
}
