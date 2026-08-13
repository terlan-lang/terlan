//! Typed iterator expansion for AOT list comprehensions.

use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreCaseClause, CoreEffectSet, CoreExpr, CoreFunction, CoreIntrinsicCall, CoreIntrinsicId,
    CoreMapPatternField, CoreMapTypeField, CoreModule, CoreParam, CorePattern,
    CorePrimitiveIntrinsic, CoreProofCoverage, CoreTupleTypeElem, CoreType,
};

use super::NativeIrResult;

mod completed;
use completed::fold_completed_effect_runs;
pub(super) use completed::lower_completed_guard_results;
pub(super) use completed::{completed_effect_list_type, lower_completed_effect_guards};

const EFFECT_CONTAINER: &str = "std.core.Effect.Effect";
const EFFECT_SUCCEED: &str = "std.core.Effect.succeed";
const RANGE_ITERATOR: &str = "std.range.Range.iterator";
const MAX_COMPREHENSIONS_PER_MODULE: usize = 128;

/// Rewrites every typed comprehension, including nested expressions, into the
/// portable iterator collector before application admission.
pub(super) fn lower_list_comprehensions(core: &mut CoreModule) -> NativeIrResult<()> {
    let mut count = 0usize;
    let original_len = core.functions.len();
    let mut helpers = Vec::new();
    for index in 0..original_len {
        let owner = core.functions[index].clone();
        for clause in &mut core.functions[index].clauses {
            if let Some(body) = clause.body.core_expr.as_mut() {
                lower_expr(body, &core.module, &owner, &mut helpers, &mut count)?;
                fold_completed_effect_runs(body, &HashMap::new());
            }
        }
    }
    core.functions.extend(helpers);
    Ok(())
}

