use std::collections::BTreeMap;

use crate::terlan_syntax::{SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput};

use super::super::render_values::static_literal_text;
use super::{
    render_syntax_static_html_nodes, render_syntax_static_markdown_field,
    render_syntax_static_template_call, render_syntax_static_template_instantiation,
    StaticSyntaxRenderError, StaticTemplateValue, SyntaxModuleOutput,
};

/// Evaluates a syntax-output expression into a static template value.
///
/// Inputs:
/// - `module`: syntax-output module used for nested template lookups.
/// - `templates`: parsed external templates for nested render calls.
/// - `markdown_imports`: Markdown documents for field-access expressions.
/// - `expr`: expression to evaluate.
///
/// Output:
/// - A static template value or an error for unsupported dynamic syntax.
///
/// Transformation:
/// - Converts literals, inline HTML, nested template instantiations, Markdown
///   HTML accesses, and record construction into renderer-local values.
pub(super) fn eval_syntax_static_template_expr(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, crate::terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, crate::terlan_html::MarkdownDocument>,
    expr: &SyntaxExprOutput,
) -> Result<StaticTemplateValue, StaticSyntaxRenderError> {
    match expr.kind {
        SyntaxExprKind::Binary => Ok(StaticTemplateValue::Text(static_literal_text(
            expr.text.as_deref().unwrap_or_default(),
        ))),
        SyntaxExprKind::Int => {
            let value = expr
                .text
                .as_deref()
                .unwrap_or_default()
                .parse()
                .map_err(|_| {
                    StaticSyntaxRenderError::Invalid(format!(
                        "invalid static integer literal `{}`",
                        expr.text.as_deref().unwrap_or_default()
                    ))
                })?;
            Ok(StaticTemplateValue::Int(value))
        }
        SyntaxExprKind::Atom if expr.text.as_deref() == Some("true") => {
            Ok(StaticTemplateValue::Bool(true))
        }
        SyntaxExprKind::Atom if expr.text.as_deref() == Some("false") => {
            Ok(StaticTemplateValue::Bool(false))
        }
        SyntaxExprKind::HtmlBlock => Ok(StaticTemplateValue::Html(
            render_syntax_static_html_nodes(module, templates, markdown_imports, &expr.html_nodes)?,
        )),
        SyntaxExprKind::TemplateInstantiate => {
            let name = expr.text.as_deref().ok_or_else(|| {
                StaticSyntaxRenderError::Invalid(
                    "static template instantiation is missing a template name".to_string(),
                )
            })?;
            Ok(StaticTemplateValue::Html(
                render_syntax_static_template_instantiation(
                    module,
                    templates,
                    markdown_imports,
                    name,
                    &expr.fields,
                )?,
            ))
        }
        SyntaxExprKind::Call => {
            if let Some(html) =
                render_syntax_static_template_call(module, templates, markdown_imports, expr)?
            {
                Ok(StaticTemplateValue::Html(html))
            } else if let Some(record) =
                eval_syntax_static_struct_constructor(module, templates, markdown_imports, expr)?
            {
                Ok(record)
            } else {
                Err(StaticSyntaxRenderError::Invalid(
                    "static template output only supports literal template props".to_string(),
                ))
            }
        }
        SyntaxExprKind::FieldAccess => Ok(StaticTemplateValue::Html(
            render_syntax_static_markdown_field(markdown_imports, expr)?,
        )),
        SyntaxExprKind::RecordConstruct => Ok(StaticTemplateValue::Record {
            name: expr.text.clone().unwrap_or_default(),
            fields: expr
                .fields
                .iter()
                .map(|field| {
                    Ok((
                        field.key.clone(),
                        eval_syntax_static_template_expr(
                            module,
                            templates,
                            markdown_imports,
                            &field.value,
                        )?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, StaticSyntaxRenderError>>()?,
        }),
        _ => Err(StaticSyntaxRenderError::Invalid(
            "static template output only supports literal template props".to_string(),
        )),
    }
}

/// Evaluates a default struct-constructor call as a static record value.
///
/// Inputs:
/// - `module`: syntax-output module containing local struct declarations.
/// - `templates` and `markdown_imports`: static render dependencies for field
///   values.
/// - `expr`: call expression to inspect.
///
/// Output:
/// - `Some(StaticTemplateValue::Record)` when `expr` is `Struct(field = value)`
///   for a local struct and every field value is itself static.
/// - `None` when the expression is not a default struct-constructor call.
///
/// Transformation:
/// - Keeps the static renderer aligned with canonical Terlan struct
///   construction while preserving the existing record-valued slot model used
///   by dotted template slots such as `${user.name}`.
fn eval_syntax_static_struct_constructor(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, crate::terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, crate::terlan_html::MarkdownDocument>,
    expr: &SyntaxExprOutput,
) -> Result<Option<StaticTemplateValue>, StaticSyntaxRenderError> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return Ok(None);
    }
    let Some(callee) = expr.children.first().and_then(|child| child.text.as_ref()) else {
        return Ok(None);
    };
    if !module.declarations.iter().any(|declaration| {
        matches!(
            &declaration.payload,
            SyntaxDeclarationPayload::Struct { name, .. } if name == callee
        )
    }) {
        return Ok(None);
    }
    if expr.children.len().saturating_sub(1) != expr.arg_names.len()
        || expr.arg_names.iter().any(Option::is_none)
    {
        return Err(StaticSyntaxRenderError::Invalid(format!(
            "static struct constructor `{}` requires named field props",
            callee
        )));
    }

    let fields = expr.children[1..]
        .iter()
        .zip(expr.arg_names.iter())
        .map(|(value, name)| {
            Ok((
                name.clone().unwrap_or_default(),
                eval_syntax_static_template_expr(module, templates, markdown_imports, value)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, StaticSyntaxRenderError>>()?;

    Ok(Some(StaticTemplateValue::Record {
        name: callee.clone(),
        fields,
    }))
}
