use super::core_intrinsic_lowering::core_intrinsic_call_expr_from_syntax;
use super::core_pattern_lowering::{core_pattern_from_syntax, core_patterns_from_syntax_slice};
use super::*;

/// Converts a syntax-output expression into a typed Core expression when covered.
///
/// Inputs:
/// - `expr`: syntax-output expression summary produced by the parser pipeline.
///
/// Output:
/// - `Some(CoreExpr)` for the first typed Core expression subset.
/// - `None` for forms that still require richer Core identity, typing, or
///   control-flow payloads.
///
/// Transformation:
/// - Reconstructs typed Core expression nodes from syntax-output kind, text,
///   and child expressions, without backend lowering or rendered summary text.
pub(crate) fn core_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    match expr.kind {
        SyntaxExprKind::Int => expr
            .text
            .as_ref()
            .and_then(|value| value.parse::<i64>().ok())
            .map(CoreExpr::Int),
        SyntaxExprKind::Float => expr.text.clone().map(CoreExpr::Float),
        SyntaxExprKind::Binary => expr.text.clone().map(CoreExpr::Binary),
        SyntaxExprKind::Atom => expr.text.clone().map(CoreExpr::Atom),
        SyntaxExprKind::Var => expr.text.clone().map(CoreExpr::Var),
        SyntaxExprKind::Tuple => core_exprs_from_syntax_children(expr).map(CoreExpr::Tuple),
        SyntaxExprKind::List => core_exprs_from_syntax_children(expr).map(CoreExpr::List),
        SyntaxExprKind::ListCons => core_list_cons_expr_from_syntax(expr),
        SyntaxExprKind::FixedArray => {
            core_exprs_from_syntax_children(expr).map(CoreExpr::FixedArray)
        }
        SyntaxExprKind::Index => core_index_expr_from_syntax(expr),
        SyntaxExprKind::IndexAssign => core_index_assign_expr_from_syntax(expr),
        SyntaxExprKind::ListComprehension => core_list_comprehension_expr_from_syntax(expr),
        SyntaxExprKind::Let => core_let_expr_from_syntax(expr),
        SyntaxExprKind::Map => core_map_expr_fields_from_syntax(expr).map(CoreExpr::Map),
        SyntaxExprKind::RecordConstruct => core_record_construct_expr_from_syntax(expr),
        SyntaxExprKind::FieldAccess => core_field_access_expr_from_syntax(expr),
        SyntaxExprKind::RecordAccess => core_record_access_expr_from_syntax(expr),
        SyntaxExprKind::RecordUpdate => core_record_update_expr_from_syntax(expr),
        SyntaxExprKind::TemplateInstantiate => core_template_instantiate_expr_from_syntax(expr),
        SyntaxExprKind::ConstructorChain => core_constructor_chain_expr_from_syntax(expr),
        SyntaxExprKind::RemoteFunRef => core_remote_fun_ref_expr_from_syntax(expr),
        SyntaxExprKind::Cast => core_cast_expr_from_syntax(expr),
        SyntaxExprKind::UnaryOp => core_unary_op_expr_from_syntax(expr),
        SyntaxExprKind::Call if expr.remote.is_some() => core_intrinsic_call_expr_from_syntax(expr)
            .or_else(|| core_remote_call_expr_from_syntax(expr)),
        SyntaxExprKind::Call if expr.remote.is_none() => core_intrinsic_call_expr_from_syntax(expr)
            .or_else(|| core_named_call_expr_from_syntax(expr)),
        SyntaxExprKind::FunctionCall => core_function_call_expr_from_syntax(expr),
        SyntaxExprKind::Case if expr.children.len() == 1 => {
            let scrutinee = Box::new(core_expr_from_syntax(&expr.children[0])?);
            let clauses = core_case_clauses_from_syntax(expr)?;
            Some(CoreExpr::Case { scrutinee, clauses })
        }
        SyntaxExprKind::Try => core_try_expr_from_syntax(expr),
        SyntaxExprKind::If => core_if_expr_from_syntax(expr),
        SyntaxExprKind::RawMacro => sql_query_core_expr_from_syntax(expr),
        SyntaxExprKind::Fun if expr.clauses.len() == 1 => {
            let clause = &expr.clauses[0];
            if clause.guard.is_some() {
                return None;
            }
            Some(CoreExpr::Lam {
                params: core_patterns_from_syntax_slice(&clause.patterns)?,
                body: Box::new(core_expr_from_syntax(&clause.body)?),
            })
        }
        SyntaxExprKind::BinaryOp => {
            let operator = expr.operator.clone()?;
            let left = expr.children.first().and_then(core_expr_from_syntax)?;
            let right = expr.children.get(1).and_then(core_expr_from_syntax)?;
            Some(CoreExpr::BinaryOp {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            })
        }
        SyntaxExprKind::Fun
        | SyntaxExprKind::Sequence
        | SyntaxExprKind::Call
        | SyntaxExprKind::Case
        | SyntaxExprKind::Macro
        | SyntaxExprKind::HtmlBlock
        | SyntaxExprKind::Quote
        | SyntaxExprKind::Unquote => None,
    }
}