fn lower_expr(
    expr: &mut CoreExpr,
    module: &str,
    owner: &CoreFunction,
    helpers: &mut Vec<CoreFunction>,
    count: &mut usize,
) -> NativeIrResult<()> {
    visit_children(expr, module, owner, helpers, count)?;
    let CoreExpr::ListComprehension {
        expr: yielded,
        generators,
        guards,
        lift,
    } = expr
    else {
        return Ok(());
    };
    if *count >= MAX_COMPREHENSIONS_PER_MODULE {
        return Err(format!(
            "error[native_ir.comprehension_budget]: module `{module}` exceeds {MAX_COMPREHENSIONS_PER_MODULE} comprehensions"
        )
        .into());
    }
    let comprehension_id = *count;
    *count += 1;
    let completed_effect = lift.as_deref() == Some(EFFECT_CONTAINER);
    if lift.is_some() && !completed_effect {
        return Err(format!(
            "error[native_ir.comprehension_lift]: AOT comprehension lift `{}` is unsupported",
            lift.as_deref().unwrap_or_default()
        )
        .into());
    }
    if completed_effect {
        lower_completed_effect_guards(guards)?;
    }
    lower_completed_guard_results(guards);
    for guard in guards.iter_mut() {
        lower_range_membership(guard);
    }
    let output_element = cast_type(yielded).ok_or_else(|| {
        format!(
            "error[native_ir.comprehension_result]: yielded expression `{}` has no concrete AOT type",
            yielded.contract_text()
        )
    })?;
    let output_type = CoreType::List(Box::new(output_element));
    let output_type = if completed_effect {
        completed_effect_list_type(&CoreType::Apply {
            constructor: EFFECT_CONTAINER.to_string(),
            args: vec![output_type],
        })?
    } else {
        output_type
    };
    let accepted = CoreExpr::Cast {
        expr: Box::new(CoreExpr::List(vec![(**yielded).clone()])),
        target_type: output_type.clone(),
    };
    let rejected = || CoreExpr::Cast {
        expr: Box::new(CoreExpr::List(Vec::new())),
        target_type: output_type.clone(),
    };
    let guard = guards
        .iter()
        .cloned()
        .reduce(|left, right| CoreExpr::BinaryOp {
            operator: "and".to_string(),
            left: Box::new(left),
            right: Box::new(right),
        });
    let mut expanded = match guard {
        Some(condition) => CoreExpr::If {
            clauses: vec![
                crate::terlan_typeck::CoreIfClause {
                    condition,
                    body: accepted,
                },
                crate::terlan_typeck::CoreIfClause {
                    condition: CoreExpr::Atom("true".to_string()),
                    body: rejected(),
                },
            ],
        },
        None => accepted,
    };
    for (index, generator) in generators.iter().enumerate().rev() {
        let source_type = cast_type(&generator.source).ok_or_else(|| {
            format!(
                "error[native_ir.comprehension_source]: generator source `{}` has no concrete AOT type",
                generator.source.contract_text()
            )
        })?;
        let element = iterable_element(&source_type)?;
        let item = format!("$comprehension_item_{comprehension_id}_{index}");
        let helper_name = format!(
            "$aot_comprehension_{}_{}_{}_{}",
            owner.name, owner.arity, comprehension_id, index
        );
        let (callback_parameter, callback_body) = match &generator.pattern {
            CorePattern::Var(name) => (name.clone(), expanded),
            pattern => (
                item.clone(),
                CoreExpr::Case {
                    scrutinee: Box::new(CoreExpr::Var(item.clone())),
                    clauses: vec![
                        CoreCaseClause {
                            pattern: pattern.clone(),
                            guard: None,
                            body: expanded,
                        },
                        CoreCaseClause {
                            pattern: CorePattern::Wildcard,
                            guard: None,
                            body: rejected(),
                        },
                    ],
                },
            ),
        };
        let iterator = if is_range(&source_type) {
            let (start, stop) = range_bounds(&generator.source)?;
            let range_helper = format!(
                "$aot_comprehension_range_{}_{}_{}_{}",
                owner.name, owner.arity, comprehension_id, index
            );
            helpers.push(build_range_helper(owner, range_helper.clone())?);
            let list_type = CoreType::List(Box::new(CoreType::Int));
            iterator_expr(
                CoreExpr::Call {
                    function: range_helper,
                    args: vec![
                        start,
                        stop,
                        CoreExpr::Cast {
                            expr: Box::new(CoreExpr::List(Vec::new())),
                            target_type: list_type.clone(),
                        },
                    ],
                },
                &list_type,
                &element,
            )?
        } else {
            iterator_expr(generator.source.clone(), &source_type, &element)?
        };
        expanded = CoreExpr::Cast {
            expr: Box::new(CoreExpr::Call {
                function: helper_name.clone(),
                args: vec![
                    iterator,
                    CoreExpr::Cast {
                        expr: Box::new(CoreExpr::List(Vec::new())),
                        target_type: output_type.clone(),
                    },
                    CoreExpr::Lam {
                        params: vec![CorePattern::Var(callback_parameter)],
                        body: Box::new(callback_body),
                    },
                ],
            }),
            target_type: output_type.clone(),
        };
        helpers.push(build_collector_helper(
            owner,
            helper_name,
            element,
            output_type.clone(),
        )?);
    }
    *expr = if completed_effect {
        CoreExpr::Call {
            function: EFFECT_SUCCEED.to_string(),
            args: vec![expanded],
        }
    } else {
        expanded
    };
    Ok(())
}

