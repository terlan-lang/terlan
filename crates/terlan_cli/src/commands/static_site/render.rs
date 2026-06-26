use std::collections::BTreeMap;

use terlan_syntax::{
    SyntaxDeclarationPayload, SyntaxExprFieldOutput, SyntaxExprKind, SyntaxExprOutput,
    SyntaxHtmlAttrValueOutput, SyntaxHtmlElementOutput, SyntaxHtmlNodeOutput, SyntaxModuleOutput,
    SyntaxTemplatePropOutput,
};

use super::render_lookup::{find_syntax_template_decl_by_tag, syntax_static_template_prop_type};
use super::render_markdown::render_syntax_static_markdown_field;
use super::render_values::{
    is_static_template_html_type, is_template_children_slot, static_literal_text,
    static_template_slot_value, static_template_value_text, StaticTemplateValue,
};
use super::TEMPLATE_CHILDREN_SLOT;
use terlan_html::{escape_html_attr, escape_html_text};

#[path = "template_calls.rs"]
mod template_calls;
use template_calls::syntax_static_template_call_fields;

/// Error returned by the static syntax renderer.
///
/// Inputs:
/// - Render-time validation failures from entrypoint, template, Markdown, and
///   HTML rendering.
///
/// Output:
/// - A structured error variant whose message can be printed by the CLI.
///
/// Transformation:
/// - Preserves user-facing validation text without coupling the renderer to
///   process exit codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StaticSyntaxRenderError {
    Invalid(String),
}

/// Renders one static entrypoint function into HTML.
///
/// Inputs:
/// - `module`: formal syntax-output module containing declarations.
/// - `templates`: parsed external HTML templates keyed by template name.
/// - `markdown_imports`: rendered Markdown imports keyed by source alias.
/// - `entrypoint`: function name to render.
///
/// Output:
/// - Rendered HTML for the entrypoint or a static render error.
///
/// Transformation:
/// - Locates the single-clause entrypoint and renders supported static return
///   forms: inline HTML, template instantiation, or Markdown HTML access.
pub(crate) fn render_syntax_static_entrypoint(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    entrypoint: &str,
) -> Result<String, StaticSyntaxRenderError> {
    let Some(clauses) =
        module
            .declarations
            .iter()
            .find_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Function { name, clauses, .. } if name == entrypoint => {
                    Some(clauses)
                }
                _ => None,
            })
    else {
        return Err(StaticSyntaxRenderError::Invalid(format!(
            "unknown static entrypoint `{}`",
            entrypoint
        )));
    };

    if clauses.len() != 1 {
        return Err(StaticSyntaxRenderError::Invalid(format!(
            "static entrypoint `{}` must have exactly one clause",
            entrypoint
        )));
    }

    let body = &clauses[0].body;
    match body.kind {
        SyntaxExprKind::HtmlBlock => {
            render_syntax_static_html_nodes(module, templates, markdown_imports, &body.html_nodes)
        }
        SyntaxExprKind::FieldAccess => render_syntax_static_markdown_field(markdown_imports, body),
        SyntaxExprKind::TemplateInstantiate => {
            let name = body.text.as_deref().ok_or_else(|| {
                StaticSyntaxRenderError::Invalid(format!(
                    "static entrypoint `{}` template instantiation is missing a template name",
                    entrypoint
                ))
            })?;
            render_syntax_static_template_instantiation(
                module,
                templates,
                markdown_imports,
                name,
                &body.fields,
            )
        }
        SyntaxExprKind::Call => {
            if let Some(html) =
                render_syntax_static_template_call(module, templates, markdown_imports, body)?
            {
                Ok(html)
            } else {
                Err(StaticSyntaxRenderError::Invalid(format!(
                    "static entrypoint `{}` must return a static html block, external template instantiation, or Markdown html",
                    entrypoint
                )))
            }
        }
        _ => Err(StaticSyntaxRenderError::Invalid(format!(
            "static entrypoint `{}` must return a static html block, external template instantiation, or Markdown html",
            entrypoint
        ))),
    }
}