/// Converts a ready syntax-output SQL raw macro into a CoreIR query payload.
///
/// Inputs:
/// - `expr`: syntax-output raw macro expression, expected to be `sql[Row]`.
///
/// Output:
/// - `Some(CoreExpr::SqlQuery)` when the SQL form has a wrapper plan.
/// - `None` for non-SQL raw macros or SQL forms blocked by wrapper readiness.
///
/// Transformation:
/// - Reuses SQL wrapper analysis to preserve row type, bound SQL, parameter
///   count, cardinality, result type, and simple projection fields at the
///   backend-neutral CoreIR boundary without emitting backend code.
pub fn sql_query_core_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    let plan = crate::sql_forms::build_sql_wrapper_plan(expr, expr.children.len())
        .ok()
        .flatten()?;

    Some(CoreExpr::SqlQuery {
        row_type: plan.row_type,
        bound_sql: plan.bound_sql,
        parameter_count: plan.parameter_count,
        cardinality: plan.cardinality.as_diagnostic_label().to_string(),
        result_type: plan.result_type,
        projection_fields: plan.projection_fields.unwrap_or_default(),
    })
}

/// Converts syntax-output expression children into typed Core expression children.
///
/// Inputs:
/// - `expr`: syntax-output parent expression whose children should be lowered.
///
/// Output:
/// - `Some(Vec<CoreExpr>)` when every child is in the current typed subset.
/// - `None` when at least one child is not yet representable as a typed Core
///   expression.
///
/// Transformation:
/// - Recursively lowers children and fails the parent conversion if any child
///   remains unsupported.
fn core_exprs_from_syntax_children(expr: &SyntaxExprOutput) -> Option<Vec<CoreExpr>> {
    expr.children.iter().map(core_expr_from_syntax).collect()
}

/// Converts a syntax-output cast expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output cast with one source child and target type text.
///
/// Output:
/// - `Some(CoreExpr::Cast)` when the source expression and target type are
///   both representable in CoreIR.
/// - `None` when the syntax shape is malformed or unsupported.
///
/// Transformation:
/// - Preserves `as` as an explicit backend-neutral conversion boundary without
///   choosing runtime conversion semantics. Typechecking decides whether the
///   boundary is already assignment-compatible or needs conversion traits.
fn core_cast_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Cast) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::Cast {
        expr: Box::new(core_expr_from_syntax(&expr.children[0])?),
        target_type: core_type_from_text(expr.text.as_deref()?)?,
    })
}

/// Converts a syntax-output list-cons expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output list-cons expression with head and tail children.
///
/// Output:
/// - `Some(CoreExpr::ListCons)` when both head and tail lower to typed Core
///   expressions.
/// - `None` when the shape is not list-cons or either side remains unsupported.
///
/// Transformation:
/// - Preserves the structural cons expression as a backend-agnostic head/tail
///   Core node without using list rendering syntax.
fn core_list_cons_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::ListCons) || expr.children.len() != 2 {
        return None;
    }

    Some(CoreExpr::ListCons {
        head: Box::new(core_expr_from_syntax(&expr.children[0])?),
        tail: Box::new(core_expr_from_syntax(&expr.children[1])?),
    })
}