fn visit_children(
    expr: &mut CoreExpr,
    module: &str,
    owner: &CoreFunction,
    helpers: &mut Vec<CoreFunction>,
    count: &mut usize,
) -> NativeIrResult<()> {
    let mut visit = |child: &mut CoreExpr| lower_expr(child, module, owner, helpers, count);
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                visit(item)?;
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
            visit(head)?;
            visit(tail)?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            visit(expr)?;
            for generator in generators {
                visit(&mut generator.source)?;
            }
            for guard in guards {
                visit(guard)?;
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                visit(&mut binding.value)?;
            }
            visit(body)?;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                visit(&mut field.value)?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                visit(&mut field.value)?;
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            visit(base)?;
            for field in fields {
                visit(&mut field.value)?;
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => visit(base)?,
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                visit(arg)?;
            }
            visit(record)?;
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. }
        | CoreExpr::Intrinsic(CoreIntrinsicCall { args, .. }) => {
            for arg in args {
                visit(arg)?;
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. }
        | CoreExpr::FunctionCall {
            callee: receiver,
            args,
        } => {
            visit(receiver)?;
            for arg in args {
                visit(arg)?;
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                visit(parameter)?;
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            visit(scrutinee)?;
            for clause in clauses {
                if let Some(guard) = clause.guard.as_mut() {
                    visit(guard)?;
                }
                visit(&mut clause.body)?;
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            visit(body)?;
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = clause.guard.as_mut() {
                    visit(guard)?;
                }
                visit(&mut clause.body)?;
            }
            if let Some(after) = after_clause {
                visit(&mut after.trigger)?;
                visit(&mut after.body)?;
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                visit(&mut clause.condition)?;
                visit(&mut clause.body)?;
            }
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

fn build_collector_helper(
    owner: &CoreFunction,
    name: String,
    element: CoreType,
    output_type: CoreType,
) -> NativeIrResult<CoreFunction> {
    let iterator = iterator_type(element.clone());
    let callback = CoreType::Arrow {
        params: vec![element.clone()],
        return_type: Box::new(output_type.clone()),
    };
    let params = vec![
        CoreParam {
            name: "$iterator".to_string(),
            ty: iterator.contract_text(),
            core_ty: Some(iterator.clone()),
        },
        CoreParam {
            name: "$result".to_string(),
            ty: output_type.contract_text(),
            core_ty: Some(output_type.clone()),
        },
        CoreParam {
            name: "$callback".to_string(),
            ty: callback.contract_text(),
            core_ty: Some(callback),
        },
    ];
    let mut helper = configured_helper(owner, name.clone(), params, output_type.clone())?;
    let step_type = CoreType::Map(vec![
        CoreMapTypeField {
            key: "value".to_string(),
            operator: ":".to_string(),
            value: element,
        },
        CoreMapTypeField {
            key: "next".to_string(),
            operator: ":".to_string(),
            value: iterator,
        },
    ]);
    let next = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IteratorNext),
        args: vec![CoreExpr::Var("$iterator".to_string())],
        return_type: CoreType::Apply {
            constructor: "std.core.Option.Option".to_string(),
            args: vec![step_type],
        },
        effects: CoreEffectSet {
            effects: Vec::new(),
        },
        span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
    });
    let collected = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::Var("$callback".to_string())),
        args: vec![CoreExpr::Var("$value".to_string())],
    };
    let accumulated = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListConcat),
        args: vec![CoreExpr::Var("$result".to_string()), collected],
        return_type: output_type.clone(),
        effects: CoreEffectSet {
            effects: Vec::new(),
        },
        span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
    });
    let rest = CoreExpr::Call {
        function: name,
        args: vec![
            CoreExpr::Var("$rest".to_string()),
            accumulated,
            CoreExpr::Var("$callback".to_string()),
        ],
    };
    let body = CoreExpr::Case {
        scrutinee: Box::new(next),
        clauses: vec![
            CoreCaseClause {
                pattern: CorePattern::Constructor {
                    name: "Some".to_string(),
                    constructor_identity: Some("std.core.Option.Some".to_string()),
                    args: vec![CorePattern::Map(vec![
                        CoreMapPatternField {
                            key: "value".to_string(),
                            required: true,
                            value: CorePattern::Var("$value".to_string()),
                        },
                        CoreMapPatternField {
                            key: "next".to_string(),
                            required: true,
                            value: CorePattern::Var("$rest".to_string()),
                        },
                    ])],
                },
                guard: None,
                body: rest,
            },
            CoreCaseClause {
                pattern: CorePattern::Constructor {
                    name: "None".to_string(),
                    constructor_identity: Some("std.core.Option.None".to_string()),
                    args: Vec::new(),
                },
                guard: None,
                body: CoreExpr::Var("$result".to_string()),
            },
        ],
    };
    install_helper_body(&mut helper, body)?;
    Ok(helper)
}