/// Renders a syntax-output template instantiation expression.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `templates`: parsed external template bodies.
/// - `markdown_imports`: Markdown documents available to prop expressions.
/// - `name`: template declaration name.
/// - `fields`: template prop assignments from syntax output.
///
/// Output:
/// - Rendered HTML for the instantiated template or a render error.
///
/// Transformation:
/// - Evaluates each prop as a static value, verifies the template exists, and
///   renders the parsed template nodes with those values.
fn render_syntax_static_template_instantiation(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    name: &str,
    fields: &[SyntaxExprFieldOutput],
) -> Result<String, StaticSyntaxRenderError> {
    let template_props = find_syntax_template_props(module, name).ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(format!("unknown static template `{}`", name))
    })?;
    let template = templates.get(name).ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(format!("missing parsed static template `{}`", name))
    })?;
    let values = fields
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
        .collect::<Result<BTreeMap<_, _>, StaticSyntaxRenderError>>()?;

    render_syntax_static_template_nodes(module, templates, name, template_props, template, &values)
}

/// Renders a call expression as a generated static template function.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `templates`: parsed external template bodies.
/// - `markdown_imports`: Markdown documents available to prop expressions.
/// - `expr`: call expression to test and possibly render.
///
/// Output:
/// - `Some(rendered_html)` when the call target is a known template.
/// - `None` when the call is not a template call.
/// - Render error when a known template call has invalid arguments.
///
/// Transformation:
/// - Converts `Page("Home")` and `Page(title = "Home")` into the existing
///   template-instantiation field model using declaration prop order.
fn render_syntax_static_template_call(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    expr: &SyntaxExprOutput,
) -> Result<Option<String>, StaticSyntaxRenderError> {
    let Some((name, fields)) = syntax_static_template_call_fields(module, expr)? else {
        return Ok(None);
    };
    render_syntax_static_template_instantiation(module, templates, markdown_imports, &name, &fields)
        .map(Some)
}