/// Converts a syntax-output index expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output index expression with receiver and index children.
///
/// Output:
/// - `Some(CoreExpr::Call)` to the canonical `IndexGet.get_at` trait method
///   when both receiver and index lower into typed Core expressions.
/// - `None` when the shape is not index syntax, has the wrong child count, or
///   either child remains unsupported.
///
/// Transformation:
/// - Desugars bracket reads into the target-neutral trait call shape selected
///   by the typechecker. This keeps CoreIR from preserving raw bracket syntax
///   while still avoiding any list, tuple, map, or backend-specific lookup
///   semantics.
fn core_index_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Index) || expr.children.len() != 2 {
        return None;
    }

    Some(CoreExpr::Call {
        function: "IndexGet.get_at".to_string(),
        args: vec![
            core_expr_from_syntax(&expr.children[0])?,
            core_expr_from_syntax(&expr.children[1])?,
        ],
    })
}

/// Converts syntax-output indexed assignment into a typed Core trait call.
///
/// Inputs:
/// - `expr`: syntax-output `IndexAssign` node with collection, index, and value
///   children.
///
/// Output:
/// - `Some(CoreExpr::Call)` to the canonical `IndexSet.set_at` trait method
///   when every child lowers into typed Core.
/// - `None` when the shape is not indexed assignment syntax, has the wrong
///   child count, or any child remains outside the current Core subset.
///
/// Transformation:
/// - Desugars bracket assignment into the target-neutral trait call shape
///   selected by the typechecker. This preserves backend neutrality while
///   leaving receiver rebinding details to target-specific lowering.
fn core_index_assign_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::IndexAssign) || expr.children.len() != 3 {
        return None;
    }

    Some(CoreExpr::Call {
        function: "IndexSet.set_at".to_string(),
        args: vec![
            core_expr_from_syntax(&expr.children[0])?,
            core_expr_from_syntax(&expr.children[1])?,
            core_expr_from_syntax(&expr.children[2])?,
        ],
    })
}

/// Converts a syntax-output list comprehension into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output list comprehension with yielded expression, source
///   expression, one generator pattern, and optional guard child.
///
/// Output:
/// - `Some(CoreExpr::ListComprehension)` when yield/source/guard expressions
///   and generator pattern all lower into typed Core.
/// - `None` when the node is not a list comprehension, has unsupported child
///   shape, or carries unsupported pattern/expression payloads.
///
/// Transformation:
/// - Preserves the generator pattern, source expression, yielded expression,
///   and optional guard as backend-neutral CoreIR without choosing backend
///   comprehension semantics.
fn core_list_comprehension_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::ListComprehension)
        || expr.patterns.len() != 1
        || !(2..=3).contains(&expr.children.len())
    {
        return None;
    }

    Some(CoreExpr::ListComprehension {
        expr: Box::new(core_expr_from_syntax(&expr.children[0])?),
        pattern: core_pattern_from_syntax(&expr.patterns[0])?,
        source: Box::new(core_expr_from_syntax(&expr.children[1])?),
        guard: match expr.children.get(2) {
            Some(guard) => Some(Box::new(core_expr_from_syntax(guard)?)),
            None => None,
        },
    })
}

