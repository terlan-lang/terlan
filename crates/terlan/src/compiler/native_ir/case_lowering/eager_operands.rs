//! Structured control operands in eager evaluation contexts.

use super::{expression_contains_case, CoreExpr, CoreLetBinding, CorePattern, ScalarCaseLowerer};
use crate::terlan_typeck::CoreIfClause;

impl ScalarCaseLowerer {
    /// Gives eager operands ordered lexical owners when any operand is control.
    ///
    /// Capture every operand, including scalar calls before a case: extracting
    /// only the case would reorder side effects. Nested lexical bindings stay
    /// inside their own values, so source-local names cannot capture siblings.
    /// Branches, lambdas and comprehensions are not eager operand lists.
    pub(super) fn hoist_eager_cases(&mut self, mut expression: CoreExpr) -> CoreExpr {
        if let CoreExpr::If { clauses } = expression {
            return self.hoist_case_conditions(clauses);
        }
        let mut has_case = false;
        visit_eager_operands(&mut expression, &mut |operand| {
            has_case |= expression_contains_case(operand);
        });
        if !has_case {
            return expression;
        }
        let mut bindings = Vec::new();
        visit_eager_operands(&mut expression, &mut |operand| {
            let ordinal = self.scrutinee_ordinal;
            self.scrutinee_ordinal = self.scrutinee_ordinal.saturating_add(1);
            let temporary = format!("$native_case_{ordinal}_result");
            bindings.push(CoreLetBinding {
                pattern: CorePattern::Var(temporary.clone()),
                value: std::mem::replace(operand, CoreExpr::Var(temporary)),
            });
        });
        CoreExpr::Let {
            bindings,
            body: Box::new(expression),
        }
    }

    /// Gives a control-valued condition a join before selecting its branch.
    /// Later conditions remain inside the preceding clauses' fallback, never
    /// outside the conditional. This lets ordinary suspending-let lowering own
    /// mixed direct runtime effects and calls inside a structured condition.
    fn hoist_case_conditions(&mut self, clauses: Vec<CoreIfClause>) -> CoreExpr {
        if !clauses
            .iter()
            .any(|clause| expression_contains_case(&clause.condition))
        {
            return CoreExpr::If { clauses };
        }
        let mut suffix = Vec::new();
        let mut owned_first = false;
        for clause in clauses.into_iter().rev() {
            owned_first = expression_contains_case(&clause.condition);
            if owned_first {
                let ordinal = self.scrutinee_ordinal;
                self.scrutinee_ordinal = self.scrutinee_ordinal.saturating_add(1);
                let temporary = format!("$native_case_{ordinal}_condition");
                suffix.insert(
                    0,
                    CoreIfClause {
                        condition: CoreExpr::Var(temporary.clone()),
                        body: clause.body,
                    },
                );
                let body = CoreExpr::Let {
                    bindings: vec![CoreLetBinding {
                        pattern: CorePattern::Var(temporary),
                        value: clause.condition,
                    }],
                    body: Box::new(CoreExpr::If { clauses: suffix }),
                };
                suffix = vec![CoreIfClause {
                    condition: CoreExpr::Atom("true".into()),
                    body,
                }];
            } else {
                suffix.insert(0, clause);
            }
        }
        if owned_first {
            suffix
                .pop()
                .expect("owned first condition has a lexical body")
                .body
        } else {
            CoreExpr::If { clauses: suffix }
        }
    }
}

/// Visits immediate eager operands in source order without allocating a list.
fn visit_eager_operands(expression: &mut CoreExpr, visit: &mut impl FnMut(&mut CoreExpr)) {
    match expression {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter_mut().for_each(visit);
        }
        CoreExpr::ListCons { head, tail } => {
            visit(head);
            visit(tail);
        }
        CoreExpr::Index { base, index } => {
            visit(base);
            visit(index);
        }
        CoreExpr::Map(fields) => fields.iter_mut().for_each(|field| visit(&mut field.value)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields.iter_mut().for_each(|field| visit(&mut field.value));
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => visit(base),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            visit(base);
            fields.iter_mut().for_each(|field| visit(&mut field.value));
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter_mut().for_each(&mut *visit);
            visit(record);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => args.iter_mut().for_each(visit),
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            visit(receiver);
            args.iter_mut().for_each(visit);
        }
        CoreExpr::FunctionCall { callee, args } => {
            visit(callee);
            args.iter_mut().for_each(visit);
        }
        CoreExpr::Cast { expr, .. } => visit(expr),
        CoreExpr::Intrinsic(call) => call.args.iter_mut().for_each(visit),
        CoreExpr::SqlQuery { parameters, .. } => parameters.iter_mut().for_each(visit),
        CoreExpr::UnaryOp { operand, .. } => visit(operand),
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if operator != "and" && operator != "or" => {
            visit(left);
            visit(right);
        }
        _ => {}
    }
}
