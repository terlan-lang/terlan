use std::collections::BTreeMap;

use terlan_syntax::{
    SyntaxDeclarationPayload, SyntaxExprFieldOutput, SyntaxExprKind, SyntaxExprOutput,
    SyntaxHtmlAttrValueOutput, SyntaxHtmlElementOutput, SyntaxHtmlNodeOutput, SyntaxModuleOutput,
    SyntaxTemplatePropOutput,
};

use super::TEMPLATE_CHILDREN_SLOT;

/// Runtime value model for static template rendering.
///
/// Inputs:
/// - Produced by evaluating syntax-output expressions used as template props.
///
/// Output:
/// - A constrained static value that can be rendered, passed to component
///   templates, or inspected by slot paths.
///
/// Transformation:
/// - Keeps only compile-time-renderable values and rejects dynamic expression
///   shapes before HTML output is written.
#[derive(Debug, Clone, PartialEq)]
enum StaticTemplateValue {
    Text(String),
    Int(i64),
    Bool(bool),
    Html(String),
    Record {
        name: String,
        fields: BTreeMap<String, StaticTemplateValue>,
    },
}

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
fn find_syntax_template_props<'a>(
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
fn render_syntax_static_template_nodes(
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

/// Finds the template declaration associated with an HTML component tag.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `templates`: parsed templates with optional tag names.
/// - `tag`: HTML tag name to resolve as a component.
///
/// Output:
/// - Template name and prop declarations when the tag is a component.
///
/// Transformation:
/// - Cross-references declarations with parsed templates by tag metadata.
fn find_syntax_template_decl_by_tag<'a>(
    module: &'a SyntaxModuleOutput,
    templates: &'a BTreeMap<String, terlan_html::HtmlTemplate>,
    tag: &str,
) -> Option<(&'a str, &'a [SyntaxTemplatePropOutput])> {
    module
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Template { name, props, .. } => {
                let template = templates.get(name)?;
                if template.tag_name.as_deref() == Some(tag) {
                    Some((name.as_str(), props.as_slice()))
                } else {
                    None
                }
            }
            _ => None,
        })
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

/// Looks up the declared type text for a template prop.
///
/// Inputs:
/// - `template_props`: prop declarations to search.
/// - `name`: prop name.
///
/// Output:
/// - Borrowed annotation text when the prop exists.
///
/// Transformation:
/// - Performs a linear name lookup across current template props.
fn syntax_static_template_prop_type<'a>(
    template_props: &'a [SyntaxTemplatePropOutput],
    name: &str,
) -> Option<&'a str> {
    template_props
        .iter()
        .find(|prop| prop.name == name)
        .map(|prop| prop.annotation.text.as_str())
}

/// Renders a supported static Markdown field access.
///
/// Inputs:
/// - `markdown_imports`: Markdown documents keyed by import alias.
/// - `expr`: syntax-output field access expression.
///
/// Output:
/// - Rendered Markdown HTML or a render error.
///
/// Transformation:
/// - Validates the access is `alias.html` for an imported Markdown document and
///   rejects non-renderable fields such as `raw`.
fn render_syntax_static_markdown_field(
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    expr: &SyntaxExprOutput,
) -> Result<String, StaticSyntaxRenderError> {
    let field = expr.text.as_deref().ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(
            "static Markdown field access is missing a field name".to_string(),
        )
    })?;
    let value = expr.children.first().ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(
            "static Markdown field access is missing a value".to_string(),
        )
    })?;
    if value.kind != SyntaxExprKind::Var {
        return Err(StaticSyntaxRenderError::Invalid(
            "static Markdown output must reference an imported Markdown alias".to_string(),
        ));
    }
    let alias = value.text.as_deref().ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(
            "static Markdown field access is missing an alias".to_string(),
        )
    })?;
    let document = markdown_imports.get(alias).ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(format!("unknown static Markdown import `{}`", alias))
    })?;

    match field {
        "html" => Ok(document.rendered_html.clone()),
        "raw" => Err(StaticSyntaxRenderError::Invalid(format!(
            "Markdown import `{}.raw` is Binary and cannot be rendered as static Html",
            alias
        ))),
        other => Err(StaticSyntaxRenderError::Invalid(format!(
            "unknown static Markdown field `{}.{}`",
            alias, other
        ))),
    }
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