/// Converts a syntax-output let expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output let expression whose patterns are binding patterns
///   and whose children are binding values plus a required final body.
///
/// Output:
/// - `Some(CoreExpr::Let)` when every binding value and body lowers to typed
///   Core.
/// - `None` when the syntax-output shape is malformed or any child remains
///   unsupported.
///
/// Transformation:
/// - Pairs each binding pattern with its value child and lowers the final child
///   as the explicit result expression. Bodyless let expressions are rejected
///   as malformed input.
fn core_let_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Let)
        || expr.patterns.is_empty()
        || expr.children.len() != expr.patterns.len() + 1
    {
        return None;
    }

    let bindings = expr
        .patterns
        .iter()
        .zip(expr.children.iter())
        .map(|(pattern, value)| {
            Some(CoreLetBinding {
                pattern: core_pattern_from_syntax(pattern)?,
                value: core_expr_from_syntax(value)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let body = core_expr_from_syntax(expr.children.get(expr.patterns.len())?)?;

    Some(CoreExpr::Let {
        bindings,
        body: Box::new(body),
    })
}

/// Converts syntax-output map-expression fields into typed Core map fields.
///
/// Inputs:
/// - `expr`: syntax-output map expression whose fields should be lowered.
///
/// Output:
/// - `Some(Vec<CoreMapExprField>)` when every field value lowers to a typed
///   Core expression.
/// - `None` when the expression has non-map syntax or any field value remains
///   unsupported.
///
/// Transformation:
/// - Preserves field keys and required/optional source mode, while recursively
///   lowering field value expressions into backend-agnostic CoreIR.
fn core_map_expr_fields_from_syntax(expr: &SyntaxExprOutput) -> Option<Vec<CoreMapExprField>> {
    if !matches!(expr.kind, SyntaxExprKind::Map) {
        return None;
    }

    expr.fields
        .iter()
        .map(|field| {
            core_expr_from_syntax(&field.value).map(|value| CoreMapExprField {
                key: field.key.clone(),
                required: field.required,
                value,
            })
        })
        .collect()
}

/// Converts a syntax-output record construction into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output record construction with a record name and fields.
///
/// Output:
/// - `Some(CoreExpr::RecordConstruct)` when every field value lowers to typed
///   Core and the record name is present.
/// - `None` when the shape is not record construction, the name is missing, or
///   any field value remains unsupported.
///
/// Transformation:
/// - Preserves record identity and field assignments as semantic CoreIR data,
///   while recursively lowering field values into typed Core expressions.
fn core_record_construct_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::RecordConstruct) {
        return None;
    }

    Some(CoreExpr::RecordConstruct {
        name: expr.text.clone()?,
        fields: core_record_expr_fields_from_syntax(expr)?,
    })
}

/// Converts syntax-output record-construction fields into typed Core fields.
///
/// Inputs:
/// - `expr`: syntax-output record construction whose fields should be lowered.
///
/// Output:
/// - `Some(Vec<CoreRecordExprField>)` when every field value lowers.
/// - `None` when any field value remains unsupported.
///
/// Transformation:
/// - Preserves field keys and source assignment mode, while recursively
///   lowering field value expressions into backend-agnostic CoreIR.
fn core_record_expr_fields_from_syntax(
    expr: &SyntaxExprOutput,
) -> Option<Vec<CoreRecordExprField>> {
    expr.fields
        .iter()
        .map(|field| {
            core_expr_from_syntax(&field.value).map(|value| CoreRecordExprField {
                key: field.key.clone(),
                required: field.required,
                value,
            })
        })
        .collect()
}

/// Converts a syntax-output field access into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output field access with exactly one receiver child and
///   field text.
///
/// Output:
/// - `Some(CoreExpr::FieldAccess)` when the receiver lowers into typed Core and
///   the field name is present.
/// - `None` when the shape is not field access, has the wrong child count, or
///   the receiver is outside the current typed Core subset.
///
/// Transformation:
/// - Preserves the field-access receiver and source field name as
///   backend-neutral CoreIR, without resolving struct layout or emitting record
///   access syntax.
fn core_field_access_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::FieldAccess) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::FieldAccess {
        base: Box::new(core_expr_from_syntax(&expr.children[0])?),
        field: expr.text.clone()?,
    })
}

/// Converts a syntax-output record access into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output record access with exactly one receiver child and
///   `RecordName.field` text.
///
/// Output:
/// - `Some(CoreExpr::RecordAccess)` when the receiver lowers into typed Core
///   and both record name and field name are present.
/// - `None` when the shape is not record access, has the wrong child count,
///   carries malformed access text, or has an unsupported receiver.
///
/// Transformation:
/// - Splits the syntax-output access label into record identity and field name,
///   then preserves both with the recursively lowered receiver as
///   backend-neutral CoreIR.
fn core_record_access_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::RecordAccess) || expr.children.len() != 1 {
        return None;
    }

    let text = expr.text.as_deref()?;
    let (name, field) = text.split_once('.')?;
    if name.is_empty() || field.is_empty() {
        return None;
    }

    Some(CoreExpr::RecordAccess {
        base: Box::new(core_expr_from_syntax(&expr.children[0])?),
        name: name.to_string(),
        field: field.to_string(),
    })
}

/// Converts a syntax-output record update into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output record update with exactly one receiver child,
///   record-name text, and expression-valued update fields.
///
/// Output:
/// - `Some(CoreExpr::RecordUpdate)` when the receiver and every update value
///   lower into typed Core and the record name is present.
/// - `None` when the shape is not record update, has the wrong child count,
///   lacks record identity, or contains unsupported receiver/field expressions.
///
/// Transformation:
/// - Preserves update receiver, record identity, and field assignments as a
///   backend-neutral CoreIR update node without lowering into construction or
///   backend record syntax.
fn core_record_update_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::RecordUpdate) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::RecordUpdate {
        base: Box::new(core_expr_from_syntax(&expr.children[0])?),
        name: expr.text.clone()?,
        fields: core_record_expr_fields_from_syntax(expr)?,
    })
}