/// Finds the declared props for a syntax-output template.
///
/// Inputs:
/// - `module`: syntax-output module to scan.
/// - `name`: template name to find.
///
/// Output:
/// - Borrowed prop declarations when the template is declared.
///
/// Transformation:
/// - Scans declarations and selects the matching template payload.
pub(super) fn find_syntax_template_props<'a>(
    module: &'a SyntaxModuleOutput,
    name: &str,
) -> Option<&'a [SyntaxTemplatePropOutput]> {
    module
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Template {
                name: template_name,
                props,
                ..
            } if template_name == name => Some(props.as_slice()),
            _ => None,
        })
}

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
fn eval_syntax_static_template_expr(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
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
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
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

/// Renders all nodes in a parsed static template.
///
/// Inputs:
/// - `module`: syntax-output module containing component declarations.
/// - `templates`: parsed template bodies.
/// - `template_name`: name of the template currently being rendered.
/// - `template_props`: declared props for the current template.
/// - `template`: parsed HTML template.
/// - `values`: evaluated prop values.
///
/// Output:
/// - Rendered HTML for the full template.
///
/// Transformation:
/// - Appends each parsed template node to a single output buffer.
pub(super) fn render_syntax_static_template_nodes(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    template_name: &str,
    template_props: &[SyntaxTemplatePropOutput],
    template: &terlan_html::HtmlTemplate,
    values: &BTreeMap<String, StaticTemplateValue>,
) -> Result<String, StaticSyntaxRenderError> {
    let mut out = String::new();
    for node in &template.nodes {
        render_syntax_static_template_node(
            module,
            templates,
            template_name,
            template_props,
            values,
            node,
            &mut out,
        )?;
    }
    Ok(out)
}

/// Renders one parsed template node into an output buffer.
///
/// Inputs:
/// - `module`: syntax-output module containing component declarations.
/// - `templates`: parsed template bodies.
/// - `template_name`: current template name for diagnostics.
/// - `template_props`: declared props for slot type checks.
/// - `values`: evaluated prop values.
/// - `node`: parsed template node to render.
/// - `out`: output buffer to append to.
///
/// Output:
/// - `Ok(())` when the node is appended, otherwise a render error.
///
/// Transformation:
/// - Emits text, comments, doctypes, regular elements, slot substitutions, and
///   component template expansions.
fn render_syntax_static_template_node(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    template_name: &str,
    template_props: &[SyntaxTemplatePropOutput],
    values: &BTreeMap<String, StaticTemplateValue>,
    node: &terlan_html::HtmlNode,
    out: &mut String,
) -> Result<(), StaticSyntaxRenderError> {
    match node {
        terlan_html::HtmlNode::Text(text) => out.push_str(text),
        terlan_html::HtmlNode::Comment(text) => {
            out.push_str("<!--");
            out.push_str(text);
            out.push_str("-->");
        }
        terlan_html::HtmlNode::Doctype(text) => {
            out.push_str("<!DOCTYPE ");
            out.push_str(text);
            out.push('>');
        }
        terlan_html::HtmlNode::Slot(slot) => {
            render_syntax_static_template_slot(template_props, values, slot, out)?;
        }
        terlan_html::HtmlNode::Element(element) => {
            if let Some((component_name, component_props)) =
                find_syntax_template_decl_by_tag(module, templates, &element.name)
            {
                render_syntax_static_template_component(
                    module,
                    templates,
                    template_name,
                    template_props,
                    component_name,
                    component_props,
                    values,
                    element,
                    out,
                )?;
                return Ok(());
            }
            out.push('<');
            out.push_str(&element.name);
            for attr in &element.attrs {
                out.push(' ');
                out.push_str(&attr.name);
                if let Some(value) = &attr.value {
                    out.push_str("=\"");
                    match value {
                        terlan_html::HtmlAttrValue::Text(text) => {
                            out.push_str(&escape_html_attr(text));
                        }
                        terlan_html::HtmlAttrValue::Slot(slot) => {
                            let value = static_template_slot_value(values, slot)
                                .map_err(StaticSyntaxRenderError::Invalid)?;
                            out.push_str(&escape_html_attr(
                                &static_template_value_text(&value)
                                    .map_err(StaticSyntaxRenderError::Invalid)?,
                            ));
                        }
                    }
                    out.push('"');
                }
            }
            out.push('>');
            for child in &element.children {
                render_syntax_static_template_node(
                    module,
                    templates,
                    template_name,
                    template_props,
                    values,
                    child,
                    out,
                )?;
            }
            out.push_str("</");
            out.push_str(&element.name);
            out.push('>');
        }
    }
    Ok(())
}

/// Renders a component used inside an external static template.
///
/// Inputs:
/// - `module`: syntax-output module containing nested component declarations.
/// - `templates`: parsed template bodies.
/// - `parent_name`: current parent template name.
/// - `parent_props`: current parent prop declarations.
/// - `component_name`: component template to render.
/// - `component_props`: component prop declarations.
/// - `parent_values`: evaluated values available in the parent template.
/// - `element`: component element from the parsed parent template.
/// - `out`: output buffer to append to.
///
/// Output:
/// - `Ok(())` when the component is appended, otherwise a render error.
///
/// Transformation:
/// - Builds component prop values from attributes/slots, renders children into
///   the reserved `children` value, then renders the component template body.
fn render_syntax_static_template_component(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    parent_name: &str,
    parent_props: &[SyntaxTemplatePropOutput],
    component_name: &str,
    component_props: &[SyntaxTemplatePropOutput],
    parent_values: &BTreeMap<String, StaticTemplateValue>,
    element: &terlan_html::HtmlElement,
    out: &mut String,
) -> Result<(), StaticSyntaxRenderError> {
    let component_template = templates.get(component_name).ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(format!(
            "missing parsed static template `{}`",
            component_name
        ))
    })?;
    let mut values = BTreeMap::new();

    if component_props
        .iter()
        .any(|prop| prop.name == TEMPLATE_CHILDREN_SLOT)
    {
        return Err(StaticSyntaxRenderError::Invalid(format!(
            "static template component `<{}>` declares reserved prop `{}`",
            element.name, TEMPLATE_CHILDREN_SLOT
        )));
    }

    for prop in component_props {
        let attr = element
            .attrs
            .iter()
            .find(|attr| attr.name == prop.name)
            .ok_or_else(|| {
                StaticSyntaxRenderError::Invalid(format!(
                    "static template component `<{}>` is missing prop `{}`",
                    element.name, prop.name
                ))
            })?;
        let value = match &attr.value {
            Some(terlan_html::HtmlAttrValue::Text(text)) => StaticTemplateValue::Text(text.clone()),
            Some(terlan_html::HtmlAttrValue::Slot(slot)) => {
                static_template_slot_value(parent_values, slot)
                    .map_err(StaticSyntaxRenderError::Invalid)?
            }
            None => {
                return Err(StaticSyntaxRenderError::Invalid(format!(
                    "static template component `<{}>` prop `{}` requires a value",
                    element.name, prop.name
                )))
            }
        };
        values.insert(prop.name.clone(), value);
    }

    let mut children = String::new();
    for child in &element.children {
        render_syntax_static_template_node(
            module,
            templates,
            parent_name,
            parent_props,
            parent_values,
            child,
            &mut children,
        )?;
    }
    values.insert(
        TEMPLATE_CHILDREN_SLOT.to_string(),
        StaticTemplateValue::Html(children),
    );

    for node in &component_template.nodes {
        render_syntax_static_template_node(
            module,
            templates,
            component_name,
            component_props,
            &values,
            node,
            out,
        )?;
    }

    for attr in &element.attrs {
        if !component_props.iter().any(|prop| prop.name == attr.name) {
            return Err(StaticSyntaxRenderError::Invalid(format!(
                "static template component `<{}>` has unknown prop `{}` in `{}`",
                element.name, attr.name, parent_name
            )));
        }
    }

    Ok(())
}