/// Resolves a slot path to a static template value.
///
/// Inputs:
/// - `values`: evaluated template prop values.
/// - `slot`: parsed slot path.
///
/// Output:
/// - Cloned static value at the slot path, or an error message.
///
/// Transformation:
/// - Resolves the root value and walks record fields for dotted slot paths.
fn static_template_slot_value(
    values: &BTreeMap<String, StaticTemplateValue>,
    slot: &terlan_html::HtmlSlot,
) -> Result<StaticTemplateValue, String> {
    let root = slot
        .path
        .first()
        .ok_or_else(|| "empty static template slot".to_string())?;
    let mut value = values
        .get(root)
        .cloned()
        .ok_or_else(|| format!("missing static template slot value `{}`", root))?;
    for field in slot.path.iter().skip(1) {
        match value {
            StaticTemplateValue::Record { ref fields, .. } => {
                value = fields.get(field).cloned().ok_or_else(|| {
                    format!(
                        "missing static template field `{}` in slot `{}`",
                        field,
                        slot.path.join(".")
                    )
                })?;
            }
            _ => {
                return Err(format!(
                    "static template slot `{}` does not reference a record field",
                    slot.path.join(".")
                ))
            }
        }
    }
    Ok(value)
}

/// Returns whether a slot path refers to the reserved `children` slot.
///
/// Inputs:
/// - `slot`: parsed slot path.
///
/// Output:
/// - `true` when the path is exactly `children`.
///
/// Transformation:
/// - Compares the single path segment with the reserved slot name.
fn is_template_children_slot(slot: &terlan_html::HtmlSlot) -> bool {
    slot.path.len() == 1
        && slot
            .path
            .first()
            .is_some_and(|root| root == TEMPLATE_CHILDREN_SLOT)
}

/// Converts a static template value into text.
///
/// Inputs:
/// - `value`: static template value.
///
/// Output:
/// - Text representation or an error when the value is not text-renderable.
///
/// Transformation:
/// - Stringifies scalar values and HTML fragments, and rejects records.
fn static_template_value_text(value: &StaticTemplateValue) -> Result<String, String> {
    match value {
        StaticTemplateValue::Text(text) => Ok(text.clone()),
        StaticTemplateValue::Int(value) => Ok(value.to_string()),
        StaticTemplateValue::Bool(value) => Ok(value.to_string()),
        StaticTemplateValue::Html(html) => Ok(html.clone()),
        StaticTemplateValue::Record { name, .. } => {
            Err(format!("cannot render static record `{}` as text", name))
        }
    }
}

/// Decodes supported static string and binary string literals.
///
/// Inputs:
/// - `text`: literal text from syntax output.
///
/// Output:
/// - Literal contents with simple escapes decoded.
///
/// Transformation:
/// - Strips `<<"...">>` or `"..."` delimiters when present, then unescapes the
///   inner text.
fn static_literal_text(text: &str) -> String {
    if let Some(inner) = text
        .strip_prefix("<<\"")
        .and_then(|text| text.strip_suffix("\">>"))
        .or_else(|| {
            text.strip_prefix('"')
                .and_then(|text| text.strip_suffix('"'))
        })
    {
        return unescape_static_literal_text(inner);
    }
    text.to_string()
}

/// Unescapes the supported static literal escape sequences.
///
/// Inputs:
/// - `text`: literal contents without delimiters.
///
/// Output:
/// - String with supported escapes decoded.
///
/// Transformation:
/// - Replaces newline, carriage-return, tab, quote, and backslash escapes while
///   preserving unknown escapes verbatim.
fn unescape_static_literal_text(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Returns whether a template prop annotation denotes HTML.
///
/// Inputs:
/// - `type_text`: annotation text from syntax output.
///
/// Output:
/// - `true` for `Html` and parameterized `Html[...]` annotations.
///
/// Transformation:
/// - Trims the annotation and checks the supported HTML type spellings.
fn is_static_template_html_type(type_text: &str) -> bool {
    let trimmed = type_text.trim();
    trimmed == "Html" || trimmed.starts_with("Html[")
}

/// Escapes text for an HTML attribute value.
///
/// Inputs:
/// - `text`: raw attribute value text.
///
/// Output:
/// - Attribute-safe HTML text.
///
/// Transformation:
/// - Escapes ampersands, quotes, and angle brackets.
fn escape_html_attr(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Escapes text for an HTML text node.
///
/// Inputs:
/// - `text`: raw text node contents.
///
/// Output:
/// - Text-node-safe HTML text.
///
/// Transformation:
/// - Escapes ampersands and angle brackets.
fn escape_html_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}