/// Converts a syntax-output template instantiation into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output template instantiation with template name text and
///   expression-valued prop fields.
///
/// Output:
/// - `Some(CoreExpr::TemplateInstantiate)` when the template name is present
///   and every prop value lowers into typed Core.
/// - `None` when the node is not template instantiation syntax, lacks template
///   identity, or contains unsupported prop value expressions.
///
/// Transformation:
/// - Preserves template identity and prop assignments as backend-neutral CoreIR
///   without treating the node as record construction.
fn core_template_instantiate_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::TemplateInstantiate) {
        return None;
    }

    Some(CoreExpr::TemplateInstantiate {
        name: expr.text.clone()?,
        fields: core_record_expr_fields_from_syntax(expr)?,
    })
}

/// Converts a syntax-output constructor chain into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output constructor-chain expression with a base call child
///   and a child record-construction expression.
///
/// Output:
/// - `Some(CoreExpr::ConstructorChain)` when the base is a local named call,
///   all base arguments lower into typed Core, and the right side lowers into
///   typed `CoreExpr::RecordConstruct`.
/// - `None` when the node is not constructor-chain syntax, has the wrong child
///   shape, uses a remote/non-name base call, has unsupported argument
///   expressions, or has a non-record right side.
///
/// Transformation:
/// - Preserves constructor-chain candidate identity as backend-neutral CoreIR
///   without resolving includes/parent eligibility or rewriting the chain into
///   backend record construction.
fn core_constructor_chain_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::ConstructorChain) || expr.children.len() != 2 {
        return None;
    }

    let base_call = &expr.children[0];
    if !matches!(base_call.kind, SyntaxExprKind::Call) || base_call.remote.is_some() {
        return None;
    }

    let (callee, args) = base_call.children.split_first()?;
    let base = match callee.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Atom => callee.text.clone()?,
        _ => return None,
    };
    let args = args
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;

    let record = core_expr_from_syntax(&expr.children[1])?;
    if !matches!(record, CoreExpr::RecordConstruct { .. }) {
        return None;
    }

    Some(CoreExpr::ConstructorChain {
        base,
        base_constructor_identity: None,
        args,
        record: Box::new(record),
    })
}

/// Converts a syntax-output remote function reference into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output remote function reference carrying module, function,
///   and arity metadata.
///
/// Output:
/// - `Some(CoreExpr::RemoteFunRef)` when module and function names are present.
/// - `None` when the syntax-output node has the wrong kind or missing metadata.
///
/// Transformation:
/// - Preserves the remote function identity as backend-neutral CoreIR metadata
///   without converting it into a call or backend function object.
fn core_remote_fun_ref_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::RemoteFunRef) {
        return None;
    }

    Some(CoreExpr::RemoteFunRef {
        module: expr.remote.clone()?,
        function: expr.text.clone()?,
        arity: expr.arity,
    })
}

/// Converts a syntax-output unary operation into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output unary operator expression with one operand child
///   and an operator payload.
///
/// Output:
/// - `Some(CoreExpr::UnaryOp)` when the operator exists and the operand lowers
///   into typed Core.
/// - `None` when the shape is not unary, has the wrong child count, lacks an
///   operator, or has an unsupported operand expression.
///
/// Transformation:
/// - Preserves the normalized unary operator token and recursively lowered
///   operand as backend-neutral CoreIR.
fn core_unary_op_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::UnaryOp) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::UnaryOp {
        operator: expr.operator.clone()?,
        operand: Box::new(core_expr_from_syntax(&expr.children[0])?),
    })
}