/// Renders one static template slot.
///
/// Inputs:
/// - `template_props`: prop declarations for slot type checks.
/// - `values`: evaluated prop values.
/// - `slot`: slot path to render.
/// - `out`: output buffer to append to.
///
/// Output:
/// - `Ok(())` when the slot is appended, otherwise a render error.
///
/// Transformation:
/// - Resolves the slot path, emits HTML slots unescaped, and escapes text slots
///   before appending them.
fn render_syntax_static_template_slot(
    template_props: &[SyntaxTemplatePropOutput],
    values: &BTreeMap<String, StaticTemplateValue>,
    slot: &terlan_html::HtmlSlot,
    out: &mut String,
) -> Result<(), StaticSyntaxRenderError> {
    let value =
        static_template_slot_value(values, slot).map_err(StaticSyntaxRenderError::Invalid)?;
    if is_template_children_slot(slot) {
        match value {
            StaticTemplateValue::Html(html) => {
                out.push_str(&html);
                return Ok(());
            }
            _ => {
                return Err(StaticSyntaxRenderError::Invalid(format!(
                    "template slot `{}` expected Html",
                    slot.path.join(".")
                )))
            }
        }
    }
    if slot.path.len() == 1
        && slot
            .path
            .first()
            .and_then(|root| syntax_static_template_prop_type(template_props, root))
            .is_some_and(is_static_template_html_type)
    {
        match value {
            StaticTemplateValue::Html(html) => {
                out.push_str(&html);
                return Ok(());
            }
            _ => {
                return Err(StaticSyntaxRenderError::Invalid(format!(
                    "template slot `{}` expected Html",
                    slot.path.join(".")
                )))
            }
        }
    }

    out.push_str(&escape_html_text(
        &static_template_value_text(&value).map_err(StaticSyntaxRenderError::Invalid)?,
    ));
    Ok(())
}

/// Renders inline syntax-output HTML nodes.
///
/// Inputs:
/// - `module`: syntax-output module containing component declarations.
/// - `templates`: parsed external templates for component tags.
/// - `markdown_imports`: Markdown documents for nested static expressions.
/// - `nodes`: inline HTML nodes from syntax output.
///
/// Output:
/// - Rendered HTML for all nodes.
///
/// Transformation:
/// - Appends each inline HTML node into a single output buffer.
fn render_syntax_static_html_nodes(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    nodes: &[SyntaxHtmlNodeOutput],
) -> Result<String, StaticSyntaxRenderError> {
    let mut out = String::new();
    for node in nodes {
        render_syntax_static_html_node(module, templates, markdown_imports, node, &mut out)?;
    }
    Ok(out)
}

