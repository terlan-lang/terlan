//! Checked external-template ownership at the CoreIR handoff.

use std::collections::BTreeSet;

use crate::terlan_typeck::{
    CoreExpr, CoreModule, CoreTemplateExpression, CoreTemplateProp, CoreTemplateRenderPlan,
    CoreType,
};

/// Builds CoreIR-owned render plans from already-validated external templates.
///
/// Inputs:
/// - `inputs`: exact parsed frontend inputs accepted by template typechecking.
/// - `core`: checked module declarations used to type expression islands.
///
/// Output:
/// - Deterministically ordered CoreIR render plans.
/// - A stable diagnostic when checked syntax has no executable CoreIR shape.
///
/// Transformation:
/// - Converts prop types/defaults and expression islands into CoreIR, then
///   moves the already-checked parsed HTML tree without reopening source.
pub(super) fn core_template_render_plans(
    inputs: Vec<crate::commands::artifacts::SyntaxTemplateFrontendInput>,
    core: &CoreModule,
) -> Result<Vec<CoreTemplateRenderPlan>, String> {
    let mut plans = inputs
        .into_iter()
        .map(|input| {
            let props = lower_props(&input)?;
            let expressions = lower_expressions(&input, &props, core)?;
            Ok(CoreTemplateRenderPlan {
                name: input.name,
                source_path: input.source_path,
                props,
                expressions,
                template: input.parsed,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    plans.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(plans)
}

/// Lowers declaration-order template props into checked CoreIR contracts.
fn lower_props(
    input: &crate::commands::artifacts::SyntaxTemplateFrontendInput,
) -> Result<Vec<CoreTemplateProp>, String> {
    input
        .props
        .iter()
        .map(|prop| {
            let ty = crate::terlan_typeck::core_type_from_text(&prop.annotation.text).ok_or_else(
                || {
                    format!(
                        "template `{}` prop `{}` has no CoreIR type for `{}`",
                        input.name, prop.name, prop.annotation.text
                    )
                },
            )?;
            let default = prop
                .default
                .as_ref()
                .map(|default| {
                    crate::terlan_typeck::core_expr_lowering::core_expr_from_syntax(default)
                        .ok_or_else(|| {
                            format!(
                                "template `{}` prop `{}` default cannot lower to CoreIR",
                                input.name, prop.name
                            )
                        })
                })
                .transpose()?;
            Ok(CoreTemplateProp {
                name: prop.name.clone(),
                ty,
                default,
            })
        })
        .collect()
}

/// Lowers every unique non-path interpolation into typed CoreIR.
fn lower_expressions(
    input: &crate::commands::artifacts::SyntaxTemplateFrontendInput,
    props: &[CoreTemplateProp],
    core: &CoreModule,
) -> Result<Vec<CoreTemplateExpression>, String> {
    let mut sources = BTreeSet::new();
    collect_expression_sources(&input.parsed.nodes, &mut sources);
    sources
        .into_iter()
        .map(|source| {
            let syntax =
                crate::terlan_syntax::parse_expr_as_syntax_output(&source).map_err(|error| {
                    format!(
                        "template `{}` expression `{source}` cannot parse for CoreIR: {error:?}",
                        input.name
                    )
                })?;
            let expr = crate::terlan_typeck::core_expr_lowering::core_expr_from_syntax(&syntax)
                .ok_or_else(|| {
                    format!(
                        "template `{}` expression `{source}` has no CoreIR lowering",
                        input.name
                    )
                })?;
            let ty = infer_expression_type(&expr, props, core).ok_or_else(|| {
                format!(
                    "template `{}` expression `{source}` has no scalar CoreIR result type",
                    input.name
                )
            })?;
            Ok(CoreTemplateExpression { source, expr, ty })
        })
        .collect()
}

/// Collects non-path interpolation sources from one parsed node tree.
fn collect_expression_sources(
    nodes: &[crate::terlan_html::HtmlNode],
    sources: &mut BTreeSet<String>,
) {
    for node in nodes {
        match node {
            crate::terlan_html::HtmlNode::Slot(slot) if slot.path.is_empty() => {
                sources.insert(slot.expression.clone());
            }
            crate::terlan_html::HtmlNode::Element(element) => {
                for attr in &element.attrs {
                    if let Some(crate::terlan_html::HtmlAttrValue::Slot(slot)) = &attr.value {
                        if slot.path.is_empty() {
                            sources.insert(slot.expression.clone());
                        }
                    }
                }
                collect_expression_sources(&element.children, sources);
            }
            crate::terlan_html::HtmlNode::Text(_)
            | crate::terlan_html::HtmlNode::Comment(_)
            | crate::terlan_html::HtmlNode::Doctype(_)
            | crate::terlan_html::HtmlNode::Slot(_) => {}
        }
    }
}

/// Infers one validator-approved expression's concrete scalar CoreIR type.
fn infer_expression_type(
    expr: &CoreExpr,
    props: &[CoreTemplateProp],
    core: &CoreModule,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) | CoreExpr::Var(value)
            if matches!(value.as_str(), "true" | "false") =>
        {
            Some(CoreType::Bool)
        }
        CoreExpr::Var(name) => props
            .iter()
            .find(|prop| prop.name == *name)
            .map(|prop| prop.ty.clone()),
        CoreExpr::FieldAccess { base, field } | CoreExpr::RecordAccess { base, field, .. } => {
            let base = infer_expression_type(base, props, core)?;
            field_type(&base, field, core)
        }
        CoreExpr::Call { function, args } => core
            .functions
            .iter()
            .find(|candidate| candidate.name == *function && candidate.arity == args.len())
            .and_then(|function| function.core_return_type.clone()),
        CoreExpr::UnaryOp { operator, operand } => match operator.as_str() {
            "-" => infer_expression_type(operand, props, core),
            "not" | "!" => Some(CoreType::Bool),
            _ => None,
        },
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            let left = infer_expression_type(left, props, core)?;
            let right = infer_expression_type(right, props, core)?;
            match operator.as_str() {
                "+" | "-" | "*" | "/" | "div" | "rem" => {
                    Some(if left == CoreType::Float || right == CoreType::Float {
                        CoreType::Float
                    } else {
                        CoreType::Int
                    })
                }
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "&&" | "or" | "||" => {
                    Some(CoreType::Bool)
                }
                _ => None,
            }
        }
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::If { clauses } => {
            let mut types = clauses
                .iter()
                .map(|clause| infer_expression_type(&clause.body, props, core));
            let first = types.next()??;
            types.all(|ty| ty.as_ref() == Some(&first)).then_some(first)
        }
        _ => None,
    }
}

/// Resolves one named struct field from checked module type declarations.
fn field_type(base: &CoreType, field: &str, core: &CoreModule) -> Option<CoreType> {
    let body = match base {
        CoreType::Struct { fields, .. } => {
            return fields
                .iter()
                .find(|candidate| candidate.name == field)
                .map(|candidate| candidate.ty.clone());
        }
        CoreType::Named(name) => core
            .types
            .iter()
            .find(|decl| decl.name == *name || name.rsplit('.').next() == Some(&decl.name))
            .and_then(|decl| decl.core_body.as_ref()),
        _ => None,
    }?;
    field_type(body, field, core)
}