/// Converts a syntax-output remote call into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output call expression with a remote module target, callee
///   child, and argument children.
///
/// Output:
/// - `Some(CoreExpr::RemoteCall)` when the module exists, callee is an atom
///   function name, and all arguments lower into typed Core.
/// - `None` for local calls, unsupported callee shapes, missing module
///   metadata, empty child lists, or unsupported argument expressions.
///
/// Transformation:
/// - Preserves module/function identity and recursively lowered arguments as
///   backend-neutral CoreIR without resolving backend import semantics.
fn core_remote_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    let module = expr.remote.clone()?;
    let (callee, args) = expr.children.split_first()?;
    let function = match core_expr_from_syntax(callee)? {
        CoreExpr::Atom(function) => function,
        _ => return None,
    };
    Some(CoreExpr::RemoteCall {
        module,
        function,
        args: args
            .iter()
            .map(core_expr_from_syntax)
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Converts a syntax-output named call into a typed Core call candidate.
///
/// Inputs:
/// - `expr`: syntax-output `Call` expression with no remote target.
///
/// Output:
/// - `Some(CoreExpr::Call)` when the callee is a lowercase local function name
///   and all arguments lower to typed Core expressions.
/// - `Some(CoreExpr::ConstructorCall)` when the callee is an uppercase
///   constructor-like name and all arguments lower to typed Core expressions.
/// - `None` for non-name callees, empty call payloads, remote calls, or
///   unsupported argument expressions.
///
/// Transformation:
/// - Preserves lowercase function calls and uppercase constructor-call
///   candidates as separate backend-neutral CoreIR nodes without resolving
///   constructor eligibility.
fn core_named_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    let name = match callee.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Atom => callee.text.clone()?,
        _ => return None,
    };
    let args = args
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;

    if starts_with_ascii_lowercase(&name) {
        Some(CoreExpr::Call {
            function: name,
            args,
        })
    } else if starts_with_ascii_uppercase(&name) {
        Some(CoreExpr::ConstructorCall {
            constructor: name,
            constructor_identity: None,
            args,
        })
    } else {
        None
    }
}

/// Converts a syntax-output function-value invocation into typed CoreIR.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression created from `callee.(args)`.
///
/// Output:
/// - `Some(CoreExpr::FunctionCall)` when the callee and every argument are
///   representable in the current typed Core subset.
/// - `None` for malformed function-call payloads or unsupported child
///   expressions.
///
/// Transformation:
/// - Preserves the callable expression separately from named calls so later
///   target profiles and backends can distinguish `f.(x)` from `f(x)`.
fn core_function_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if expr.kind != SyntaxExprKind::FunctionCall || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    Some(CoreExpr::FunctionCall {
        callee: Box::new(core_expr_from_syntax(callee)?),
        args: args
            .iter()
            .map(core_expr_from_syntax)
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Checks whether a name begins with an ASCII lowercase character.
///
/// Inputs:
/// - `name`: source-level identifier text.
///
/// Output:
/// - `true` when the first character is ASCII lowercase.
/// - `false` for empty strings and non-lowercase leading characters.
///
/// Transformation:
/// - Reads only the first Unicode scalar value and applies the Terlan
///   source-mode ASCII lowercase convention used for function names.
fn starts_with_ascii_lowercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase())
}

/// Checks whether a name begins with an ASCII uppercase character.
///
/// Inputs:
/// - `name`: source-level identifier text.
///
/// Output:
/// - `true` when the first character is ASCII uppercase.
/// - `false` for empty strings and non-uppercase leading characters.
///
/// Transformation:
/// - Reads only the first Unicode scalar value and applies the Terlan
///   source-mode ASCII uppercase convention used for constructor candidates.
fn starts_with_ascii_uppercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

/// Converts syntax-output case clauses into typed Core case clauses.
///
/// Inputs:
/// - `expr`: syntax-output case expression whose clauses should be lowered.
///
/// Output:
/// - `Some(Vec<CoreCaseClause>)` when every branch has one covered pattern,
///   has a covered body expression, and has either no guard or a covered guard
///   expression.
/// - `None` when any branch still needs unsupported patterns, richer bodies,
///   unsupported guards, or richer body modeling.
///
/// Transformation:
/// - Recursively lowers branch patterns and bodies into typed Core payloads and
///   fails the whole case conversion if any branch remains unsupported.
fn core_case_clauses_from_syntax(expr: &SyntaxExprOutput) -> Option<Vec<CoreCaseClause>> {
    expr.clauses
        .iter()
        .map(core_case_clause_from_syntax)
        .collect()
}

