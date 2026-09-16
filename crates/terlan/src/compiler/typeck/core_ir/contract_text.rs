//! Deterministic backend-neutral expression contract rendering.

use super::{
    CoreCaseClause, CoreExpr, CoreIfClause, CoreLetBinding, CoreMapExprField, CoreRecordExprField,
    CoreTryAfter,
};

/// Preserves explicit type arguments in fingerprints without changing untyped calls.
fn call_type_suffix(type_args: &[super::CoreType]) -> String {
    if type_args.is_empty() {
        String::new()
    } else {
        format!(
            "[{}]",
            type_args
                .iter()
                .map(super::CoreType::contract_text)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl CoreExpr {
    /// Renders a typed Core expression as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core expression from the initial Lean-covered subset.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the structural Core expression without source spans,
    ///   backend syntax, or syntax-output summary text.
    pub(crate) fn contract_text(&self) -> String {
        match self {
            CoreExpr::Int(value) => format!("Int({value})"),
            CoreExpr::Float(value) => format!("Float({value})"),
            CoreExpr::Binary(value) => format!("Binary({value})"),
            CoreExpr::Atom(value) => format!("Atom({value})"),
            CoreExpr::Var(name) => format!("Var({name})"),
            CoreExpr::Tuple(elements) => format!(
                "Tuple({})",
                elements
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::List(elements) => format!(
                "List({})",
                elements
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::ListCons { head, tail } => {
                format!(
                    "ListCons({}|{})",
                    head.contract_text(),
                    tail.contract_text()
                )
            }
            CoreExpr::FixedArray(elements) => format!(
                "FixedArray({})",
                elements
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::Index { base, index } => {
                format!("Index({};{})", base.contract_text(), index.contract_text())
            }
            CoreExpr::ListComprehension {
                expr,
                generators,
                guards,
                lift,
            } => {
                let generators = generators
                    .iter()
                    .map(|generator| {
                        format!(
                            "{}<-{}",
                            generator.pattern.contract_text(),
                            generator.source.contract_text()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let lift = lift
                    .as_ref()
                    .map(|container| format!(" lift {container}"))
                    .unwrap_or_default();
                if guards.is_empty() {
                    format!("ListComprehension({}|{}{lift})", expr.contract_text(), generators)
                } else {
                    format!(
                        "ListComprehension({}|{} if {}{lift})",
                        expr.contract_text(),
                        generators,
                        guards
                            .iter()
                            .map(CoreExpr::contract_text)
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                }
            }
            CoreExpr::Let { bindings, body } => format!(
                "Let({};{})",
                bindings
                    .iter()
                    .map(CoreLetBinding::contract_text)
                    .collect::<Vec<_>>()
                    .join(";"),
                body.contract_text()
            ),
            CoreExpr::Map(fields) => format!(
                "Map({})",
                fields
                    .iter()
                    .map(CoreMapExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::RecordConstruct { name, fields } => format!(
                "RecordConstruct({name};{})",
                fields
                    .iter()
                    .map(CoreRecordExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::FieldAccess { base, field } => {
                format!("FieldAccess({}.{})", base.contract_text(), field)
            }
            CoreExpr::RecordAccess { base, name, field } => {
                format!("RecordAccess({}#{}.{})", base.contract_text(), name, field)
            }
            CoreExpr::RecordUpdate { base, name, fields } => format!(
                "RecordUpdate({}#{};{})",
                base.contract_text(),
                name,
                fields
                    .iter()
                    .map(CoreRecordExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::TemplateInstantiate { name, fields } => format!(
                "TemplateInstantiate({name};{})",
                fields
                    .iter()
                    .map(CoreRecordExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::ConstructorChain {
                base,
                base_constructor_identity,
                args,
                record,
            } => {
                let args = args
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",");
                match base_constructor_identity {
                    Some(identity) => format!(
                        "ConstructorChain({base};identity={identity};{args} with {})",
                        record.contract_text()
                    ),
                    None => format!(
                        "ConstructorChain({base};{args} with {})",
                        record.contract_text()
                    ),
                }
            }
            CoreExpr::RemoteFunRef {
                module,
                function,
                arity,
            } => format!("RemoteFunRef({module}:{function}/{arity})"),
        CoreExpr::RemoteCall { type_args,
                module,
                function,
                args,
            } => format!(
                "RemoteCall({module}:{function}{};{})",
                call_type_suffix(type_args),
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::ConstructorCall {
                constructor,
                constructor_identity,
                args,
            } => {
                let args = args
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",");
                match constructor_identity {
                    Some(identity) => {
                        format!("ConstructorCall({constructor};identity={identity};{args})")
                    }
                    None => format!("ConstructorCall({constructor};{args})"),
                }
            }
            CoreExpr::Call { type_args, function, args } => format!(
                "Call({}{};{})",
                function,
                call_type_suffix(type_args),
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::MutableReceiverCall {
                receiver,
                method,
                args,
                effects,
            } => format!(
                "MutableReceiverCall({}.{};args={};effects={})",
                receiver.contract_text(),
                method,
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(","),
                effects.contract_text()
            ),
            CoreExpr::FunctionCall { callee, args } => format!(
                "FunctionCall({};{})",
                callee.contract_text(),
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::Cast { expr, target_type } => {
                format!(
                    "Cast({} as {})",
                    expr.contract_text(),
                    target_type.contract_text()
                )
            }
            CoreExpr::Intrinsic(call) => call.contract_text(),
            CoreExpr::SqlQuery {
                row_type,
                bound_sql,
                parameters,
                query_kind,
                transaction_requirement,
                cardinality,
                result_type,
                projection_fields,
                ..
            } => format!(
                "SqlQuery(row_type={row_type};params=[{}];kind={query_kind};transaction={transaction_requirement};cardinality={cardinality};result={result_type};projection={};sql={bound_sql})",
                parameters
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(","),
                projection_fields.join(",")
            ),
            CoreExpr::Case { scrutinee, clauses } => format!(
                "Case({};{})",
                scrutinee.contract_text(),
                clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            CoreExpr::Try {
                body,
                of_clauses,
                catch_clauses,
                after_clause,
            } => {
                let of_clauses = of_clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|");
                let catch_clauses = catch_clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|");
                match after_clause {
                    Some(after_clause) => format!(
                        "Try({};of={};catch={};after={})",
                        body.contract_text(),
                        of_clauses,
                        catch_clauses,
                        after_clause.contract_text()
                    ),
                    None => format!(
                        "Try({};of={};catch={})",
                        body.contract_text(),
                        of_clauses,
                        catch_clauses
                    ),
                }
            }
            CoreExpr::If { clauses } => format!(
                "If({})",
                clauses
                    .iter()
                    .map(CoreIfClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            CoreExpr::Lam { params, parameter_types, body } => format!(
                "Lam({};{})",
                params
                    .iter()
                    .enumerate()
                    .map(|(index, pattern)| match parameter_types.get(index).and_then(Option::as_ref) {
                        Some(ty) => format!("{}:{}", pattern.contract_text(), ty.contract_text()),
                        None => pattern.contract_text(),
                    })
                    .collect::<Vec<_>>()
                    .join(","),
                body.contract_text()
            ),
            CoreExpr::UnaryOp { operator, operand } => {
                format!("UnaryOp({};{})", operator, operand.contract_text())
            }
            CoreExpr::BinaryOp {
                operator,
                left,
                right,
            } => format!(
                "BinaryOp({};{}, {})",
                operator,
                left.contract_text(),
                right.contract_text()
            ),
        }
    }
}

impl CoreMapExprField {
    /// Renders a typed Core map-expression field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core map-expression field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the field key and recursively rendered value expression
    ///   without backend-specific syntax.
    fn contract_text(&self) -> String {
        format!("{}:{}", self.key, self.value.contract_text())
    }
}

impl CoreLetBinding {
    /// Renders one typed Core let binding as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: local binding lowered from syntax output.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the binding pattern and recursively rendered value
    ///   expression without source spans or backend syntax.
    fn contract_text(&self) -> String {
        format!(
            "{}={}",
            self.pattern.contract_text(),
            self.value.contract_text()
        )
    }
}

impl CoreRecordExprField {
    /// Renders a typed Core record-construction field as deterministic text.
    ///
    /// Inputs:
    /// - `self`: typed Core record field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the field key, source field assignment operator, and
    ///   recursively rendered value expression without backend-specific syntax.
    fn contract_text(&self) -> String {
        let operator = if self.required { "=" } else { "=>" };
        format!("{}{}{}", self.key, operator, self.value.contract_text())
    }
}

impl CoreCaseClause {
    /// Renders a typed Core case clause as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed unguarded case clause from the current Core subset.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the pattern/body pair without source spans, backend syntax,
    ///   or syntax-output summary text.
    fn contract_text(&self) -> String {
        let body = self.body.contract_text();
        match &self.guard {
            Some(guard) => format!(
                "{} where {}=>{}",
                self.pattern.contract_text(),
                guard.contract_text(),
                body
            ),
            None => format!("{}=>{}", self.pattern.contract_text(), body),
        }
    }
}

impl CoreIfClause {
    /// Renders a typed Core if clause as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed condition/body branch from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the condition/body pair without source spans, backend
    ///   syntax, or syntax-output summary text.
    fn contract_text(&self) -> String {
        format!(
            "{}=>{}",
            self.condition.contract_text(),
            self.body.contract_text()
        )
    }
}

impl CoreTryAfter {
    /// Renders a typed Core try cleanup branch as deterministic text.
    ///
    /// Inputs:
    /// - `self`: typed try cleanup branch from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the cleanup trigger/body pair without source spans,
    ///   backend syntax, or syntax-output summary text.
    fn contract_text(&self) -> String {
        format!(
            "{}=>{}",
            self.trigger.contract_text(),
            self.body.contract_text()
        )
    }
}