fn configured_helper(
    owner: &CoreFunction,
    name: String,
    params: Vec<CoreParam>,
    output_type: CoreType,
) -> NativeIrResult<CoreFunction> {
    let mut helper = owner.clone();
    helper.name = name;
    helper.public = false;
    helper.native_operation = None;
    helper.params = params;
    helper.arity = helper.params.len();
    helper.return_type = output_type.contract_text();
    helper.core_return_type = Some(output_type);
    helper.clauses.truncate(1);
    let clause = helper
        .clauses
        .first_mut()
        .ok_or_else(|| "error[native_ir.comprehension_helper]: owner has no clause".to_string())?;
    clause.patterns = helper
        .params
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect();
    clause.core_patterns = helper
        .params
        .iter()
        .map(|parameter| Some(CorePattern::Var(parameter.name.clone())))
        .collect();
    clause.pattern_proof_coverage = vec![CoreProofCoverage::RuntimeBoundary; helper.params.len()];
    clause.pattern_checked_preservation_evidence = vec![None; helper.params.len()];
    clause.guard = None;
    Ok(helper)
}

fn install_helper_body(helper: &mut CoreFunction, body: CoreExpr) -> NativeIrResult<()> {
    let clause = helper
        .clauses
        .first_mut()
        .ok_or_else(|| "error[native_ir.comprehension_helper]: helper has no clause".to_string())?;
    clause.body.kind = "case".to_string();
    clause.body.core_expr = Some(body);
    clause.body.proof_coverage = CoreProofCoverage::RuntimeBoundary;
    Ok(())
}

fn build_range_helper(owner: &CoreFunction, name: String) -> NativeIrResult<CoreFunction> {
    let output_type = CoreType::List(Box::new(CoreType::Int));
    let params = ["$current", "$stop"]
        .into_iter()
        .map(|name| CoreParam {
            name: name.to_string(),
            ty: CoreType::Int.contract_text(),
            core_ty: Some(CoreType::Int),
        })
        .chain(std::iter::once(CoreParam {
            name: "$result".to_string(),
            ty: output_type.contract_text(),
            core_ty: Some(output_type.clone()),
        }))
        .collect();
    let mut helper = configured_helper(owner, name.clone(), params, output_type.clone())?;
    let append_current = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListConcat),
        args: vec![
            CoreExpr::Var("$result".to_string()),
            CoreExpr::Cast {
                expr: Box::new(CoreExpr::List(vec![CoreExpr::Var("$current".to_string())])),
                target_type: output_type.clone(),
            },
        ],
        return_type: output_type,
        effects: CoreEffectSet {
            effects: Vec::new(),
        },
        span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
    });
    let recurse = |operator: &str| CoreExpr::Call {
        function: name.clone(),
        args: vec![
            CoreExpr::BinaryOp {
                operator: operator.to_string(),
                left: Box::new(CoreExpr::Var("$current".to_string())),
                right: Box::new(CoreExpr::Int(1)),
            },
            CoreExpr::Var("$stop".to_string()),
            append_current.clone(),
        ],
    };
    install_helper_body(
        &mut helper,
        CoreExpr::If {
            clauses: vec![
                crate::terlan_typeck::CoreIfClause {
                    condition: CoreExpr::BinaryOp {
                        operator: "==".to_string(),
                        left: Box::new(CoreExpr::Var("$current".to_string())),
                        right: Box::new(CoreExpr::Var("$stop".to_string())),
                    },
                    body: append_current.clone(),
                },
                crate::terlan_typeck::CoreIfClause {
                    condition: CoreExpr::BinaryOp {
                        operator: "<".to_string(),
                        left: Box::new(CoreExpr::Var("$current".to_string())),
                        right: Box::new(CoreExpr::Var("$stop".to_string())),
                    },
                    body: recurse("+"),
                },
                crate::terlan_typeck::CoreIfClause {
                    condition: CoreExpr::Atom("true".to_string()),
                    body: recurse("-"),
                },
            ],
        },
    )?;
    Ok(helper)
}
fn iterator_expr(source: CoreExpr, ty: &CoreType, element: &CoreType) -> NativeIrResult<CoreExpr> {
    let primitive = if list_element(ty).is_some() {
        Some(CorePrimitiveIntrinsic::ListIterator)
    } else if set_element(ty).is_some() {
        Some(CorePrimitiveIntrinsic::SetIterator)
    } else if map_elements(ty).is_some() {
        Some(CorePrimitiveIntrinsic::MapIterator)
    } else {
        None
    };
    if let Some(primitive) = primitive {
        return Ok(CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(primitive),
            args: vec![source],
            return_type: iterator_type(element.clone()),
            effects: CoreEffectSet {
                effects: Vec::new(),
            },
            span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
        }));
    }
    if is_range(ty) {
        return Ok(CoreExpr::Cast {
            expr: Box::new(CoreExpr::Call {
                function: RANGE_ITERATOR.to_string(),
                args: vec![source],
            }),
            target_type: iterator_type(element.clone()),
        });
    }
    Err(format!(
        "error[native_ir.comprehension_source]: `{}` has no admitted iterator lowering",
        ty.contract_text()
    )
    .into())
}