/// Converts one syntax-output case clause into a typed Core case clause.
///
/// Inputs:
/// - `clause`: syntax-output case clause.
///
/// Output:
/// - `Some(CoreCaseClause)` for one-pattern clauses in the current typed
///   subset, including supported guarded forms.
/// - `None` for multi-pattern clauses, unsupported patterns, unsupported
///   guards, or unsupported bodies.
///
/// Transformation:
/// - Lowers the branch pattern and body without using backend syntax or
///   rendered summary text.
fn core_case_clause_from_syntax(
    clause: &terlan_syntax::SyntaxClauseOutput,
) -> Option<CoreCaseClause> {
    if clause.patterns.len() != 1 {
        return None;
    }
    let guard = clause
        .guard
        .as_ref()
        .and_then(|guard| core_expr_from_syntax(guard.as_ref()));
    Some(CoreCaseClause {
        pattern: core_pattern_from_syntax(&clause.patterns[0])?,
        guard,
        body: core_expr_from_syntax(&clause.body)?,
    })
}

/// Converts a syntax-output try expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output try expression with body, `of` clauses, `catch`
///   clauses, and optional cleanup branch.
///
/// Output:
/// - `Some(CoreExpr::Try)` when the body, every clause, and optional cleanup
///   branch lower into typed Core.
/// - `None` when the node is not try syntax or any child remains unsupported.
///
/// Transformation:
/// - Preserves try body, success clauses, catch clauses, and optional cleanup
///   branch as a backend-neutral CoreIR keyword expression.
fn core_try_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Try) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::Try {
        body: Box::new(core_expr_from_syntax(&expr.children[0])?),
        of_clauses: core_case_clauses_from_syntax(expr)?,
        catch_clauses: expr
            .catch_clauses
            .iter()
            .map(core_case_clause_from_syntax)
            .collect::<Option<Vec<_>>>()?,
        after_clause: match expr.try_after.as_ref() {
            Some(after_clause) => Some(core_try_after_from_syntax(after_clause)?),
            None => None,
        },
    })
}

/// Converts a syntax-output try cleanup branch into typed Core.
///
/// Inputs:
/// - `after_clause`: syntax-output try cleanup trigger/body payload.
///
/// Output:
/// - `Some(CoreTryAfter)` when both trigger and body lower into typed Core.
/// - `None` when either expression remains unsupported.
///
/// Transformation:
/// - Preserves cleanup trigger and body as a try-specific CoreIR branch without
///   backend cleanup semantics.
fn core_try_after_from_syntax(
    after_clause: &terlan_syntax::syntax_output::SyntaxTryAfterOutput,
) -> Option<CoreTryAfter> {
    Some(CoreTryAfter {
        trigger: Box::new(core_expr_from_syntax(&after_clause.trigger)?),
        body: Box::new(core_expr_from_syntax(&after_clause.body)?),
    })
}

/// Converts a syntax-output if expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output if expression whose clauses carry conditions in
///   `guard` and branch bodies in `body`.
///
/// Output:
/// - `Some(CoreExpr::If)` when every condition and body lowers into typed Core.
/// - `None` when the node is not an if expression, contains pattern payloads,
///   lacks a condition, or contains unsupported condition/body expressions.
///
/// Transformation:
/// - Reconstructs condition/body branches from syntax-output clauses without
///   treating them as pattern-matching case clauses.
fn core_if_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::If) {
        return None;
    }

    expr.clauses
        .iter()
        .map(core_if_clause_from_syntax)
        .collect::<Option<Vec<_>>>()
        .map(|clauses| CoreExpr::If { clauses })
}

/// Converts one syntax-output if clause into typed Core.
///
/// Inputs:
/// - `clause`: syntax-output if clause with no patterns, condition in `guard`,
///   and branch body in `body`.
///
/// Output:
/// - `Some(CoreIfClause)` when condition and body are typed Core expressions.
/// - `None` when patterns are present, condition is missing, or either
///   expression remains unsupported.
///
/// Transformation:
/// - Lowers the condition/body pair while preserving the if-specific branch
///   shape independently from case-pattern clauses.
fn core_if_clause_from_syntax(clause: &terlan_syntax::SyntaxClauseOutput) -> Option<CoreIfClause> {
    if !clause.patterns.is_empty() {
        return None;
    }
    Some(CoreIfClause {
        condition: core_expr_from_syntax(clause.guard.as_ref()?.as_ref())?,
        body: core_expr_from_syntax(&clause.body)?,
    })
}