/// Renders one inline syntax-output HTML node.
///
/// Inputs:
/// - `module`: syntax-output module containing component declarations.
/// - `templates`: parsed external templates for component tags.
/// - `markdown_imports`: Markdown documents for nested static expressions.
/// - `node`: inline HTML node to render.
/// - `out`: output buffer to append to.
///
/// Output:
/// - `Ok(())` when the node is appended, otherwise a render error.
///
/// Transformation:
/// - Emits static text/elements, expands component tags, escapes attributes,
///   and rejects dynamic interpolation that cannot be statically rendered yet.
fn render_syntax_static_html_node(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    node: &SyntaxHtmlNodeOutput,
    out: &mut String,
) -> Result<(), StaticSyntaxRenderError> {
    match node {
        SyntaxHtmlNodeOutput::Text { text } => out.push_str(text),
        SyntaxHtmlNodeOutput::Expr { .. } => {
            return Err(StaticSyntaxRenderError::Invalid(
                "static HTML output does not support dynamic interpolation yet".to_string(),
            ))
        }
        SyntaxHtmlNodeOutput::Element { element } => {
            if let Some((component_name, component_props)) =
                find_syntax_template_decl_by_tag(module, templates, &element.name)
            {
                render_syntax_static_inline_template_component(
                    module,
                    templates,
                    markdown_imports,
                    component_name,
                    component_props,
                    element,
                    out,
                )?;
                return Ok(());
            }

            out.push('<');
            out.push_str(&element.name);
            for attr in &element.attrs {
                out.push(' ');
                out.push_str(&attr.name);
                if let Some(value) = &attr.value {
                    match value {
                        SyntaxHtmlAttrValueOutput::Text { text } => {
                            out.push_str("=\"");
                            out.push_str(&escape_html_attr(text));
                            out.push('"');
                        }
                        SyntaxHtmlAttrValueOutput::Expr { .. } => {
                            return Err(StaticSyntaxRenderError::Invalid(format!(
                                "static HTML output does not support dynamic attribute `{}` yet",
                                attr.name
                            )))
                        }
                    }
                }
            }
            out.push('>');
            for child in &element.children {
                render_syntax_static_html_node(module, templates, markdown_imports, child, out)?;
            }
            out.push_str("</");
            out.push_str(&element.name);
            out.push('>');
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            return Err(StaticSyntaxRenderError::Invalid(format!(
                "named slot `@{}` must be inside a static template component",
                slot.name
            )))
        }
    }
    Ok(())
}