fn range_bounds(source: &CoreExpr) -> NativeIrResult<(CoreExpr, CoreExpr)> {
    let source = match source {
        CoreExpr::Cast { expr, .. } => expr.as_ref(),
        source => source,
    };
    let CoreExpr::BinaryOp {
        operator,
        left,
        right,
    } = source
    else {
        return Err(
            "error[native_ir.comprehension_range]: non-sugar Range sources require direct Range iterator admission"
                .into(),
        );
    };
    if operator != ".." {
        return Err(
            "error[native_ir.comprehension_range]: unsupported range source operator".into(),
        );
    }
    Ok(((**left).clone(), (**right).clone()))
}

fn lower_range_membership(expr: &mut CoreExpr) {
    let CoreExpr::BinaryOp {
        operator,
        left,
        right,
    } = expr
    else {
        return;
    };
    lower_range_membership(left);
    lower_range_membership(right);
    if operator != "in" {
        return;
    }
    let CoreExpr::BinaryOp {
        operator: range_operator,
        left: start,
        right: stop,
    } = right.as_ref()
    else {
        return;
    };
    if range_operator != ".." {
        return;
    }
    let value = (**left).clone();
    let start = (**start).clone();
    let stop = (**stop).clone();
    let comparison = |operator: &str, left: CoreExpr, right: CoreExpr| CoreExpr::BinaryOp {
        operator: operator.to_string(),
        left: Box::new(left),
        right: Box::new(right),
    };
    let conjunction = |left: CoreExpr, right: CoreExpr| CoreExpr::BinaryOp {
        operator: "and".to_string(),
        left: Box::new(left),
        right: Box::new(right),
    };
    let ascending = conjunction(
        comparison("<=", start.clone(), stop.clone()),
        conjunction(
            comparison(">=", value.clone(), start.clone()),
            comparison("<=", value.clone(), stop.clone()),
        ),
    );
    let descending = conjunction(
        comparison(">", start.clone(), stop.clone()),
        conjunction(
            comparison("<=", value.clone(), start),
            comparison(">=", value, stop),
        ),
    );
    *expr = CoreExpr::BinaryOp {
        operator: "or".to_string(),
        left: Box::new(ascending),
        right: Box::new(descending),
    };
}

fn cast_type(expr: &CoreExpr) -> Option<CoreType> {
    match expr {
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        _ => None,
    }
}

fn iterable_element(ty: &CoreType) -> NativeIrResult<CoreType> {
    if let Some(element) = list_element(ty).or_else(|| set_element(ty)) {
        return Ok(element.clone());
    }
    if let Some((key, value)) = map_elements(ty) {
        return Ok(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(key.clone()),
            CoreTupleTypeElem::Type(value.clone()),
        ]));
    }
    if is_range(ty) {
        return Ok(CoreType::Int);
    }
    Err(format!(
        "error[native_ir.comprehension_source]: `{}` is not an admitted Iterable",
        ty.contract_text()
    )
    .into())
}

fn list_element(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::List(element) => Some(element),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

fn set_element(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Set") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

fn map_elements(ty: &CoreType) -> Option<(&CoreType, &CoreType)> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Map") && args.len() == 2 =>
        {
            Some((&args[0], &args[1]))
        }
        _ => None,
    }
}

fn is_range(ty: &CoreType) -> bool {
    match ty {
        CoreType::Named(name)
        | CoreType::Struct { name, .. }
        | CoreType::Apply {
            constructor: name, ..
        } => name.rsplit('.').next() == Some("Range"),
        _ => false,
    }
}

fn iterator_type(element: CoreType) -> CoreType {
    CoreType::Apply {
        constructor: "std.collections.Iterator.Iterator".to_string(),
        args: vec![element],
    }
}