/// Renders a component used inside syntax-output inline HTML.
///
/// Inputs:
/// - `module`: syntax-output module containing nested component declarations.
/// - `templates`: parsed template bodies.
/// - `markdown_imports`: Markdown documents for prop expressions.
/// - `component_name`: component template to render.
/// - `component_props`: component prop declarations.
/// - `element`: inline HTML component element.
/// - `out`: output buffer to append to.
///
/// Output:
/// - `Ok(())` when the component is appended, otherwise a render error.
///
/// Transformation:
/// - Builds component values from attributes and named slots, injects
///   `children`, validates unknown props, and renders the component template.
fn render_syntax_static_inline_template_component(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    component_name: &str,
    component_props: &[SyntaxTemplatePropOutput],
    element: &SyntaxHtmlElementOutput,
    out: &mut String,
) -> Result<(), StaticSyntaxRenderError> {
    let component_template = templates.get(component_name).ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(format!(
            "missing parsed static template `{}`",
            component_name
        ))
    })?;
    let mut values = BTreeMap::new();

    if component_props
        .iter()
        .any(|prop| prop.name == TEMPLATE_CHILDREN_SLOT)
    {
        return Err(StaticSyntaxRenderError::Invalid(format!(
            "static template component `<{}>` declares reserved prop `{}`",
            element.name, TEMPLATE_CHILDREN_SLOT
        )));
    }

    let (children, mut named_slots) = render_syntax_static_inline_template_component_children(
        module,
        templates,
        markdown_imports,
        element,
    )?;

    for prop in component_props {
        let attr = element.attrs.iter().find(|attr| attr.name == prop.name);
        let named_slot = named_slots.remove(&prop.name);

        if attr.is_some() && named_slot.is_some() {
            return Err(StaticSyntaxRenderError::Invalid(format!(
                "static template component `<{}>` prop `{}` is set by both attribute and named slot",
                element.name, prop.name
            )));
        }

        let value = if let Some(attr) = attr {
            match &attr.value {
                Some(SyntaxHtmlAttrValueOutput::Text { text }) => {
                    StaticTemplateValue::Text(text.clone())
                }
                Some(SyntaxHtmlAttrValueOutput::Expr { expr }) => {
                    eval_syntax_static_template_expr(module, templates, markdown_imports, expr)?
                }
                None => {
                    return Err(StaticSyntaxRenderError::Invalid(format!(
                        "static template component `<{}>` prop `{}` requires a value",
                        element.name, prop.name
                    )))
                }
            }
        } else if let Some(html) = named_slot {
            if !is_static_template_html_type(&prop.annotation.text) {
                return Err(StaticSyntaxRenderError::Invalid(format!(
                    "static template component `<{}>` named slot `@{}` requires prop `{}` to be Html, found `{}`",
                    element.name, prop.name, prop.name, prop.annotation.text
                )));
            }
            StaticTemplateValue::Html(html)
        } else {
            return Err(StaticSyntaxRenderError::Invalid(format!(
                "static template component `<{}>` is missing prop `{}`",
                element.name, prop.name
            )));
        };

        values.insert(prop.name.clone(), value);
    }

    if let Some(name) = named_slots.keys().next() {
        return Err(StaticSyntaxRenderError::Invalid(format!(
            "static template component `<{}>` has unknown named slot `@{}`",
            element.name, name
        )));
    }

    values.insert(
        TEMPLATE_CHILDREN_SLOT.to_string(),
        StaticTemplateValue::Html(children),
    );

    for attr in &element.attrs {
        if !component_props.iter().any(|prop| prop.name == attr.name) {
            return Err(StaticSyntaxRenderError::Invalid(format!(
                "static template component `<{}>` has unknown prop `{}`",
                element.name, attr.name
            )));
        }
    }

    for node in &component_template.nodes {
        render_syntax_static_template_node(
            module,
            templates,
            component_name,
            component_props,
            &values,
            node,
            out,
        )?;
    }

    Ok(())
}

/// Renders inline component children and extracts named slots.
///
/// Inputs:
/// - `module`: syntax-output module for nested component rendering.
/// - `templates`: parsed template bodies.
/// - `markdown_imports`: Markdown documents for nested static expressions.
/// - `element`: inline component element whose children are being processed.
///
/// Output:
/// - A tuple of default children HTML and named-slot HTML by prop name.
///
/// Transformation:
/// - Separates `@slot` children from normal children, rejects duplicates and
///   reserved names, and renders each child subtree to static HTML.
fn render_syntax_static_inline_template_component_children(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    element: &SyntaxHtmlElementOutput,
) -> Result<(String, BTreeMap<String, String>), StaticSyntaxRenderError> {
    let mut children = String::new();
    let mut named_slots = BTreeMap::new();

    for child in &element.children {
        match child {
            SyntaxHtmlNodeOutput::NamedSlot { slot } => {
                if slot.name == TEMPLATE_CHILDREN_SLOT {
                    return Err(StaticSyntaxRenderError::Invalid(format!(
                        "static template component `<{}>` uses reserved named slot `@{}`",
                        element.name, TEMPLATE_CHILDREN_SLOT
                    )));
                }
                if named_slots.contains_key(&slot.name) {
                    return Err(StaticSyntaxRenderError::Invalid(format!(
                        "static template component `<{}>` has duplicate named slot `@{}`",
                        element.name, slot.name
                    )));
                }

                let mut html = String::new();
                for slot_child in &slot.children {
                    render_syntax_static_html_node(
                        module,
                        templates,
                        markdown_imports,
                        slot_child,
                        &mut html,
                    )?;
                }
                named_slots.insert(slot.name.clone(), html);
            }
            _ => render_syntax_static_html_node(
                module,
                templates,
                markdown_imports,
                child,
                &mut children,
            )?,
        }
    }

    Ok((children, named_slots))
}

#[cfg(test)]
#[path = "render_test.rs"]
mod render_test;
